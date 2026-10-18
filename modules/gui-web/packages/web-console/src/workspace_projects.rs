//! 用户级最近工程与启动器共用一份原子选择记录。
use super::*;
use app_launcher::launch_paths as launch;

pub(super) fn reserved_path() -> Option<PathBuf> {
    user_home_dir().map(|home| launch::reserved_workspace_path(&home))
}

fn is_reserved(path: &Path) -> bool {
    reserved_path().is_some_and(|default| launch::same_canonical(path, &default))
}

/// 目录变更也保护包含默认工程的父目录，避免通过父工程绕过。
pub(super) fn protects_reserved(path: &Path, default: &Path) -> bool {
    let path = launch::canonical_key(path);
    let default = launch::canonical_key(default);
    default == path || default.starts_with(&format!("{path}/"))
}

pub(super) fn ensure_reserved_not_mutated(path: &Path) -> ApiResult<()> {
    if reserved_path().is_some_and(|default| protects_reserved(path, &default)) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "默认工程用于启动恢复，不能删除或重命名该目录及其父目录。",
        ));
    }
    Ok(())
}

pub(super) fn state_root() -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        return Ok(ROOT
            .get_or_init(|| tempfile::tempdir().expect("测试工程状态目录"))
            .path()
            .into());
    }
    #[cfg(not(test))]
    launch::user_state_root_for(
        env::var_os("LOCALAPPDATA").as_deref().map(Path::new),
        user_home_dir().as_deref(),
        env::var_os(app_launcher::USER_STATE_ROOT_ENV)
            .or_else(|| env::var_os("COOLZHU_USER_STATE_DIR"))
            .as_deref()
            .map(Path::new),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn remembered() -> Result<Option<PathBuf>, String> {
    let root = state_root()?;
    let state = launch::load_user_state(&root).map_err(|error| error.to_string())?;
    let Some(selection) = state.selection else {
        return Ok(None);
    };
    let path = match selection.workspace_root.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let default = reserved_path().ok_or("无法定位用户默认工程目录")?;
            std::fs::create_dir_all(&default).map_err(|error| format!("无法创建默认工程：{error}"))?;
            eprintln!("[WORKSPACE] 上次工程不存在，暂用默认工程；原选择保留：{}", selection.workspace_root.display());
            default.canonicalize().map_err(|error| error.to_string())?
        }
        Err(error) => return Err(format!("最近工程不可访问：{error}；请修复权限后重试。")),
    };
    if !path.is_dir() {
        return Err("最近工程不是目录，请重新选择。".into());
    }
    Ok(Some(path))
}

pub(super) fn remember(path: &Path) -> ApiResult<()> {
    let root =
        state_root().map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    launch::select_workspace(
        &root,
        path,
        &launch::RealWorkspaceAccess,
        unix_timestamp_millis(),
    )
    .map_err(|error| {
        api_error(
            StatusCode::CONFLICT,
            &format!("最近工程记录未保存：{error}"),
        )
    })?;
    Ok(())
}

pub(super) async fn list() -> ApiResult<Json<JsonValue>> {
    let root =
        state_root().map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    let state = launch::load_user_state(&root)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?;
    let current = active_workspace_path();
    let default = reserved_path();
    if let Some(path) = &default {
        std::fs::create_dir_all(path).map_err(io_api_error)?;
    }
    let recovery = state.selection.as_ref().is_some_and(|selected|
        is_reserved(&current) && !launch::same_canonical(&selected.workspace_root, &current)
            && !selected.workspace_root.exists());
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for (path, updated) in std::iter::once((current.clone(), unix_timestamp_millis()))
        .chain(default.clone().into_iter().map(|path| (path, 0)))
        .chain(
            state
                .selection
                .into_iter()
                .map(|item| (item.workspace_root, item.recorded_at_ms)),
        )
        .chain(
            state
                .legacy_selections
                .into_iter()
                .rev()
                .map(|item| (item.workspace_root, item.imported_at_ms)),
        )
    {
        if seen.insert(launch::canonical_key(&path)) {
            projects.push(json!({"path":path,"current":launch::same_canonical(&path,&current),"last_opened_at":updated,"available":path.is_dir(),"protected":is_reserved(&path)}));
        }
        if projects.len() >= 20 {
            break;
        }
    }
    Ok(Json(json!({"projects":projects,"notice":recovery.then_some("上次工程不可用，已打开默认工程；原记录保留。")})))
}

#[derive(Deserialize)]
pub(super) struct ProjectRequest {
    path: String,
}

pub(super) async fn create(Json(payload): Json<ProjectRequest>) -> ApiResult<Json<JsonValue>> {
    let path = PathBuf::from(payload.path.trim());
    if !path.is_absolute()
        || payload.path.trim().starts_with("\\\\")
        || payload.path.trim().starts_with("//")
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "请填写本地工程的绝对路径。",
        ));
    }
    let parent = path
        .parent()
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "父目录不存在或不可访问。"))?;
    let name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "工程名称不能为空。"))?;
    let target = parent.join(name);
    if target.exists() {
        return Err(api_error(
            StatusCode::CONFLICT,
            "同名目录已存在，请选择其它名称或直接打开。",
        ));
    }
    if !workspace_is_within_allowed_roots(&parent, &allowed_workspace_roots()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "父目录不在允许的工程目录范围内。",
        ));
    }
    for key in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "PROGRAMDATA"] {
        if env::var_os(key)
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok())
            .is_some_and(|p| parent.starts_with(p))
        {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "不能在系统或程序目录内新建工程。",
            ));
        }
    }
    std::fs::create_dir(&target).map_err(io_api_error)?;
    eprintln!("[WORKSPACE] created project={}", target.display());
    Ok(Json(json!({"path":target})))
}

pub(super) async fn remove(Json(payload): Json<ProjectRequest>) -> ApiResult<Json<JsonValue>> {
    let path = PathBuf::from(payload.path.trim());
    if is_reserved(&path) {
        return Err(api_error(StatusCode::FORBIDDEN, "默认工程用于启动恢复，不能移除。"));
    }
    if launch::same_canonical(&path, &active_workspace_path()) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "请先切换到其它工程，再移除当前工程。",
        ));
    }
    forget_path(&path)?;
    eprintln!(
        "[WORKSPACE] removed recent project={}, files_kept=true",
        path.display()
    );
    Ok(Json(json!({"removed":true,"files_kept":true})))
}

pub(super) fn forget_path(path: &Path) -> ApiResult<()> {
    if is_reserved(path) {
        return Err(api_error(StatusCode::FORBIDDEN, "默认工程用于启动恢复，不能移除。"));
    }
    let root =
        state_root().map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, &message))?;
    let mut state = launch::load_user_state(&root)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    if state.selection.as_ref().is_some_and(|selected| launch::same_canonical(&selected.workspace_root, path)) {
        state.selection = None;
    }
    state
        .legacy_selections
        .retain(|item| !launch::same_canonical(&item.workspace_root, path));
    launch::store_user_state(&root, &state, state.revision)
        .map_err(|e| api_error(StatusCode::CONFLICT, &e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 默认工程移除接口拒绝且内部忘记入口同样拒绝() {
        let default = reserved_path().expect("测试环境应有用户目录");
        let result = remove(Json(ProjectRequest { path: default.to_string_lossy().into_owned() })).await;
        assert_eq!(result.unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(forget_path(&default).unwrap_err().0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn 删除保护覆盖默认目录和父目录但允许其子工程() {
        let root = tempfile::tempdir().unwrap();
        let default = root.path().join("coolzhuagent");
        std::fs::create_dir_all(&default).unwrap();
        assert!(protects_reserved(&default, &default));
        assert!(protects_reserved(root.path(), &default));
        assert!(!protects_reserved(&default.join("child"), &default));
        assert!(!protects_reserved(&root.path().join("coolzhuagent-other"), &default));
    }

    #[test]
    fn 文件树变更保护默认目录及父目录且允许普通子目录() {
        let default = reserved_path().expect("测试环境应有用户目录");
        assert_eq!(ensure_reserved_not_mutated(&default).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(ensure_reserved_not_mutated(default.parent().unwrap()).unwrap_err().0, StatusCode::FORBIDDEN);
        assert!(ensure_reserved_not_mutated(&default.join("ordinary-child")).is_ok());
    }
}
