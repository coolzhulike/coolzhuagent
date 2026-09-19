//! 受控原生笔画：固定 helper 只接受窗口身份和数值路径，不提供脚本执行入口。
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use super::MousePoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrokeWindow {
    pub handle: isize,
    pub process_id: u32,
    pub rect: [i32; 4],
    pub dpi: u32,
}

pub fn validate_stroke(window: StrokeWindow, bounds: [i32; 4], points: &[MousePoint], duration_ms: u64) -> Result<(), String> {
    let valid_rect = |r: [i32; 4]| r[2] > 0 && r[3] > 0 && r[0].checked_add(r[2]).is_some() && r[1].checked_add(r[3]).is_some();
    if window.handle == 0 || window.process_id == 0 || window.dpi == 0 || !valid_rect(window.rect) || !valid_rect(bounds) {
        return Err("invalid_stroke_bounds: 窗口身份或边界无效".into());
    }
    if bounds[0] < window.rect[0] || bounds[1] < window.rect[1]
        || i64::from(bounds[0]) + i64::from(bounds[2]) > i64::from(window.rect[0]) + i64::from(window.rect[2])
        || i64::from(bounds[1]) + i64::from(bounds[3]) > i64::from(window.rect[1]) + i64::from(window.rect[3]) {
        return Err("invalid_stroke_bounds: 画布超出已观察窗口".into());
    }
    if !(2..=256).contains(&points.len()) || duration_ms > 5_000 {
        return Err("invalid_stroke_path: 笔画需要 2–256 点且耗时不超过 5000ms".into());
    }
    if points.iter().any(|p| p.x < bounds[0] || p.y < bounds[1] || p.x >= bounds[0] + bounds[2] || p.y >= bounds[1] + bounds[3]) {
        return Err("invalid_stroke_path: 笔画坐标超出画布".into());
    }
    Ok(())
}

/// 取消会通知 helper；helper 的 finally 总会尝试释放鼠标，Escape 也可取消。
pub fn controlled_drag_path(window: StrokeWindow, bounds: [i32; 4], points: &[MousePoint], duration_ms: u64, timeout: Duration, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    validate_stroke(window, bounds, points, duration_ms)?;
    if cancelled() { return Err("stroke_cancelled: 输入开始前已取消".into()); }
    run_helper(serde_json::json!({"mode":"stroke", "window":window, "bounds":bounds, "points":points, "duration_ms":duration_ms}), timeout, cancelled).map(|_| ())
}

/// 只截取当前已绑定窗口的真实屏幕像素，不绘制或导入任何图片。
pub fn capture_window_image(window: StrokeWindow, timeout: Duration) -> Result<serde_json::Value, String> {
    if window.rect[2] <= 0 || window.rect[3] <= 0 || i64::from(window.rect[2]) * i64::from(window.rect[3]) > 16_777_216 {
        return Err("窗口截图尺寸无效或超过 1600 万像素".into());
    }
    let output = run_helper(serde_json::json!({"mode":"capture", "window":window}), timeout, &|| false)?;
    serde_json::from_str(output.trim()).map_err(|error| format!("窗口截图结果无效: {error}"))
}

fn run_helper(mut request: serde_json::Value, timeout: Duration, cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    if !cfg!(windows) { return Err("受控桌面输入仅支持 Windows".into()); }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let cancel_file = std::env::temp_dir().join(format!("coolzhu-stroke-cancel-{}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(), SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    request["cancel_file"] = serde_json::json!(cancel_file.to_string_lossy());
    let script = format!("$ErrorActionPreference='Stop'; $OutputEncoding=[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{}\n'@\n{}", include_str!("input_stroke_native.cs"), HELPER_ENTRY);
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    let mut child = command.spawn().map_err(|error| format!("无法启动受控输入 helper: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("helper stdin 不可用")?;
    let mut stdout = child.stdout.take().ok_or("helper stdout 不可用")?;
    let mut stderr = child.stderr.take().ok_or("helper stderr 不可用")?;
    let out_reader = std::thread::spawn(move || { let mut data=Vec::new(); let _=stdout.read_to_end(&mut data); data });
    let err_reader = std::thread::spawn(move || { let mut data=Vec::new(); let _=stderr.read_to_end(&mut data); data });
    if let Err(error) = stdin.write_all(request.to_string().as_bytes()) {
        let _=child.kill(); let _=child.wait(); drop(stdin);
        let _=out_reader.join();let _=err_reader.join();
        if request["mode"] == "stroke" { emergency_release()?; }
        return Err(format!("输入 helper 请求失败: {error}"));
    }
    drop(stdin);
    let started = Instant::now();
    let mut cancellation_at = None;
    let mut forced_kill = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {},
            Err(error) => { let _=child.kill(); let _=child.wait(); forced_kill=true; break Err(error.to_string()); }
        }
        if cancellation_at.is_none() && (cancelled() || started.elapsed() >= timeout) {
            // 独立哨兵文件只承载取消信号，不接收任何用户代码或命令。
            let _ = std::fs::write(&cancel_file, b"cancel");
            cancellation_at = Some(Instant::now());
        }
        if cancellation_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(2)) {
            let _ = child.kill(); forced_kill = true;
            break child.wait().map_err(|error| error.to_string());
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    let output = out_reader.join().unwrap_or_default();
    let errors = err_reader.join().unwrap_or_default();
    let _ = std::fs::remove_file(&cancel_file);
    if forced_kill && request["mode"] == "stroke" {
        // helper 异常失去响应时，先回收进程，再独立发送释放；禁止旧进程晚到重按。
        emergency_release()?;
    }
    if String::from_utf8_lossy(&errors).contains("mouse_release_failed") { return Err(format!("mouse_release_failed: {}",String::from_utf8_lossy(&errors).trim())); }
    if cancellation_at.is_some() { return Err("stroke_cancelled: 已取消或超时".into()); }
    if !status.map_err(|error|format!("输入 helper 状态读取失败: {error}"))?.success() { return Err(format!("受控桌面操作失败: {}", String::from_utf8_lossy(&errors).trim())); }
    Ok(String::from_utf8_lossy(&output).to_string())
}

fn emergency_release() -> Result<(),String> {
    // 只释放左键，绝不移动光标；递归仅在 stroke 模式发生，此分支为 release。
    run_helper(serde_json::json!({"mode":"release"}),Duration::from_secs(6),&||false)
        .map(|_|()).map_err(|error|format!("mouse_release_failed: 独立释放失败: {error}"))
}

const HELPER_ENTRY: &str = r#"
$r=[Console]::In.ReadToEnd()|ConvertFrom-Json
if($r.mode -eq 'release') { [CoolzhuStroke.Native]::EmergencyRelease(); 'released'; exit }
$w=$r.window
$native=[CoolzhuStroke.Native]::new([long]$w.handle,[uint32]$w.process_id,[int[]]$w.rect,[uint32]$w.dpi,[string]$r.cancel_file)
if($r.mode -eq 'capture') { $native.Capture()|ConvertTo-Json -Compress; exit }
if($r.mode -ne 'stroke') { throw 'unsupported helper mode' }
$points=@($r.points|ForEach-Object{[CoolzhuStroke.Point]::new([int]$_.x,[int]$_.y)})
[CoolzhuStroke.Engine]::Run($native,[CoolzhuStroke.Point[]]$points,[int[]]$r.bounds,[int]$r.duration_ms)
'released'
"#;

#[cfg(test)]
mod tests {
    use super::*;
    fn window() -> StrokeWindow { StrokeWindow { handle: 1, process_id: 2, rect: [-100, 50, 500, 300], dpi: 144 } }
    #[test]
    fn stroke_bounds_reject_window_escape_overflow_and_invalid_lengths() {
        let points=[MousePoint{x:0,y:100},MousePoint{x:100,y:200}];
        assert!(validate_stroke(window(),[-50,75,200,200],&points,100).is_ok());
        assert!(validate_stroke(window(),[-101,75,200,200],&points,100).is_err());
        assert!(validate_stroke(window(),[i32::MAX,0,2,2],&points,100).is_err());
        assert!(validate_stroke(window(),[-50,75,100,200],&points,100).is_err());
        assert!(validate_stroke(window(),[-50,75,200,200],&points,5001).is_err());
        assert!(validate_stroke(window(),[-50,75,200,200],&points[..1],100).is_err());
    }
    #[test]
    fn stroke_cancel_before_start_never_launches_backend() {
        let result=controlled_drag_path(window(),window().rect,&[MousePoint{x:0,y:100},MousePoint{x:100,y:200}],0,Duration::from_secs(1),&||true);
        assert!(result.unwrap_err().contains("stroke_cancelled"));
    }
    #[test]
    #[cfg(windows)]
    fn stroke_native_engine_mock_guarantees_release_on_failure_and_cancel() {
        // 运行与生产相同的 C# Engine，但注入纯内存驱动；不调用 Native/User32。
        let script=format!("$ErrorActionPreference='Stop'; Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{}\n'@\n[CoolzhuStroke.MockChecks]::Run()",include_str!("input_stroke_native.cs"));
        let result=super::super::run_powershell(&script,Duration::from_secs(12)).unwrap();
        assert_eq!(result.trim(),"mock-path-cancel-failure-release:ok");
    }
}
