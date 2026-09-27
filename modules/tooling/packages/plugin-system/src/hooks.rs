use std::ffi::OsStr;
use std::process::Command;

use serde_json::json;

use crate::{PluginError, PluginHooks, PluginRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
}

impl HookEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRunResult {
    denied: bool,
    messages: Vec<String>,
    /// 因**未授权**而未运行的 hook 数（0 = 全部都按授权判定后运行了）。
    ///
    /// 为什么要有它：`is_denied() == false` 以前既可能是"hook 允许"，也可能是"hook 根本没跑"。
    /// 把两者用一个布尔混起来，等于把"没跑"悄悄读成"允许"——这正是 P2-2 要消除的歧义。
    unauthorized_skips: usize,
}

impl HookRunResult {
    #[must_use]
    pub fn allow(messages: Vec<String>) -> Self {
        Self {
            denied: false,
            messages,
            unauthorized_skips: 0,
        }
    }

    /// 因未授权而未运行的 hook 数（`false` 语义不再与"允许"混淆）。
    #[must_use]
    pub const fn unauthorized_skips(&self) -> usize {
        self.unauthorized_skips
    }

    #[must_use]
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

/// **Hook 授权服务**（P2-2）：决定"这一次 hook 是否被允许运行"。
///
/// 为什么必须与"配置"分开：`PluginHooks` 里写了 hook，只说明**它想运行**（配置），
/// 不等于**宿主允许它运行**（授权）。插件是第三方内容，它的脚本能看到工具参数、
/// 甚至阻断调用——若"配置了就生效"，等于让插件配置本身成为权限来源。
///
/// 口径与运行时侧 `runtime::hooks::HookRunner` **一致**：
/// **默认什么都不授权**（fail-closed），宿主显式给出授权后 hook 才会运行；
/// 且插件自己的 `allow` 不构成对宿主拒绝的覆盖（宿主权限门禁独立生效）。
pub trait HookAuthorizationService: Send + Sync {
    /// 是否允许该事件／该工具上的 hook 运行。
    fn authorize(&self, event: HookEvent, tool_name: &str) -> bool;
}

/// 默认授权服务：**什么都不授权**。
///
/// 这不是"留个开关方便打开"，而是首期的正确默认——没有独立授权的外部 hook 一律不运行。
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHookAuthorization;

impl HookAuthorizationService for NoHookAuthorization {
    fn authorize(&self, _event: HookEvent, _tool_name: &str) -> bool {
        false
    }
}

/// 显式允许指定事件（其它事件仍不授权）。供宿主把"已授权的范围"写清楚。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllowHookEvents {
    pre_tool_use: bool,
    post_tool_use: bool,
}

impl AllowHookEvents {
    /// 允许 `PreToolUse` 上的 hook 运行。
    #[must_use]
    pub const fn allow_pre_tool_use(mut self) -> Self {
        self.pre_tool_use = true;
        self
    }

    /// 允许 `PostToolUse` 上的 hook 运行。
    #[must_use]
    pub const fn allow_post_tool_use(mut self) -> Self {
        self.post_tool_use = true;
        self
    }
}

impl HookAuthorizationService for AllowHookEvents {
    fn authorize(&self, event: HookEvent, _tool_name: &str) -> bool {
        match event {
            HookEvent::PreToolUse => self.pre_tool_use,
            HookEvent::PostToolUse => self.post_tool_use,
        }
    }
}

#[derive(Clone)]
pub struct HookRunner {
    hooks: PluginHooks,
    /// hook 的执行授权。缺省为"什么都不授权"（见 [`NoHookAuthorization`]）。
    ///
    /// 用 `Arc<dyn ...>` 而不是泛型：本类型要保持 `Clone`（既有调用依赖），
    /// 授权服务因此按**共享对象**存放；`Debug`/`PartialEq` 手写（见下），
    /// 相等性按"**同一个授权对象**"判定，而不是按内容——授权是外部策略，
    /// 内容相等无法代表"授权相同"。
    authorization: std::sync::Arc<dyn HookAuthorizationService>,
}

impl std::fmt::Debug for HookRunner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HookRunner")
            .field("pre_tool_use", &self.hooks.pre_tool_use.len())
            .field("post_tool_use", &self.hooks.post_tool_use.len())
            .field("authorization", &"<authorization service>")
            .finish()
    }
}

impl PartialEq for HookRunner {
    fn eq(&self, other: &Self) -> bool {
        // 配置按值比较；授权按**同一对象**比较（外部策略，内容相等不代表授权相同）。
        self.hooks == other.hooks
            && std::sync::Arc::ptr_eq(&self.authorization, &other.authorization)
    }
}

impl Eq for HookRunner {}

impl HookRunner {
    #[must_use]
    pub fn new(hooks: PluginHooks) -> Self {
        Self {
            hooks,
            authorization: std::sync::Arc::new(NoHookAuthorization),
        }
    }

    /// 显式授予 hook 执行授权。**未调用它之前，任何 hook 都不会运行**。
    #[must_use]
    pub fn with_authorization(
        mut self,
        authorization: impl HookAuthorizationService + 'static,
    ) -> Self {
        self.authorization = std::sync::Arc::new(authorization);
        self
    }

    pub fn from_registry(plugin_registry: &PluginRegistry) -> Result<Self, PluginError> {
        Ok(Self::new(plugin_registry.aggregated_hooks()?))
    }

    #[must_use]
    pub fn run_pre_tool_use(&self, tool_name: &str, tool_input: &str) -> HookRunResult {
        self.run_commands(
            HookEvent::PreToolUse,
            &self.hooks.pre_tool_use,
            tool_name,
            tool_input,
            None,
            false,
        )
    }

    #[must_use]
    pub fn run_post_tool_use(
        &self,
        tool_name: &str,
        tool_input: &str,
        tool_output: &str,
        is_error: bool,
    ) -> HookRunResult {
        self.run_commands(
            HookEvent::PostToolUse,
            &self.hooks.post_tool_use,
            tool_name,
            tool_input,
            Some(tool_output),
            is_error,
        )
    }

    fn run_commands(
        &self,
        event: HookEvent,
        commands: &[String],
        tool_name: &str,
        tool_input: &str,
        tool_output: Option<&str>,
        is_error: bool,
    ) -> HookRunResult {
        if commands.is_empty() {
            return HookRunResult::allow(Vec::new());
        }
        // 注意：`commands` 非空但**未授权**时，下面逐条跳过并如实记数（不是 allow）。

        let payload = json!({
            "hook_event_name": event.as_str(),
            "tool_name": tool_name,
            "tool_input": parse_tool_input(tool_input),
            "tool_input_json": tool_input,
            "tool_output": tool_output,
            "tool_result_is_error": is_error,
        })
        .to_string();

        let mut messages = Vec::new();
        let mut unauthorized_skips = 0usize;

        for command in commands {
            // **配置 ≠ 授权**：没被授权就不是"运行后发现被拒"，而是**根本不运行**。
            // 如实记数并留一条消息，绝不把它算成 allow（那会把"没跑"读成"允许"）。
            if !self.authorization.authorize(event, tool_name) {
                unauthorized_skips += 1;
                messages.push(format!(
                    "{} hook `{command}` 未获授权，未运行（配置 ≠ 授权；需宿主显式授权）",
                    event.as_str()
                ));
                continue;
            }
            match self.run_command(
                command,
                event,
                tool_name,
                tool_input,
                tool_output,
                is_error,
                &payload,
            ) {
                HookCommandOutcome::Allow { message } => {
                    if let Some(message) = message {
                        messages.push(message);
                    }
                }
                HookCommandOutcome::Deny { message } => {
                    messages.push(message.unwrap_or_else(|| {
                        format!("{} hook denied tool `{tool_name}`", event.as_str())
                    }));
                    return HookRunResult {
                        denied: true,
                        messages,
                        unauthorized_skips,
                    };
                }
                HookCommandOutcome::Warn { message } => messages.push(message),
            }
        }

        HookRunResult {
            denied: false,
            messages,
            unauthorized_skips,
        }
    }

    #[allow(clippy::too_many_arguments, clippy::unused_self)]
    fn run_command(
        &self,
        command: &str,
        event: HookEvent,
        tool_name: &str,
        tool_input: &str,
        tool_output: Option<&str>,
        is_error: bool,
        payload: &str,
    ) -> HookCommandOutcome {
        let mut child = shell_command(command);
        child.stdin(std::process::Stdio::piped());
        child.stdout(std::process::Stdio::piped());
        child.stderr(std::process::Stdio::piped());
        child.env("HOOK_EVENT", event.as_str());
        child.env("HOOK_TOOL_NAME", tool_name);
        child.env("HOOK_TOOL_INPUT", tool_input);
        child.env("HOOK_TOOL_IS_ERROR", if is_error { "1" } else { "0" });
        if let Some(tool_output) = tool_output {
            child.env("HOOK_TOOL_OUTPUT", tool_output);
        }

        match child.output_with_stdin(payload.as_bytes()) {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let message = (!stdout.is_empty()).then_some(stdout);
                match output.status.code() {
                    Some(0) => HookCommandOutcome::Allow { message },
                    Some(2) => HookCommandOutcome::Deny { message },
                    Some(code) => HookCommandOutcome::Warn {
                        message: format_hook_warning(
                            command,
                            code,
                            message.as_deref(),
                            stderr.as_str(),
                        ),
                    },
                    None => HookCommandOutcome::Warn {
                        message: format!(
                            "{} hook `{command}` terminated by signal while handling `{tool_name}`",
                            event.as_str()
                        ),
                    },
                }
            }
            Err(error) => HookCommandOutcome::Warn {
                message: format!(
                    "{} hook `{command}` failed to start for `{tool_name}`: {error}",
                    event.as_str()
                ),
            },
        }
    }
}

enum HookCommandOutcome {
    Allow { message: Option<String> },
    Deny { message: Option<String> },
    Warn { message: String },
}

fn parse_tool_input(tool_input: &str) -> serde_json::Value {
    serde_json::from_str(tool_input).unwrap_or_else(|_| json!({ "raw": tool_input }))
}

fn format_hook_warning(command: &str, code: i32, stdout: Option<&str>, stderr: &str) -> String {
    let mut message =
        format!("Hook `{command}` exited with status {code}; allowing tool execution to continue");
    if let Some(stdout) = stdout.filter(|stdout| !stdout.is_empty()) {
        message.push_str(": ");
        message.push_str(stdout);
    } else if !stderr.is_empty() {
        message.push_str(": ");
        message.push_str(stderr);
    }
    message
}

fn shell_command(command: &str) -> CommandWithStdin {
    #[cfg(windows)]
    let command_builder = {
        let mut command_builder = Command::new("cmd");
        command_builder.arg("/C").arg(command);
        CommandWithStdin::new(command_builder)
    };

    #[cfg(not(windows))]
    let command_builder = if Path::new(command).exists() {
        let mut command_builder = Command::new("sh");
        command_builder.arg(command);
        CommandWithStdin::new(command_builder)
    } else {
        let mut command_builder = Command::new("sh");
        command_builder.arg("-lc").arg(command);
        CommandWithStdin::new(command_builder)
    };

    command_builder
}

struct CommandWithStdin {
    command: Command,
}

impl CommandWithStdin {
    fn new(command: Command) -> Self {
        Self { command }
    }

    fn stdin(&mut self, cfg: std::process::Stdio) -> &mut Self {
        self.command.stdin(cfg);
        self
    }

    fn stdout(&mut self, cfg: std::process::Stdio) -> &mut Self {
        self.command.stdout(cfg);
        self
    }

    fn stderr(&mut self, cfg: std::process::Stdio) -> &mut Self {
        self.command.stderr(cfg);
        self
    }

    fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, value);
        self
    }

    fn output_with_stdin(&mut self, stdin: &[u8]) -> std::io::Result<std::process::Output> {
        let mut child = self.command.spawn()?;
        if let Some(mut child_stdin) = child.stdin.take() {
            use std::io::Write as _;
            child_stdin.write_all(stdin)?;
        }
        child.wait_with_output()
    }
}

#[cfg(test)]
mod tests {
    use super::{AllowHookEvents, HookAuthorizationService, HookRunResult, HookRunner};
    use crate::{PluginHooks, PluginManager, PluginManagerConfig};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("plugins-hook-runner-{label}-{nanos}"))
    }

    fn write_hook_plugin(root: &Path, name: &str, pre_message: &str, post_message: &str) {
        let extension = hook_script_extension();
        fs::create_dir_all(root.join(".claw-plugin")).expect("manifest dir");
        fs::create_dir_all(root.join("hooks")).expect("hooks dir");
        fs::write(
            root.join("hooks").join(format!("pre.{extension}")),
            hook_echo_script(pre_message),
        )
        .expect("write pre hook");
        fs::write(
            root.join("hooks").join(format!("post.{extension}")),
            hook_echo_script(post_message),
        )
        .expect("write post hook");
        fs::write(
            root.join(".claw-plugin").join("plugin.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"1.0.0\",\n  \"description\": \"hook plugin\",\n  \"hooks\": {{\n    \"PreToolUse\": [\"./hooks/pre.{extension}\"],\n    \"PostToolUse\": [\"./hooks/post.{extension}\"]\n  }}\n}}"
            ),
        )
        .expect("write plugin manifest");
    }

    /// **P2-2**：**配置 ≠ 授权**——没被授权时 hook **根本不运行**，且不得被读成「允许」。
    ///
    /// 与运行时侧 `runtime::hooks::HookRunner` 同口径：默认什么都不授权（fail-closed），
    /// 宿主显式授权后才运行；插件自己的脚本（哪怕它写的是 allow）也不构成授权。
    #[test]
    fn unconfigured_authorization_prevents_plugin_hooks_from_running() {
        // 用「会写文件的脚本」当证据：跑了就一定留下文件，没跑就一定没有。
        let directory = temp_dir("authorization");
        fs::create_dir_all(&directory).expect("work dir");
        let marker = directory.join("hook-ran.marker");
        let script = format!("echo ran > {}", marker.display());
        let hooks = PluginHooks {
            pre_tool_use: vec![script.clone()],
            post_tool_use: Vec::new(),
        };

        // ① 默认（未授权）：不运行，且结果里如实记数。
        let runner = HookRunner::new(hooks.clone());
        let result = runner.run_pre_tool_use("bash", "{}");
        assert!(
            !result.is_denied(),
            "未授权不是「被拒」，而是「没跑」——两者必须可分辨"
        );
        assert_eq!(result.unauthorized_skips(), 1);
        assert!(!marker.exists(), "未授权的 hook 绝不能运行（配置 ≠ 授权）");
        assert!(
            result
                .messages()
                .iter()
                .any(|message| message.contains("未获授权")),
            "必须留下可见说明，而不是静默通过：{:?}",
            result.messages()
        );

        // ② 显式授权该事件后才运行。
        let runner = HookRunner::new(hooks)
            .with_authorization(AllowHookEvents::default().allow_pre_tool_use());
        let result = runner.run_pre_tool_use("bash", "{}");
        assert_eq!(result.unauthorized_skips(), 0);
        assert!(marker.exists(), "获授权后 hook 必须真的运行");

        // ③ 只授权 PreToolUse 时，PostToolUse 仍不运行。
        let post_marker = directory.join("post-ran.marker");
        let hooks = PluginHooks {
            pre_tool_use: Vec::new(),
            post_tool_use: vec![format!("echo ran > {}", post_marker.display())],
        };
        let runner = HookRunner::new(hooks)
            .with_authorization(AllowHookEvents::default().allow_pre_tool_use());
        let result = runner.run_post_tool_use("bash", "{}", "ok", false);
        assert_eq!(result.unauthorized_skips(), 1);
        assert!(!post_marker.exists(), "未授权的事件不得运行");
    }

    #[test]
    fn collects_and_runs_hooks_from_enabled_plugins() {
        let config_home = temp_dir("config");
        let first_source_root = temp_dir("source-a");
        let second_source_root = temp_dir("source-b");
        write_hook_plugin(
            &first_source_root,
            "first",
            "plugin pre one",
            "plugin post one",
        );
        write_hook_plugin(
            &second_source_root,
            "second",
            "plugin pre two",
            "plugin post two",
        );

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager
            .install(first_source_root.to_str().expect("utf8 path"))
            .expect("first plugin install should succeed");
        manager
            .install(second_source_root.to_str().expect("utf8 path"))
            .expect("second plugin install should succeed");
        manager
            .enable("first@external")
            .expect("explicit enable first");
        manager
            .enable("second@external")
            .expect("explicit enable second");
        let registry = manager.plugin_registry().expect("registry should build");

        // P2-2：显式授权（未授权时 hook 不运行——本用例要测的正是"运行"）。
        let runner = HookRunner::from_registry(&registry)
            .expect("plugin hooks should load")
            .with_authorization(
                AllowHookEvents::default()
                    .allow_pre_tool_use()
                    .allow_post_tool_use(),
            );

        assert_eq!(
            runner.run_pre_tool_use("Read", r#"{"path":"README.md"}"#),
            HookRunResult::allow(vec![
                "plugin pre one".to_string(),
                "plugin pre two".to_string(),
            ])
        );
        assert_eq!(
            runner.run_post_tool_use("Read", r#"{"path":"README.md"}"#, "ok", false),
            HookRunResult::allow(vec![
                "plugin post one".to_string(),
                "plugin post two".to_string(),
            ])
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(first_source_root);
        let _ = fs::remove_dir_all(second_source_root);
    }

    #[test]
    fn pre_tool_use_denies_when_plugin_hook_exits_two() {
        // P2-2：本用例测的是"运行后拒绝"，因此必须**显式授权**——未授权时 hook 根本不运行。
        let runner = HookRunner::new(crate::PluginHooks {
            pre_tool_use: vec![hook_deny_script("blocked by plugin")],
            post_tool_use: Vec::new(),
        })
        .with_authorization(
            AllowHookEvents::default()
                .allow_pre_tool_use()
                .allow_post_tool_use(),
        );

        let result = runner.run_pre_tool_use("Bash", r#"{"command":"pwd"}"#);

        assert!(result.is_denied());
        assert_eq!(result.messages(), &["blocked by plugin".to_string()]);
    }

    #[cfg(windows)]
    fn hook_script_extension() -> &'static str {
        "cmd"
    }

    #[cfg(not(windows))]
    fn hook_script_extension() -> &'static str {
        "sh"
    }

    #[cfg(windows)]
    fn hook_echo_script(message: &str) -> String {
        format!("@echo off\r\necho {message}\r\n")
    }

    #[cfg(not(windows))]
    fn hook_echo_script(message: &str) -> String {
        format!("#!/bin/sh\nprintf '%s\\n' '{message}'\n")
    }

    #[cfg(windows)]
    fn hook_deny_script(message: &str) -> String {
        format!("echo {message} && exit /b 2")
    }

    #[cfg(not(windows))]
    fn hook_deny_script(message: &str) -> String {
        format!("printf '{message}'; exit 2")
    }
}
