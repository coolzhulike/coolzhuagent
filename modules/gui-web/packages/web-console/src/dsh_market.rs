//! DSH 社区目录只用于发现。目录文本与仓库链接都不是本机插件安装授权。
use super::*;
use futures_util::StreamExt;
use std::io::Read;

const CATALOG_URL: &str = "https://awesome-dsh-plugin.com/plugins.json";
const CATALOG_LIMIT: usize = 16 * 1024 * 1024;
const MANIFEST_LIMIT: usize = 64 * 1024;
const PAGE_SIZE: usize = 24;
const CACHE_AGE: Duration = Duration::from_secs(300);
// 完整社区目录约 5 MB，直连慢速链路的下载不能沿用小清单的 15 秒预算。
const CATALOG_TIMEOUT: Duration = Duration::from_secs(90);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Deserialize)]
struct DshEntry {
    name: String,
    owner: String,
    url: String,
    #[serde(default)]
    category: JsonValue,
    #[serde(default)]
    description: HashMap<String, String>,
    #[serde(default)]
    npm: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    stars: Option<u64>,
    #[serde(default)]
    deprecated: bool,
}

#[derive(Clone, Deserialize)]
struct DshCatalog {
    updated: String,
    categories: HashMap<String, HashMap<String, String>>,
    plugins: Vec<DshEntry>,
}

#[derive(Default)]
struct DshCache {
    catalog: Option<Arc<DshCatalog>>,
    checked_at: Option<Instant>,
    retry_after: Option<Instant>,
}

fn dsh_cache() -> &'static tokio::sync::Mutex<DshCache> {
    static CACHE: OnceLock<tokio::sync::Mutex<DshCache>> = OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(DshCache::default()))
}

pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/extension-market/dsh", get(dsh_list))
        .route("/api/extension-market/dsh/detail", get(dsh_detail))
        .route(
            "/api/extension-market/dsh/compatibility",
            post(dsh_compatibility),
        )
}

fn dsh_client(timeout: Duration) -> ApiResult<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent("coolzhu-web-console/DSH-directory")
        .build()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建市场目录连接"))
}

async fn dsh_get_limited(url: &str, limit: usize, timeout: Duration) -> ApiResult<Option<Vec<u8>>> {
    let response = dsh_client(timeout)?
        .get(url)
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .send()
        .await
        .map_err(|_| api_error(StatusCode::BAD_GATEWAY, "市场来源连接失败或超时"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(api_error(StatusCode::BAD_GATEWAY, "市场来源返回错误状态"));
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(api_error(StatusCode::BAD_GATEWAY, "市场响应超过大小限制"));
    }
    let compressed = match response.headers().get(reqwest::header::CONTENT_ENCODING) {
        None => false,
        Some(value) if value.as_bytes().eq_ignore_ascii_case(b"identity") => false,
        Some(value) if value.as_bytes().eq_ignore_ascii_case(b"gzip") => true,
        _ => return Err(api_error(StatusCode::BAD_GATEWAY, "市场响应编码不受支持")),
    };
    let mut result = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            if error.is_timeout() {
                api_error(StatusCode::GATEWAY_TIMEOUT, "市场下载超时，请稍后重试")
            } else {
                api_error(StatusCode::BAD_GATEWAY, "市场响应读取中断")
            }
        })?;
        if result.len().saturating_add(chunk.len()) > limit {
            return Err(api_error(StatusCode::BAD_GATEWAY, "市场响应超过大小限制"));
        }
        result.extend_from_slice(&chunk);
    }
    if compressed {
        result = dsh_decode_gzip(&result, limit)?;
    }
    Ok(Some(result))
}

fn dsh_decode_gzip(bytes: &[u8], limit: usize) -> ApiResult<Vec<u8>> {
    // 传输体和解压体分别限长，不能让压缩响应绕过原有 16 MB/64 KB 门限。
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| api_error(StatusCode::BAD_GATEWAY, "市场压缩响应无效"))?;
    if decoded.len() > limit {
        return Err(api_error(StatusCode::BAD_GATEWAY, "市场响应超过大小限制"));
    }
    Ok(decoded)
}

fn dsh_parse_catalog(bytes: &[u8]) -> ApiResult<DshCatalog> {
    let catalog: DshCatalog = serde_json::from_slice(bytes)
        .map_err(|_| api_error(StatusCode::BAD_GATEWAY, "市场目录格式无效"))?;
    if catalog.plugins.is_empty() || catalog.categories.is_empty() || catalog.updated.is_empty() {
        return Err(api_error(StatusCode::BAD_GATEWAY, "市场目录缺少必要内容"));
    }
    Ok(catalog)
}

async fn dsh_catalog(force: bool) -> ApiResult<(Arc<DshCatalog>, bool, Option<String>)> {
    let started = Instant::now();
    let mut cache = dsh_cache().lock().await;
    if cache
        .retry_after
        .is_some_and(|retry_after| retry_after > Instant::now())
    {
        return cache
            .catalog
            .as_ref()
            .map(|catalog| {
                (
                    catalog.clone(),
                    true,
                    Some("远程刷新失败；显示上次读取的目录，请稍后再试。".into()),
                )
            })
            .ok_or_else(|| api_error(StatusCode::BAD_GATEWAY, "远程目录暂时不可用，请稍后重试"));
    }
    if let (Some(catalog), Some(checked_at)) = (&cache.catalog, cache.checked_at) {
        // 等待同一个刷新任务的请求直接复用结果，即使它原本也要求强制刷新。
        if cache.retry_after.is_none()
            && (checked_at >= started || (!force && checked_at.elapsed() < CACHE_AGE))
        {
            return Ok((catalog.clone(), false, None));
        }
    }
    let fresh = async {
        let bytes = dsh_get_limited(CATALOG_URL, CATALOG_LIMIT, CATALOG_TIMEOUT)
            .await?
            .ok_or_else(|| api_error(StatusCode::BAD_GATEWAY, "市场目录不存在"))?;
        dsh_parse_catalog(&bytes)
    }
    .await;
    match fresh {
        Ok(catalog) => {
            let catalog = Arc::new(catalog);
            cache.catalog = Some(catalog.clone());
            cache.checked_at = Some(Instant::now());
            cache.retry_after = None;
            Ok((catalog, false, None))
        }
        Err(error) => {
            cache.retry_after = Some(Instant::now() + Duration::from_secs(30));
            if let Some(catalog) = &cache.catalog {
                // 旧目录必须带明确状态，不能让网络故障看起来像刚刷新成功。
                Ok((
                    catalog.clone(),
                    true,
                    Some(format!(
                        "远程刷新失败；显示上次读取的目录：{}",
                        error.1 .0.error
                    )),
                ))
            } else {
                Err(error)
            }
        }
    }
}

fn dsh_categories(entry: &DshEntry) -> Vec<String> {
    match &entry.category {
        JsonValue::String(value) if !value.is_empty() => vec![value.clone()],
        JsonValue::Array(values) => values
            .iter()
            .filter_map(JsonValue::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn dsh_id(entry: &DshEntry) -> String {
    format!("{}/{}", entry.owner, entry.name)
}

fn dsh_description(entry: &DshEntry) -> String {
    entry
        .description
        .get("zh")
        .filter(|value| !value.is_empty())
        .or_else(|| entry.description.get("en"))
        .cloned()
        .unwrap_or_default()
}

#[derive(Serialize)]
struct DshItem<'a> {
    id: String,
    name: &'a str,
    owner: &'a str,
    description: String,
    categories: Vec<String>,
    version: Option<&'a str>,
    stars: Option<u64>,
    deprecated: bool,
    compatibility: &'static str,
}

fn dsh_item(entry: &DshEntry) -> DshItem<'_> {
    DshItem {
        id: dsh_id(entry),
        name: &entry.name,
        owner: &entry.owner,
        description: dsh_description(entry),
        categories: dsh_categories(entry),
        version: entry.version.as_deref(),
        stars: entry.stars,
        deprecated: entry.deprecated,
        compatibility: "unknown",
    }
}

#[derive(Deserialize)]
struct DshListQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    category: String,
    page: Option<usize>,
    #[serde(default)]
    refresh: bool,
}

#[derive(Serialize)]
struct DshCategory {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct DshListResponse<'a> {
    workspace_id: String,
    source: &'static str,
    updated: &'a str,
    stale: bool,
    warning: Option<String>,
    total: usize,
    page: usize,
    page_size: usize,
    page_count: usize,
    categories: Vec<DshCategory>,
    items: Vec<DshItem<'a>>,
}

fn dsh_filtered<'a>(catalog: &'a DshCatalog, query: &DshListQuery) -> Vec<&'a DshEntry> {
    let needle = query.q.trim().to_lowercase();
    catalog
        .plugins
        .iter()
        .filter(|entry| {
            let description = dsh_description(entry);
            (query.category.is_empty()
                || dsh_categories(entry).iter().any(|id| id == &query.category))
                && (needle.is_empty()
                    || [
                        entry.name.as_str(),
                        entry.owner.as_str(),
                        description.as_str(),
                    ]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&needle)))
        })
        .collect()
}

async fn dsh_list(Query(query): Query<DshListQuery>) -> ApiResult<Json<JsonValue>> {
    if query.q.chars().count() > 160
        || query.category.chars().count() > 80
        || query.page.unwrap_or(1) == 0
    {
        return Err(api_error(StatusCode::BAD_REQUEST, "市场查询参数无效"));
    }
    let (catalog, stale, warning) = dsh_catalog(query.refresh).await?;
    let matches = dsh_filtered(&catalog, &query);
    let total = matches.len();
    let page_count = total.div_ceil(PAGE_SIZE).max(1);
    let page = query.page.unwrap_or(1).min(page_count);
    let offset = (page - 1) * PAGE_SIZE;
    let mut categories = catalog
        .categories
        .iter()
        .map(|(id, names)| DshCategory {
            id: id.clone(),
            name: names
                .get("zh")
                .or_else(|| names.get("en"))
                .cloned()
                .unwrap_or_else(|| id.clone()),
        })
        .collect::<Vec<_>>();
    categories.sort_by(|a, b| a.name.cmp(&b.name));
    let response = DshListResponse {
        workspace_id: workspace_identity(&active_workspace_path()),
        source: CATALOG_URL,
        updated: &catalog.updated,
        stale,
        warning,
        total,
        page,
        page_size: PAGE_SIZE,
        page_count,
        categories,
        items: matches[offset..matches.len().min(offset + PAGE_SIZE)]
            .iter()
            .map(|entry| dsh_item(entry))
            .collect(),
    };
    Ok(Json(serde_json::to_value(response).map_err(|_| {
        api_error(StatusCode::INTERNAL_SERVER_ERROR, "市场列表序列化失败")
    })?))
}

#[derive(Deserialize)]
struct DshDetailQuery {
    id: String,
}

async fn dsh_detail(Query(query): Query<DshDetailQuery>) -> ApiResult<Json<JsonValue>> {
    if query.id.len() > 240 {
        return Err(api_error(StatusCode::BAD_REQUEST, "市场条目 id 无效"));
    }
    let (catalog, stale, warning) = dsh_catalog(false).await?;
    let entry = catalog
        .plugins
        .iter()
        .find(|entry| dsh_id(entry) == query.id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "市场条目不存在"))?;
    Ok(Json(json!({
        "workspace_id": workspace_identity(&active_workspace_path()),
        "item": dsh_item(entry), "source": CATALOG_URL, "updated": catalog.updated,
        "stale": stale, "warning": warning, "repository": entry.url,
        "npm": entry.npm, "description_zh": entry.description.get("zh"),
        "description_en": entry.description.get("en"),
        "compatibility": "unknown",
        "compatibility_reason": "目录收录不等于 COOLZHU 原生插件兼容；可检查仓库清单。"
    })))
}

fn dsh_github_repo(entry: &DshEntry) -> Option<(String, String)> {
    let path = entry
        .url
        .strip_prefix("https://github.com/")?
        .trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repo_part = parts.next()?;
    let repo = repo_part.strip_suffix(".git").unwrap_or(repo_part);
    if parts.next().is_some()
        || owner != entry.owner
        || !dsh_repo_segment(owner)
        || !dsh_repo_segment(repo)
    {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn dsh_repo_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

#[derive(Deserialize)]
struct DshCompatibilityRequest {
    expected_workspace: String,
    id: String,
}

async fn dsh_compatibility(
    Json(request): Json<DshCompatibilityRequest>,
) -> ApiResult<Json<JsonValue>> {
    let workspace = active_workspace_path();
    if request.expected_workspace != workspace_identity(&workspace) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工程目录已切换，请刷新后再检查",
        ));
    }
    if request.id.len() > 240 {
        return Err(api_error(StatusCode::BAD_REQUEST, "市场条目 id 无效"));
    }
    let (catalog, stale, _) = dsh_catalog(false).await?;
    let entry = catalog
        .plugins
        .iter()
        .find(|entry| dsh_id(entry) == request.id)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "市场条目不存在"))?;
    let Some((owner, repo)) = dsh_github_repo(entry) else {
        return Ok(Json(
            json!({"workspace_id": request.expected_workspace, "id": request.id,
            "status": "unknown", "reason": "目录仓库地址无法严格核实为该作者的 GitHub 仓库；未检查包内容。",
            "installable": false, "stale_catalog": stale}),
        ));
    };
    // 一个总预算覆盖仓库与两个清单路径；路径顺序与 PluginManager 的加载器一致。
    let check = async {
        let repository_url = format!("https://api.github.com/repos/{owner}/{repo}");
        if !matches!(
            dsh_get_limited(&repository_url, MANIFEST_LIMIT, MANIFEST_TIMEOUT).await,
            Ok(Some(_))
        ) {
            return ("unknown", "GitHub 仓库不可访问，无法判断清单是否存在。");
        }
        for path in ["plugin.json", ".claw-plugin/plugin.json"] {
            let url = format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}");
            match dsh_get_limited(&url, MANIFEST_LIMIT, MANIFEST_TIMEOUT).await {
                Ok(None) => continue,
                Err(_) => return ("unknown", "GitHub 清单读取失败，无法判断兼容性。"),
                Ok(Some(bytes)) => {
                    let Some(manifest) = dsh_decode_manifest(&bytes) else {
                        return ("unknown", "清单内容无法解析，不能确认兼容性。");
                    };
                    if !["name", "version", "description"].iter().all(|key| {
                        manifest
                            .get(*key)
                            .and_then(JsonValue::as_str)
                            .is_some_and(|value| !value.trim().is_empty())
                    }) {
                        return ("incompatible", "清单缺少当前加载器要求的名称、版本或说明。");
                    }
                    return ("manifest_found_unverified", "发现原生插件清单，但未按当前加载器核验完整仓库与固定提交；当前不提供远程安装。");
                }
            }
        }
        (
            "incompatible",
            "仓库根目录未发现 plugin.json 或 .claw-plugin/plugin.json，当前加载器无法安装。",
        )
    };
    let (status, reason) = tokio::time::timeout(Duration::from_secs(25), check)
        .await
        .unwrap_or(("unknown", "GitHub 清单检查超时，无法判断兼容性。"));
    Ok(Json(
        json!({"workspace_id": request.expected_workspace, "id": request.id,
        "status": status, "reason": reason, "installable": false, "stale_catalog": stale}),
    ))
}

fn dsh_decode_manifest(bytes: &[u8]) -> Option<JsonValue> {
    let content: JsonValue = serde_json::from_slice(bytes).ok()?;
    if content.get("encoding")?.as_str()? != "base64" {
        return None;
    }
    let encoded = content.get("content")?.as_str()?;
    let encoded = encoded
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<String>();
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
    let manifest: JsonValue = serde_json::from_slice(&decoded).ok()?;
    manifest.is_object().then_some(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsh_gzip_is_bounded_and_rejects_corruption() {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"real catalog payload shape").unwrap();
        let bytes = encoder.finish().unwrap();
        assert_eq!(dsh_decode_gzip(&bytes, 128).unwrap(), b"real catalog payload shape");
        assert!(dsh_decode_gzip(&bytes, 4).is_err());
        assert!(dsh_decode_gzip(&bytes[..bytes.len() - 3], 128).is_err());
    }

    #[test]
    fn dsh_repo_rejects_untrusted_url_shapes() {
        let mut entry = DshEntry {
            name: "plugin".into(),
            owner: "owner".into(),
            url: "https://github.com/owner/repo".into(),
            category: json!("agi"),
            description: HashMap::new(),
            npm: None,
            version: None,
            stars: None,
            deprecated: false,
        };
        assert_eq!(
            dsh_github_repo(&entry),
            Some(("owner".into(), "repo".into()))
        );
        for url in [
            "http://github.com/owner/repo",
            "https://github.com/other/repo",
            "https://github.com/owner/repo/tree/main",
            "https://github.com/owner/repo?x=1",
        ] {
            entry.url = url.into();
            assert!(
                dsh_github_repo(&entry).is_none(),
                "unexpected repository: {url}"
            );
        }
    }
}
