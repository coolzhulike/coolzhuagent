//! 原生恢复确认：前端只提供挑战编号，详情、身份与一次性证明均走受控本机通道。
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

static CONFIRMING: AtomicBool = AtomicBool::new(false);
struct ConfirmationGuard;
impl Drop for ConfirmationGuard {
    fn drop(&mut self) { CONFIRMING.store(false, Ordering::Release); }
}

#[cfg(windows)]
static BACKEND: std::sync::OnceLock<Result<windows_process_guard::ProcessIdentity, String>> = std::sync::OnceLock::new();

pub fn initialize() {
    #[cfg(windows)]
    BACKEND.get_or_init(|| {
        let args: Vec<String> = std::env::args().collect();
        let pid = super::web_console_parent_pid(&args).ok_or("请从完整桌面启动器启动应用后进行原生恢复确认")?;
        let process = windows_process_guard::capture_live_process_identity(pid).map_err(|_| "无法验证启动器指定的后台进程")?;
        let image = std::path::Path::new(process.image_path().ok_or("后台进程路径不可读")?);
        let current = std::env::current_exe().map_err(|_| "桌面程序路径不可读")?;
        let same_directory = image.parent().zip(current.parent()).is_some_and(|(left, right)| {
            let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok().map(|path| path.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase());
            canonical(left).zip(canonical(right)).is_some_and(|(left,right)| left == right)
        });
        if !same_directory || !image.file_name().is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("coolzhu-web-console.exe")) {
            return Err("原生恢复需要同一安装包的后台与桌面程序；请通过完整桌面启动器启动".into());
        }
        Ok(process)
    });
}

fn challenge_text(challenge: &Value, sid: &str) -> Result<String, String> {
    let scope = challenge.get("scope").and_then(Value::as_str).ok_or("后台未返回恢复范围")?;
    let reason = challenge.get("reason").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or("后台未返回恢复理由")?;
    let blocks = challenge.get("blocks").and_then(Value::as_array).ok_or("后台未返回具体阻断原因")?;
    let mut detail = String::new();
    for block in blocks {
        let id = block.get("block_id").and_then(Value::as_str).ok_or("阻断编号缺失")?;
        let why = block.get("reason").and_then(Value::as_str).ok_or("阻断原因缺失")?;
        detail.push_str(&format!("\n• {id}：{why}"));
    }
    let runs = challenge.get("acknowledged_run_ids").and_then(Value::as_array).ok_or("后台未返回遗留运行集合")?;
    let run_details = runs.iter().map(|value| value.as_str().ok_or("遗留运行编号格式无效")).collect::<Result<Vec<_>, _>>()?.join("、");
    let permits = challenge.get("unsettled_permits").and_then(Value::as_array).ok_or("后台未返回未结输入许可")?;
    let mut permit_details = String::new();
    for permit in permits {
        let id = permit.get("id").and_then(Value::as_str).ok_or("输入许可编号缺失")?;
        let state = permit.get("state").and_then(Value::as_str).ok_or("输入许可状态缺失")?;
        let revision = permit.get("revision").and_then(Value::as_u64).ok_or("输入许可版本缺失")?;
        let accepted = permit.get("unknown_accepted").and_then(Value::as_bool).ok_or("输入许可风险接受状态缺失")?;
        let description = match state { "outcome_unknown" => "执行结果未知", "dispatch_committed" => "已提交执行", "executing" => "执行中", _ => "已记录状态" };
        permit_details.push_str(&format!("\n• {id}：{description} ({state})，版本 {revision}，此前是否接受未知风险：{}", if accepted { "是" } else { "否" }));
    }
    let executors = challenge.get("executor_facts").and_then(Value::as_array).ok_or("后台未返回执行进程事实")?;
    let mut executor_details = String::new();
    for executor in executors {
        let id = executor.get("id").and_then(Value::as_str).ok_or("执行进程编号缺失")?;
        let state = executor.get("state").and_then(Value::as_str).ok_or("执行进程状态缺失")?;
        let pid = executor.get("pid").and_then(Value::as_u64).ok_or("执行进程 PID 缺失")?;
        let revision = executor.get("revision").and_then(Value::as_u64).ok_or("执行进程版本缺失")?;
        let created = executor.get("created").and_then(Value::as_u64).map_or_else(|| "未记录".to_owned(), |value| value.to_string());
        let description = match state { "exited_confirmed" => "已确认退出", "verified_alive" => "仍在运行", _ => "退出尚未确认" };
        executor_details.push_str(&format!("\n• {id}：{description} ({state})，PID {pid}，创建标识 {created}，版本 {revision}"));
    }
    let text = format!("确认申请恢复桌面操作？\n\n当前 Windows 身份：{sid}\n范围：{scope}\n恢复理由：{reason}\n\n当前阻断：{detail}\n\n确认接受的遗留运行：{}\n\n本次涉及的输入许可：{}\n\n执行进程事实：{}\n\n同意表示接受上述尚未确定的历史执行结果；未知结果仍保留为未知，不会改写为成功。确认后后台仍须完成机器安全复核，未通过会继续保持隔离。此窗口确认不是密码或 Windows Hello 身份重认证。", if run_details.is_empty() { "无" } else { &run_details }, if permit_details.is_empty() { "无" } else { &permit_details }, if executor_details.is_empty() { "无" } else { &executor_details });
    // Windows 原生字符串以 NUL 结束，不能让申请内容截断后面的阻断说明。
    if text.contains('\0') { return Err("恢复详情含有无法安全显示的字符，未执行恢复".into()); }
    if text.encode_utf16().count() > 24000 { return Err("待确认内容过长，未执行恢复；请减少此次请求范围".into()); }
    Ok(text)
}

#[cfg(windows)]
fn pipe_request(request: Value) -> Result<Value, String> {
    use windows_process_guard::{local_recovery_pipe_request, recovery_pipe_name};
    let backend = BACKEND.get().ok_or("原生恢复未初始化")?.as_ref().map_err(Clone::clone)?;
    let raw = serde_json::to_vec(&request).map_err(|_| "原生恢复请求无法编码")?;
    let response = local_recovery_pipe_request(&recovery_pipe_name(backend), backend, &raw, std::time::Duration::from_secs(5))
        .map_err(|_| "本机恢复通道不可用或桌面实例未登记，请使用完整桌面启动器重新打开")?;
    let response: Value = serde_json::from_slice(&response).map_err(|_| "本机恢复通道返回了无效数据")?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(response.get("error").and_then(Value::as_str).unwrap_or("本机恢复通道拒绝本次请求").chars().take(1000).collect());
    }
    Ok(response)
}

#[cfg(windows)]
fn backend_still_current() -> bool {
    BACKEND.get().and_then(|value| value.as_ref().ok()).is_some_and(|expected| {
        windows_process_guard::capture_live_process_identity(expected.pid()).is_ok_and(|current| {
            current.creation_time_filetime() == expected.creation_time_filetime() && current.image_path() == expected.image_path()
        })
    })
}

#[tauri::command]
pub async fn confirm_recovery(webview: tauri::Webview, app: tauri::AppHandle, challenge_id: String) -> Result<Value, String> {
    if webview.label() != super::CONSOLE_LABEL || !webview.url().ok().is_some_and(|url| super::browser_panel::trusted_console_url(&url)) {
        return Err("此页面无权打开原生恢复确认".into());
    }
    if challenge_id.is_empty() || challenge_id.len() > 256 || !challenge_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err("恢复挑战编号无效".into());
    }
    if CONFIRMING.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return Err("已有原生确认窗口，请先完成或取消它".into());
    }
    let _guard = ConfirmationGuard;
    #[cfg(not(windows))]
    { let _ = app; Err("当前系统不支持此原生恢复通道".into()) }
    #[cfg(windows)]
    {
        let challenge_key = challenge_id.clone();
        let response = tauri::async_runtime::spawn_blocking(move || pipe_request(json!({"op":"inspect_challenge","challenge_id":challenge_key}))).await.map_err(|_| "读取恢复详情中断")??;
        let challenge = response.get("challenge").cloned().ok_or("后台未返回恢复详情")?;
        if challenge.get("challenge_id").and_then(Value::as_str) != Some(challenge_id.as_str()) { return Err("后台返回的挑战编号不符".into()); }
        let sid = windows_process_guard::process_peer_identity(std::process::id()).map_err(|_| "无法读取当前 Windows 身份")?.user_sid;
        let text = challenge_text(&challenge, &sid)?;
        let owner = app.get_window(super::CONSOLE_LABEL).ok_or("聊天窗口已关闭")?.hwnd().map_err(|_| "原生窗口不可用")?.0 as usize;
        let confirmed = tauri::async_runtime::spawn_blocking(move || {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_YESNO, MB_ICONWARNING, MB_DEFBUTTON2};
            let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            let title: Vec<u16> = "Coolzhu Agent · 恢复桌面操作".encode_utf16().chain(Some(0)).collect();
            unsafe { MessageBoxW(owner as _, text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) == IDYES }
        }).await.map_err(|_| "原生确认窗口中断")?;
        if !confirmed { return Ok(json!({"outcome":"cancelled","message":"已取消，保持隔离"})); }
        if !webview.url().ok().is_some_and(|url| super::browser_panel::trusted_console_url(&url)) { return Err("聊天页面已变化，请重新发起恢复确认".into()); }
        let approval = tauri::async_runtime::spawn_blocking(move || pipe_request(json!({"op":"approve_recovery","challenge_id":challenge_id}))).await.map_err(|_| "原生确认提交中断")??;
        if approval.get("operator").and_then(Value::as_str) != Some(sid.as_str()) { return Err("原生操作员身份不匹配".into()); }
        let proof = approval.get("proof").and_then(Value::as_str).filter(|value| !value.is_empty()).ok_or("后台未返回一次性恢复凭证")?;
        let request = json!({
            "challenge_id": challenge["challenge_id"],
            "reason": challenge["reason"],
            "acknowledged_block_ids": challenge["acknowledged_block_ids"],
            "acknowledged_run_ids": challenge["acknowledged_run_ids"],
            "evidence_refs": challenge["evidence_refs"],
            "operator": sid,
            "proof": proof,
        });
        if !backend_still_current() { return Err("后台实例已变化，请重新启动桌面应用".into()); }
        let mut url = webview.url().map_err(|_| "聊天地址不可用")?;
        url.set_path("/api/system/release-isolation"); url.set_query(None); url.set_fragment(None);
        let client = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(15)).build().map_err(|_| "无法创建恢复连接")?;
        let mut result = client.post(url.as_str()).json(&request).send().await.map_err(|_| "恢复提交未收到结果，请查询状态后再操作")?;
        let status = result.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = result.chunk().await.map_err(|_| "恢复结果读取中断，请查询状态")? {
            if bytes.len() + chunk.len() > 65536 { return Err("恢复结果超出大小限制，请查询状态".into()); }
            bytes.extend_from_slice(&chunk);
        }
        if !backend_still_current() { return Err("后台实例已变化，无法确认恢复结果".into()); }
        let result: Value = serde_json::from_slice(&bytes).map_err(|_| "恢复结果格式无效，请查询状态")?;
        if !status.is_success() { return Err(result.get("error").and_then(Value::as_str).unwrap_or("恢复未获放行，资源保持隔离").chars().take(1000).collect()); }
        // 一次性 proof 只在原生进程到后端间传输，不回传给网页。
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_dialog_uses_canonical_details_and_does_not_hide_block_reasons() {
        let challenge = json!({"scope":"physical:1","reason":"确认清理完成","blocks":[{"block_id":"block-1","reason":"执行者未确认退出"}],"acknowledged_run_ids":["run-2"],"unsettled_permits":[{"id":"permit-3","state":"outcome_unknown","revision":4,"unknown_accepted":false}],"executor_facts":[{"id":"executor-5","state":"verified_alive","pid":123,"created":456,"revision":6}]});
        let text = challenge_text(&challenge, "S-1-5-21-test").unwrap();
        assert!(text.contains("执行者未确认退出") && text.contains("run-2") && text.contains("S-1-5-21-test"));
        assert!(text.contains("仍须完成机器安全复核") && text.contains("不是密码或 Windows Hello"));
        assert!(text.contains("permit-3：执行结果未知") && text.contains("executor-5：仍在运行") && text.contains("未知结果仍保留为未知"));
        assert!(challenge_text(&json!({"scope":"x","reason":"x"}), "SID").is_err());
        let mut truncated = challenge;
        truncated["reason"] = json!("表面理由\0隐藏后文");
        assert!(challenge_text(&truncated, "SID").is_err());
    }
}
