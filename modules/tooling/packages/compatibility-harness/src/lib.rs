use std::fs;
use std::path::{Path, PathBuf};

use commands::{CommandManifestEntry, CommandRegistry, CommandSource};
use runtime::{BootstrapPhase, BootstrapPlan};
use tools::{ToolManifestEntry, ToolRegistry, ToolSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamPaths {
    repo_root: PathBuf,
}

impl UpstreamPaths {
    #[must_use]
    pub fn from_repo_root(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    #[must_use]
    pub fn from_workspace_dir(workspace_dir: impl AsRef<Path>) -> Self {
        let workspace_dir = workspace_dir
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| workspace_dir.as_ref().to_path_buf());
        let primary_repo_root = workspace_dir
            .parent()
            .map_or_else(|| PathBuf::from(".."), Path::to_path_buf);
        let repo_root = resolve_upstream_repo_root(&primary_repo_root);
        Self { repo_root }
    }

    #[must_use]
    pub fn commands_path(&self) -> PathBuf {
        self.repo_root.join("src/commands.ts")
    }

    #[must_use]
    pub fn tools_path(&self) -> PathBuf {
        self.repo_root.join("src/tools.ts")
    }

    #[must_use]
    pub fn cli_path(&self) -> PathBuf {
        self.repo_root.join("src/entrypoints/cli.tsx")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedManifest {
    pub commands: CommandRegistry,
    pub tools: ToolRegistry,
    pub bootstrap: BootstrapPlan,
}

fn resolve_upstream_repo_root(primary_repo_root: &Path) -> PathBuf {
    let candidates = upstream_repo_candidates(primary_repo_root);
    candidates
        .into_iter()
        .find(|candidate| candidate.join("src/commands.ts").is_file())
        .unwrap_or_else(|| primary_repo_root.to_path_buf())
}

fn upstream_repo_candidates(primary_repo_root: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![primary_repo_root.to_path_buf()];

    if let Some(explicit) = std::env::var_os("CLAW_CODE_UPSTREAM") {
        candidates.push(PathBuf::from(explicit));
    }

    for ancestor in primary_repo_root.ancestors().take(4) {
        candidates.push(ancestor.join("claw-code"));
    }

    candidates.push(primary_repo_root.join("reference-source").join("claw-code"));
    candidates.push(primary_repo_root.join("vendor").join("claw-code"));

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.iter().any(|seen: &PathBuf| seen == &candidate) {
            deduped.push(candidate);
        }
    }
    deduped
}

pub fn extract_manifest(paths: &UpstreamPaths) -> std::io::Result<ExtractedManifest> {
    let commands_source = fs::read_to_string(paths.commands_path())?;
    let tools_source = fs::read_to_string(paths.tools_path())?;
    let cli_source = fs::read_to_string(paths.cli_path())?;

    Ok(ExtractedManifest {
        commands: extract_commands(&commands_source),
        tools: extract_tools(&tools_source),
        bootstrap: extract_bootstrap_plan(&cli_source),
    })
}

#[must_use]
pub fn extract_commands(source: &str) -> CommandRegistry {
    let mut entries = Vec::new();
    let mut in_internal_block = false;

    for raw_line in source.lines() {
        let line = raw_line.trim();

        if line.starts_with("export const INTERNAL_ONLY_COMMANDS = [") {
            in_internal_block = true;
            continue;
        }

        if in_internal_block {
            if line.starts_with(']') {
                in_internal_block = false;
                continue;
            }
            if let Some(name) = first_identifier(line) {
                entries.push(CommandManifestEntry {
                    name,
                    source: CommandSource::InternalOnly,
                });
            }
            continue;
        }

        if line.starts_with("import ") {
            for imported in imported_symbols(line) {
                entries.push(CommandManifestEntry {
                    name: imported,
                    source: CommandSource::Builtin,
                });
            }
        }

        if line.contains("feature('") && line.contains("./commands/") {
            if let Some(name) = first_assignment_identifier(line) {
                entries.push(CommandManifestEntry {
                    name,
                    source: CommandSource::FeatureGated,
                });
            }
        }
    }

    dedupe_commands(entries)
}

#[must_use]
pub fn extract_tools(source: &str) -> ToolRegistry {
    let mut entries = Vec::new();

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.starts_with("import ") && line.contains("./tools/") {
            for imported in imported_symbols(line) {
                if imported.ends_with("Tool") {
                    entries.push(ToolManifestEntry {
                        name: imported,
                        source: ToolSource::Base,
                    });
                }
            }
        }

        if line.contains("feature('") && line.contains("Tool") {
            if let Some(name) = first_assignment_identifier(line) {
                if name.ends_with("Tool") || name.ends_with("Tools") {
                    entries.push(ToolManifestEntry {
                        name,
                        source: ToolSource::Conditional,
                    });
                }
            }
        }
    }

    dedupe_tools(entries)
}

#[must_use]
pub fn extract_bootstrap_plan(source: &str) -> BootstrapPlan {
    let mut phases = vec![BootstrapPhase::CliEntry];

    if source.contains("--version") {
        phases.push(BootstrapPhase::FastPathVersion);
    }
    if source.contains("startupProfiler") {
        phases.push(BootstrapPhase::StartupProfiler);
    }
    if source.contains("--dump-system-prompt") {
        phases.push(BootstrapPhase::SystemPromptFastPath);
    }
    if source.contains("--claude-in-chrome-mcp") {
        phases.push(BootstrapPhase::ChromeMcpFastPath);
    }
    if source.contains("--daemon-worker") {
        phases.push(BootstrapPhase::DaemonWorkerFastPath);
    }
    if source.contains("remote-control") {
        phases.push(BootstrapPhase::BridgeFastPath);
    }
    if source.contains("args[0] === 'daemon'") {
        phases.push(BootstrapPhase::DaemonFastPath);
    }
    if source.contains("args[0] === 'ps'") || source.contains("args.includes('--bg')") {
        phases.push(BootstrapPhase::BackgroundSessionFastPath);
    }
    if source.contains("args[0] === 'new' || args[0] === 'list' || args[0] === 'reply'") {
        phases.push(BootstrapPhase::TemplateFastPath);
    }
    if source.contains("environment-runner") {
        phases.push(BootstrapPhase::EnvironmentRunnerFastPath);
    }
    phases.push(BootstrapPhase::MainRuntime);

    BootstrapPlan::from_phases(phases)
}

fn imported_symbols(line: &str) -> Vec<String> {
    let Some(after_import) = line.strip_prefix("import ") else {
        return Vec::new();
    };

    let before_from = after_import
        .split(" from ")
        .next()
        .unwrap_or_default()
        .trim();
    if before_from.starts_with('{') {
        return before_from
            .trim_matches(|c| c == '{' || c == '}')
            .split(',')
            .filter_map(|part| {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    return None;
                }
                Some(trimmed.split_whitespace().next()?.to_string())
            })
            .collect();
    }

    let first = before_from.split(',').next().unwrap_or_default().trim();
    if first.is_empty() {
        Vec::new()
    } else {
        vec![first.to_string()]
    }
}

fn first_assignment_identifier(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let candidate = trimmed.split('=').next()?.trim();
    first_identifier(candidate)
}

fn first_identifier(line: &str) -> Option<String> {
    let mut out = String::new();
    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if !out.is_empty() {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn dedupe_commands(entries: Vec<CommandManifestEntry>) -> CommandRegistry {
    let mut deduped = Vec::new();
    for entry in entries {
        let exists = deduped.iter().any(|seen: &CommandManifestEntry| {
            seen.name == entry.name && seen.source == entry.source
        });
        if !exists {
            deduped.push(entry);
        }
    }
    CommandRegistry::new(deduped)
}

fn dedupe_tools(entries: Vec<ToolManifestEntry>) -> ToolRegistry {
    let mut deduped = Vec::new();
    for entry in entries {
        let exists = deduped
            .iter()
            .any(|seen: &ToolManifestEntry| seen.name == entry.name && seen.source == entry.source);
        if !exists {
            deduped.push(entry);
        }
    }
    ToolRegistry::new(deduped)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 打印一行**不被 libtest 捕获**的输出。
    ///
    /// 环境例外必须对"通过的运行"也可见：libtest 会捕获通过用例的 `println!`，
    /// 只写进被捕获的缓冲就等于**静默通过**。直接写 `std::io::stdout()` 不经过捕获通道。
    /// （按"各 crate 独立"的约定，本辅助在本 crate 自己有一份。）
    fn announce_env(line: &str) {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }

    /// ## 本 crate 测试已声明的环境要求
    ///
    /// 1. `std::env::temp_dir()`（Windows 上是 `TEMP`/`TMP`）必须可写：人工夹具建在它下面；
    /// 2. **可选**外部输入：上游 `claw-code` 参考源（`src/commands.ts`、`src/tools.ts`、
    ///    `src/entrypoints/cli.tsx`）。本仓**不附带**它；可用环境变量 `CLAW_CODE_UPSTREAM`
    ///    指定检出位置，或放到候选目录之一。缺失时相关用例**明确跳过并独立统计**（见下）。
    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("compat-harness-{label}-{nanos}"))
    }

    /// 上游检出候选根目录（与生产 `UpstreamPaths::from_workspace_dir` 用**同一份**候选规则：
    /// 本仓根 → `CLAW_CODE_UPSTREAM` → 各层 `claw-code` → `reference-source` → `vendor`）。
    ///
    /// 用途只有一个：跳过时把**探测过哪些位置**如实打出来，便于独立统计与人工补前置。
    fn fixture_candidates() -> Vec<PathBuf> {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let workspace_dir = workspace_dir
            .canonicalize()
            .unwrap_or_else(|_| workspace_dir.clone());
        let primary_repo_root = workspace_dir
            .parent()
            .map_or_else(|| PathBuf::from(".."), Path::to_path_buf);
        upstream_repo_candidates(&primary_repo_root)
    }

    fn fixture_paths() -> UpstreamPaths {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        UpstreamPaths::from_workspace_dir(workspace_dir)
    }

    fn has_upstream_fixture(paths: &UpstreamPaths) -> bool {
        paths.commands_path().is_file()
            && paths.tools_path().is_file()
            && paths.cli_path().is_file()
    }

    /// **显式环境例外**（RPR-01b 裁决 §5.2）：需要上游参考源的用例必须先调用本函数；
    /// 返回 `false` 表示本机不满足前置，调用方必须立即返回（跳过，不做任何断言）。
    ///
    /// ## 为什么必须走这一层，而**不能**再写裸 `if !.. { return; }`
    ///
    /// 改前这三个用例是裸 `return;`：libtest 把"什么都没做就返回"报成 **ok**，于是
    /// `cargo test` 全绿显示 `3 passed`，而门禁其实**拿不到**"本机未验证"这个信息 ——
    /// 这正是裁决禁止的"用 skip 令发布门禁全绿"。现在：
    /// ① 前置判定是**真实探测**（三个文件同时存在才算齐全，见 `has_upstream_fixture`）；
    /// ② 缺失时打印**绕过 libtest 捕获**的 `[env-skip]` 行，写明探测过的候选位置与可用的
    ///    `CLAW_CODE_UPSTREAM` 覆盖，并明说"本机不验证任何行为、**不是通过**"，
    ///    因此可被独立统计（数 `[env-skip]` 行即可）；
    /// ③ 解析逻辑另有**不依赖外部检出**的用例（`synthetic_upstream_fixture_drives_extraction`），
    ///    所以"上游缺失"不等于这条链在本机完全没有被验证。
    ///
    /// 这里的取舍是**有依据的**：上游参考源属**可选外部输入**（本仓不附带），
    /// 对应裁决 §5.2 表中"可选外部依赖缺失且前置有真实探测 ⇒ 可明确跳过并独立统计"；
    /// 但它**绝不**计入通过。
    fn require_upstream_fixture(test_name: &str, paths: &UpstreamPaths) -> bool {
        if has_upstream_fixture(paths) {
            return true;
        }
        let candidates = fixture_candidates()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        announce_env(&format!(
            "[env-skip] {test_name}: 本机未验证——找不到上游参考源（需 src/commands.ts、src/tools.ts、\
             src/entrypoints/cli.tsx 三个文件同时存在）。已探测候选：{candidates}。\
             可用 CLAW_CODE_UPSTREAM 指定上游检出，或把检出放到上述任一位置。\
             本用例在该机器上不验证任何行为，因此**不是通过**（独立统计：数 [env-skip] 行）。"
        ));
        false
    }

    /// 人工构造的上游形状：只放**解析器会识别**的形态（`import`、`x = feature('./commands/..')`、
    /// 内部清单块）。注意赋值行不带 `const` —— 解析器取的是 `=` 左边的第一个标识符。
    const SYNTHETIC_COMMANDS: &str = r#"import { addDir } from "./commands/addDir"
review = feature('./commands/review')
export const INTERNAL_ONLY_COMMANDS = [
  shouldNotAppear
]
"#;

    const SYNTHETIC_TOOLS: &str = r#"import { AgentTool } from "./tools/AgentTool"
BashTool = feature('./tools/BashTool')
"#;

    const SYNTHETIC_CLI: &str = r#"const argv = process.argv
if (argv.includes('--version')) { probe_version() }
"#;

    #[test]
    fn extracts_non_empty_manifests_from_upstream_repo() {
        let paths = fixture_paths();
        if !require_upstream_fixture("extracts_non_empty_manifests_from_upstream_repo", &paths) {
            return;
        }
        let manifest = extract_manifest(&paths).expect("manifest should load");
        assert!(!manifest.commands.entries().is_empty());
        assert!(!manifest.tools.entries().is_empty());
        assert!(!manifest.bootstrap.phases().is_empty());
    }

    #[test]
    fn detects_known_upstream_command_symbols() {
        let paths = fixture_paths();
        if !require_upstream_fixture("detects_known_upstream_command_symbols", &paths) {
            return;
        }
        let commands =
            extract_commands(&fs::read_to_string(paths.commands_path()).expect("commands.ts"));
        let names: Vec<_> = commands
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"addDir"));
        assert!(names.contains(&"review"));
        assert!(!names.contains(&"INTERNAL_ONLY_COMMANDS"));
    }

    #[test]
    fn detects_known_upstream_tool_symbols() {
        let paths = fixture_paths();
        if !require_upstream_fixture("detects_known_upstream_tool_symbols", &paths) {
            return;
        }
        let tools = extract_tools(&fs::read_to_string(paths.tools_path()).expect("tools.ts"));
        let names: Vec<_> = tools
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"AgentTool"));
        assert!(names.contains(&"BashTool"));
    }

    /// **不依赖外部检出**的判别性用例（RPR-01b）：用人工夹具把"前置探测"与"解析"两条都钉住。
    ///
    /// 判别性体现在三处，任何一处回归都会让本用例变红：
    /// 1. 空目录、只有一半文件的目录**不得**被判为"齐全"（探测不是恒真）；
    /// 2. 三个文件补齐后**必须**判为"齐全"（探测不是恒假）；
    /// 3. 解析结果与 `detects_known_upstream_*` 完全相同（`addDir`/`review` 在、
    ///    `INTERNAL_ONLY_COMMANDS` 不在、`AgentTool`/`BashTool` 在、引导阶段多于一个）。
    ///
    /// 因此"上游参考源缺失"时，本 crate 仍有这条链在**任何机器**上真实执行断言。
    #[test]
    fn synthetic_upstream_fixture_drives_extraction() {
        let root = temp_dir("synthetic");

        // (1) 目录尚未建立（空）→ 必须判"没有"。
        let absent = UpstreamPaths::from_repo_root(root.clone());
        assert!(
            !has_upstream_fixture(&absent),
            "不存在的目录不得被判为齐全的上游检出"
        );

        // (2) 只放一半（缺 tools.ts / cli.tsx）→ 仍必须判"没有"。
        fs::create_dir_all(root.join("src")).expect("src dir");
        fs::write(root.join("src/commands.ts"), SYNTHETIC_COMMANDS).expect("commands fixture");
        let partial = UpstreamPaths::from_repo_root(root.clone());
        assert!(
            !has_upstream_fixture(&partial),
            "只有 commands.ts 不得被判为齐全"
        );

        // (3) 补齐三个文件 → 必须判"有"。
        fs::create_dir_all(root.join("src/entrypoints")).expect("entrypoints dir");
        fs::write(root.join("src/tools.ts"), SYNTHETIC_TOOLS).expect("tools fixture");
        fs::write(root.join("src/entrypoints/cli.tsx"), SYNTHETIC_CLI).expect("cli fixture");
        let full = UpstreamPaths::from_repo_root(root.clone());
        assert!(
            has_upstream_fixture(&full),
            "三个文件齐全时必须被判为齐全：{full:?}"
        );

        // (4) 解析：与 `detects_known_upstream_command_symbols` /
        //     `detects_known_upstream_tool_symbols` / `extracts_non_empty_manifests_from_upstream_repo`
        //     逐条同义，只是输入换成人工夹具。
        let manifest = extract_manifest(&full).expect("synthetic manifest should load");
        let commands: Vec<_> = manifest
            .commands
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(commands.contains(&"addDir"), "commands={commands:?}");
        assert!(commands.contains(&"review"), "commands={commands:?}");
        assert!(
            !commands.contains(&"INTERNAL_ONLY_COMMANDS"),
            "内部清单块的**块名**不得作为命令登记：{commands:?}"
        );
        let tools: Vec<_> = manifest
            .tools
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(tools.contains(&"AgentTool"), "tools={tools:?}");
        assert!(tools.contains(&"BashTool"), "tools={tools:?}");
        assert!(
            manifest.bootstrap.phases().len() > 1,
            "含 --version 的入口必须比单阶段引导多出阶段：{:?}",
            manifest.bootstrap.phases()
        );

        fs::remove_dir_all(&root).expect("cleanup synthetic fixture");
    }
}
