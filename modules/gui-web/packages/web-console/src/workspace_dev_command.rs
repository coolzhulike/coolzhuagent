use serde_json::Value;
use std::path::{Component, Path, PathBuf};

pub(crate) fn is_shell_tool(tool_name: &str) -> bool {
    matches!(tool_name, "bash" | "PowerShell")
}

/// 工作区内单条常用开发命令（构建/测试/只读 git）。命令首词按系统 PATH 解析，不要求位于工作区。
/// 含链接/重定向/替换/换行、关闭沙箱、后台运行或 cwd 越出工作区时一律不豁免，仍走原审批。
pub(crate) fn is_workspace_dev_command(tool_name: &str, input: &Value, workspace_root: &Path) -> bool {
    if !is_shell_tool(tool_name) {
        return false;
    }
    if input.get("dangerouslyDisableSandbox").and_then(Value::as_bool).unwrap_or(false)
        || input.get("run_in_background").and_then(Value::as_bool).unwrap_or(false)
    {
        return false;
    }
    if let Some(cwd) = input.get("cwd").and_then(Value::as_str).filter(|c| !c.trim().is_empty()) {
        if !cwd_inside(cwd, workspace_root) {
            return false;
        }
    }
    input.get("command").and_then(Value::as_str).is_some_and(|command| command_allowed(command, workspace_root))
}

fn cwd_inside(cwd: &str, root: &Path) -> bool {
    let path = PathBuf::from(cwd.trim());
    let joined = if path.is_absolute() { path } else { root.join(path) };
    if joined.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_start_matches("\\\\?\\").trim_end_matches('\\').to_lowercase();
    let (joined, root) = (norm(&joined), norm(root));
    joined == root || joined.starts_with(&format!("{root}\\"))
}

/// 只剥离"收集输出"的尾部：`| Out-String`、`| Select-Object -Last N`、`2>&1`，不放宽任何其它管道/重定向。
fn strip_output_suffix(command: &str) -> &str {
    let mut command = command.trim();
    let lower = command.to_lowercase();
    if let Some(idx) = lower.rfind('|') {
        let tail = lower[idx + 1..].trim();
        let last_n = tail.strip_prefix("select-object -last ").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if tail == "out-string" || last_n {
            command = command[..idx].trim_end();
        }
    }
    command.strip_suffix("2>&1").map_or(command, str::trim_end)
}

fn split_first_separator(s: &str) -> Option<(&str, &str)> {
    let idx = s.find(|c| c == ';' || c == '&')?;
    let rest = &s[idx..];
    let skip = if rest.starts_with("&&") { 2 } else { 1 };
    Some((&s[..idx], &s[idx + skip..]))
}

/// 开头的 `Set-Location/cd/Push-Location <工作区内路径>`（cwd 已由工具参数限定，此处只是冗余切换）。
fn strip_leading_chdir<'a>(command: &'a str, root: &Path) -> Option<&'a str> {
    let lower = command.to_lowercase();
    let verb = ["set-location ", "push-location ", "cd "].into_iter().find(|v| lower.starts_with(v));
    let Some(verb) = verb else { return Some(command) };
    let (head, rest) = split_first_separator(command)?;
    let mut path = head[verb.len()..].trim();
    loop {
        let token = path.split_whitespace().next().unwrap_or("");
        let flag = token.eq_ignore_ascii_case("/d") || token.eq_ignore_ascii_case("-literalpath") || token.eq_ignore_ascii_case("-path");
        if !flag { break; }
        path = path[token.len()..].trim_start();
    }
    let path = path.trim().trim_matches(|c| c == '"' || c == '\'');
    (!path.is_empty() && !path.contains(['"', '\'', '$', '`', '|', '<', '>', '(', ')', '%']) && cwd_inside(path, root)).then_some(rest.trim())
}

/// 末尾仅回显退出码：`; Write-Output "X=$LASTEXITCODE"`、`& echo X=%ERRORLEVEL%`。
fn strip_trailing_exit_echo(command: &str) -> &str {
    let Some(idx) = command.rfind(|c| c == ';' || c == '&') else { return command };
    if command[..idx].ends_with('>') { return command; }
    let tail = command[idx + 1..].trim().to_lowercase();
    let body = ["write-output ", "write-host ", "echo "].into_iter().find_map(|v| tail.strip_prefix(v)).or_else(|| tail.starts_with(['"', '\'']).then_some(tail.as_str()));
    let Some(body) = body else { return command };
    let body = body.trim().trim_matches(|c| c == '"' || c == '\'');
    let Some((name, value)) = body.split_once('=') else { return command };
    let name_ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if name_ok && matches!(value, "$lastexitcode" | "%errorlevel%") {
        return command[..idx].trim_end();
    }
    command
}

fn path_inside(arg: &str, root: &Path) -> bool {
    let arg = arg.trim_matches(|c| c == '"' || c == '\'');
    let path = Path::new(arg);
    if path.is_absolute() || arg.starts_with("\\\\") { return cwd_inside(arg, root); }
    !path.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir))
}

fn command_allowed(command: &str, root: &Path) -> bool {
    let Some(command) = strip_leading_chdir(command.trim(), root) else { return false };
    let command = strip_output_suffix(strip_trailing_exit_echo(strip_output_suffix(command)));
    if command.is_empty() || command.chars().any(|c| matches!(c, ';' | '&' | '|' | '<' | '>' | '`' | '\n' | '\r' | '$' | '(' | ')' | '{' | '}' | '%' | '^')) {
        return false;
    }
    let tokens: Vec<String> = command.split_whitespace().map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_string()).collect();
    let Some(head) = tokens.first() else { return false };
    if head.contains(['\\', '/']) {
        return false;
    }
    let head = head.to_lowercase();
    let head = head.strip_suffix(".exe").unwrap_or(&head);
    let args: Vec<String> = tokens[1..].iter().map(|t| t.to_lowercase()).collect();
    if args.iter().any(|a| a == "--manifest-path" || a.starts_with("--manifest-path=") || a == "--target-dir" || a.starts_with("--target-dir=")) {
        return false;
    }
    let first = args.first().map(String::as_str).unwrap_or("");
    let second = args.get(1).map(String::as_str).unwrap_or("");
    let version_only = matches!(first, "--version" | "-v" | "-version") && args.len() == 1;
    match head {
        "cargo" => version_only || matches!(first, "build" | "test" | "check" | "clippy" | "fmt" | "metadata" | "tree" | "doc"),
        "rustc" | "rustfmt" | "node" | "python" | "python3" | "py" | "go" => version_only,
        "git" => matches!(first, "status" | "diff" | "log" | "show" | "rev-parse" | "branch" | "ls-files") || version_only,
        "npm" | "pnpm" | "yarn" => version_only || first == "test" || (first == "run" && matches!(second, "test" | "build" | "lint" | "check")),
        "dir" | "ls" | "get-childitem" | "gci" | "tree" => tokens[1..].iter().all(|a| {
            let flag = a.starts_with('-') || (a.starts_with('/') && a.len() <= 3);
            flag || path_inside(a, root)
        }),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(tool: &str, input: Value) -> bool {
        is_workspace_dev_command(tool, &input, Path::new("D:\\w\\ws"))
    }

    #[test]
    fn allows_common_dev_commands_in_workspace() {
        assert!(ok("PowerShell", json!({"command": "cargo test --offline"})));
        assert!(ok("bash", json!({"command": "git status", "cwd": "D:\\w\\ws\\sub"})));
        assert!(ok("PowerShell", json!({"command": "cargo.exe build", "cwd": "."})));
        assert!(ok("PowerShell", json!({"command": "python --version"})));
        assert!(ok("PowerShell", json!({"command": "npm run test"})));
        assert!(ok("PowerShell", json!({"command": "cargo test --offline 2>&1 | Out-String"})));
        assert!(ok("PowerShell", json!({"command": "cargo test 2>&1 | Select-Object -Last 40"})));
        assert!(ok("PowerShell", json!({"command": "cargo test --offline; Write-Output \"EXIT=$LASTEXITCODE\""})));
        assert!(ok("PowerShell", json!({"command": "Set-Location -LiteralPath 'D:\\w\\ws'; cargo test --offline 2>&1 | Out-String; \"EXIT=$LASTEXITCODE\""})));
        assert!(ok("PowerShell", json!({"command": "Set-Location 'D:\\w\\ws'; cargo test --offline; Write-Output \"EXITCODE=$LASTEXITCODE\""})));
        assert!(ok("bash", json!({"command": "cd /d D:\\w\\ws && cargo test --offline 2>&1 & echo EXIT_CODE=%ERRORLEVEL%"})));
        assert!(ok("bash", json!({"command": "dir /b D:\\w\\ws"})));
        assert!(ok("PowerShell", json!({"command": "Get-ChildItem -Recurse -Force src"})));
        assert!(ok("bash", json!({"command": "ls"})));
    }

    #[test]
    fn rejects_chaining_outside_cwd_and_unlisted() {
        assert!(!ok("PowerShell", json!({"command": "cargo test; Remove-Item x"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test | Out-File x"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test | Select-Object -Last 4; calc"})));
        assert!(!ok("PowerShell", json!({"command": "Remove-Item x 2>&1"})));
        assert!(!ok("PowerShell", json!({"command": "Set-Location D:\\other; cargo test"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test; Write-Output \"x=$(calc)\""})));
        assert!(!ok("PowerShell", json!({"command": "cargo test; Remove-Item x; Write-Output \"EXIT=$LASTEXITCODE\""})));
        assert!(!ok("PowerShell", json!({"command": "Get-ChildItem D:\\other"})));
        assert!(!ok("PowerShell", json!({"command": "dir ..\\x"})));
        assert!(!ok("PowerShell", json!({"command": "Get-ChildItem -Recurse | Remove-Item"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test --manifest-path D:\\other\\Cargo.toml"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test > out.txt"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test $(calc)"})));
        assert!(!ok("PowerShell", json!({"command": "Remove-Item -Recurse x"})));
        assert!(!ok("PowerShell", json!({"command": "python -c print(1)"})));
        assert!(!ok("PowerShell", json!({"command": "git push"})));
        assert!(!ok("PowerShell", json!({"command": "C:\\evil\\cargo.exe test"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test", "cwd": "D:\\other"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test", "cwd": "D:\\w\\ws\\..\\x"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test", "cwd": "D:\\w\\ws2"})));
        assert!(!ok("PowerShell", json!({"command": "cargo test", "run_in_background": true})));
        assert!(!ok("bash", json!({"command": "cargo test", "dangerouslyDisableSandbox": true})));
        assert!(!ok("write_file", json!({"command": "cargo test"})));
    }
}
