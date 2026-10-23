//! 单端口替换：只向同一用户、身份已核对的本产品进程请求优雅退出，不按端口强杀。
use std::{path::{Path,PathBuf},net::{SocketAddr,TcpStream},time::{Duration,Instant},io::{Read,Write}};
use serde_json::{json,Value};

pub fn token_path(root:&Path,pid:u32)->PathBuf {root.join(format!("service-restart-{pid}.json"))}
pub fn current_state_root()->Result<PathBuf,String> {
    crate::launch_paths::user_state_root_for(std::env::var_os("LOCALAPPDATA").as_deref().map(Path::new),
        std::env::var_os("USERPROFILE").or_else(||std::env::var_os("HOME")).as_deref().map(Path::new),
        std::env::var_os("COOLZHU_USER_STATE_DIR").as_deref().map(Path::new)).map_err(|e|e.to_string())
}
fn verified(value:&Value)->bool {
    value["same_user"].as_bool()==Some(true) && value["name"].as_str().is_some_and(|n|
        n.eq_ignore_ascii_case("coolzhu-web-console.exe") || n.eq_ignore_ascii_case("coolzhu-web-console"))
        && value["pid"].as_u64().is_some_and(|pid|pid>0&&pid!=u64::from(std::process::id()))
}
#[cfg(windows)]
fn powershell(script:&str)->Result<std::process::Output,String> {
    use std::os::windows::process::CommandExt;
    let output=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",script])
        .creation_flags(0x08000000).output().map_err(|e|e.to_string())?;
    if !output.status.success(){return Err("无法核对或等待原服务进程；未强制终止".into());}Ok(output)
}
/// 返回是否替换了既有进程；调用者随后启动最新构建。
pub fn stop_owned_instance(health_url:&str,root:&Path)->Result<bool,String> {
    let (host,port,_)=crate::parse_http_url(health_url).ok_or("启动健康地址无效")?;
    let addr:SocketAddr=format!("{host}:{port}").parse().map_err(|_|"启动地址必须为本机 IP")?;
    if !addr.ip().is_loopback(){return Err("自动重启只支持本机回环地址".into());}
    if port==0 || TcpStream::connect_timeout(&addr,Duration::from_millis(200)).is_err(){return Ok(false);}
    #[cfg(not(windows))] {let _=root;return Err("该平台尚未提供监听进程属主核验，请先关闭既有服务".into());}
    #[cfg(windows)] {
        let output=powershell(&format!("$c=Get-NetTCPConnection -State Listen -LocalPort {port} -ErrorAction Stop | Select-Object -First 1; $p=Get-CimInstance Win32_Process -Filter \"ProcessId=$($c.OwningProcess)\"; $owner=Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid; [pscustomobject]@{{pid=[int]$p.ProcessId;name=$p.Name;same_user=($owner.Sid -eq [Security.Principal.WindowsIdentity]::GetCurrent().User.Value)}} | ConvertTo-Json -Compress"))?;
        let owner:Value=serde_json::from_slice(&output.stdout).map_err(|_|"监听进程身份回读失败")?;
        if !verified(&owner){return Err(format!("端口 {port} 由其它用户或未知程序占用，未关闭该进程"));}
        let identity=crate::service_identity::fetch_service_identity(health_url);
        if !identity.protocol_shape_ok || identity.reported_port!=Some(port){return Err("原服务身份接口不兼容，未自动关闭".into());}
        let pid=owner["pid"].as_u64().and_then(|n|u32::try_from(n).ok()).ok_or("原服务 PID 无效")?;
        let token:Value=std::fs::read(token_path(root,pid)).ok().and_then(|bytes|serde_json::from_slice(&bytes).ok())
            .ok_or("原服务版本不支持安全重启，请先正常退出原服务，再启动最新版本")?;
        let secret=token["token"].as_str().filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or("原服务重启凭据无效")?;
        let body=json!({"pid":pid,"token":secret}).to_string();
        let mut socket=TcpStream::connect_timeout(&addr,Duration::from_secs(2)).map_err(|e|e.to_string())?;
        socket.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e|e.to_string())?;
        socket.write_all(format!("POST /api/system/restart HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).map_err(|e|e.to_string())?;
        let mut response=String::new();socket.take(64*1024).read_to_string(&mut response).map_err(|e|e.to_string())?;
        if !response.lines().next().is_some_and(|line|line.contains(" 200 ")){
            let message=response.split_once("\r\n\r\n").map(|(_,b)|b).unwrap_or("原服务未接受重启");
            return Err(format!("原服务未退出：{message}"));
        }
        // 等进程真正结束，避免端口已释放、数据库写锁仍未释放的半退出状态。
        powershell(&format!("$p=Get-Process -Id {pid} -ErrorAction SilentlyContinue; if($p){{Wait-Process -Id {pid} -Timeout 25 -ErrorAction Stop}}"))?;
        let deadline=Instant::now()+Duration::from_secs(3);
        while TcpStream::connect_timeout(&addr,Duration::from_millis(100)).is_ok(){
            if Instant::now()>=deadline{return Err("原服务已退出，但端口又被占用；未关闭新的占用者".into());}
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(true)
    }
}
#[cfg(test)] mod tests {use super::*;
    #[test] fn rejects_unknown_owner_and_program(){
        assert!(verified(&json!({"pid":1,"name":"coolzhu-web-console.exe","same_user":true})));
        assert!(!verified(&json!({"pid":1,"name":"other.exe","same_user":true})));
        assert!(!verified(&json!({"pid":1,"name":"coolzhu-web-console.exe","same_user":false})));
        assert!(!verified(&json!({"pid":std::process::id(),"name":"coolzhu-web-console.exe","same_user":true})));
    }
}
