//! CLI 与桌面聊天室共用的插件配置根与相对路径解析。
use crate::{ConfigLoader, RuntimeConfig};
use plugins::{PluginManager, PluginManagerConfig};
use std::path::{Path, PathBuf};

pub fn plugin_manager_for_workspace(
    workspace: &Path,
    loader: &ConfigLoader,
    runtime_config: &RuntimeConfig,
) -> PluginManager {
    let settings = runtime_config.plugins();
    let mut config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    config.enabled_plugins = settings.enabled_plugins().clone();
    config.external_dirs = settings.external_directories().iter()
        .map(|path| resolve_plugin_path(workspace, loader.config_home(), path)).collect();
    config.install_root = settings.install_root()
        .map(|path| resolve_plugin_path(workspace, loader.config_home(), path));
    config.registry_path = settings.registry_path()
        .map(|path| resolve_plugin_path(workspace, loader.config_home(), path));
    config.bundled_root = settings.bundled_root()
        .map(|path| resolve_plugin_path(workspace, loader.config_home(), path));
    PluginManager::new(config)
}

fn resolve_plugin_path(workspace: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() { path }
    else if value.starts_with('.') { workspace.join(path) }
    else { config_home.join(path) }
}
