//! 宿主内部登记通道：不把令牌、控制端口或 IPC 暴露给外部网页。
//! 当前只登记可见面板资源；DOM 快照与类型化动作将分别实现。
use std::{io::Read, path::PathBuf, time::{Duration, SystemTime, UNIX_EPOCH}};
use native_browser_protocol::{token_filename, HostReceipt, HostState, ObservationReply, STATE_PATH, OBSERVATION_PATH};
use tauri::AppHandle;

pub(super) fn start(app: &AppHandle) {
    let Some(origin) = super::browser_panel::console_origin() else { return; };
    let Ok(endpoint) = origin.join(STATE_PATH) else { return; };
    let Ok(observation_endpoint) = origin.join(OBSERVATION_PATH) else { return; };
    let Ok(client) = reqwest::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(2)).build() else { return; };
    let Some(port) = endpoint.port_or_known_default() else { return; };
    let Some(token_path) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .map(|root| root.join("CoolzhuAgent/runtime").join(token_filename(port))) else { return; };
    let app = app.clone();
    let boot = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let host_id = format!("native-{}-{boot:x}", std::process::id());
    tauri::async_runtime::spawn(async move {
        let mut sequence = 0u64;
        loop {
            sequence = sequence.saturating_add(1);
            // 后台重启会换令牌，宿主逐次读取；不持久化到前端或诊断日志。
            let token = std::fs::File::open(&token_path).and_then(|file| {
                let mut token = String::new();
                file.take(65).read_to_string(&mut token)?;
                Ok(token)
            });
            if let Ok(token) = token {
                if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    let state = HostState {host_id:host_id.clone(), sequence,
                        resource:super::browser_panel::input_resource(&app)};
                    // 登记失败仅撤销可用性，不触发页面输入、自动开页或回退 Chrome。
                    if let Ok(response) = client.post(endpoint.clone()).bearer_auth(&token).json(&state).send().await {
                        if response.status().is_success() {
                            // 响应最多一个固定观察请求；不接受方法名、脚本或网页指定参数。
                            if let Ok(receipt) = response.json::<HostReceipt>().await {
                                if let Some(request) = receipt.observation {
                                    let app = app.clone();
                                    let client = client.clone();
                                    let endpoint = observation_endpoint.clone();
                                    let host_id = host_id.clone();
                                    let token = token.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let observed = super::native_browser_observation::observe(&app, &request.resource).await;
                                        let (observation, error) = match observed {Ok(value) => (Some(value),None), Err(error) => (None,Some(error))};
                                        let reply = ObservationReply {host_id, request_id:request.request_id,
                                            resource:request.resource, observation, error};
                                        let _ = client.post(endpoint).bearer_auth(token).json(&reply).send().await;
                                    });
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}
