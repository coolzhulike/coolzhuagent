use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use api::provider_option;
use runtime::ConfigLoader;
use serde::{Deserialize, Serialize};

const DEFAULT_CHAT_MODEL: &str = "glm-free";
const DEFAULT_VISION_MODEL: &str = "glm-vision";
const DEFAULT_VISION_BACKEND: &str = "local-openai";
const DEFAULT_LOCAL_VISION_MODEL: &str = "qwen2.5-vl-3b";
const DEFAULT_LOCAL_VISION_BASE_URL: &str = "http://127.0.0.1:8001/v1";
const DEFAULT_PROVIDER: &str = "zhipu";
const DEFAULT_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";
const DEFAULT_MOUSE_INJECTION_BACKEND: &str = "sendinput";
const DEFAULT_DESKTOP_CAPTURE_ENABLED: bool = false;
const DEFAULT_CAPTURE_INTERVAL_MS: u64 = 1800;
const DEFAULT_CAPTURE_CACHE_LIMIT: usize = 12;
const DEFAULT_CHAT_AGENT_TIMEOUT_SECONDS: u64 = 180;
const DEFAULT_VISION_AGENT_TIMEOUT_SECONDS: u64 = 180;
const DEFAULT_TOOL_TIMEOUT_SECONDS: u64 = 120;
const DEFAULT_INTERCEPTION_MOUSE_DEVICE_ID: u32 = 11;
const DEFAULT_INTERCEPTION_KEYBOARD_DEVICE_ID: u32 = 1;
const GUI_CONFIG_FILE_NAME: &str = "gui-settings.json";
const DEMO_CONFIG_FILE_NAME: &str = "CLAW_GUI_DEMO_CONFIG.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiConfig {
    pub chat_model: String,
    pub vision_model: String,
    pub vision_backend: String,
    pub local_vision_model: String,
    pub local_vision_base_url: String,
    pub local_vision_api_key: String,
    pub cloud_vision_fallback_enabled: bool,
    pub workspace: String,
    pub api_key_path: String,
    pub api_base_url: String,
    pub provider: String,
    pub agent_profile: String,
    pub context_engine: String,
    pub fast_mode: bool,
    pub mouse_injection_backend: String,
    pub interception_dll_path: String,
    pub interception_mouse_device_id: u32,
    pub interception_keyboard_device_id: u32,
    pub desktop_capture_enabled: bool,
    pub desktop_capture_interval_ms: u64,
    pub desktop_cache_limit: usize,
    pub chat_agent_timeout_seconds: u64,
    pub vision_agent_timeout_seconds: u64,
    pub tool_timeout_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct GuiConfigStore {
    cwd: PathBuf,
    path: PathBuf,
    config: GuiConfig,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            chat_model: DEFAULT_CHAT_MODEL.to_string(),
            vision_model: DEFAULT_VISION_MODEL.to_string(),
            vision_backend: DEFAULT_VISION_BACKEND.to_string(),
            local_vision_model: DEFAULT_LOCAL_VISION_MODEL.to_string(),
            local_vision_base_url: DEFAULT_LOCAL_VISION_BASE_URL.to_string(),
            local_vision_api_key: String::new(),
            cloud_vision_fallback_enabled: false,
            workspace: ".".to_string(),
            api_key_path: String::new(),
            api_base_url: DEFAULT_BASE_URL.to_string(),
            provider: DEFAULT_PROVIDER.to_string(),
            agent_profile: "default".to_string(),
            context_engine: "focused".to_string(),
            fast_mode: true,
            mouse_injection_backend: DEFAULT_MOUSE_INJECTION_BACKEND.to_string(),
            interception_dll_path: String::new(),
            interception_mouse_device_id: DEFAULT_INTERCEPTION_MOUSE_DEVICE_ID,
            interception_keyboard_device_id: DEFAULT_INTERCEPTION_KEYBOARD_DEVICE_ID,
            desktop_capture_enabled: DEFAULT_DESKTOP_CAPTURE_ENABLED,
            desktop_capture_interval_ms: DEFAULT_CAPTURE_INTERVAL_MS,
            desktop_cache_limit: DEFAULT_CAPTURE_CACHE_LIMIT,
            chat_agent_timeout_seconds: DEFAULT_CHAT_AGENT_TIMEOUT_SECONDS,
            vision_agent_timeout_seconds: DEFAULT_VISION_AGENT_TIMEOUT_SECONDS,
            tool_timeout_seconds: DEFAULT_TOOL_TIMEOUT_SECONDS,
        }
    }
}

impl GuiConfig {
    #[must_use]
    pub fn default_for(cwd: &Path) -> Self {
        let workspace_root = detect_workspace_root(cwd);
        let loader = ConfigLoader::default_for(cwd.to_path_buf());
        let runtime_config = loader.load().ok();
        let agent = runtime_config.as_ref().map(runtime::RuntimeConfig::agent);

        Self {
            chat_model: runtime_config
                .as_ref()
                .and_then(runtime::RuntimeConfig::model)
                .unwrap_or(DEFAULT_CHAT_MODEL)
                .to_string(),
            vision_model: DEFAULT_VISION_MODEL.to_string(),
            vision_backend: env::var("CLAW_VISION_BACKEND")
                .unwrap_or_else(|_| DEFAULT_VISION_BACKEND.to_string()),
            local_vision_model: env::var("CLAW_LOCAL_VISION_MODEL")
                .unwrap_or_else(|_| DEFAULT_LOCAL_VISION_MODEL.to_string()),
            local_vision_base_url: env::var("CLAW_LOCAL_VISION_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_LOCAL_VISION_BASE_URL.to_string()),
            local_vision_api_key: env::var("CLAW_LOCAL_VISION_API_KEY").unwrap_or_default(),
            cloud_vision_fallback_enabled: env::var("CLAW_CLOUD_VISION_FALLBACK").is_ok_and(
                |value| {
                    matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
                },
            ),
            workspace: workspace_root.display().to_string(),
            api_key_path: default_api_key_path()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            api_base_url: env::var("ZAI_BASE_URL")
                .or_else(|_| env::var("BIGMODEL_BASE_URL"))
                .or_else(|_| env::var("OPENAI_BASE_URL"))
                .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string()),
            provider: env::var("PROVIDER").unwrap_or_else(|_| DEFAULT_PROVIDER.to_string()),
            agent_profile: agent
                .map(|value| value.profile().to_string())
                .unwrap_or_else(runtime::active_profile_name),
            context_engine: agent
                .map(|value| value.context_engine().as_str().to_string())
                .unwrap_or_else(|| "focused".to_string()),
            fast_mode: agent.is_none_or(runtime::RuntimeAgentConfig::fast_mode),
            mouse_injection_backend: env::var("CLAW_MOUSE_BACKEND")
                .unwrap_or_else(|_| DEFAULT_MOUSE_INJECTION_BACKEND.to_string()),
            interception_dll_path: env::var("CLAW_INTERCEPTION_DLL_PATH").unwrap_or_default(),
            interception_mouse_device_id: env::var("CLAW_INTERCEPTION_MOUSE_DEVICE_ID")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(DEFAULT_INTERCEPTION_MOUSE_DEVICE_ID),
            interception_keyboard_device_id: env::var("CLAW_INTERCEPTION_KEYBOARD_DEVICE_ID")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(DEFAULT_INTERCEPTION_KEYBOARD_DEVICE_ID),
            desktop_capture_enabled: DEFAULT_DESKTOP_CAPTURE_ENABLED,
            desktop_capture_interval_ms: DEFAULT_CAPTURE_INTERVAL_MS,
            desktop_cache_limit: DEFAULT_CAPTURE_CACHE_LIMIT,
            chat_agent_timeout_seconds: DEFAULT_CHAT_AGENT_TIMEOUT_SECONDS,
            vision_agent_timeout_seconds: DEFAULT_VISION_AGENT_TIMEOUT_SECONDS,
            tool_timeout_seconds: DEFAULT_TOOL_TIMEOUT_SECONDS,
        }
        .sanitized()
    }

    #[must_use]
    pub fn sanitized(&self) -> Self {
        let mut sanitized = self.clone();
        trim_to_default(&mut sanitized.chat_model, DEFAULT_CHAT_MODEL);
        trim_to_default(&mut sanitized.vision_model, DEFAULT_VISION_MODEL);
        sanitized.vision_backend = normalize_vision_backend(&sanitized.vision_backend);
        trim_to_default(
            &mut sanitized.local_vision_model,
            DEFAULT_LOCAL_VISION_MODEL,
        );
        trim_to_default(
            &mut sanitized.local_vision_base_url,
            DEFAULT_LOCAL_VISION_BASE_URL,
        );
        sanitized.local_vision_api_key = sanitized.local_vision_api_key.trim().to_string();
        trim_to_default(&mut sanitized.workspace, ".");
        trim_to_default(&mut sanitized.api_base_url, DEFAULT_BASE_URL);
        trim_to_default(&mut sanitized.provider, DEFAULT_PROVIDER);
        trim_to_default(&mut sanitized.agent_profile, "default");
        sanitized.context_engine = normalize_context_engine(&sanitized.context_engine);
        sanitized.api_key_path = sanitized.api_key_path.trim().to_string();
        sanitized.mouse_injection_backend =
            normalize_mouse_injection_backend(&sanitized.mouse_injection_backend);
        sanitized.interception_dll_path = sanitized.interception_dll_path.trim().to_string();
        sanitized.interception_mouse_device_id =
            sanitized.interception_mouse_device_id.clamp(11, 20);
        sanitized.interception_keyboard_device_id =
            sanitized.interception_keyboard_device_id.clamp(1, 10);
        sanitized.desktop_capture_interval_ms =
            sanitized.desktop_capture_interval_ms.clamp(500, 15_000);
        sanitized.desktop_cache_limit = sanitized.desktop_cache_limit.clamp(2, 64);
        sanitized.chat_agent_timeout_seconds = sanitized.chat_agent_timeout_seconds.clamp(5, 900);
        sanitized.vision_agent_timeout_seconds =
            sanitized.vision_agent_timeout_seconds.clamp(5, 900);
        sanitized.tool_timeout_seconds = sanitized.tool_timeout_seconds.clamp(10, 900);
        sanitized
    }

    #[must_use]
    pub fn workspace_path(&self) -> PathBuf {
        let trimmed = self.workspace.trim();
        if trimmed.is_empty() {
            detect_workspace_root(&env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        } else {
            PathBuf::from(trimmed)
        }
    }

    #[must_use]
    pub fn api_key_exists(&self) -> bool {
        let trimmed = self.api_key_path.trim();
        !trimmed.is_empty() && Path::new(trimmed).is_file()
    }

    pub fn apply_process_env(&self) {
        let config = self.sanitized();

        if let Some(api_key) = config
            .api_key_exists()
            .then(|| fs::read_to_string(config.api_key_path.trim()).ok())
            .flatten()
            .map(|contents| contents.trim().to_string())
            .filter(|api_key| !api_key.is_empty())
        {
            for env_name in api_key_envs_for_provider(&config.provider) {
                env::set_var(env_name, &api_key);
            }
        }

        if !config.api_base_url.is_empty() {
            for env_name in base_url_envs_for_provider(&config.provider) {
                env::set_var(env_name, &config.api_base_url);
            }
            env::set_var("API_BASE_URL", &config.api_base_url);
        }

        if !config.provider.is_empty() {
            env::set_var("PROVIDER", &config.provider);
        }

        env::set_var("CLAW_VISION_BACKEND", &config.vision_backend);
        env::set_var("CLAW_LOCAL_VISION_MODEL", &config.local_vision_model);
        env::set_var("CLAW_LOCAL_VISION_BASE_URL", &config.local_vision_base_url);
        env::set_var(
            "CLAW_CLOUD_VISION_FALLBACK",
            if config.cloud_vision_fallback_enabled {
                "true"
            } else {
                "false"
            },
        );
        if config.local_vision_api_key.is_empty() {
            env::remove_var("CLAW_LOCAL_VISION_API_KEY");
        } else {
            env::set_var("CLAW_LOCAL_VISION_API_KEY", &config.local_vision_api_key);
        }

        if !config.agent_profile.is_empty() {
            env::set_var("CLAW_PROFILE", &config.agent_profile);
        }

        if !config.context_engine.is_empty() {
            env::set_var("CLAW_CONTEXT_ENGINE", &config.context_engine);
        }

        env::set_var(
            "CLAW_FAST_MODE",
            if config.fast_mode { "true" } else { "false" },
        );
        env::set_var("CLAW_MOUSE_BACKEND", &config.mouse_injection_backend);
        env::set_var(
            "CLAW_INTERCEPTION_MOUSE_DEVICE_ID",
            config.interception_mouse_device_id.to_string(),
        );
        env::set_var(
            "CLAW_INTERCEPTION_KEYBOARD_DEVICE_ID",
            config.interception_keyboard_device_id.to_string(),
        );
        if !config.interception_dll_path.is_empty() {
            env::set_var("CLAW_INTERCEPTION_DLL_PATH", &config.interception_dll_path);
        } else {
            env::remove_var("CLAW_INTERCEPTION_DLL_PATH");
        }
    }
}

impl GuiConfigStore {
    pub fn load_or_create(cwd: impl Into<PathBuf>) -> Result<Self, String> {
        let cwd = cwd.into();
        let path = default_gui_config_path(&cwd);
        if path.exists() {
            let config = load_gui_config_from_path(&path)?;
            Ok(Self { cwd, path, config })
        } else {
            let config = GuiConfig::default_for(&cwd);
            let store = Self { cwd, path, config };
            store.save()?;
            Ok(store)
        }
    }

    #[must_use]
    pub fn fallback(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let path = default_gui_config_path(&cwd);
        let config = GuiConfig::default_for(&cwd);
        Self { cwd, path, config }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn config(&self) -> &GuiConfig {
        &self.config
    }

    pub fn replace(&mut self, config: GuiConfig) {
        self.config = config.sanitized();
    }

    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        if self.path.exists() {
            self.config = load_gui_config_from_path(&self.path)?;
        } else {
            self.config = GuiConfig::default_for(&self.cwd);
            self.save()?;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        write_gui_config_to_path(&self.path, &self.config)
    }
}

#[must_use]
pub fn desktop_demo_config_path(cwd: &Path) -> PathBuf {
    desktop_dir()
        .unwrap_or_else(|| cwd.to_path_buf())
        .join(DEMO_CONFIG_FILE_NAME)
}

pub fn load_gui_config_from_path(path: &Path) -> Result<GuiConfig, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read GUI config {}: {error}", path.display()))?;
    let config = serde_json::from_str::<GuiConfig>(&content)
        .map_err(|error| format!("failed to parse GUI config {}: {error}", path.display()))?;
    Ok(config.sanitized())
}

pub fn write_gui_config_to_path(path: &Path, config: &GuiConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create GUI config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let content = serde_json::to_string_pretty(&config.sanitized())
        .map_err(|error| format!("failed to serialize GUI config: {error}"))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| format!("failed to write GUI config {}: {error}", path.display()))
}

fn default_gui_config_path(cwd: &Path) -> PathBuf {
    let loader = ConfigLoader::default_for(cwd.to_path_buf());
    loader.config_home().join(GUI_CONFIG_FILE_NAME)
}

fn default_api_key_path() -> Option<PathBuf> {
    env::var("COOLZHU_API_KEY_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|root| root.join("api-key.txt")))
}

fn desktop_dir() -> Option<PathBuf> {
    home_dir().map(|root| root.join("Desktop"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn trim_to_default(value: &mut String, default: &str) {
    let trimmed = value.trim();
    *value = if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    };
}

fn normalize_context_engine(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "minimal" => "minimal".to_string(),
        "full" => "full".to_string(),
        _ => "focused".to_string(),
    }
}

fn normalize_mouse_injection_backend(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "interception" => "interception".to_string(),
        _ => "sendinput".to_string(),
    }
}

fn normalize_vision_backend(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "zhipu" | "glm" | "cloud" | "remote" => "zhipu".to_string(),
        _ => DEFAULT_VISION_BACKEND.to_string(),
    }
}

fn api_key_envs_for_provider(provider: &str) -> &'static [&'static str] {
    match normalize_provider_slug(provider).as_str() {
        "xai" => &["XAI_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "aliyun" | "dashscope" | "qwen" => &["DASHSCOPE_API_KEY"],
        "baidu" | "qianfan" | "ernie" => &["QIANFAN_API_KEY"],
        "bytedance" | "ark" | "doubao" => &["ARK_API_KEY"],
        "deepseek" => &["DEEPSEEK_API_KEY"],
        _ => &["ZAI_API_KEY", "BIGMODEL_API_KEY"],
    }
}

fn base_url_envs_for_provider(provider: &str) -> &'static [&'static str] {
    match normalize_provider_slug(provider).as_str() {
        "xai" => &["XAI_BASE_URL"],
        "openai" => &["OPENAI_BASE_URL"],
        "aliyun" | "dashscope" | "qwen" => &["DASHSCOPE_BASE_URL"],
        "baidu" | "qianfan" | "ernie" => &["QIANFAN_BASE_URL"],
        "bytedance" | "ark" | "doubao" => &["ARK_BASE_URL"],
        "deepseek" => &["DEEPSEEK_BASE_URL"],
        _ => &["ZAI_BASE_URL", "BIGMODEL_BASE_URL"],
    }
}

fn normalize_provider_slug(value: &str) -> String {
    provider_option(value)
        .map(|option| option.slug.to_string())
        .unwrap_or_else(|| value.trim().to_ascii_lowercase())
}

fn detect_workspace_root(start: &Path) -> PathBuf {
    start
        .ancestors()
        .find(|candidate| is_workspace_root(candidate))
        .map_or_else(|| start.to_path_buf(), Path::to_path_buf)
}

fn is_workspace_root(candidate: &Path) -> bool {
    candidate.join("Cargo.toml").is_file() && candidate.join("crates").is_dir()
}
