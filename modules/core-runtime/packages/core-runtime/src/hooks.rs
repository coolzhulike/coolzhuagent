use std::ffi::OsStr;
use std::process::Command;

use serde_json::json;

use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
use crate::permissions::{PermissionMode, PermissionOutcome, PermissionPolicy};

/// pre-tool hook 作为独立"能力"参与权限判定时使用的名字（§7.3 独立执行授权）。
///
/// 复用 `PermissionPolicy` 的能力命名空间，但使用独立实例：hook 的授权与目标
/// 工具的授权互不污染——目标工具被放行，不代表 hook 也被放行。
pub const HOOK_CAPABILITY_PRE_TOOL_USE: &str = "hook:PreToolUse";

/// post-tool hook 的能力名。与 pre hook 分开声明，遵循最小权限。
pub const HOOK_CAPABILITY_POST_TOOL_USE: &str = "hook:PostToolUse";

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

    /// 该事件对应的独立授权能力名。
    fn capability(self) -> &'static str {
        match self {
            Self::PreToolUse => HOOK_CAPABILITY_PRE_TOOL_USE,
            Self::PostToolUse => HOOK_CAPABILITY_POST_TOOL_USE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRunResult {
    denied: bool,
    executed_commands: usize,
    messages: Vec<String>,
}

impl HookRunResult {
    #[must_use]
    pub fn allow(messages: Vec<String>) -> Self {
        Self {
            denied: false,
            executed_commands: 0,
            messages,
        }
    }

    #[must_use]
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// 本次真正发起运行的 hook 命令数。
    ///
    /// `0` 表示**没有任何 hook 运行**：既可能是没配置，也可能是没有独立执行
    /// 授权。宿主据此决定"hook 之后是否必须重新判定审批"（hook 没跑 ⇒ 中间没有
    /// 任何东西改变过，原审批仍成立）。
    #[must_use]
    pub fn executed_commands(&self) -> usize {
        self.executed_commands
    }

    #[must_use]
    pub fn with_executed_commands(mut self, executed_commands: usize) -> Self {
        self.executed_commands = executed_commands;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRunner {
    config: RuntimeHookConfig,
    /// hook 自身的执行授权。缺省为"没有任何 hook 被授权"——见
    /// [`HookRunner::new`] / [`HookRunner::with_authorization`]。
    authorization: PermissionPolicy,
}

/// 首期默认授权：**没有任何 hook 拥有独立执行授权**。
///
/// 返回的策略不含任何 hook 能力声明，因此每次判定都会命中
/// `PermissionMode::Unspecified` 的 fail-closed 分支（配置错误），而不是回退成
/// 某个低权限档位被放行。这正是 §7.3 要求的"没有独立授权的外部 hook 一律不
/// 运行"，而不是给它加一个宽松开关。
fn unauthorized_policy() -> PermissionPolicy {
    PermissionPolicy::new(PermissionMode::ReadOnly)
}

impl Default for HookRunner {
    fn default() -> Self {
        Self {
            config: RuntimeHookConfig::default(),
            authorization: unauthorized_policy(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct HookCommandRequest<'a> {
    event: HookEvent,
    tool_name: &'a str,
    tool_input: &'a str,
    tool_output: Option<&'a str>,
    is_error: bool,
    payload: &'a str,
}

impl HookRunner {
    /// 用给定配置构造 runner；**不授予任何独立执行授权**，因此所有 hook 都不会
    /// 运行，直到宿主显式调用 [`HookRunner::with_authorization`]。
    #[must_use]
    pub fn new(config: RuntimeHookConfig) -> Self {
        Self {
            config,
            authorization: unauthorized_policy(),
        }
    }

    #[must_use]
    pub fn from_feature_config(feature_config: &RuntimeFeatureConfig) -> Self {
        Self::new(feature_config.hooks().clone())
    }

    /// 授予 hook 自身的执行授权。
    ///
    /// 调用方必须为每个要运行的能力**显式声明最低权限**。hook 是任意 shell，
    /// 因此诚实的最低权限是 `DangerFullAccess`：
    ///
    /// ```text
    /// PermissionPolicy::new(PermissionMode::DangerFullAccess)
    ///     .with_tool_requirement(HOOK_CAPABILITY_PRE_TOOL_USE, PermissionMode::DangerFullAccess)
    /// ```
    ///
    /// 未声明的能力按既有 fail-closed 规则拒绝：配置了 hook 命令也不会运行。
    #[must_use]
    pub fn with_authorization(mut self, authorization: PermissionPolicy) -> Self {
        self.authorization = authorization;
        self
    }

    #[must_use]
    pub fn authorization(&self) -> &PermissionPolicy {
        &self.authorization
    }

    #[must_use]
    pub fn run_pre_tool_use(&self, tool_name: &str, tool_input: &str) -> HookRunResult {
        self.run_commands(
            HookEvent::PreToolUse,
            self.config.pre_tool_use(),
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
            self.config.post_tool_use(),
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

        // 独立执行授权闸门：未获授权的 hook **不运行**，且不得因此拒绝目标工具
        // （hook 没授权是配置问题，不是这次调用的错）。事实照实记录，供宿主审计。
        let capability = event.capability();
        if let PermissionOutcome::Deny { reason } =
            self.authorization.authorize(capability, tool_input, None)
        {
            return HookRunResult::allow(vec![format!(
                "{} hook not run: `{capability}` has no independent execution authorization ({reason})",
                event.as_str()
            )]);
        }

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
        let mut executed_commands = 0usize;

        for command in commands {
            executed_commands += 1;
            match Self::run_command(
                command,
                HookCommandRequest {
                    event,
                    tool_name,
                    tool_input,
                    tool_output,
                    is_error,
                    payload: &payload,
                },
            ) {
                HookCommandOutcome::Allow { message } => {
                    if let Some(message) = message {
                        messages.push(message);
                    }
                }
                HookCommandOutcome::Deny { message } => {
                    let message = message.unwrap_or_else(|| {
                        format!("{} hook denied tool `{tool_name}`", event.as_str())
                    });
                    messages.push(message);
                    return HookRunResult {
                        denied: true,
                        executed_commands,
                        messages,
                    };
                }
                HookCommandOutcome::Warn { message } => messages.push(message),
            }
        }

        HookRunResult::allow(messages).with_executed_commands(executed_commands)
    }

    fn run_command(command: &str, request: HookCommandRequest<'_>) -> HookCommandOutcome {
        let mut child = shell_command(command);
        child.stdin(std::process::Stdio::piped());
        child.stdout(std::process::Stdio::piped());
        child.stderr(std::process::Stdio::piped());
        child.env("HOOK_EVENT", request.event.as_str());
        child.env("HOOK_TOOL_NAME", request.tool_name);
        child.env("HOOK_TOOL_INPUT", request.tool_input);
        child.env(
            "HOOK_TOOL_IS_ERROR",
            if request.is_error { "1" } else { "0" },
        );
        if let Some(tool_output) = request.tool_output {
            child.env("HOOK_TOOL_OUTPUT", tool_output);
        }

        match child.output_with_stdin(request.payload.as_bytes()) {
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
                            "{} hook `{command}` terminated by signal while handling `{}`",
                            request.event.as_str(),
                            request.tool_name
                        ),
                    },
                }
            }
            Err(error) => HookCommandOutcome::Warn {
                message: format!(
                    "{} hook `{command}` failed to start for `{}`: {error}",
                    request.event.as_str(),
                    request.tool_name
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
    let command_builder = {
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
            use std::io::Write;
            child_stdin.write_all(stdin)?;
        }
        child.wait_with_output()
    }
}

/// 测试专用：可计数的"假 hook"。
///
/// 断言"hook 没有运行"不能只看最终结果（最终结果可能因为别的分支而相同）。
/// 这里让 hook 真运行时同时留下**两条外部可观察证据**：
/// 1. 磁盘上的标记文件（相对进程 CWD，即 crate 包根目录）；
/// 2. stdout 文本 `hook-ran-<label>`（退出码 0 或 2 时都会被宿主当作 hook 事实）。
///
/// 标记文件名不含空格、引号与路径分隔符，因此 `cmd /C` 与 `sh -lc` 的
/// `echo … > file` 重定向都能直接使用。
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug)]
    pub(crate) struct CountingHook {
        label: String,
        marker: PathBuf,
    }

    impl CountingHook {
        pub(crate) fn new(label: &str) -> Self {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let marker =
                PathBuf::from(format!("hook-marker-{label}-{}-{id}.txt", std::process::id()));
            Self {
                label: label.to_string(),
                marker,
            }
        }

        /// 退出码 0 的 hook 命令（allow）：回显 `hook-ran-<label>` 并留下标记文件。
        pub(crate) fn allow_command(&self) -> String {
            let text = self.transcript_marker();
            format!("echo {text} && echo {text}> {}", self.marker.display())
        }

        /// 退出码 2 的 hook 命令（按既有约定表示 deny），同样回显并留下标记文件。
        pub(crate) fn deny_command(&self) -> String {
            let text = self.transcript_marker();
            format!(
                "echo {text} && echo {text}> {} && {}",
                self.marker.display(),
                exit_two()
            )
        }

        /// 外部证据：hook 是否真的运行过。
        pub(crate) fn ran(&self) -> bool {
            self.marker.exists()
        }

        /// hook stdout 会在宿主记录中出现的那段文本。
        pub(crate) fn transcript_marker(&self) -> String {
            format!("hook-ran-{}", self.label)
        }

        /// 尽力清理标记文件，避免把临时产物留在包根目录。
        pub(crate) fn cleanup(&self) {
            let _ = std::fs::remove_file(&self.marker);
        }
    }

    #[cfg(windows)]
    fn exit_two() -> &'static str {
        "exit /b 2"
    }

    #[cfg(not(windows))]
    fn exit_two() -> &'static str {
        "exit 2"
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::CountingHook;
    use super::{
        HookRunResult, HookRunner, HOOK_CAPABILITY_POST_TOOL_USE, HOOK_CAPABILITY_PRE_TOOL_USE,
    };
    use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
    use crate::permissions::{PermissionMode, PermissionPolicy};

    /// hook 自身的独立执行授权（§7.3）。未声明的能力按 fail-closed 拒绝。
    fn hook_authorization(pre: bool, post: bool) -> PermissionPolicy {
        let mut policy = PermissionPolicy::new(PermissionMode::ReadOnly);
        if pre {
            policy = policy
                .with_tool_requirement(HOOK_CAPABILITY_PRE_TOOL_USE, PermissionMode::ReadOnly);
        }
        if post {
            policy = policy
                .with_tool_requirement(HOOK_CAPABILITY_POST_TOOL_USE, PermissionMode::ReadOnly);
        }
        policy
    }

    /// 正对照：授权之后同一个 hook 确实会运行。
    ///
    /// 没有这条对照，"没跑"类断言可能只是恒真（例如命令本身写不出文件）。
    #[test]
    fn authorized_pre_hook_runs_and_its_stdout_becomes_a_message() {
        let hook = CountingHook::new("authorized-pre");
        let runner = HookRunner::new(RuntimeHookConfig::new(vec![hook.allow_command()], Vec::new()))
            .with_authorization(hook_authorization(true, false));

        let result = runner.run_pre_tool_use("Read", r#"{"path":"README.md"}"#);

        assert_eq!(result.executed_commands(), 1);
        assert!(!result.is_denied());
        assert_eq!(result.messages().to_vec(), vec![hook.transcript_marker()]);
        assert!(hook.ran(), "an authorized hook must have produced its effect");
        hook.cleanup();
    }

    /// 判别性：配置了 hook 命令但**没有独立执行授权** ⇒ 命令一次都不运行。
    #[test]
    fn unauthorized_hook_is_not_run_at_all() {
        let hook = CountingHook::new("unauthorized-pre");
        let runner = HookRunner::new(RuntimeHookConfig::new(vec![hook.allow_command()], Vec::new()));

        let result = runner.run_pre_tool_use("Read", r#"{"path":"README.md"}"#);

        assert_eq!(
            result.executed_commands(),
            0,
            "no hook command may run without independent authorization"
        );
        assert!(
            !hook.ran(),
            "an unauthorized hook must not even produce its side effect"
        );
        assert!(
            !result.is_denied(),
            "missing hook authorization is a host configuration fact, not a reason to block the tool"
        );
        assert!(
            result.messages().iter().any(|message| message.contains("not run")
                && message.contains(HOOK_CAPABILITY_PRE_TOOL_USE)),
            "the skip fact must still be recorded: {:?}",
            result.messages()
        );
        hook.cleanup();
    }

    /// 最小权限：授权 pre hook 不等于授权 post hook。
    #[test]
    fn pre_hook_authorization_does_not_grant_post_hook() {
        let pre = CountingHook::new("pre-only");
        let post = CountingHook::new("post-only");
        let runner = HookRunner::new(RuntimeHookConfig::new(
            vec![pre.allow_command()],
            vec![post.allow_command()],
        ))
        .with_authorization(hook_authorization(true, false));

        let pre_result = runner.run_pre_tool_use("Read", "{}");
        let post_result = runner.run_post_tool_use("Read", "{}", "ok", false);

        assert_eq!(pre_result.executed_commands(), 1);
        assert_eq!(post_result.executed_commands(), 0);
        assert!(pre.ran(), "authorized pre hook must run");
        assert!(!post.ran(), "unauthorized post hook must not run");
        pre.cleanup();
        post.cleanup();
    }

    #[test]
    fn allows_exit_code_zero_and_captures_stdout() {
        let hook = CountingHook::new("exit-zero");
        let runner = HookRunner::new(RuntimeHookConfig::new(vec![hook.allow_command()], Vec::new()))
            .with_authorization(hook_authorization(true, false));

        let result = runner.run_pre_tool_use("Read", r#"{"path":"README.md"}"#);

        assert!(
            matches!(result, HookRunResult { .. }),
            "sanity: result is a hook run result"
        );
        assert!(!result.is_denied());
        assert_eq!(result.messages().to_vec(), vec![hook.transcript_marker()]);
        hook.cleanup();
    }

    #[test]
    fn denies_exit_code_two() {
        let hook = CountingHook::new("deny-two");
        let runner = HookRunner::new(RuntimeHookConfig::new(vec![hook.deny_command()], Vec::new()))
            .with_authorization(hook_authorization(true, false));

        let result = runner.run_pre_tool_use("Bash", r#"{"command":"pwd"}"#);

        assert!(result.is_denied());
        assert_eq!(result.executed_commands(), 1);
        assert_eq!(result.messages().to_vec(), vec![hook.transcript_marker()]);
        hook.cleanup();
    }

    #[test]
    fn warns_for_other_non_zero_statuses() {
        let runner = HookRunner::from_feature_config(
            &RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![shell_snippet("printf 'warning hook'; exit 1")],
                Vec::new(),
            )),
        )
        .with_authorization(hook_authorization(true, false));

        let result = runner.run_pre_tool_use("Edit", r#"{"file":"src/lib.rs"}"#);

        assert!(!result.is_denied());
        assert!(result
            .messages()
            .iter()
            .any(|message| message.contains("allowing tool execution to continue")));
    }

    #[cfg(windows)]
    fn shell_snippet(script: &str) -> String {
        cmd_shell_snippet(script)
    }

    #[cfg(not(windows))]
    fn shell_snippet(script: &str) -> String {
        script.to_string()
    }

    #[cfg(windows)]
    fn cmd_shell_snippet(script: &str) -> String {
        let script = script.trim();
        if let Some(after_prefix) = script.strip_prefix("printf '") {
            if let Some((message, suffix)) = after_prefix.split_once('\'') {
                let message = message
                    .replace('&', "^&")
                    .replace('|', "^|")
                    .replace('<', "^<")
                    .replace('>', "^>");
                let suffix = suffix.trim();
                if let Some(code) = suffix.strip_prefix("; exit ") {
                    return format!("echo {message} && exit /b {}", code.trim());
                }
                return format!("echo {message}");
            }
        }
        script.replace('\'', "\"")
    }
}
