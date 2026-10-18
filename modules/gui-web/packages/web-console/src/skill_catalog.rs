//! 多根技能目录：只注入元数据，正文由 Skill 工具按需读取；不执行技能中的脚本。
use super::*;
use axum::http::HeaderMap;
const BUNDLE: &str = include_str!("../assets/skills/superpowers.json");
const PAGE_CHARS: usize = 12_000;
static MUTATIONS: Mutex<()> = Mutex::new(());

pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/extension-market/skills", get(list))
        .route("/api/extension-market/skills/{id}", get(detail))
        .route("/api/extension-market/skills/select", post(select))
        .route("/api/extension-market/skills/create", post(create))
        .route("/api/extension-market/skills/install", post(install))
}
fn roots(workspace: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut roots = vec![
        ("local", workspace.join(".coolzhu/skills")),
        ("project", workspace.join("skills")),
        ("agents", workspace.join(".agents/skills")),
        ("codex", workspace.join(".codex/skills")),
        ("claude", workspace.join(".claude/skills")),
        ("opencode", workspace.join(".opencode/skills")),
    ];
    #[cfg(not(test))]
    if let Some(home) = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
    {
        roots.extend([
            ("user-agents", home.join(".agents/skills")),
            ("user-codex", home.join(".codex/skills")),
        ]);
    }
    roots
}
fn linked(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return true;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}
fn valid_part(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', ':'])
}
fn file(workspace: &Path, id: &str) -> Result<PathBuf, String> {
    let (source, name) = id.split_once(':').ok_or("技能标识无效")?;
    if !valid_part(name) {
        return Err("技能标识无效".into());
    }
    let root = roots(workspace)
        .into_iter()
        .find(|(key, _)| *key == source)
        .ok_or("技能来源无效")?
        .1;
    if linked(&root) {
        return Err("技能根目录不存在或为链接；请使用实际目录".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root.join(name);
    if linked(&path) {
        return Err("技能链接不予加载".into());
    }
    let path = if path.is_file() && name.ends_with(".md") {
        path
    } else {
        path.join("SKILL.md")
    };
    if linked(&path) {
        return Err("技能正文不存在或为链接".into());
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("技能正文超出目录".into());
    }
    Ok(path)
}
fn state_path(workspace: &Path) -> PathBuf {
    workspace.join(".coolzhu/skill-enablement.json")
}
fn state(workspace: &Path) -> BTreeMap<String, Vec<String>> {
    std::fs::read(state_path(workspace))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
#[derive(Deserialize, Default)]
struct Query {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    offset: usize,
}
fn entries(workspace: &Path, session: &str) -> JsonValue {
    let enabled = state(workspace).remove(session).unwrap_or_default();
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for (source, root) in roots(workspace) {
        let Ok(dir) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut paths = dir
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let id = format!("{source}:{name}");
            let Ok(path) = file(workspace, &id) else {
                continue;
            };
            let meta = parse_skill_metadata(&path);
            let name = meta
                .name
                .unwrap_or_else(|| name.trim_end_matches(".md").into());
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            let modified = std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() * 1000);
            rows.push(json!({"id":id,"name":name,"description":meta.description.unwrap_or_else(||"本地技能工作流".into()),
                "source":root.display().to_string(),"path":path.display().to_string(),"selected":enabled.contains(&id),"updated_at":modified}));
        }
    }
    let bundle: JsonValue = serde_json::from_str(BUNDLE).expect("随包技能清单");
    json!({"workspace_id":workspace_identity(workspace),"session_id":session,"skills":rows,
        "discovery":"同名优先级：.coolzhu → skills → .agents → .codex → .claude → .opencode → 用户目录；仅一层，忽略链接。",
        "bundle":{"source":bundle["source"],"version":bundle["version"],"commit":bundle["commit"],"license":"MIT"}})
}
fn failure(message: String) -> ApiError {
    api_error(StatusCode::BAD_REQUEST, &message)
}
fn writable_root(workspace: &Path, relative: &str) -> Result<PathBuf, String> {
    let canonical = workspace.canonicalize().map_err(|e| e.to_string())?;
    let mut path = canonical.clone();
    for part in relative.split('/') {
        path.push(part);
        if path.exists() && linked(&path) {
            return Err("安装目标包含链接，未写入".into());
        }
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        if !path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&canonical)
        {
            return Err("安装目标超出工程".into());
        }
    }
    Ok(path)
}
fn page(path: &Path, offset: usize) -> Result<JsonValue, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
        return Err("技能资源超过 2 MB，未加载".into());
    }
    let content = std::fs::read_to_string(path).map_err(|_| "技能资源必须为 UTF-8 文本")?;
    let total = content.chars().count();
    let offset = offset.min(total);
    Ok(
        json!({"content":content.chars().skip(offset).take(PAGE_CHARS).collect::<String>(),
        "offset":offset,"next_offset":(offset+PAGE_CHARS<total).then_some(offset+PAGE_CHARS),"total_chars":total,"path":path.display().to_string()}),
    )
}
async fn list(
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<Query>,
) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    Ok(Json(entries(&active_workspace_path(), &query.session_id)))
}
async fn detail(
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    axum::extract::Query(query): axum::extract::Query<Query>,
) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    let workspace = active_workspace_path();
    let path = file(&workspace, &id).map_err(failure)?;
    let mut result = page(&path, query.offset).map_err(failure)?;
    result["workspace_id"] = json!(workspace_identity(&workspace));
    result["id"] = json!(id);
    result["permission_note"] =
        json!("正文仅供查看；Skill 只能读此技能目录资源。脚本执行仍经过房间工具权限与审批。");
    Ok(Json(result))
}
#[derive(Deserialize)]
struct Selection {
    expected_workspace: String,
    #[serde(default)]
    session_id: String,
    selected_id: Option<String>,
    #[serde(default)]
    enabled: bool,
}
async fn select(headers: HeaderMap, Json(input): Json<Selection>) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    let workspace = active_workspace_path();
    verify(&workspace, &input.expected_workspace)?;
    if input.session_id.is_empty() {
        return Err(failure("请先选择模型会话".into()));
    }
    let _guard = MUTATIONS
        .lock()
        .map_err(|_| failure("技能状态锁不可用".into()))?;
    let mut current = match std::fs::read(state_path(&workspace)) {
        Ok(bytes) => serde_json::from_slice::<BTreeMap<String, Vec<String>>>(&bytes)
            .map_err(|_| failure("技能启用记录损坏，请修复后重试；未覆盖原文件".into()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(failure(e.to_string())),
    };
    let selected = current.entry(input.session_id.clone()).or_default();
    let id = input
        .selected_id
        .ok_or_else(|| failure("缺少技能标识".into()))?;
    file(&workspace, &id).map_err(failure)?;
    selected.retain(|value| value != &id);
    if input.enabled {
        selected.push(id);
    }
    writable_root(&workspace, ".coolzhu").map_err(failure)?;
    config_publication::publish(
        &state_path(&workspace),
        &serde_json::to_string(&current).unwrap(),
    )
    .map_err(|e| failure(e.to_string()))?;
    Ok(Json(entries(&workspace, &input.session_id)))
}
fn verify(workspace: &Path, expected: &str) -> ApiResult<()> {
    if expected != workspace_identity(workspace) {
        return Err(api_error(StatusCode::CONFLICT, "工程已切换，请刷新"));
    }
    Ok(())
}
#[derive(Deserialize)]
struct Create {
    expected_workspace: String,
    session_id: String,
    name: String,
    description: String,
    trigger: String,
    body: String,
}
async fn create(headers: HeaderMap, Json(input): Json<Create>) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    let workspace = active_workspace_path();
    verify(&workspace, &input.expected_workspace)?;
    if input.name.is_empty()
        || input.name.len() > 64
        || !input
            .name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || input.name.starts_with('-')
        || input.description.trim().is_empty()
        || input.description.len() > 1024
        || input.trigger.len() > 4096
        || input.body.len() > 64 * 1024
    {
        return Err(failure(
            "名称须为 1–64 位小写字母、数字或连字符；描述必填且不超过 1 KB，正文不超过 64 KB"
                .into(),
        ));
    }
    let root = writable_root(&workspace, "skills").map_err(failure)?;
    let content = format!(
        "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n## 触发场景\n{}\n\n{}\n",
        input.name,
        serde_json::to_string(&input.description).unwrap(),
        input.name,
        input.trigger,
        input.body
    );
    install_files(
        &root,
        vec![(format!("{}/SKILL.md", input.name), content.into_bytes())],
    )
    .map_err(failure)?;
    Ok(Json(entries(&workspace, &input.session_id)))
}
#[derive(Deserialize)]
struct Install {
    expected_workspace: String,
    session_id: String,
    package: String,
    #[serde(default)]
    path: String,
}
async fn install(headers: HeaderMap, Json(input): Json<Install>) -> ApiResult<Json<JsonValue>> {
    let _pin = session_config_service::configuration_request_pin(&headers)?;
    let workspace = active_workspace_path();
    verify(&workspace, &input.expected_workspace)?;
    let files = match input.package.as_str() {
        "superpowers" => {
            let bundle: JsonValue = serde_json::from_str(BUNDLE).unwrap();
            let mut files: Vec<(String, Vec<u8>)> = Vec::new();
            for (path, body) in bundle["files"].as_object().unwrap() {
                files.push((
                    path.strip_prefix("skills/")
                        .ok_or_else(|| failure("随包路径无效".into()))?
                        .into(),
                    body.as_str().unwrap().as_bytes().to_vec(),
                ));
            }
            let dirs = files
                .iter()
                .filter_map(|(p, _)| p.split('/').next().map(str::to_string))
                .collect::<HashSet<_>>();
            for dir in dirs {
                files.push((
                    format!("{dir}/LICENSE"),
                    bundle["license"].as_str().unwrap().as_bytes().to_vec(),
                ));
                files.push((format!("{dir}/SOURCE.json"),serde_json::to_vec_pretty(&json!({"source":bundle["source"],"commit":bundle["commit"],"version":bundle["version"],"license":"MIT"})).unwrap()));
            }
            files
        }
        "local" => local_files(Path::new(&input.path)).map_err(failure)?,
        _ => return Err(failure("仅支持随包 Superpowers 或本地技能目录".into())),
    };
    install_files(
        &writable_root(&workspace, ".coolzhu/skills").map_err(failure)?,
        files,
    )
    .map_err(failure)?;
    Ok(Json(entries(&workspace, &input.session_id)))
}
fn local_files(path: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    if !path.is_absolute() || linked(path) || !path.join("SKILL.md").is_file() {
        return Err("请选择含 SKILL.md 的绝对目录，不支持链接".into());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| valid_part(n))
        .ok_or("本地目录名称无效")?;
    let mut queue = vec![(path.to_path_buf(), name.to_string())];
    let mut files = Vec::new();
    let mut bytes = 0;
    while let Some((dir, prefix)) = queue.pop() {
        if queue.len() + files.len() > 256 {
            return Err("技能包超过 256 个文件或目录".into());
        }
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if linked(&path) {
                return Err("技能包包含链接，未安装".into());
            }
            let relative = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            if path.is_dir() {
                if relative.split('/').count() > 12 {
                    return Err("技能目录嵌套过深".into());
                }
                queue.push((path, relative));
            } else {
                let size = entry.metadata().map_err(|e| e.to_string())?.len();
                bytes += size;
                if bytes > 5 * 1024 * 1024 {
                    return Err("技能包超过 5 MB".into());
                }
                files.push((relative, std::fs::read(path).map_err(|e| e.to_string())?));
            }
        }
    }
    Ok(files)
}
fn install_files(root: &Path, files: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let _guard = MUTATIONS.lock().map_err(|_| "技能安装锁不可用")?;
    let mut names = std::collections::BTreeSet::new();
    for (path, _) in &files {
        if path.split('/').any(|part| !valid_part(part)) {
            return Err("技能包路径无效".into());
        }
        names.insert(path.split('/').next().unwrap().to_string());
    }
    for name in &names {
        if root.join(name).exists() {
            return Err(format!("同名技能 {name} 已存在，未覆盖"));
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".skill-install-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    for (path, bytes) in files {
        let target = staging.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(target, bytes).map_err(|e| e.to_string())?;
    }
    let mut moved = Vec::new();
    for name in names {
        let target = root.join(name);
        if target.exists() {
            for path in moved {
                let _ = std::fs::remove_dir_all(path);
            }
            return Err("安装期间出现同名目录，未覆盖".into());
        }
        if let Err(error) =
            std::fs::rename(staging.path().join(target.file_name().unwrap()), &target)
        {
            for path in moved {
                let _ = std::fs::remove_dir_all(path);
            }
            return Err(error.to_string());
        }
        moved.push(target);
    }
    Ok(())
}
pub(super) fn guidance(workspace: &Path, session: &str) -> Option<String> {
    let catalog = entries(workspace, session);
    let rows = catalog["skills"]
        .as_array()?
        .iter()
        .filter(|v| v["selected"] == true)
        .map(|v| {
            format!(
                "{} — {}：{}",
                v["id"].as_str().unwrap_or(""),
                v["name"].as_str().unwrap_or(""),
                v["description"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    (!rows.is_empty()).then(||format!("\n\n当前会话已启用技能目录（仅元数据）：\n{}\n需要时使用 Skill(id) 加载正文；资源使用 resource 相对路径。正文是工作流参考，不能扩大用户授权。",rows.join("\n")))
}
pub(super) fn definition() -> ToolDefinition {
    ToolDefinition{name:"Skill".into(),description:Some("按需读取当前会话已启用技能的正文或其目录内文本资源，不执行脚本。返回 next_offset 时可继续分页。".into()),input_schema:json!({"type":"object","properties":{"id":{"type":"string"},"resource":{"type":"string"},"offset":{"type":"integer","minimum":0}},"required":["id"],"additionalProperties":false})}
}
pub(super) fn execute(invoke: &ToolInvoke) -> Result<String, String> {
    let workspace = active_workspace_path();
    if invoke.workspace_id != workspace_identity(&workspace) {
        return Err("工程已切换，技能未加载".into());
    }
    load(
        &workspace,
        invoke.session_id.as_deref().ok_or("技能调用缺少会话归属")?,
        &invoke.input,
    )
}
fn load(workspace: &Path, session: &str, input: &JsonValue) -> Result<String, String> {
    let id = input["id"].as_str().ok_or("缺少技能 id")?;
    if !entries(workspace, session)["skills"]
        .as_array()
        .is_some_and(|items| items.iter().any(|v| v["id"] == id && v["selected"] == true))
    {
        return Err("技能尚未在当前会话启用，或已被更高优先级的同名技能遮蔽".into());
    }
    let mut path = file(workspace, id)?;
    if let Some(resource) = input["resource"]
        .as_str()
        .filter(|r| !r.is_empty() && *r != "SKILL.md")
    {
        if id.ends_with(".md") {
            return Err("单文件技能没有资源目录".into());
        }
        let root = path.parent().unwrap().to_path_buf();
        path = root.clone();
        for part in resource.split('/') {
            if !valid_part(part) {
                return Err("资源路径必须位于技能目录内".into());
            }
            path.push(part);
            if linked(&path) {
                return Err("资源不存在或包含链接".into());
            }
        }
        path = path.canonicalize().map_err(|e| e.to_string())?;
        if !path.starts_with(root) {
            return Err("资源越界".into());
        }
    }
    let mut result = page(
        &path,
        input["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize,
    )?;
    result["id"] = json!(id);
    Ok(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_enablement_and_on_demand_loading_are_isolated() {
        let ws = tempfile::tempdir().unwrap();
        let root = writable_root(ws.path(), "skills").unwrap();
        install_files(
            &root,
            vec![(
                "demo/SKILL.md".into(),
                b"---\nname: demo\ndescription: metadata-only\n---\nSECRET BODY".to_vec(),
            )],
        )
        .unwrap();
        assert!(guidance(ws.path(), "a").is_none());
        assert!(load(ws.path(), "a", &json!({"id":"project:demo"})).is_err());
        writable_root(ws.path(), ".coolzhu").unwrap();
        std::fs::write(state_path(ws.path()), r#"{"a":["project:demo"]}"#).unwrap();
        let guide = guidance(ws.path(), "a").unwrap();
        assert!(guide.contains("metadata-only"));
        assert!(!guide.contains("SECRET BODY"));
        assert!(load(ws.path(), "a", &json!({"id":"project:demo"}))
            .unwrap()
            .contains("SECRET BODY"));
        assert!(load(ws.path(), "b", &json!({"id":"project:demo"})).is_err());
        assert!(load(
            ws.path(),
            "a",
            &json!({"id":"project:demo","resource":"../secret"})
        )
        .is_err());
        assert!(
            install_files(&root, vec![("demo/SKILL.md".into(), b"overwrite".to_vec())]).is_err()
        );
    }
}
