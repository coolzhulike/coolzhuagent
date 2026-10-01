//! 当前工程的插件管理与 SKILL 选用；目录状态和模型可执行权限分开呈现。
use super::*;
use plugins::{PluginKind, PluginSummary};

pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/extension-market/plugins", get(list_plugins))
        .route("/api/extension-market/plugins/install", post(install_plugin))
        .route("/api/plugins/install", post(install_plugin))
        .route("/api/extension-market/plugins/action", post(plugin_action))
        .route("/api/extension-market/skills", get(list_skills))
        .route("/api/extension-market/skills/{id}", get(skill_detail))
        .route("/api/extension-market/skills/select", post(select_skill))
        .merge(dsh_market::routes())
}

#[derive(Serialize)]
struct PluginEntry {
    id: String,
    name: String,
    version: String,
    description: String,
    source: String,
    enabled: bool,
    installed: bool,
    kind: String,
    installable: bool,
    manageable: bool,
    loaded_in_chat: bool,
}

fn plugin_entry(summary: PluginSummary, installed: bool, loaded: bool) -> PluginEntry {
    let metadata = summary.metadata;
    let name = metadata.dsh.as_ref().map_or_else(|| metadata.name.clone(), |p| p.receipt.name.clone());
    PluginEntry {
        id: metadata.id,
        name,
        version: metadata.version,
        description: metadata.description,
        source: metadata.source,
        enabled: summary.enabled,
        installed,
        kind: metadata.kind.to_string(),
        installable: false,
        manageable: installed && metadata.kind != PluginKind::Builtin,
        loaded_in_chat: loaded,
    }
}

pub(super) fn manager(workspace: &Path) -> ApiResult<plugins::PluginManager> {
    let loader = runtime::ConfigLoader::default_for(workspace);
    let config = loader.load().map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,
        &format!("插件配置读取失败：{error}")))?;
    Ok(runtime::plugin_manager_for_workspace(workspace, &loader, &config))
}

fn plugin_error(error: plugins::PluginError) -> ApiError {
    api_error(StatusCode::BAD_REQUEST, &format!("插件操作失败：{error}"))
}

fn assert_workspace(expected: &str, workspace: &Path) -> ApiResult<()> {
    if expected != workspace_identity(workspace) {
        return Err(api_error(StatusCode::CONFLICT, "工程目录已切换，请刷新后再操作"));
    }
    Ok(())
}

#[derive(Serialize)]
struct PluginListResponse {
    workspace_id: String,
    plugins: Vec<PluginEntry>,
    candidates: Vec<PluginCandidate>,
    runtime_error: Option<String>,
}

fn plugin_catalog(workspace: &Path, manager: &plugins::PluginManager) -> ApiResult<PluginListResponse> {
    let installed = manager.list_installed_plugins().map_err(plugin_error)?
        .into_iter().map(|item| item.metadata.id).collect::<HashSet<_>>();
    let runtime_error = plugin_runtime::definitions(workspace, &[]).err();
    let loaded = if runtime_error.is_none() {
        manager.aggregated_tools().map_err(plugin_error)?.into_iter()
            .map(|tool| tool.plugin_id().to_string()).collect::<HashSet<_>>()
    } else { HashSet::new() };
    let plugins = manager.list_plugins().map_err(plugin_error)?.into_iter()
        .map(|item| { let is_installed = installed.contains(&item.metadata.id);
            let is_loaded = loaded.contains(&item.metadata.id);
            plugin_entry(item, is_installed, is_loaded) }).collect();
    Ok(PluginListResponse { workspace_id: workspace_identity(workspace),
        plugins, candidates: local_plugin_candidates(workspace), runtime_error })
}

#[derive(Serialize)]
struct PluginCandidate {
    id: String,
    name: String,
    version: String,
    description: String,
    source: String,
}

fn local_plugin_candidates(workspace: &Path) -> Vec<PluginCandidate> {
    let source = workspace.join(".coolzhu").join("plugins");
    let Ok(root) = source.canonicalize() else { return Vec::new(); };
    child_directories(&root).into_iter().filter_map(|directory| {
        let canonical = directory.canonicalize().ok()?;
        if !canonical.starts_with(&root) { return None; }
        let manifest = plugins::load_plugin_from_directory(&canonical).ok()?;
        Some(PluginCandidate { id: format!("plugin.{}", manifest.name), name: manifest.name,
            version: manifest.version, description: manifest.description,
            source: "当前工程本地目录".into() })
    }).collect()
}

async fn list_plugins() -> ApiResult<Json<PluginListResponse>> {
    let workspace = active_workspace_path();
    tokio::task::spawn_blocking(move || {
        let manager = manager(&workspace)?;
        Ok(Json(plugin_catalog(&workspace, &manager)?))
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "插件目录任务中断"))?
}

#[derive(Deserialize)]
struct PluginMutation {
    expected_workspace: String,
    id: String,
    #[serde(default)]
    action: String,
}

fn find_candidate_path(workspace: &Path, id: &str) -> ApiResult<PathBuf> {
    let name = id.strip_prefix("plugin.").ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "插件 id 无效"))?;
    let root = workspace.join(".coolzhu").join("plugins").canonicalize()
        .map_err(|_| api_error(StatusCode::NOT_FOUND, "当前工程无本地插件候选目录"))?;
    let matches = child_directories(&root).into_iter().filter_map(|path| {
        let canonical = path.canonicalize().ok()?;
        if !canonical.starts_with(&root) { return None; }
        let manifest = plugins::load_plugin_from_directory(&canonical).ok()?;
        (manifest.name == name).then_some(canonical)
    }).collect::<Vec<_>>();
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(api_error(StatusCode::NOT_FOUND, "当前工程未找到该插件候选")),
        _ => Err(api_error(StatusCode::CONFLICT, "当前工程有同名插件候选，请先整理目录")),
    }
}

async fn install_plugin(Json(payload): Json<PluginMutation>) -> ApiResult<Json<PluginListResponse>> {
    let workspace = active_workspace_path();
    assert_workspace(&payload.expected_workspace, &workspace)?;
    tokio::task::spawn_blocking(move || {
        let source = find_candidate_path(&workspace, &payload.id)?;
        let mut manager = manager(&workspace)?;
        manager.install(&source.to_string_lossy()).map_err(plugin_error)?;
        // 从事务后的登记状态重新读取，不凭请求成功猜测启用或已加载。
        Ok(Json(plugin_catalog(&workspace, &manager)?))
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "插件安装任务中断"))?
}

async fn plugin_action(Json(payload): Json<PluginMutation>) -> ApiResult<Json<PluginListResponse>> {
    let workspace = active_workspace_path();
    assert_workspace(&payload.expected_workspace, &workspace)?;
    tokio::task::spawn_blocking(move || {
        let mut manager = manager(&workspace)?;
        match payload.action.as_str() {
            "enable" => manager.enable(&payload.id).map_err(plugin_error)?,
            "disable" => manager.disable(&payload.id).map_err(plugin_error)?,
            "update" => { manager.update(&payload.id).map_err(plugin_error)?; },
            "uninstall" => manager.uninstall(&payload.id).map_err(plugin_error)?,
            _ => return Err(api_error(StatusCode::BAD_REQUEST, "插件操作无效")),
        }
        Ok(Json(plugin_catalog(&workspace, &manager)?))
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "插件管理任务中断"))?
}

#[derive(Serialize)]
struct SkillEntry {
    id: String,
    name: String,
    description: String,
    source: String,
    selected: bool,
}

#[derive(Serialize)]
struct SkillListResponse {
    workspace_id: String,
    skills: Vec<SkillEntry>,
    selected_id: Option<String>,
}

fn skill_roots(workspace: &Path) -> [(&'static str, PathBuf); 2] {
    [("local", workspace.join(".coolzhu").join("skills")),
     ("project", workspace.join("skills"))]
}

fn skill_file(workspace: &Path, id: &str) -> ApiResult<PathBuf> {
    let (source, directory) = id.split_once(':').ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "SKILL id 无效"))?;
    if directory.is_empty() || directory.contains('/') || directory.contains('\\') || directory == "." || directory == ".." {
        return Err(api_error(StatusCode::BAD_REQUEST, "SKILL id 无效"));
    }
    let (_, root) = skill_roots(workspace).into_iter().find(|(kind, _)| *kind == source)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "SKILL 来源无效"))?;
    let root = root.canonicalize().map_err(|_| api_error(StatusCode::NOT_FOUND, "SKILL 目录不存在"))?;
    let file = root.join(directory).join("SKILL.md").canonicalize()
        .map_err(|_| api_error(StatusCode::NOT_FOUND, "SKILL 文档不存在"))?;
    if !file.starts_with(&root) || file.parent().and_then(Path::parent) != Some(root.as_path()) {
        return Err(api_error(StatusCode::FORBIDDEN, "SKILL 文档超出当前工程目录"));
    }
    Ok(file)
}

fn selection_path(workspace: &Path) -> PathBuf { workspace.join(".coolzhu").join("active-skill.json") }

fn selected_skill_id(workspace: &Path) -> Option<String> {
    let text = std::fs::read_to_string(selection_path(workspace)).ok()?;
    let selected = serde_json::from_str::<JsonValue>(&text).ok()?
        .get("selected_id")?.as_str()?.to_string();
    skill_file(workspace, &selected).ok().map(|_| selected)
}

fn skill_entries(workspace: &Path) -> SkillListResponse {
    let selected_id = selected_skill_id(workspace);
    let mut skills = Vec::new();
    for (source, root) in skill_roots(workspace) {
        let Ok(root) = root.canonicalize() else { continue; };
        for directory in child_directories(&root) {
            let Some(slug) = directory.file_name().and_then(|name| name.to_str()) else { continue; };
            let id = format!("{source}:{slug}");
            let Ok(file) = skill_file(workspace, &id) else { continue; };
            let metadata = parse_skill_metadata(&file);
            skills.push(SkillEntry { selected: selected_id.as_deref() == Some(&id), id,
                name: metadata.name.unwrap_or_else(|| slug.to_string()),
                description: metadata.description.unwrap_or_else(|| "本地 SKILL 工作流".into()),
                source: if source == "local" { "当前工程 .coolzhu/skills" } else { "当前工程 skills" }.into() });
        }
    }
    SkillListResponse { workspace_id: workspace_identity(workspace), skills, selected_id }
}

async fn list_skills() -> ApiResult<Json<SkillListResponse>> {
    Ok(Json(skill_entries(&active_workspace_path())))
}

#[derive(Serialize)]
struct SkillDetailResponse { workspace_id: String, id: String, content: String, selected: bool }

async fn skill_detail(AxumPath(id): AxumPath<String>) -> ApiResult<Json<SkillDetailResponse>> {
    let workspace = active_workspace_path();
    let file = skill_file(&workspace, &id)?;
    let content = read_skill_content(&file)?;
    Ok(Json(SkillDetailResponse { workspace_id: workspace_identity(&workspace),
        selected: selected_skill_id(&workspace).as_deref() == Some(&id), id, content }))
}

fn read_skill_content(file: &Path) -> ApiResult<String> {
    let metadata = std::fs::metadata(file).map_err(|_| api_error(StatusCode::NOT_FOUND, "SKILL 文档不可读"))?;
    if metadata.len() > 24_000 { return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "SKILL 文档超过 24 KB，无法加载")); }
    std::fs::read_to_string(file).map_err(|_| api_error(StatusCode::BAD_REQUEST, "SKILL 文档不是 UTF-8"))
}

#[derive(Deserialize)]
struct SkillSelection { expected_workspace: String, selected_id: Option<String> }

async fn select_skill(Json(payload): Json<SkillSelection>) -> ApiResult<Json<SkillListResponse>> {
    let workspace = active_workspace_path();
    assert_workspace(&payload.expected_workspace, &workspace)?;
    if let Some(id) = payload.selected_id.as_deref() {
        read_skill_content(&skill_file(&workspace, id)?)?;
    }
    let path = selection_path(&workspace);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建工程 SKILL 配置目录"))?;
    let data = serde_json::to_vec(&json!({"selected_id": payload.selected_id}))
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法保存 SKILL 选择"))?;
    config_publication::publish(&path, &String::from_utf8(data).map_err(|_| api_error(
        StatusCode::INTERNAL_SERVER_ERROR, "无法保存 SKILL 选择"))?)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法保存 SKILL 选择"))?;
    Ok(Json(skill_entries(&workspace)))
}

pub(super) fn active_skill_guidance(workspace: &Path) -> Option<String> {
    let id = selected_skill_id(workspace)?;
    let file = skill_file(workspace, &id).ok()?;
    let content = read_skill_content(&file).ok()?;
    Some(format!("\n\n当前工程已选用 SKILL ({id})：\n{content}"))
}
