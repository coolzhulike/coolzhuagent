use api::{ProviderClient, ProviderKind};

#[test]
fn alibaba_bailian_provider_kind() {
    assert_eq!(ProviderKind::AlibabaBailian.id(), "alibaba-bailian");
}

#[test]
fn provider_client_routes_qwen_through_alibaba_bailian() {
    let _lock = env_lock();
    let _dashscope_key = EnvVarGuard::set("DASHSCOPE_API_KEY", Some("dashscope-test-key"));

    let client = ProviderClient::from_model("qwen-plus").expect("qwen should resolve");

    assert_eq!(client.provider_kind(), ProviderKind::AlibabaBailian);
}

#[test]
fn provider_client_routes_glm_51_through_alibaba_china() {
    let _lock = env_lock();
    let _dashscope_key = EnvVarGuard::set("DASHSCOPE_API_KEY", Some("dashscope-test-key"));

    let client = ProviderClient::from_model("glm-5.1").expect("Bailian GLM-5.1 should resolve");

    assert_eq!(client.provider_kind(), ProviderKind::AlibabaBailian);
}

#[test]
fn provider_client_reports_missing_dashscope_credentials() {
    let _lock = env_lock();
    let _dashscope_key = EnvVarGuard::set("DASHSCOPE_API_KEY", None);

    let error = ProviderClient::from_model("qwen-plus")
        .expect_err("qwen requests without DASHSCOPE_API_KEY should fail");

    match error {
        api::ApiError::MissingCredentials { provider, env_vars } => {
            assert_eq!(provider, "AlibabaBailian");
            let env_vars_str: Vec<&str> = env_vars.to_vec();
            assert!(env_vars_str.contains(&"DASHSCOPE_API_KEY"));
        }
        other => panic!("expected missing DashScope credentials, got {other:?}"),
    }
}

#[test]
fn anthropic_provider_kind() {
    assert_eq!(ProviderKind::Anthropic.id(), "anthropic");
}

#[test]
fn custom_provider_kind() {
    assert_eq!(ProviderKind::Custom.id(), "custom");
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let original = std::env::var_os(key);
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}
