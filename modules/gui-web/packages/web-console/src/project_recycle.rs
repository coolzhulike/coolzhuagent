//! 工程删除独立于移除最近记录；只允许当前工程内的已登记子工程进入回收站。
use super::*;

struct Pending {
    path: PathBuf,
    workspace: PathBuf,
    expires: u64,
}
fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static ITEMS: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    ITEMS.get_or_init(Default::default)
}
#[derive(Deserialize)]
pub(super) struct Prepare {
    path: String,
}
#[derive(Deserialize)]
pub(super) struct Confirm {
    token: String,
    name: String,
    confirmed_twice: bool,
}

fn validate_path(path: &Path, workspace: &Path) -> Result<PathBuf, String> {
    if workspace_projects::reserved_path()
        .is_some_and(|default| workspace_projects::protects_reserved(path, &default))
    {
        return Err("默认工程用于启动恢复，不能删除它或包含它的父工程。".into());
    }
    let root = workspace.canonicalize().map_err(|_| "当前工程不可访问")?;
    let path = path
        .canonicalize()
        .map_err(|_| "待删除工程不存在或不可访问")?;
    #[cfg(windows)]
    if path.components().any(|part| matches!(part, std::path::Component::Prefix(prefix)
        if !matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))) {
        return Err("网络共享与设备路径不支持可靠的回收站恢复，请使用系统文件管理器。".into());
    }
    if path == root || !path.starts_with(&root) || !path.is_dir() {
        return Err("只能删除当前工程目录内的子工程；不能删除当前工程、父目录或工程外路径。请先切换到其父工程。".into());
    }
    if path.components().any(|part|matches!(part,std::path::Component::Normal(name) if name==".coolzhu" || name==".git")) {
        return Err("不能将工程数据目录或 Git 元数据作为工程删除。".into());
    }
    Ok(path)
}

fn inspect(path: &Path) -> Result<(), String> {
    let config = read_workspace_config_before_writer(path).map_err(|e| e.to_string())?;
    if config
        .scheduled_tasks
        .tasks
        .iter()
        .any(|task| !matches!(task.status.as_str(), "completed" | "cancelled" | "disabled"))
    {
        return Err("该工程仍有定时任务，请先打开工程并停止或删除任务。".into());
    }
    let db = workspace_scope_paths_for(path, &config).session_db;
    if !db.exists() {
        return Ok(());
    }
    let db = db.canonicalize().map_err(|e| e.to_string())?;
    if !db.starts_with(path) {
        return Err("该工程使用外部数据目录，请先核对数据归属；未删除工程。".into());
    }
    // 另一进程持有写者锁时立即拒绝，不发送终止进程指令。
    let _writer = session_writer::acquire(&db).map_err(|e| e.to_string())?;
    let c = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    for (table, predicate) in [
        ("goals", "status NOT IN ('completed','cancelled')"),
        (
            "runtime_runs",
            "state IN ('accepted','running','stop_requested')",
        ),
        (
            "computer_use_runs",
            "state NOT IN ('succeeded','failed','blocked','cancelled','timed_out')",
        ),
        (
            "devin_acp_attempts",
            "state NOT IN ('terminal','not_sent') OR process_drained=0",
        ),
        (
            "execution_index",
            "state NOT IN ('completed','failed','interrupted','cancelled')",
        ),
    ] {
        let exists: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists
            && c.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {predicate})"),
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
        {
            return Err(
                "该工程仍有运行任务、未完成 Goal 或未确认的执行，请先打开工程处理后再删除。".into(),
            );
        }
    }
    Ok(())
}

pub(super) async fn prepare(Json(request): Json<Prepare>) -> ApiResult<Json<JsonValue>> {
    let _pin =
        workspace_activity::pin_workspace().map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    #[cfg(not(windows))]
    return Err(api_error(
        StatusCode::BAD_REQUEST,
        "此平台尚未提供回收站删除，请使用系统文件管理器。",
    ));
    #[cfg(windows)]
    {
        let workspace = active_workspace_path();
        let path = validate_path(Path::new(&request.path), &workspace)
            .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;
        let state = app_launcher::launch_paths::load_user_state(
            &workspace_projects::state_root()
                .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e))?,
        )
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
        if !state
            .legacy_selections
            .iter()
            .any(|p| app_launcher::launch_paths::same_canonical(&p.workspace_root, &path))
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "仅可删除最近列表中已打开过的子工程。",
            ));
        }
        let checked = path.clone();
        tokio::task::spawn_blocking(move || inspect(&checked))
            .await
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
            .map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
        let token = random_hex_identifier(24, "工程删除确认")
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e))?;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut items = pending()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "删除状态不可用"))?;
        items.retain(|_, v| v.expires > unix_timestamp_millis());
        if items.len() >= 20 {
            return Err(api_error(
                StatusCode::CONFLICT,
                "待确认操作过多，请稍后重试",
            ));
        }
        items.insert(
            token.clone(),
            Pending {
                path: path.clone(),
                workspace,
                expires: unix_timestamp_millis() + 300_000,
            },
        );
        Ok(Json(
            json!({"token":token,"name":name,"path":path,"impact":"工程中的所有文件、.coolzhu 聊天历史及配置将一起移入系统回收站，可通过系统回收站恢复。"}),
        ))
    }
}

pub(super) async fn confirm(Json(request): Json<Confirm>) -> ApiResult<Json<JsonValue>> {
    let _change = workspace_activity::begin_workspace_change()
        .map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    let item = pending()
        .lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "删除状态不可用"))?
        .remove(&request.token)
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "确认已过期或已使用，请重新选择工程。",
            )
        })?;
    if item.expires < unix_timestamp_millis()
        || !request.confirmed_twice
        || item.path.file_name().and_then(|n| n.to_str()) != Some(request.name.as_str())
        || !app_launcher::launch_paths::same_canonical(&item.workspace, &active_workspace_path())
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "工程名称、二次确认或当前工程已变化，未删除任何文件。",
        ));
    }
    let path = validate_path(&item.path, &item.workspace)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;
    let done = path.clone();
    tokio::task::spawn_blocking(move || {
        inspect(&done)?;
        recycle(&done)
    })
    .await
    .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    .map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    diag_log(&format!("[WORKSPACE] recycled project={}", path.display()));
    // 路径已不存在时仍按原记录路径比较；列表保存失败也如实告知文件已回收。
    let removed = workspace_projects::forget_path(&path);
    Ok(Json(
        json!({"recycled":true,"path":path,"recent_removed":removed.is_ok(),"warning":removed.err().map(|_|"文件已回收，但最近列表更新失败，请手动移除该条目。")}),
    ))
}

#[cfg(windows)]
fn recycle_target(path: &Path) -> Result<PathBuf, String> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    // Rust 规范路径带有 \\?\ 前缀，Windows 回收站不接受这种设备路径格式。
    // 仅转换已经通过边界检查的本地盘符路径，转换后再次核对指向同一目录。
    let target = match path.components().next() {
        Some(std::path::Component::Prefix(prefix)) => match prefix.kind() {
            std::path::Prefix::Disk(_) => path.to_path_buf(),
            std::path::Prefix::VerbatimDisk(_) => PathBuf::from(std::ffi::OsString::from_wide(
                &path.as_os_str().encode_wide().skip(4).collect::<Vec<_>>(),
            )),
            _ => return Err("回收站仅支持本地盘符路径".into()),
        },
        _ => return Err("回收站目标必须为本地绝对路径".into()),
    };
    if target.canonicalize().map_err(|e| e.to_string())?
        != path.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("回收站目标路径转换后发生变化，未删除工程".into());
    }
    Ok(target)
}

#[cfg(windows)]
fn recycle(path: &Path) -> Result<(), String> {
    let target = recycle_target(path)?;
    // 固定脚本通过环境传递目标，路径不会解释为命令；无永久删除降级。
    let script = r#"$ErrorActionPreference='Stop'; Add-Type -AssemblyName Microsoft.VisualBasic; $target=[IO.Path]::GetFullPath($env:COOLZHU_RECYCLE_TARGET); if (-not [IO.Directory]::Exists($target)) { throw '工程不存在' }; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($target,[Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,[Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin,[Microsoft.VisualBasic.FileIO.UICancelOption]::ThrowException)"#;
    let result = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .env("COOLZHU_RECYCLE_TARGET", target)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "系统回收站操作失败，未改用永久删除：{}",
            diagnostics::redact_text(&decode_console_output(&result.stderr))
        ));
    }
    if path.exists() {
        return Err("系统未确认工程已回收，请核对回收站与原目录。".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn recycle(_path: &Path) -> Result<(), String> {
    Err("此平台未提供回收站操作".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 回收站入口拒绝默认工程及其父工程() {
        let default = workspace_projects::reserved_path().unwrap();
        for target in [&default, default.parent().unwrap()] {
            assert!(validate_path(target, default.parent().unwrap())
                .unwrap_err().contains("默认工程"));
        }
    }
    #[cfg(windows)]
    #[test]
    fn 回收站路径兼容规范前缀并拒绝网络设备路径() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("中文 空格工程");
        std::fs::create_dir(&child).unwrap();
        let canonical = child.canonicalize().unwrap();
        let target = recycle_target(&canonical).unwrap();
        assert!(!target.as_os_str().to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(target.canonicalize().unwrap(), canonical);
        assert!(recycle_target(Path::new(r"\\server\share\project")).is_err());
        assert!(recycle_target(Path::new(r"\\.\C:\project")).is_err());
    }
    #[test]
    fn 拒绝当前工程父目录和外部目录且只接受子工程() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        let child = workspace.join("child");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir(&outside).unwrap();
        assert!(validate_path(&workspace, &workspace).is_err());
        assert!(validate_path(root.path(), &workspace).is_err());
        assert!(validate_path(&outside, &workspace).is_err());
        assert_eq!(
            validate_path(&child, &workspace).unwrap(),
            child.canonicalize().unwrap()
        );
    }
    #[test]
    fn 未完成目标阻断工程删除且不改数据库() {
        let root = tempfile::tempdir().unwrap();
        let config = WorkspaceConfig::default();
        let db = workspace_scope_paths_for(root.path(), &config).session_db;
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let c = Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE goals(status TEXT); INSERT INTO goals VALUES('paused');")
            .unwrap();
        drop(c);
        assert!(inspect(&root.path().canonicalize().unwrap())
            .unwrap_err()
            .contains("未完成 Goal"));
        assert!(db.exists());
    }
}
