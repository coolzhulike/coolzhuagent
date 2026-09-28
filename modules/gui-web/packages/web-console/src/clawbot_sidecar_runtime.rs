use std::env;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

enum SidecarState {
    Unconfigured(String),
    Failed(String),
    External,
    Owned {
        child: Child,
        gateway_url: String,
        nonce: String,
        #[cfg(windows)]
        _job: windows_process_guard::ChildProcessJob,
    },
}

pub(super) enum SidecarConnection {
    External,
    Owned { gateway_url: String, nonce: String },
}

fn state() -> &'static Mutex<SidecarState> {
    static STATE: OnceLock<Mutex<SidecarState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(SidecarState::Unconfigured("未配置真实微信服务".to_string())))
}

fn configured_bind_address() -> Result<SocketAddr, String> {
    let text = env::var("COOLZHU_CLAWBOT_SIDECAR_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let address: SocketAddr = text.parse().map_err(|_| "微信辅助服务监听地址无效".to_string())?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err("微信辅助服务仅允许监听固定本机地址与非零端口".to_string());
    }
    Ok(address)
}

fn sidecar_executable() -> Result<PathBuf, String> {
    let current = env::current_exe().map_err(|error| format!("读取 Web 控制台路径失败：{error}"))?;
    let path = current.with_file_name(if cfg!(windows) {
        "coolzhu-clawbot-sidecar.exe"
    } else {
        "coolzhu-clawbot-sidecar"
    });
    if !path.is_file() {
        return Err(format!("微信辅助服务程序不存在：{}", path.display()));
    }
    Ok(path)
}

pub(super) fn start(gateway_url: &str) {
    let result = start_owned(gateway_url);
    let mut slot = state().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = match result {
        Ok(owned) => owned,
        Err(error) if error == "未配置真实微信服务" => SidecarState::Unconfigured(error),
        Err(error) => SidecarState::Failed(error),
    };
}

fn start_owned(gateway_url: &str) -> Result<SidecarState, String> {
    // 用户显式指定的地址沿用原有外部 sidecar 接入语义；本进程不负责启停它。
    if let Ok(url) = env::var("COOLZHU_CLAWBOT_SIDECAR_URL") {
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| "外部微信辅助服务地址无效".to_string())?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("外部微信辅助服务地址必须是 HTTP(S) URL".to_string());
        }
        return Ok(SidecarState::External);
    }
    let kind = env::var("COOLZHU_CLAWBOT_PROVIDER_KIND").unwrap_or_default();
    if kind.trim().is_empty() {
        return Err("未配置真实微信服务".to_string());
    }
    if kind.trim() != "http" {
        return Err("正式微信连接仅支持已配置的真实 HTTP provider；不会自动使用 mock".to_string());
    }
    if env::var("COOLZHU_CLAWBOT_PROVIDER_URL")
        .ok()
        .is_none_or(|url| url.trim().is_empty())
    {
        return Err("真实微信 provider 地址未配置".to_string());
    }

    let bind = configured_bind_address()?;
    if bind.to_string() != "127.0.0.1:8787" {
        return Err("自动托管仅支持默认 127.0.0.1:8787；其他地址请显式配置外部辅助服务 URL".to_string());
    }
    // 端口已有监听时绝不复用或结束未知进程；启动后的健康响应还须核对受管子进程。
    let reservation = TcpListener::bind(bind)
        .map_err(|error| format!("微信辅助服务端口 {bind} 已被占用或不可用：{error}"))?;
    let executable = sidecar_executable()?;
    drop(reservation);

    let mut nonce_bytes = [0u8; 16];
    getrandom::fill(&mut nonce_bytes)
        .map_err(|error| format!("生成微信辅助服务启动身份失败：{error}"))?;
    let nonce = nonce_bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();

    let mut command = Command::new(executable);
    command
        .env("COOLZHU_CLAWBOT_PROVIDER_KIND", "http")
        .env("COOLZHU_CLAWBOT_SIDECAR_BIND", bind.to_string())
        .env("COOLZHU_WEB_CONSOLE_URL", gateway_url)
        .env("COOLZHU_CLAWBOT_INSTANCE_NONCE", &nonce)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        let (child, job) = windows_process_guard::ChildProcessJob::spawn_managed(&mut command)
            .map_err(|error| format!("启动微信辅助服务失败：{error}"))?;
        Ok(SidecarState::Owned { child, gateway_url: gateway_url.trim_end_matches('/').to_string(), nonce, _job: job })
    }
    #[cfg(not(windows))]
    {
        let child = command.spawn().map_err(|error| format!("启动微信辅助服务失败：{error}"))?;
        Ok(SidecarState::Owned { child, gateway_url: gateway_url.trim_end_matches('/').to_string(), nonce })
    }
}

pub(super) fn connection() -> Result<SidecarConnection, String> {
    let mut slot = state().lock().map_err(|_| "微信辅助服务状态锁已损坏".to_string())?;
    match &mut *slot {
        SidecarState::Unconfigured(reason) | SidecarState::Failed(reason) => Err(reason.clone()),
        SidecarState::External => Ok(SidecarConnection::External),
        SidecarState::Owned { child, gateway_url, nonce, .. } => match child.try_wait() {
            Ok(None) => Ok(SidecarConnection::Owned { gateway_url: gateway_url.clone(), nonce: nonce.clone() }),
            Ok(Some(status)) => {
                let error = format!("微信辅助服务已退出：{status}");
                *slot = SidecarState::Failed(error.clone());
                Err(error)
            }
            Err(error) => Err(format!("检查微信辅助服务进程失败：{error}")),
        },
    }
}

pub(super) fn stop() {
    let mut slot = state().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let SidecarState::Owned { child, .. } = &mut *slot {
        let _ = child.kill();
        let _ = child.wait();
    }
    *slot = SidecarState::Unconfigured("微信辅助服务已随应用停止".to_string());
}
