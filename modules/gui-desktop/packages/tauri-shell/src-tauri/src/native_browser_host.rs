//! 宿主内部登记通道：不把令牌、控制端口或 IPC 暴露给外部网页。
//! 当前只登记可见面板资源；DOM 快照与类型化动作将分别实现。
use std::{io::Read, path::PathBuf, time::Duration};
use native_browser_protocol::{token_filename, HostIdentity, HostReceipt, HostState, ObservationReply,
    STATE_PATH, OBSERVATION_PATH, MAX_HOST_RECEIPT_BYTES};
use tauri::AppHandle;
use native_browser_protocol::{ComputerUseActivityReceipt, ACTIVITY_PATH, MAX_ACTIVITY_BYTES};

pub(super) fn start(app: &AppHandle) {
    let Some(origin) = super::browser_panel::console_origin() else { return; };
    let Ok(endpoint) = origin.join(STATE_PATH) else { return; };
    let Ok(observation_endpoint) = origin.join(OBSERVATION_PATH) else { return; };
    let Ok(input_endpoint) = origin.join(native_browser_protocol::INPUT_PATH) else { return; };
    let Ok(activity_endpoint) = origin.join(ACTIVITY_PATH) else { return; };
    let Ok(client) = reqwest::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(2)).build() else { return; };
    let Some(port) = endpoint.port_or_known_default() else { return; };
    let Some(token_path) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .map(|root| root.join("CoolzhuAgent/runtime").join(token_filename(port))) else { return; };
    let app = app.clone();
    let Ok(boot_id) = random_id() else { return; };
    let host_id = format!("native-{boot_id}");
    let identity = capture_identity(&boot_id);
    super::computer_use_indicator::start_watchdog(&app);
    tauri::async_runtime::spawn(async move {
        let mut sequence = 0u64;
        loop {
            let mut activity = None;
            sequence = sequence.saturating_add(1);
            // 后台重启会换令牌，宿主逐次读取；不持久化到前端或诊断日志。
            let token = std::fs::File::open(&token_path).and_then(|file| {
                let mut token = String::new();
                file.take(65).read_to_string(&mut token)?;
                Ok(token)
            });
            if let Ok(token) = token {
                if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    let state = HostState {host_id:host_id.clone(), sequence, identity:identity.clone(),
                        resource:super::browser_panel::input_resource(&app)};
                    // 登记失败仅撤销可用性，不触发页面输入、自动开页或回退 Chrome。
                    if let Ok(response) = client.post(endpoint.clone()).bearer_auth(&token).json(&state).send().await {
                        if response.status().is_success() {
                            // 响应最多一个固定观察请求；不接受方法名、脚本或网页指定参数。
                            if let Some(receipt) = read_host_receipt(response).await {
                                if let Some(request) = receipt.input {
                                    let app=app.clone();let client=client.clone();let endpoint=input_endpoint.clone();
                                    let host_id=host_id.clone();let token=token.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let reply=super::native_browser_input::handle(&app,host_id,request).await;
                                        let _=client.post(endpoint).bearer_auth(token).json(&reply).send().await;
                                    });
                                }
                                if let Some(request) = receipt.observation {
                                    let app = app.clone();
                                    let client = client.clone();
                                    let endpoint = observation_endpoint.clone();
                                    let host_id = host_id.clone();
                                    let token = token.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let observed = super::native_browser_observation::observe(&app, &request.resource, &request.request_id).await;
                                        let (observation, error) = match observed {Ok(value) => (Some(value),None), Err(error) => (None,Some(error))};
                                        let reply = ObservationReply {host_id, request_id:request.request_id,
                                            resource:request.resource, observation, error};
                                        let _ = client.post(endpoint).bearer_auth(token).json(&reply).send().await;
                                    });
                                }
                            }
                        }
                    }
                    // 活动展示与浏览器资源登记独立；不要求打开浏览器，也不接收网页命令。
                    let started = std::time::Instant::now();
                    if let Ok(response) = client.get(activity_endpoint.clone()).bearer_auth(&token).send().await {
                        activity = read_activity(response, started).await;
                    }
                }
            }
            super::computer_use_indicator::refresh(&app, activity);
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    });
}

fn random_id() -> Result<String, ()> {
    let mut bytes = [0u8;16];
    getrandom::fill(&mut bytes).map_err(|_| ())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn capture_identity(boot_id: &str) -> Option<HostIdentity> {
    #[cfg(windows)]
    {
        let process = windows_process_guard::capture_live_process_identity(std::process::id()).ok()?;
        let image = std::path::Path::new(process.image_path()?).canonicalize().ok()?;
        if image != std::env::current_exe().ok()?.canonicalize().ok()? { return None; }
        Some(HostIdentity {instance_id:random_id().ok()?,boot_id:boot_id.into(),pid:std::process::id(),
            creation_time_filetime:process.creation_time_filetime(),canonical_executable:image.to_str()?.into()})
    }
    #[cfg(not(windows))]
    { let _ = boot_id; None }
}

async fn read_host_receipt(mut response: reqwest::Response) -> Option<HostReceipt> {
    if !response.status().is_success()
        || response.content_length().is_some_and(|size| size > MAX_HOST_RECEIPT_BYTES as u64) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_HOST_RECEIPT_BYTES.saturating_sub(bytes.len()) { return None; }
        bytes.extend_from_slice(&chunk);
    }
    let receipt: HostReceipt = serde_json::from_slice(&bytes).ok()?;
    if !receipt.accepted || ((receipt.observation.is_some() || receipt.input.is_some()) && !receipt.resource_registered)
        || receipt.input.as_ref().is_some_and(|request| !request.valid_shape())
        || receipt.observation.as_ref().is_some_and(|request|
        !native_browser_protocol::opaque_id(&request.request_id) || !request.resource.valid_shape()) { return None; }
    Some(receipt)
}

async fn read_activity(mut response: reqwest::Response, started: std::time::Instant) -> Option<ComputerUseActivityReceipt> {
    if !response.status().is_success() || response.content_length().is_some_and(|size| size > MAX_ACTIVITY_BYTES as u64) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_ACTIVITY_BYTES.saturating_sub(bytes.len()) { return None; }
        bytes.extend_from_slice(&chunk);
    }
    let mut receipt: ComputerUseActivityReceipt = serde_json::from_slice(&bytes).ok()?;
    if !receipt.valid_shape() { return None; }
    // 扣除整个往返耗时，迟到活动回包不能重新获得完整展示租约。
    receipt.lease_ms = receipt.lease_ms.saturating_sub(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
    receipt.active = receipt.active && receipt.lease_ms > 0;
    Some(receipt)
}
