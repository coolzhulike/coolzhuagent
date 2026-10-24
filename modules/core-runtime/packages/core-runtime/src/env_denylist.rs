//! 子进程环境变量 deny-list。
//!
//! 模型驱动的 `execute_bash` / 授权 hook 子进程默认继承宿主全部环境变量，
//! 其中包含凭据（`*_API_KEY`/`*_TOKEN`/`*_SECRET` 等）与加载器注入变量
//! （`LD_PRELOAD`/`NODE_OPTIONS`/`PYTHONSTARTUP` 等）。
//! 注入变量可被子进程利用绕过沙箱，凭据变量会以进程 env 形式泄露给任意子进程
//! （`/proc/<pid>/environ`、进程枚举、child 再外发）。
//!
//! 对齐 Claude Code `bash.ts` env deny-list：在 `Command` spawn 前调用
//! [`sanitize_command_env`] 移除命中项；进程内仍保留这些变量（宿主自身需要），
//! 只是不再传给模型可影响的子进程。
//!
//! 有意偏差（如实记录）：
//! - 代理变量 `HTTP_PROXY/HTTPS_PROXY/ALL_PROXY/NO_PROXY` **保留**——本机可能
//!   依赖代理访问网络（cargo/git/web_fetch），剥离会破坏合法命令；泄露风险
//!   低于可用性损失。
//! - `Path`/`PATHEXT`/`SystemRoot`/`COMSPEC`/`PSModulePath`/`HOME`/`USERPROFILE`/
//!   `TEMP/TMP` 等系统必需变量保留，否则子进程无法正常解析/启动。
//! - `COOLZHU_*` 运行时变量（如 `COOLZHU_RUNTIME_DIR`）保留——子进程需要定位
//!   会话数据；其不含密钥材料。

/// 精确匹配的变量名（大小写不敏感）。
const DENIED_NAMES: &[&str] = &[
    // ── 加载器/运行时注入（劫持子进程执行）──
    "BASH_ENV",
    "ENV",
    "CDPATH",
    "IFS",
    "PS4",
    "GLOBIGNORE",
    "SHELLOPTS",
    "BASHOPTS",
    "PROMPT_COMMAND",
    "INPUTRC",
    "ZDOTDIR",
    "NODE_OPTIONS",
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "PYTHONINSPECT",
    "PYTHONBREAKPOINT",
    "PYTHONUSERBASE",
    "RUBYOPT",
    "RUBYLIB",
    "BUNDLE_GEMFILE",
    "JAVA_TOOL_OPTIONS",
    "JDK_JAVA_OPTIONS",
    "_JAVA_OPTIONS",
    "DOTNET_STARTUP_HOOKS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "GOFLAGS",
    "GIT_CONFIG_COUNT", // git -c 注入链：GIT_CONFIG_COUNT/KEY_n/VALUE_n
    "GIT_CONFIG_PARAMETERS",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "SSH_ASKPASS_REQUIRE",
    // ── 凭据/会话材料（泄露给任意子进程）──
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    "DOCKER_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GITLAB_TOKEN",
    "GITLAB_PRIVATE_TOKEN",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_CREDENTIAL_FILE",
    "AZURE_CLIENT_SECRET",
    "NPM_TOKEN",
    "PYPI_TOKEN",
    "CARGO_REGISTRY_TOKEN",
    "NUGET_API_KEY",
    "VAULT_TOKEN",
    "DATABASE_URL",
    "DB_PASSWORD",
    "KUBECONFIG",
    "GNUPGHOME",
    "PASSWORD",
];

/// 前缀匹配（大小写不敏感）——整条命名空间都是注入面。
const DENIED_PREFIXES: &[&str] = &[
    "LD_",           // LD_PRELOAD/LD_LIBRARY_PATH/LD_AUDIT/LD_DEBUG/…
    "DYLD_",         // macOS dyld 注入
    "PERL5",         // PERL5OPT/PERL5LIB/PERL5DB
    "BASH_FUNC_",    // shellshock 导出的函数定义
    "GIT_CONFIG_KEY_",
    "GIT_CONFIG_VALUE_",
    "GIT_SSH_",      // GIT_SSH_COMMAND/GIT_SSH_VARIANT
];

/// 后缀匹配（大小写不敏感）——凭据命名惯例。
const DENIED_SUFFIXES: &[&str] = &[
    "_API_KEY",
    "_APIKEY",
    "_SECRET",
    "_SECRET_KEY",
    "_TOKEN",
    "_PASSWORD",
    "_PASSWD",
    "_PRIVATE_KEY",
    "_CREDENTIALS",
    "_ACCESS_KEY",
    "_SESSION_KEY",
    "_AUTH_TOKEN",
];

/// 变量名是否命中 deny-list。
#[must_use]
pub fn is_denied_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    DENIED_NAMES.iter().any(|denied| upper == *denied)
        || DENIED_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix))
        || DENIED_SUFFIXES
            .iter()
            .any(|suffix| upper.ends_with(suffix))
}

/// 对将要 spawn 的 `Command` 应用 deny-list：逐一 env_remove。
/// 必须在所有 `env()` 显式设置（如 HOOK_*）**之前**调用，调用方随后可自行覆盖。
pub fn sanitize_command_env(command: &mut std::process::Command) {
    // Command 无法枚举其待继承环境；对完整宿主 env 扫描并移除命中项。
    for (name, _) in std::env::vars_os() {
        if name
            .to_str()
            .is_some_and(|name| is_denied_env_name(name))
        {
            command.env_remove(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_denied_env_name;

    #[test]
    fn denies_injection_and_credentials() {
        for name in [
            "LD_PRELOAD",
            "ld_library_path",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "PYTHONSTARTUP",
            "BASH_ENV",
            "PROMPT_COMMAND",
            "PERL5OPT",
            "GIT_CONFIG_KEY_0",
            "GIT_SSH_COMMAND",
            "SSH_AUTH_SOCK",
            "GITHUB_TOKEN",
            "OPENAI_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "CUSTOM_MODEL_API_KEY",
            "MY_SERVICE_SESSION_TOKEN",
            "TEAM_PRIVATE_KEY",
            "DATABASE_URL",
        ] {
            assert!(is_denied_env_name(name), "{name} should be denied");
        }
    }

    #[test]
    fn allows_required_runtime_variables() {
        for name in [
            "Path",
            "PATH",
            "PATHEXT",
            "SystemRoot",
            "COMSPEC",
            "PSModulePath",
            "HOME",
            "USERPROFILE",
            "TEMP",
            "TMP",
            "LANG",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "COOLZHU_RUNTIME_DIR",
            "RUST_BACKTRACE",
            "CARGO",
            "RUSTUP_HOME",
            "USERNAME",
        ] {
            assert!(!is_denied_env_name(name), "{name} should be allowed");
        }
    }
}
