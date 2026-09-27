//! 应用更新第一阶段：只读本地状态，显式检查官方正式发布。
//! 安装、下载与重启不属于这两个接口。

use std::future::Future;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use semver::Version;
use serde::{Deserialize, Serialize};

const RELEASE_API_URL: &str =
    "https://api.github.com/repos/coolzhulike/coolzhuagent/releases/latest";
const RELEASE_PAGE_BASE: &str = "https://github.com/coolzhulike/coolzhuagent/releases/tag";
const MAX_RELEASE_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum UpdateStatus {
    NotChecked,
    Checking,
    UpToDate,
    UpdateAvailable,
    Unavailable,
    CheckFailed,
    NoPublishedRelease,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AppUpdateResponse {
    current_version: Option<String>,
    build_version: String,
    channel: &'static str,
    status: UpdateStatus,
    latest_version: Option<String>,
    release_url: Option<String>,
    checked_at: Option<u64>,
    message: String,
}

#[derive(Clone, Debug)]
struct CheckResult {
    status: UpdateStatus,
    latest_version: Option<String>,
    release_url: Option<String>,
    checked_at: Option<u64>,
    message: String,
}

impl CheckResult {
    fn initial() -> Self {
        Self {
            status: UpdateStatus::NotChecked,
            latest_version: None,
            release_url: None,
            checked_at: None,
            message: "尚未检查官方正式发布。".to_string(),
        }
    }

    fn checking() -> Self {
        Self {
            status: UpdateStatus::Checking,
            latest_version: None,
            release_url: None,
            checked_at: None,
            message: "正在检查官方正式发布。".to_string(),
        }
    }

    fn failure(message: impl Into<String>) -> Self {
        Self {
            status: UpdateStatus::CheckFailed,
            latest_version: None,
            release_url: None,
            checked_at: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

#[derive(Debug)]
struct PublishedRelease {
    version: Version,
    release_url: String,
}

static CHECK_STATE: OnceLock<Mutex<CheckResult>> = OnceLock::new();
static CHECK_GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn check_state() -> &'static Mutex<CheckResult> {
    CHECK_STATE.get_or_init(|| Mutex::new(CheckResult::initial()))
}

fn check_gate() -> &'static tokio::sync::Mutex<()> {
    CHECK_GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn product_version(raw: Option<&str>) -> Option<Version> {
    let version = Version::parse(raw?.trim()).ok()?;
    // MSI 的 Version 是三段数字；预发布或附带元数据的构建不能冒充已安装正式版。
    (version.pre.is_empty() && version.build.is_empty()).then_some(version)
}

fn current_version() -> Option<Version> {
    // build-msi.ps1 在打包编译时注入，开发构建不使用 workspace/Cargo 的 0.2.0 猜测。
    product_version(option_env!("COOLZHU_RELEASE_VERSION"))
}

fn response(result: &CheckResult) -> AppUpdateResponse {
    let current = current_version();
    let message = if result.status == UpdateStatus::NotChecked && current.is_none() {
        "当前构建未携带有效发行版本；尚未检查官方正式发布。".to_string()
    } else {
        result.message.clone()
    };
    AppUpdateResponse {
        current_version: current.as_ref().map(Version::to_string),
        build_version: crate::web_console_build_version(),
        channel: if current.is_some() {
            "release"
        } else {
            "development"
        },
        status: result.status,
        latest_version: result.latest_version.clone(),
        release_url: result.release_url.clone(),
        checked_at: result.checked_at,
        message,
    }
}

pub(super) async fn api_get() -> Json<AppUpdateResponse> {
    let result = check_state()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    Json(response(&result))
}

pub(super) async fn api_check() -> Json<AppUpdateResponse> {
    check_with(fetch_latest_release()).await
}

async fn check_with<F>(fetch: F) -> Json<AppUpdateResponse>
where
    F: Future<Output = Result<Option<PublishedRelease>, String>>,
{
    // 连续点击串行检查，GET 始终能立刻读到本地状态。
    let _gate = check_gate().lock().await;
    *check_state()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = CheckResult::checking();

    let mut result = match fetch.await {
        Ok(Some(release)) => compare_release(release, current_version()),
        Ok(None) => CheckResult {
            status: UpdateStatus::NoPublishedRelease,
            latest_version: None,
            release_url: None,
            checked_at: None,
            message: "官方仓库目前没有正式稳定版发布。".to_string(),
        },
        Err(message) => CheckResult::failure(message),
    };
    result.checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_millis() as u64);
    *check_state()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = result.clone();
    Json(response(&result))
}

fn compare_release(release: PublishedRelease, current: Option<Version>) -> CheckResult {
    let latest_version = release.version.to_string();
    let (status, message) = match current {
        None => (
            UpdateStatus::Unavailable,
            "已找到官方正式发布，但当前构建版本未知，无法判断是否需要更新。".to_string(),
        ),
        Some(current) if release.version > current => (
            UpdateStatus::UpdateAvailable,
            format!("官方正式发布 {latest_version} 高于当前版本 {current}。"),
        ),
        Some(current) if release.version == current => (
            UpdateStatus::UpToDate,
            "已检查官方正式发布，当前没有可用的新版本。".to_string(),
        ),
        Some(current) => (
            UpdateStatus::UpToDate,
            format!("当前版本 {current} 高于官方最新正式发布 {latest_version}。"),
        ),
    };
    CheckResult {
        status,
        latest_version: Some(latest_version),
        release_url: Some(release.release_url),
        checked_at: None,
        message,
    }
}

async fn fetch_latest_release() -> Result<Option<PublishedRelease>, String> {
    // 不接受网页传来的地址、身份凭据或代理配置；拒绝重定向以保持固定来源。
    // 保留系统代理支持，以便已通过系统代理访问 GitHub 的用户完成检查。
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化官方发布源连接。".to_string())?;
    let mut remote = client
        .get(RELEASE_API_URL)
        .header(reqwest::header::USER_AGENT, "coolzhuagent-app-update/1")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "连接官方发布源失败或超时，请稍后重试。".to_string())?;
    if !release_response_has_body(remote.status())? {
        return Ok(None);
    }
    if remote
        .content_length()
        .is_some_and(|size| size > MAX_RELEASE_RESPONSE_BYTES as u64)
    {
        return Err("官方发布信息超过允许大小。".to_string());
    }
    let mut body = Vec::new();
    while let Some(chunk) = remote
        .chunk()
        .await
        .map_err(|_| "读取官方发布信息失败。".to_string())?
    {
        if chunk.len() > MAX_RELEASE_RESPONSE_BYTES.saturating_sub(body.len()) {
            return Err("官方发布信息超过允许大小。".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    parse_release(&body)
}

fn release_response_has_body(status: reqwest::StatusCode) -> Result<bool, String> {
    match status {
        reqwest::StatusCode::NOT_FOUND => Ok(false),
        reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::TOO_MANY_REQUESTS => {
            Err("官方发布源限流或拒绝请求，请稍后重试。".to_string())
        }
        reqwest::StatusCode::OK => Ok(true),
        status => Err(format!("官方发布源返回 HTTP {status}，请稍后重试。")),
    }
}

fn parse_release(body: &[u8]) -> Result<Option<PublishedRelease>, String> {
    let release: GithubRelease =
        serde_json::from_slice(body).map_err(|_| "官方发布信息格式无效。".to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let tag = release.tag_name.trim();
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| "官方发布版本号无效，无法比较。".to_string())?;
    if !version.pre.is_empty() {
        return Ok(None);
    }
    let mut page = reqwest::Url::parse(RELEASE_PAGE_BASE).expect("固定的官方发布页 URL 必须有效");
    page.path_segments_mut()
        .expect("固定的官方发布页 URL 必须有路径")
        .push(tag);
    Ok(Some(PublishedRelease {
        version,
        release_url: page.into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn release(tag: &str, draft: bool, prerelease: bool) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": prerelease
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn product_version_requires_explicit_stable_release_identity() {
        assert_eq!(product_version(None), None);
        assert_eq!(
            product_version(Some("0.2.17")),
            Some(Version::new(0, 2, 17))
        );
        assert_eq!(product_version(Some("9eae3aec · 2026-09-27")), None);
        assert_eq!(product_version(Some("0.2.18-beta.1")), None);
    }

    #[test]
    fn official_release_metadata_and_semver_comparison() {
        let published = parse_release(&release("v0.2.18", false, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            published.release_url,
            "https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.18"
        );
        let update = compare_release(published, Some(Version::new(0, 2, 17)));
        assert_eq!(update.status, UpdateStatus::UpdateAvailable);
        assert_eq!(update.latest_version.as_deref(), Some("0.2.18"));
        let latest = parse_release(&release("v0.2.18", false, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            compare_release(latest, Some(Version::new(0, 2, 18))).status,
            UpdateStatus::UpToDate
        );
    }

    #[test]
    fn unknown_current_or_unpublished_release_never_claims_up_to_date() {
        let latest = parse_release(&release("v0.2.17", false, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            compare_release(latest, None).status,
            UpdateStatus::Unavailable
        );
        assert!(parse_release(&release("v0.2.18", true, false))
            .unwrap()
            .is_none());
        assert!(parse_release(&release("v0.2.18-rc.1", false, true))
            .unwrap()
            .is_none());
        assert!(parse_release(&release("v0.2.18-rc.1", false, false))
            .unwrap()
            .is_none());
        assert!(parse_release(&release("next", false, false)).is_err());
    }

    #[test]
    fn response_contract_keeps_failure_distinct_from_no_update() {
        let response = response(&CheckResult::failure("测试失败"));
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(json["status"], "check_failed");
        assert!(json["current_version"].is_null() || json["current_version"].is_string());
        assert!(json["latest_version"].is_null());
        assert!(json["release_url"].is_null());
        assert!(json["checked_at"].is_null());
        assert!(json["build_version"].is_string());
        assert!(json["channel"].is_string());
    }

    #[test]
    fn upstream_http_errors_do_not_mean_no_update() {
        assert_eq!(
            release_response_has_body(reqwest::StatusCode::NOT_FOUND),
            Ok(false)
        );
        assert_eq!(release_response_has_body(reqwest::StatusCode::OK), Ok(true));
        assert!(release_response_has_body(reqwest::StatusCode::FORBIDDEN).is_err());
        assert!(release_response_has_body(reqwest::StatusCode::TOO_MANY_REQUESTS).is_err());
        assert!(release_response_has_body(reqwest::StatusCode::FOUND).is_err());
    }

    #[tokio::test]
    async fn local_get_and_check_failure_keep_the_json_http_contract() {
        let failed = check_with(async { Err("受控测试失败".to_string()) })
            .await
            .into_response();
        assert_eq!(failed.status(), StatusCode::OK);
        let body = failed.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "check_failed");
        assert!(json["checked_at"].is_number());
        assert!(json["release_url"].is_null());

        // GET 只读进程内最近检查结果；构造路由和读取该状态均不触发远端请求。
        let cached = crate::app()
            .oneshot(
                Request::builder()
                    .uri("/api/system/app-update")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached.status(), StatusCode::OK);
        let body = cached.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "check_failed");

        let unpublished = check_with(async { Ok(None) }).await.into_response();
        assert_eq!(unpublished.status(), StatusCode::OK);
        let body = unpublished.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "no_published_release");
        assert!(json["checked_at"].is_number());
    }
}
