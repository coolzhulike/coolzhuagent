use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::OAuthConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceCodePair {
    pub verifier: String,
    pub challenge: String,
    pub challenge_method: PkceChallengeMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceChallengeMethod {
    S256,
}

impl PkceChallengeMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::S256 => "S256",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthorizationRequest {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: PkceChallengeMethod,
    pub extra_params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenExchangeRequest {
    pub grant_type: &'static str,
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub code_verifier: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthRefreshRequest {
    pub grant_type: &'static str,
    pub refresh_token: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOAuthCredentials {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<u64>,
    #[serde(default)]
    scopes: Vec<String>,
}

impl From<OAuthTokenSet> for StoredOAuthCredentials {
    fn from(value: OAuthTokenSet) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            expires_at: value.expires_at,
            scopes: value.scopes,
        }
    }
}

impl From<StoredOAuthCredentials> for OAuthTokenSet {
    fn from(value: StoredOAuthCredentials) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            expires_at: value.expires_at,
            scopes: value.scopes,
        }
    }
}

impl OAuthAuthorizationRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        redirect_uri: impl Into<String>,
        state: impl Into<String>,
        pkce: &PkceCodePair,
    ) -> Self {
        Self {
            authorize_url: config.authorize_url.clone(),
            client_id: config.client_id.clone(),
            redirect_uri: redirect_uri.into(),
            scopes: config.scopes.clone(),
            state: state.into(),
            code_challenge: pkce.challenge.clone(),
            code_challenge_method: pkce.challenge_method,
            extra_params: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_extra_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_params.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn build_url(&self) -> String {
        let mut params = vec![
            ("response_type", "code".to_string()),
            ("client_id", self.client_id.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("scope", self.scopes.join(" ")),
            ("state", self.state.clone()),
            ("code_challenge", self.code_challenge.clone()),
            (
                "code_challenge_method",
                self.code_challenge_method.as_str().to_string(),
            ),
        ];
        params.extend(
            self.extra_params
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone())),
        );
        let query = params
            .into_iter()
            .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(&value)))
            .collect::<Vec<_>>()
            .join("&");
        format!(
            "{}{}{}",
            self.authorize_url,
            if self.authorize_url.contains('?') {
                '&'
            } else {
                '?'
            },
            query
        )
    }
}

impl OAuthTokenExchangeRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        code: impl Into<String>,
        state: impl Into<String>,
        verifier: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "authorization_code",
            code: code.into(),
            redirect_uri: redirect_uri.into(),
            client_id: config.client_id.clone(),
            code_verifier: verifier.into(),
            state: state.into(),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("code", self.code.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", self.code_verifier.clone()),
            ("state", self.state.clone()),
        ])
    }
}

impl OAuthRefreshRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        refresh_token: impl Into<String>,
        scopes: Option<Vec<String>>,
    ) -> Self {
        Self {
            grant_type: "refresh_token",
            refresh_token: refresh_token.into(),
            client_id: config.client_id.clone(),
            scopes: scopes.unwrap_or_else(|| config.scopes.clone()),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("refresh_token", self.refresh_token.clone()),
            ("client_id", self.client_id.clone()),
            ("scope", self.scopes.join(" ")),
        ])
    }
}

pub fn generate_pkce_pair() -> io::Result<PkceCodePair> {
    let verifier = generate_random_token(32)?;
    Ok(PkceCodePair {
        challenge: code_challenge_s256(&verifier),
        verifier,
        challenge_method: PkceChallengeMethod::S256,
    })
}

pub fn generate_state() -> io::Result<String> {
    generate_random_token(32)
}

#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_encode(&digest)
}

#[must_use]
pub fn loopback_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

pub fn credentials_path() -> io::Result<PathBuf> {
    Ok(credentials_home_dir()?.join("credentials.json"))
}

/// 版本在登录、刷新、注销时均推进，给跨进程刷新提供比较并交换依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCredentialsSnapshot {
    pub token_set: Option<OAuthTokenSet>,
    pub revision: u64,
}

pub fn load_oauth_credentials_snapshot() -> io::Result<OAuthCredentialsSnapshot> {
    let path = credentials_path()?;
    for _ in 0..3 {
        let snapshot = crate::oauth_store::read(&path)?;
        let tokens = snapshot.data.as_ref().map(|value| serde_json::from_value::<StoredOAuthCredentials>(value.clone()))
            .transpose().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "OAuth 凭据结构无效"))?;
        if snapshot.needs_migration {
            if let Some(tokens) = tokens {
                let serialized = serde_json::to_value(&tokens).map_err(|_| io::Error::other("OAuth 凭据编码失败"))?;
                // 只有旧格式成功解析、新保护数据成功原子发布之后才替换原值。
                if !crate::oauth_store::save(&path, Some(&serialized), Some(snapshot.revision))? { continue; }
                return Ok(OAuthCredentialsSnapshot { token_set: Some(tokens.into()), revision: snapshot.revision + 1 });
            }
        }
        return Ok(OAuthCredentialsSnapshot { token_set: tokens.map(Into::into), revision: snapshot.revision });
    }
    Err(io::Error::new(io::ErrorKind::WouldBlock, "OAuth 凭据正在变更，请重试"))
}

pub fn load_oauth_credentials() -> io::Result<Option<OAuthTokenSet>> {
    Ok(load_oauth_credentials_snapshot()?.token_set)
}

pub fn save_oauth_credentials(token_set: &OAuthTokenSet) -> io::Result<()> {
    let value = serde_json::to_value(StoredOAuthCredentials::from(token_set.clone())).map_err(|_| io::Error::other("OAuth 凭据编码失败"))?;
    crate::oauth_store::save(&credentials_path()?, Some(&value), None)?;
    Ok(())
}

pub fn save_oauth_credentials_if_revision(token_set: &OAuthTokenSet, expected_revision: u64) -> io::Result<bool> {
    let value = serde_json::to_value(StoredOAuthCredentials::from(token_set.clone())).map_err(|_| io::Error::other("OAuth 凭据编码失败"))?;
    crate::oauth_store::save(&credentials_path()?, Some(&value), Some(expected_revision))
}

/// 刷新仅持有独立 refresh 锁；登录/注销可以修改文件并让迟到刷新 CAS 失败。
pub fn acquire_oauth_refresh_guard() -> io::Result<std::fs::File> {
    crate::oauth_store::lock(&credentials_path()?, true)
}

pub fn clear_oauth_credentials() -> io::Result<()> {
    crate::oauth_store::save(&credentials_path()?, None, None)?;
    Ok(())
}

pub fn parse_oauth_callback_request_target(target: &str) -> Result<OAuthCallbackParams, String> {
    let (path, query) = target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query));
    if path != "/callback" {
        return Err(format!("unexpected callback path: {path}"));
    }
    parse_oauth_callback_query(query)
}

pub fn parse_oauth_callback_query(query: &str) -> Result<OAuthCallbackParams, String> {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('=')
            .map_or((pair, ""), |(key, value)| (key, value));
        params.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok(OAuthCallbackParams {
        code: params.get("code").cloned(),
        state: params.get("state").cloned(),
        error: params.get("error").cloned(),
        error_description: params.get("error_description").cloned(),
    })
}

fn generate_random_token(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error.to_string()))?;
    Ok(base64url_encode(&buffer))
}

fn credentials_home_dir() -> io::Result<PathBuf> {
    Ok(crate::config::default_config_home())
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        output.push(TABLE[(block & 0x3F) as usize] as char);
        index += 3;
    }
    match bytes.len().saturating_sub(index) {
        1 => {
            let block = u32::from(bytes[index]) << 16;
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        }
        2 => {
            let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        }
        _ => {}
    }
    output
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = decode_hex(bytes[index + 1])?;
                let lo = decode_hex(bytes[index + 2])?;
                decoded.push((hi << 4) | lo);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

fn decode_hex(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid percent-encoding byte: {byte}")),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        clear_oauth_credentials, code_challenge_s256, credentials_path, generate_pkce_pair,
        generate_state, load_oauth_credentials, loopback_redirect_uri, parse_oauth_callback_query,
        parse_oauth_callback_request_target, save_oauth_credentials, OAuthAuthorizationRequest,
        OAuthConfig, OAuthRefreshRequest, OAuthTokenExchangeRequest, OAuthTokenSet,
    };
    use crate::test_env::{env_set, ScopedEnv};

    fn sample_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "runtime-client".to_string(),
            authorize_url: "https://console.test/oauth/authorize".to_string(),
            token_url: "https://console.test/oauth/token".to_string(),
            callback_port: Some(4545),
            manual_redirect_url: Some("https://console.test/oauth/callback".to_string()),
            scopes: vec!["org:read".to_string(), "user:write".to_string()],
        }
    }

    fn temp_config_home() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "runtime-oauth-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn s256_challenge_matches_expected_vector() {
        assert_eq!(
            code_challenge_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generates_pkce_pair_and_state() {
        let pair = generate_pkce_pair().expect("pkce pair");
        let state = generate_state().expect("state");
        assert!(!pair.verifier.is_empty());
        assert!(!pair.challenge.is_empty());
        assert!(!state.is_empty());
    }

    #[test]
    fn builds_authorize_url_and_form_requests() {
        let config = sample_config();
        let pair = generate_pkce_pair().expect("pkce");
        let url = OAuthAuthorizationRequest::from_config(
            &config,
            loopback_redirect_uri(4545),
            "state-123",
            &pair,
        )
        .with_extra_param("login_hint", "user@example.com")
        .build_url();
        assert!(url.starts_with("https://console.test/oauth/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=runtime-client"));
        assert!(url.contains("scope=org%3Aread%20user%3Awrite"));
        assert!(url.contains("login_hint=user%40example.com"));

        let exchange = OAuthTokenExchangeRequest::from_config(
            &config,
            "auth-code",
            "state-123",
            pair.verifier,
            loopback_redirect_uri(4545),
        );
        assert_eq!(
            exchange.form_params().get("grant_type").map(String::as_str),
            Some("authorization_code")
        );

        let refresh = OAuthRefreshRequest::from_config(&config, "refresh-token", None);
        assert_eq!(
            refresh.form_params().get("scope").map(String::as_str),
            Some("org:read user:write")
        );
    }

    /// I2（P-08 裁决）：本用例是 `oauth.rs` 里**唯一**写进程环境变量的点，改用本 crate
    /// 已有的「统一锁 + RAII guard」，**不新增第二把锁**；OAuth 语义不变。
    ///
    /// 改前是手写 `set_var("CLAW_CONFIG_HOME", ...) → ... → 末尾 remove_var(...)`：
    /// 一旦用例中途 panic 或提前返回，末尾的清理语句执行不到，测试值就留在**当前测试进程的
    /// 后续执行**里（不是"用户系统环境变量被永久修改"）。而 `credentials_path()` →
    /// `config::default_config_home()` 优先读 `CLAW_CONFIG_HOME`，后续用例会静默把凭据
    /// 读写落到本用例的临时目录。guard 用 `Option<OsString>` 无损保存三态原值。
    #[test]
    fn oauth_credentials_round_trip_and_clear_preserves_other_fields() {
        let config_home = temp_config_home();
        let _env = ScopedEnv::new(vec![env_set("CLAW_CONFIG_HOME", &config_home)]);
        let path = credentials_path().expect("credentials path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        std::fs::write(&path, r#"{"other":"value","oauth":{"accessToken":"legacy-access","refreshToken":null}}"#).expect("seed credentials");
        assert_eq!(load_oauth_credentials().unwrap().unwrap().access_token, "legacy-access");
        #[cfg(windows)] assert!(!std::fs::read_to_string(&path).unwrap().contains("legacy-access"));

        let token_set = OAuthTokenSet {
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(123),
            scopes: vec!["scope:a".to_string()],
        };
        save_oauth_credentials(&token_set).expect("save credentials");
        assert_eq!(
            load_oauth_credentials().expect("load credentials"),
            Some(token_set)
        );
        let saved = std::fs::read_to_string(&path).expect("read saved file");
        assert!(saved.contains("\"other\": \"value\""));
        assert!(saved.contains("\"oauth\""));
        #[cfg(windows)] { assert!(!saved.contains("access-token")); assert!(saved.contains("windows_dpapi_v1")); }

        let before_logout = super::load_oauth_credentials_snapshot().unwrap().revision;
        clear_oauth_credentials().expect("clear credentials");
        assert!(!super::save_oauth_credentials_if_revision(&OAuthTokenSet { access_token: "late-refresh".into(), refresh_token: None, expires_at: None, scopes: vec![] }, before_logout).unwrap());
        assert_eq!(load_oauth_credentials().expect("load cleared"), None);
        let cleared = std::fs::read_to_string(&path).expect("read cleared file");
        assert!(cleared.contains("\"other\": \"value\""));
        assert!(!cleared.contains("\"oauth\""));

        std::fs::remove_dir_all(config_home).expect("cleanup temp dir");
    }

    /// 回归（P-08/I2）：I2 那个作用域必须做到「**提前返回**也恢复三态」，
    /// 且恢复之后生产读取路径 `credentials_path()` 不得再指向本用例的临时目录。
    ///
    /// 判别性：改前是手写 `set_var` + 末尾 `remove_var`；下面 `probe` 里的提前返回
    /// **走不到**末尾清理语句，于是"恢复后"的对照断言会失败。
    /// 对照用"进入作用域前"的实测值，因此本机环境本来就设了 CLAW_CONFIG_HOME / CLAW_PROFILE 也不会误判。
    #[test]
    fn oauth_config_home_restores_on_early_return_and_path_is_clean() {
        fn probe(config_home: &std::path::Path) -> std::path::PathBuf {
            let _env = ScopedEnv::new(vec![env_set("CLAW_CONFIG_HOME", config_home)]);
            let path = credentials_path().expect("credentials path");
            // 作用域内确实被重定向了 —— 否则下面的"恢复后"断言没有判别力。
            assert!(
                path.starts_with(config_home),
                "作用域内 credentials_path 必须落在临时 config home 下，实际 {}",
                path.display()
            );
            // 提前返回（`return`，不是 panic）：末尾没有任何手写恢复语句可依赖。
            return path;
        }

        let config_home = temp_config_home();
        std::fs::create_dir_all(&config_home).expect("create temp config home");

        // 全程持本 crate 的统一锁（空目标 guard）：基线读与事后对照读都必须排他进行，
        // 否则并发的环境用例（CLAW_CONFIG_HOME / CLAW_PROFILE 都会影响 credentials_path）会让对照失真。
        let _lock = ScopedEnv::new(vec![]);

        let before = std::env::var_os("CLAW_CONFIG_HOME");
        let baseline_path = credentials_path().expect("baseline credentials path");

        let scoped_path = probe(&config_home);
        assert!(scoped_path.starts_with(&config_home));

        assert_eq!(
            std::env::var_os("CLAW_CONFIG_HOME"),
            before,
            "提前返回后 CLAW_CONFIG_HOME 必须三态还原"
        );
        assert_eq!(
            credentials_path().expect("credentials path after scope"),
            baseline_path,
            "恢复后生产读取路径不得再指向作用域内的临时目录"
        );

        std::fs::remove_dir_all(config_home).expect("cleanup temp dir");
    }

    #[test]
    fn parses_callback_query_and_target() {
        let params =
            parse_oauth_callback_query("code=abc123&state=state-1&error_description=needs%20login")
                .expect("parse query");
        assert_eq!(params.code.as_deref(), Some("abc123"));
        assert_eq!(params.state.as_deref(), Some("state-1"));
        assert_eq!(params.error_description.as_deref(), Some("needs login"));

        let params = parse_oauth_callback_request_target("/callback?code=abc&state=xyz")
            .expect("parse callback target");
        assert_eq!(params.code.as_deref(), Some("abc"));
        assert_eq!(params.state.as_deref(), Some("xyz"));
        assert!(parse_oauth_callback_request_target("/wrong?code=abc").is_err());
    }
}
