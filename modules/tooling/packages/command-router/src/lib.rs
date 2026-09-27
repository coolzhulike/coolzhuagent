use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use plugins::{PluginError, PluginManager, PluginSummary};
use runtime::{compact_session, CompactionConfig, Session};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandManifestEntry {
    pub name: String,
    pub source: CommandSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSource {
    Builtin,
    InternalOnly,
    FeatureGated,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandRegistry {
    entries: Vec<CommandManifestEntry>,
}

impl CommandRegistry {
    #[must_use]
    pub fn new(entries: Vec<CommandManifestEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn entries(&self) -> &[CommandManifestEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandCategory {
    Core,
    Workspace,
    Session,
    Git,
    Automation,
}

impl SlashCommandCategory {
    const fn title(self) -> &'static str {
        match self {
            Self::Core => "Core flow",
            Self::Workspace => "Workspace & memory",
            Self::Session => "Sessions & output",
            Self::Git => "Git & GitHub",
            Self::Automation => "Automation & discovery",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub argument_hint: Option<&'static str>,
    pub resume_supported: bool,
    pub category: SlashCommandCategory,
}

const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        name: "help",
        aliases: &[],
        summary: "Show available slash commands",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "status",
        aliases: &[],
        summary: "Show current session status",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "compact",
        aliases: &[],
        summary: "Compact local session history",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "model",
        aliases: &[],
        summary: "Show or switch the active model",
        argument_hint: Some("[model]"),
        resume_supported: false,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "permissions",
        aliases: &[],
        summary: "Show or switch the active permission mode",
        argument_hint: Some("[read-only|workspace-write|danger-full-access]"),
        resume_supported: false,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "clear",
        aliases: &[],
        summary: "Start a fresh local session",
        argument_hint: Some("[--confirm]"),
        resume_supported: true,
        category: SlashCommandCategory::Session,
    },
    SlashCommandSpec {
        name: "cost",
        aliases: &[],
        summary: "Show cumulative token usage for this session",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Core,
    },
    SlashCommandSpec {
        name: "resume",
        aliases: &[],
        summary: "Load a saved session into the REPL",
        argument_hint: Some("<session-path>"),
        resume_supported: false,
        category: SlashCommandCategory::Session,
    },
    SlashCommandSpec {
        name: "config",
        aliases: &[],
        summary: "Inspect Claw config files or merged sections",
        argument_hint: Some("[env|hooks|model|agent|plugins]"),
        resume_supported: true,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "memory",
        aliases: &[],
        summary: "Inspect loaded Claw instruction memory files",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "init",
        aliases: &[],
        summary: "Create a starter CLAW.md for this repo",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "diff",
        aliases: &[],
        summary: "Show git diff for current workspace changes",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "version",
        aliases: &[],
        summary: "Show CLI version and build information",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "bughunter",
        aliases: &[],
        summary: "Inspect the codebase for likely bugs",
        argument_hint: Some("[scope]"),
        resume_supported: false,
        category: SlashCommandCategory::Automation,
    },
    SlashCommandSpec {
        name: "branch",
        aliases: &[],
        summary: "List, create, or switch git branches",
        argument_hint: Some("[list|create <name>|switch <name>]"),
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "worktree",
        aliases: &[],
        summary: "List, add, remove, or prune git worktrees",
        argument_hint: Some("[list|add <path> [branch]|remove <path>|prune]"),
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "commit",
        aliases: &[],
        summary: "Generate a commit message and create a git commit",
        argument_hint: None,
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "commit-push-pr",
        aliases: &[],
        summary: "Commit workspace changes, push the branch, and open a PR",
        argument_hint: Some("[context]"),
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "pr",
        aliases: &[],
        summary: "Draft or create a pull request from the conversation",
        argument_hint: Some("[context]"),
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "issue",
        aliases: &[],
        summary: "Draft or create a GitHub issue from the conversation",
        argument_hint: Some("[context]"),
        resume_supported: false,
        category: SlashCommandCategory::Git,
    },
    SlashCommandSpec {
        name: "ultraplan",
        aliases: &[],
        summary: "Run a deep planning prompt with multi-step reasoning",
        argument_hint: Some("[task]"),
        resume_supported: false,
        category: SlashCommandCategory::Automation,
    },
    SlashCommandSpec {
        name: "teleport",
        aliases: &[],
        summary: "Jump to a file or symbol by searching the workspace",
        argument_hint: Some("<symbol-or-path>"),
        resume_supported: false,
        category: SlashCommandCategory::Workspace,
    },
    SlashCommandSpec {
        name: "debug-tool-call",
        aliases: &[],
        summary: "Replay the last tool call with debug details",
        argument_hint: None,
        resume_supported: false,
        category: SlashCommandCategory::Automation,
    },
    SlashCommandSpec {
        name: "export",
        aliases: &[],
        summary: "Export the current conversation to a file",
        argument_hint: Some("[file]"),
        resume_supported: true,
        category: SlashCommandCategory::Session,
    },
    SlashCommandSpec {
        name: "session",
        aliases: &[],
        summary: "List or switch managed local sessions",
        argument_hint: Some("[list|switch <session-id>]"),
        resume_supported: false,
        category: SlashCommandCategory::Session,
    },
    SlashCommandSpec {
        name: "plugin",
        aliases: &["plugins", "marketplace"],
        summary: "Manage COOLZHU CODE Agent plugins",
        argument_hint: Some(
            "[list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]",
        ),
        resume_supported: false,
        category: SlashCommandCategory::Automation,
    },
    SlashCommandSpec {
        name: "agents",
        aliases: &[],
        summary: "List configured agents",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Automation,
    },
    SlashCommandSpec {
        name: "skills",
        aliases: &[],
        summary: "List available skills",
        argument_hint: None,
        resume_supported: true,
        category: SlashCommandCategory::Automation,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Help,
    Status,
    Compact,
    Branch {
        action: Option<String>,
        target: Option<String>,
    },
    Bughunter {
        scope: Option<String>,
    },
    Worktree {
        action: Option<String>,
        path: Option<String>,
        branch: Option<String>,
    },
    Commit,
    CommitPushPr {
        context: Option<String>,
    },
    Pr {
        context: Option<String>,
    },
    Issue {
        context: Option<String>,
    },
    Ultraplan {
        task: Option<String>,
    },
    Teleport {
        target: Option<String>,
    },
    DebugToolCall,
    Model {
        model: Option<String>,
    },
    Permissions {
        mode: Option<String>,
    },
    Clear {
        confirm: bool,
    },
    Cost,
    Resume {
        session_path: Option<String>,
    },
    Config {
        section: Option<String>,
    },
    Memory,
    Init,
    Diff,
    Version,
    Export {
        path: Option<String>,
    },
    Session {
        action: Option<String>,
        target: Option<String>,
    },
    Plugins {
        action: Option<String>,
        target: Option<String>,
    },
    Agents {
        args: Option<String>,
    },
    Skills {
        args: Option<String>,
    },
    Unknown(String),
}

impl SlashCommand {
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let mut parts = trimmed.trim_start_matches('/').split_whitespace();
        let command = parts.next().unwrap_or_default();
        Some(match command {
            "help" => Self::Help,
            "status" => Self::Status,
            "compact" => Self::Compact,
            "branch" => Self::Branch {
                action: parts.next().map(ToOwned::to_owned),
                target: parts.next().map(ToOwned::to_owned),
            },
            "bughunter" => Self::Bughunter {
                scope: remainder_after_command(trimmed, command),
            },
            "worktree" => Self::Worktree {
                action: parts.next().map(ToOwned::to_owned),
                path: parts.next().map(ToOwned::to_owned),
                branch: parts.next().map(ToOwned::to_owned),
            },
            "commit" => Self::Commit,
            "commit-push-pr" => Self::CommitPushPr {
                context: remainder_after_command(trimmed, command),
            },
            "pr" => Self::Pr {
                context: remainder_after_command(trimmed, command),
            },
            "issue" => Self::Issue {
                context: remainder_after_command(trimmed, command),
            },
            "ultraplan" => Self::Ultraplan {
                task: remainder_after_command(trimmed, command),
            },
            "teleport" => Self::Teleport {
                target: remainder_after_command(trimmed, command),
            },
            "debug-tool-call" => Self::DebugToolCall,
            "model" => Self::Model {
                model: parts.next().map(ToOwned::to_owned),
            },
            "permissions" => Self::Permissions {
                mode: parts.next().map(ToOwned::to_owned),
            },
            "clear" => Self::Clear {
                confirm: parts.next() == Some("--confirm"),
            },
            "cost" => Self::Cost,
            "resume" => Self::Resume {
                session_path: parts.next().map(ToOwned::to_owned),
            },
            "config" => Self::Config {
                section: parts.next().map(ToOwned::to_owned),
            },
            "memory" => Self::Memory,
            "init" => Self::Init,
            "diff" => Self::Diff,
            "version" => Self::Version,
            "export" => Self::Export {
                path: parts.next().map(ToOwned::to_owned),
            },
            "session" => Self::Session {
                action: parts.next().map(ToOwned::to_owned),
                target: parts.next().map(ToOwned::to_owned),
            },
            "plugin" | "plugins" | "marketplace" => Self::Plugins {
                action: parts.next().map(ToOwned::to_owned),
                target: {
                    let remainder = parts.collect::<Vec<_>>().join(" ");
                    (!remainder.is_empty()).then_some(remainder)
                },
            },
            "agents" => Self::Agents {
                args: remainder_after_command(trimmed, command),
            },
            "skills" => Self::Skills {
                args: remainder_after_command(trimmed, command),
            },
            other => Self::Unknown(other.to_string()),
        })
    }
}

fn remainder_after_command(input: &str, command: &str) -> Option<String> {
    input
        .trim()
        .strip_prefix(&format!("/{command}"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[must_use]
pub fn slash_command_specs() -> &'static [SlashCommandSpec] {
    SLASH_COMMAND_SPECS
}

#[must_use]
pub fn resume_supported_slash_commands() -> Vec<&'static SlashCommandSpec> {
    slash_command_specs()
        .iter()
        .filter(|spec| spec.resume_supported)
        .collect()
}

#[must_use]
pub fn render_slash_command_help() -> String {
    let mut lines = vec![
        "Slash commands".to_string(),
        "  Tab completes commands inside the REPL.".to_string(),
        "  [resume] = also available via coolzhu-cli --resume SESSION.json".to_string(),
    ];

    for category in [
        SlashCommandCategory::Core,
        SlashCommandCategory::Workspace,
        SlashCommandCategory::Session,
        SlashCommandCategory::Git,
        SlashCommandCategory::Automation,
    ] {
        lines.push(String::new());
        lines.push(category.title().to_string());
        lines.extend(
            slash_command_specs()
                .iter()
                .filter(|spec| spec.category == category)
                .map(render_slash_command_entry),
        );
    }

    lines.join("\n")
}

fn render_slash_command_entry(spec: &SlashCommandSpec) -> String {
    let alias_suffix = if spec.aliases.is_empty() {
        String::new()
    } else {
        format!(
            " (aliases: {})",
            spec.aliases
                .iter()
                .map(|alias| format!("/{alias}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let resume = if spec.resume_supported {
        " [resume]"
    } else {
        ""
    };
    format!(
        "  {name:<46} {}{alias_suffix}{resume}",
        spec.summary,
        name = render_slash_command_name(spec),
    )
}

fn render_slash_command_name(spec: &SlashCommandSpec) -> String {
    match spec.argument_hint {
        Some(argument_hint) => format!("/{} {}", spec.name, argument_hint),
        None => format!("/{}", spec.name),
    }
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let cost = usize::from(left_char != *right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right_chars.len()]
}

#[must_use]
pub fn suggest_slash_commands(input: &str, limit: usize) -> Vec<String> {
    let normalized = input.trim().trim_start_matches('/').to_ascii_lowercase();
    if normalized.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut ranked = slash_command_specs()
        .iter()
        .filter_map(|spec| {
            let score = std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(str::to_ascii_lowercase)
                .filter_map(|alias| {
                    if alias == normalized {
                        Some((0_usize, alias.len()))
                    } else if alias.starts_with(&normalized) {
                        Some((1, alias.len()))
                    } else if alias.contains(&normalized) {
                        Some((2, alias.len()))
                    } else {
                        let distance = levenshtein_distance(&alias, &normalized);
                        (distance <= 2).then_some((3 + distance, alias.len()))
                    }
                })
                .min();

            score.map(|(bucket, len)| (bucket, len, render_slash_command_name(spec)))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| left.cmp(right));
    ranked.dedup_by(|left, right| left.2 == right.2);
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, display)| display)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandResult {
    pub message: String,
    pub session: Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginsCommandResult {
    pub message: String,
    pub reload_runtime: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionSource {
    ProjectCodex,
    ProjectClaw,
    UserCodexHome,
    UserCodex,
    UserClaw,
}

impl DefinitionSource {
    fn label(self) -> &'static str {
        match self {
            Self::ProjectCodex => "Project (.codex)",
            Self::ProjectClaw => "Project (.claw)",
            Self::UserCodexHome => "User ($CODEX_HOME)",
            Self::UserCodex => "User (~/.codex)",
            Self::UserClaw => "User (~/.claw)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSummary {
    name: String,
    description: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    source: DefinitionSource,
    shadowed_by: Option<DefinitionSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillSummary {
    name: String,
    description: Option<String>,
    source: DefinitionSource,
    shadowed_by: Option<DefinitionSource>,
    origin: SkillOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillOrigin {
    SkillsDir,
    LegacyCommandsDir,
}

impl SkillOrigin {
    fn detail_label(self) -> Option<&'static str> {
        match self {
            Self::SkillsDir => None,
            Self::LegacyCommandsDir => Some("legacy /commands"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillRoot {
    source: DefinitionSource,
    path: PathBuf,
    origin: SkillOrigin,
}

#[allow(clippy::too_many_lines)]
pub fn handle_plugins_slash_command(
    action: Option<&str>,
    target: Option<&str>,
    manager: &mut PluginManager,
) -> Result<PluginsCommandResult, PluginError> {
    match action {
        None | Some("list") => Ok(PluginsCommandResult {
            message: render_plugins_report(&manager.list_installed_plugins()?),
            reload_runtime: false,
        }),
        Some("install") => {
            let Some(target) = target else {
                return Ok(PluginsCommandResult {
                    message: "Usage: /plugins install <path>".to_string(),
                    reload_runtime: false,
                });
            };
            let install = manager.install(target)?;
            let plugin = manager
                .list_installed_plugins()?
                .into_iter()
                .find(|plugin| plugin.metadata.id == install.plugin_id);
            Ok(PluginsCommandResult {
                message: render_plugin_install_report(&install.plugin_id, plugin.as_ref()),
                reload_runtime: true,
            })
        }
        Some("enable") => {
            let Some(target) = target else {
                return Ok(PluginsCommandResult {
                    message: "Usage: /plugins enable <name>".to_string(),
                    reload_runtime: false,
                });
            };
            let plugin = resolve_plugin_target(manager, target)?;
            manager.enable(&plugin.metadata.id)?;
            Ok(PluginsCommandResult {
                message: format!(
                    "Plugins\n  Result           enabled {}\n  Name             {}\n  Version          {}\n  Status           enabled",
                    plugin.metadata.id, plugin.metadata.name, plugin.metadata.version
                ),
                reload_runtime: true,
            })
        }
        Some("disable") => {
            let Some(target) = target else {
                return Ok(PluginsCommandResult {
                    message: "Usage: /plugins disable <name>".to_string(),
                    reload_runtime: false,
                });
            };
            let plugin = resolve_plugin_target(manager, target)?;
            manager.disable(&plugin.metadata.id)?;
            Ok(PluginsCommandResult {
                message: format!(
                    "Plugins\n  Result           disabled {}\n  Name             {}\n  Version          {}\n  Status           disabled",
                    plugin.metadata.id, plugin.metadata.name, plugin.metadata.version
                ),
                reload_runtime: true,
            })
        }
        Some("uninstall") => {
            let Some(target) = target else {
                return Ok(PluginsCommandResult {
                    message: "Usage: /plugins uninstall <plugin-id>".to_string(),
                    reload_runtime: false,
                });
            };
            manager.uninstall(target)?;
            Ok(PluginsCommandResult {
                message: format!("Plugins\n  Result           uninstalled {target}"),
                reload_runtime: true,
            })
        }
        Some("update") => {
            let Some(target) = target else {
                return Ok(PluginsCommandResult {
                    message: "Usage: /plugins update <plugin-id>".to_string(),
                    reload_runtime: false,
                });
            };
            let update = manager.update(target)?;
            let plugin = manager
                .list_installed_plugins()?
                .into_iter()
                .find(|plugin| plugin.metadata.id == update.plugin_id);
            Ok(PluginsCommandResult {
                message: format!(
                    "Plugins\n  Result           updated {}\n  Name             {}\n  Old version      {}\n  New version      {}\n  Status           {}",
                    update.plugin_id,
                    plugin
                        .as_ref()
                        .map_or_else(|| update.plugin_id.clone(), |plugin| plugin.metadata.name.clone()),
                    update.old_version,
                    update.new_version,
                    plugin
                        .as_ref()
                        .map_or("unknown", |plugin| if plugin.enabled { "enabled" } else { "disabled" }),
                ),
                reload_runtime: true,
            })
        }
        Some(other) => Ok(PluginsCommandResult {
            message: format!(
                "Unknown /plugins action '{other}'. Use list, install, enable, disable, uninstall, or update."
            ),
            reload_runtime: false,
        }),
    }
}

pub fn handle_agents_slash_command(args: Option<&str>, cwd: &Path) -> std::io::Result<String> {
    match normalize_optional_args(args) {
        None | Some("list") => {
            let roots = discover_definition_roots(cwd, "agents");
            let agents = load_agents_from_roots(&roots)?;
            Ok(render_agents_report(&agents))
        }
        Some("-h" | "--help" | "help") => Ok(render_agents_usage(None)),
        Some(args) => Ok(render_agents_usage(Some(args))),
    }
}

pub fn handle_skills_slash_command(args: Option<&str>, cwd: &Path) -> std::io::Result<String> {
    match normalize_optional_args(args) {
        None | Some("list") => {
            let roots = discover_skill_roots(cwd);
            let skills = load_skills_from_roots(&roots)?;
            Ok(render_skills_report(&skills))
        }
        Some("-h" | "--help" | "help") => Ok(render_skills_usage(None)),
        Some(args) => Ok(render_skills_usage(Some(args))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPushPrRequest {
    pub commit_message: Option<String>,
    pub pr_title: String,
    pub pr_body: String,
    pub branch_name_hint: String,
}

pub fn handle_branch_slash_command(
    action: Option<&str>,
    target: Option<&str>,
    cwd: &Path,
) -> io::Result<String> {
    match normalize_optional_args(action) {
        None | Some("list") => {
            let branches = git_stdout(cwd, &["branch", "--list", "--verbose"])?;
            let trimmed = branches.trim();
            Ok(if trimmed.is_empty() {
                "Branch\n  Result           no branches found".to_string()
            } else {
                format!("Branch\n  Result           listed\n\n{}", trimmed)
            })
        }
        Some("create") => {
            let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
                return Ok("Usage: /branch create <name>".to_string());
            };
            git_status_ok(cwd, &["switch", "-c", target])?;
            Ok(format!(
                "Branch\n  Result           created and switched\n  Branch           {target}"
            ))
        }
        Some("switch") => {
            let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
                return Ok("Usage: /branch switch <name>".to_string());
            };
            git_status_ok(cwd, &["switch", target])?;
            Ok(format!(
                "Branch\n  Result           switched\n  Branch           {target}"
            ))
        }
        Some(other) => Ok(format!(
            "Unknown /branch action '{other}'. Use /branch list, /branch create <name>, or /branch switch <name>."
        )),
    }
}

pub fn handle_worktree_slash_command(
    action: Option<&str>,
    path: Option<&str>,
    branch: Option<&str>,
    cwd: &Path,
) -> io::Result<String> {
    match normalize_optional_args(action) {
        None | Some("list") => {
            let worktrees = git_stdout(cwd, &["worktree", "list"])?;
            let trimmed = worktrees.trim();
            Ok(if trimmed.is_empty() {
                "Worktree\n  Result           no worktrees found".to_string()
            } else {
                format!("Worktree\n  Result           listed\n\n{}", trimmed)
            })
        }
        Some("add") => {
            let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
                return Ok("Usage: /worktree add <path> [branch]".to_string());
            };
            if let Some(branch) = branch.filter(|value| !value.trim().is_empty()) {
                if branch_exists(cwd, branch) {
                    git_status_ok(cwd, &["worktree", "add", path, branch])?;
                } else {
                    git_status_ok(cwd, &["worktree", "add", path, "-b", branch])?;
                }
                Ok(format!(
                    "Worktree\n  Result           added\n  Path             {path}\n  Branch           {branch}"
                ))
            } else {
                git_status_ok(cwd, &["worktree", "add", path])?;
                Ok(format!(
                    "Worktree\n  Result           added\n  Path             {path}"
                ))
            }
        }
        Some("remove") => {
            let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
                return Ok("Usage: /worktree remove <path>".to_string());
            };
            git_status_ok(cwd, &["worktree", "remove", path])?;
            Ok(format!(
                "Worktree\n  Result           removed\n  Path             {path}"
            ))
        }
        Some("prune") => {
            git_status_ok(cwd, &["worktree", "prune"])?;
            Ok("Worktree\n  Result           pruned".to_string())
        }
        Some(other) => Ok(format!(
            "Unknown /worktree action '{other}'. Use /worktree list, /worktree add <path> [branch], /worktree remove <path>, or /worktree prune."
        )),
    }
}

pub fn handle_commit_slash_command(message: &str, cwd: &Path) -> io::Result<String> {
    let status = git_stdout(cwd, &["status", "--short"])?;
    if status.trim().is_empty() {
        return Ok(
            "Commit\n  Result           skipped\n  Reason           no workspace changes"
                .to_string(),
        );
    }

    let message = message.trim();
    if message.is_empty() {
        return Err(io::Error::other("generated commit message was empty"));
    }

    git_status_ok(cwd, &["add", "-A"])?;
    let path = write_temp_text_file("claw-commit-message", "txt", message)?;
    let path_string = path.to_string_lossy().into_owned();
    git_status_ok(cwd, &["commit", "--file", path_string.as_str()])?;

    Ok(format!(
        "Commit\n  Result           created\n  Message file     {}\n\n{}",
        path.display(),
        message
    ))
}

pub fn handle_commit_push_pr_slash_command(
    request: &CommitPushPrRequest,
    cwd: &Path,
) -> io::Result<String> {
    if !command_exists("gh") {
        return Err(io::Error::other("gh CLI is required for /commit-push-pr"));
    }

    let default_branch = detect_default_branch(cwd)?;
    let mut branch = current_branch(cwd)?;
    let mut created_branch = false;
    if branch == default_branch {
        let hint = if request.branch_name_hint.trim().is_empty() {
            request.pr_title.as_str()
        } else {
            request.branch_name_hint.as_str()
        };
        let next_branch = build_branch_name(hint);
        git_status_ok(cwd, &["switch", "-c", next_branch.as_str()])?;
        branch = next_branch;
        created_branch = true;
    }

    let workspace_has_changes = !git_stdout(cwd, &["status", "--short"])?.trim().is_empty();
    let commit_report = if workspace_has_changes {
        let Some(message) = request.commit_message.as_deref() else {
            return Err(io::Error::other(
                "commit message is required when workspace changes are present",
            ));
        };
        Some(handle_commit_slash_command(message, cwd)?)
    } else {
        None
    };

    let branch_diff = git_stdout(
        cwd,
        &["diff", "--stat", &format!("{default_branch}...HEAD")],
    )?;
    if branch_diff.trim().is_empty() {
        return Ok(
            "Commit/Push/PR\n  Result           skipped\n  Reason           no branch changes to push or open as a pull request"
                .to_string(),
        );
    }

    git_status_ok(cwd, &["push", "--set-upstream", "origin", branch.as_str()])?;

    let body_path = write_temp_text_file("claw-pr-body", "md", request.pr_body.trim())?;
    let body_path_string = body_path.to_string_lossy().into_owned();
    let create = Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            request.pr_title.as_str(),
            "--body-file",
            body_path_string.as_str(),
            "--base",
            default_branch.as_str(),
        ])
        .current_dir(cwd)
        .output()?;

    let (result, url) = if create.status.success() {
        (
            "created",
            parse_pr_url(&String::from_utf8_lossy(&create.stdout))
                .unwrap_or_else(|| "<unknown>".to_string()),
        )
    } else {
        let view = Command::new("gh")
            .args(["pr", "view", "--json", "url"])
            .current_dir(cwd)
            .output()?;
        if !view.status.success() {
            return Err(io::Error::other(command_failure(
                "gh",
                &["pr", "create"],
                &create,
            )));
        }
        (
            "existing",
            parse_pr_json_url(&String::from_utf8_lossy(&view.stdout))
                .unwrap_or_else(|| "<unknown>".to_string()),
        )
    };

    let mut lines = vec![
        "Commit/Push/PR".to_string(),
        format!("  Result           {result}"),
        format!("  Branch           {branch}"),
        format!("  Base             {default_branch}"),
        format!("  Body file        {}", body_path.display()),
        format!("  URL              {url}"),
    ];
    if created_branch {
        lines.insert(2, "  Branch action    created and switched".to_string());
    }
    if let Some(report) = commit_report {
        lines.push(String::new());
        lines.push(report);
    }
    Ok(lines.join("\n"))
}

pub fn detect_default_branch(cwd: &Path) -> io::Result<String> {
    if let Ok(reference) = git_stdout(cwd, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        if let Some(branch) = reference
            .trim()
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
        {
            return Ok(branch.to_string());
        }
    }

    for branch in ["main", "master"] {
        if branch_exists(cwd, branch) {
            return Ok(branch.to_string());
        }
    }

    current_branch(cwd)
}

fn git_stdout(cwd: &Path, args: &[&str]) -> io::Result<String> {
    run_command_stdout("git", args, cwd)
}

fn git_status_ok(cwd: &Path, args: &[&str]) -> io::Result<()> {
    run_command_success("git", args, cwd)
}

fn run_command_stdout(program: &str, args: &[&str], cwd: &Path) -> io::Result<String> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(io::Error::other(command_failure(program, args, &output)));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn run_command_success(program: &str, args: &[&str], cwd: &Path) -> io::Result<()> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(io::Error::other(command_failure(program, args, &output)));
    }
    Ok(())
}

fn command_failure(program: &str, args: &[&str], output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    if detail.is_empty() {
        format!("{program} {} failed", args.join(" "))
    } else {
        format!("{program} {} failed: {detail}", args.join(" "))
    }
}

fn branch_exists(cwd: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(cwd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn current_branch(cwd: &Path) -> io::Result<String> {
    let branch = git_stdout(cwd, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        Err(io::Error::other("unable to determine current git branch"))
    } else {
        Ok(branch.to_string())
    }
}

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn write_temp_text_file(prefix: &str, extension: &str, contents: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = env::temp_dir().join(format!("{prefix}-{nanos}.{extension}"));
    fs::write(&path, contents)?;
    Ok(path)
}

fn build_branch_name(hint: &str) -> String {
    let slug = slugify(hint);
    let owner = env::var("SAFEUSER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("USER")
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
    match owner {
        Some(owner) => format!("{owner}/{slug}"),
        None => slug,
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
}

fn parse_pr_url(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(ToOwned::to_owned)
}

fn parse_pr_json_url(stdout: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()?
        .get("url")?
        .as_str()
        .map(ToOwned::to_owned)
}

#[must_use]
pub fn render_plugins_report(plugins: &[PluginSummary]) -> String {
    let mut lines = vec!["Plugins".to_string()];
    if plugins.is_empty() {
        lines.push("  No plugins installed.".to_string());
        return lines.join("\n");
    }
    for plugin in plugins {
        let enabled = if plugin.enabled {
            "enabled"
        } else {
            "disabled"
        };
        lines.push(format!(
            "  {name:<20} v{version:<10} {enabled}",
            name = plugin.metadata.name,
            version = plugin.metadata.version,
        ));
    }
    lines.join("\n")
}

fn render_plugin_install_report(plugin_id: &str, plugin: Option<&PluginSummary>) -> String {
    let name = plugin.map_or(plugin_id, |plugin| plugin.metadata.name.as_str());
    let version = plugin.map_or("unknown", |plugin| plugin.metadata.version.as_str());
    let enabled = plugin.is_some_and(|plugin| plugin.enabled);
    format!(
        "Plugins\n  Result           installed {plugin_id}\n  Name             {name}\n  Version          {version}\n  Status           {}",
        if enabled { "enabled" } else { "disabled" }
    )
}

fn resolve_plugin_target(
    manager: &PluginManager,
    target: &str,
) -> Result<PluginSummary, PluginError> {
    let mut matches = manager
        .list_installed_plugins()?
        .into_iter()
        .filter(|plugin| plugin.metadata.id == target || plugin.metadata.name == target)
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(PluginError::NotFound(format!(
            "plugin `{target}` is not installed or discoverable"
        ))),
        _ => Err(PluginError::InvalidManifest(format!(
            "plugin name `{target}` is ambiguous; use the full plugin id"
        ))),
    }
}

fn discover_definition_roots(cwd: &Path, leaf: &str) -> Vec<(DefinitionSource, PathBuf)> {
    let codex_home = non_empty_path_env("CODEX_HOME");
    let user_home = cli_user_home_dir();
    discover_definition_roots_from(cwd, leaf, codex_home.as_deref(), user_home.as_deref())
}

fn discover_definition_roots_from(
    cwd: &Path,
    leaf: &str,
    codex_home: Option<&Path>,
    user_home: Option<&Path>,
) -> Vec<(DefinitionSource, PathBuf)> {
    let mut roots = Vec::new();

    for ancestor in cwd.ancestors() {
        push_unique_root(
            &mut roots,
            DefinitionSource::ProjectCodex,
            ancestor.join(".codex").join(leaf),
        );
        push_unique_root(
            &mut roots,
            DefinitionSource::ProjectClaw,
            ancestor.join(".claw").join(leaf),
        );
    }

    if let Some(codex_home) = codex_home {
        push_unique_root(
            &mut roots,
            DefinitionSource::UserCodexHome,
            codex_home.join(leaf),
        );
    }

    if let Some(home) = user_home {
        push_unique_root(
            &mut roots,
            DefinitionSource::UserCodex,
            home.join(".codex").join(leaf),
        );
        push_unique_root(
            &mut roots,
            DefinitionSource::UserClaw,
            home.join(".claw").join(leaf),
        );
    }

    roots
}

fn discover_skill_roots(cwd: &Path) -> Vec<SkillRoot> {
    let codex_home = non_empty_path_env("CODEX_HOME");
    let user_home = cli_user_home_dir();
    discover_skill_roots_from(cwd, codex_home.as_deref(), user_home.as_deref())
}

fn discover_skill_roots_from(
    cwd: &Path,
    codex_home: Option<&Path>,
    user_home: Option<&Path>,
) -> Vec<SkillRoot> {
    let mut roots = Vec::new();

    for ancestor in cwd.ancestors() {
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::ProjectCodex,
            ancestor.join(".codex").join("skills"),
            SkillOrigin::SkillsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::ProjectClaw,
            ancestor.join(".claw").join("skills"),
            SkillOrigin::SkillsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::ProjectCodex,
            ancestor.join(".codex").join("commands"),
            SkillOrigin::LegacyCommandsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::ProjectClaw,
            ancestor.join(".claw").join("commands"),
            SkillOrigin::LegacyCommandsDir,
        );
    }

    if let Some(codex_home) = codex_home {
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserCodexHome,
            codex_home.join("skills"),
            SkillOrigin::SkillsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserCodexHome,
            codex_home.join("commands"),
            SkillOrigin::LegacyCommandsDir,
        );
    }

    if let Some(home) = user_home {
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserCodex,
            home.join(".codex").join("skills"),
            SkillOrigin::SkillsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserCodex,
            home.join(".codex").join("commands"),
            SkillOrigin::LegacyCommandsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserClaw,
            home.join(".claw").join("skills"),
            SkillOrigin::SkillsDir,
        );
        push_unique_skill_root(
            &mut roots,
            DefinitionSource::UserClaw,
            home.join(".claw").join("commands"),
            SkillOrigin::LegacyCommandsDir,
        );
    }

    roots
}

fn non_empty_path_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn cli_user_home_dir() -> Option<PathBuf> {
    preferred_user_home(
        non_empty_path_env("HOME"),
        non_empty_path_env("USERPROFILE"),
    )
}

fn preferred_user_home(home: Option<PathBuf>, user_profile: Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|path| !path.as_os_str().is_empty())
        .or_else(|| user_profile.filter(|path| !path.as_os_str().is_empty()))
}

fn push_unique_root(
    roots: &mut Vec<(DefinitionSource, PathBuf)>,
    source: DefinitionSource,
    path: PathBuf,
) {
    if path.is_dir() && !roots.iter().any(|(_, existing)| existing == &path) {
        roots.push((source, path));
    }
}

fn push_unique_skill_root(
    roots: &mut Vec<SkillRoot>,
    source: DefinitionSource,
    path: PathBuf,
    origin: SkillOrigin,
) {
    if path.is_dir() && !roots.iter().any(|existing| existing.path == path) {
        roots.push(SkillRoot {
            source,
            path,
            origin,
        });
    }
}

fn load_agents_from_roots(
    roots: &[(DefinitionSource, PathBuf)],
) -> std::io::Result<Vec<AgentSummary>> {
    let mut agents = Vec::new();
    let mut active_sources = BTreeMap::<String, DefinitionSource>::new();

    for (source, root) in roots {
        let mut root_agents = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.path().extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let contents = fs::read_to_string(entry.path())?;
            let fallback_name = entry.path().file_stem().map_or_else(
                || entry.file_name().to_string_lossy().to_string(),
                |stem| stem.to_string_lossy().to_string(),
            );
            root_agents.push(AgentSummary {
                name: parse_toml_string(&contents, "name").unwrap_or(fallback_name),
                description: parse_toml_string(&contents, "description"),
                model: parse_toml_string(&contents, "model"),
                reasoning_effort: parse_toml_string(&contents, "model_reasoning_effort"),
                source: *source,
                shadowed_by: None,
            });
        }
        root_agents.sort_by(|left, right| left.name.cmp(&right.name));

        for mut agent in root_agents {
            let key = agent.name.to_ascii_lowercase();
            if let Some(existing) = active_sources.get(&key) {
                agent.shadowed_by = Some(*existing);
            } else {
                active_sources.insert(key, agent.source);
            }
            agents.push(agent);
        }
    }

    Ok(agents)
}

fn load_skills_from_roots(roots: &[SkillRoot]) -> std::io::Result<Vec<SkillSummary>> {
    let mut skills = Vec::new();
    let mut active_sources = BTreeMap::<String, DefinitionSource>::new();

    for root in roots {
        let mut root_skills = Vec::new();
        for entry in fs::read_dir(&root.path)? {
            let entry = entry?;
            match root.origin {
                SkillOrigin::SkillsDir => {
                    if !entry.path().is_dir() {
                        continue;
                    }
                    let skill_path = entry.path().join("SKILL.md");
                    if !skill_path.is_file() {
                        continue;
                    }
                    let contents = fs::read_to_string(skill_path)?;
                    let (name, description) = parse_skill_frontmatter(&contents);
                    root_skills.push(SkillSummary {
                        name: name
                            .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string()),
                        description,
                        source: root.source,
                        shadowed_by: None,
                        origin: root.origin,
                    });
                }
                SkillOrigin::LegacyCommandsDir => {
                    let path = entry.path();
                    let markdown_path = if path.is_dir() {
                        let skill_path = path.join("SKILL.md");
                        if !skill_path.is_file() {
                            continue;
                        }
                        skill_path
                    } else if path
                        .extension()
                        .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("md"))
                    {
                        path
                    } else {
                        continue;
                    };

                    let contents = fs::read_to_string(&markdown_path)?;
                    let fallback_name = markdown_path.file_stem().map_or_else(
                        || entry.file_name().to_string_lossy().to_string(),
                        |stem| stem.to_string_lossy().to_string(),
                    );
                    let (name, description) = parse_skill_frontmatter(&contents);
                    root_skills.push(SkillSummary {
                        name: name.unwrap_or(fallback_name),
                        description,
                        source: root.source,
                        shadowed_by: None,
                        origin: root.origin,
                    });
                }
            }
        }
        root_skills.sort_by(|left, right| left.name.cmp(&right.name));

        for mut skill in root_skills {
            let key = skill.name.to_ascii_lowercase();
            if let Some(existing) = active_sources.get(&key) {
                skill.shadowed_by = Some(*existing);
            } else {
                active_sources.insert(key, skill.source);
            }
            skills.push(skill);
        }
    }

    Ok(skills)
}

fn parse_toml_string(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} =");
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(value) = trimmed.strip_prefix(&prefix) else {
            continue;
        };
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        else {
            continue;
        };
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_skill_frontmatter(contents: &str) -> (Option<String>, Option<String>) {
    let mut lines = contents.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }

    let mut name = None;
    let mut description = None;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("name:") {
            let value = unquote_frontmatter_value(value.trim());
            if !value.is_empty() {
                name = Some(value);
            }
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("description:") {
            let value = unquote_frontmatter_value(value.trim());
            if !value.is_empty() {
                description = Some(value);
            }
        }
    }

    (name, description)
}

fn unquote_frontmatter_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn render_agents_report(agents: &[AgentSummary]) -> String {
    if agents.is_empty() {
        return [
            "Agents",
            "  No agent definition files found.",
            "  Meaning          CLI agent definitions, not GUI model sessions",
            "  Sources          project roots plus $CODEX_HOME and $HOME/$USERPROFILE",
        ]
        .join("\n");
    }

    let total_active = agents
        .iter()
        .filter(|agent| agent.shadowed_by.is_none())
        .count();
    let mut lines = vec![
        "Agents".to_string(),
        format!("  {total_active} active agents"),
        String::new(),
    ];

    for source in [
        DefinitionSource::ProjectCodex,
        DefinitionSource::ProjectClaw,
        DefinitionSource::UserCodexHome,
        DefinitionSource::UserCodex,
        DefinitionSource::UserClaw,
    ] {
        let group = agents
            .iter()
            .filter(|agent| agent.source == source)
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }

        lines.push(format!("{}:", source.label()));
        for agent in group {
            let detail = agent_detail(agent);
            match agent.shadowed_by {
                Some(winner) => lines.push(format!("  (shadowed by {}) {detail}", winner.label())),
                None => lines.push(format!("  {detail}")),
            }
        }
        lines.push(String::new());
    }

    lines.join("\n").trim_end().to_string()
}

fn agent_detail(agent: &AgentSummary) -> String {
    let mut parts = vec![agent.name.clone()];
    if let Some(description) = &agent.description {
        parts.push(description.clone());
    }
    if let Some(model) = &agent.model {
        parts.push(model.clone());
    }
    if let Some(reasoning) = &agent.reasoning_effort {
        parts.push(reasoning.clone());
    }
    parts.join(" · ")
}

fn render_skills_report(skills: &[SkillSummary]) -> String {
    if skills.is_empty() {
        return [
            "Skills",
            "  No CLI-discoverable skills found.",
            "  Meaning          CLI skills, not GUI model-session configuration",
            "  Sources          project roots plus $CODEX_HOME and $HOME/$USERPROFILE; includes legacy /commands",
        ]
        .join("\n");
    }

    let total_active = skills
        .iter()
        .filter(|skill| skill.shadowed_by.is_none())
        .count();
    let mut lines = vec![
        "Skills".to_string(),
        format!("  {total_active} available skills"),
        String::new(),
    ];

    for source in [
        DefinitionSource::ProjectCodex,
        DefinitionSource::ProjectClaw,
        DefinitionSource::UserCodexHome,
        DefinitionSource::UserCodex,
        DefinitionSource::UserClaw,
    ] {
        let group = skills
            .iter()
            .filter(|skill| skill.source == source)
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }

        lines.push(format!("{}:", source.label()));
        for skill in group {
            let mut parts = vec![skill.name.clone()];
            if let Some(description) = &skill.description {
                parts.push(description.clone());
            }
            if let Some(detail) = skill.origin.detail_label() {
                parts.push(detail.to_string());
            }
            let detail = parts.join(" · ");
            match skill.shadowed_by {
                Some(winner) => lines.push(format!("  (shadowed by {}) {detail}", winner.label())),
                None => lines.push(format!("  {detail}")),
            }
        }
        lines.push(String::new());
    }

    lines.join("\n").trim_end().to_string()
}

fn normalize_optional_args(args: Option<&str>) -> Option<&str> {
    args.map(str::trim).filter(|value| !value.is_empty())
}

fn render_agents_usage(unexpected: Option<&str>) -> String {
    let mut lines = vec![
        "Agents".to_string(),
        "  Usage            /agents".to_string(),
        "  Direct CLI       coolzhu-cli agents".to_string(),
        "  Meaning          Agent definition files, not GUI model sessions".to_string(),
        "  Sources          project roots plus $CODEX_HOME and $HOME/$USERPROFILE".to_string(),
    ];
    if let Some(args) = unexpected {
        lines.push(format!("  Unexpected       {args}"));
    }
    lines.join("\n")
}

fn render_skills_usage(unexpected: Option<&str>) -> String {
    let mut lines = vec![
        "Skills".to_string(),
        "  Usage            /skills".to_string(),
        "  Direct CLI       coolzhu-cli skills".to_string(),
        "  Meaning          CLI-discoverable skills; GUI model sessions are separate".to_string(),
        "  Sources          project roots plus $CODEX_HOME and $HOME/$USERPROFILE; includes legacy /commands".to_string(),
    ];
    if let Some(args) = unexpected {
        lines.push(format!("  Unexpected       {args}"));
    }
    lines.join("\n")
}

#[must_use]
pub fn handle_slash_command(
    input: &str,
    session: &Session,
    compaction: CompactionConfig,
) -> Option<SlashCommandResult> {
    match SlashCommand::parse(input)? {
        SlashCommand::Compact => {
            let result = compact_session(session, compaction);
            let message = if result.removed_message_count == 0 {
                "Compaction skipped: session is below the compaction threshold.".to_string()
            } else {
                format!(
                    "Compacted {} messages into a resumable system summary.",
                    result.removed_message_count
                )
            };
            Some(SlashCommandResult {
                message,
                session: result.compacted_session,
            })
        }
        SlashCommand::Help => Some(SlashCommandResult {
            message: render_slash_command_help(),
            session: session.clone(),
        }),
        SlashCommand::Status
        | SlashCommand::Branch { .. }
        | SlashCommand::Bughunter { .. }
        | SlashCommand::Worktree { .. }
        | SlashCommand::Commit
        | SlashCommand::CommitPushPr { .. }
        | SlashCommand::Pr { .. }
        | SlashCommand::Issue { .. }
        | SlashCommand::Ultraplan { .. }
        | SlashCommand::Teleport { .. }
        | SlashCommand::DebugToolCall
        | SlashCommand::Model { .. }
        | SlashCommand::Permissions { .. }
        | SlashCommand::Clear { .. }
        | SlashCommand::Cost
        | SlashCommand::Resume { .. }
        | SlashCommand::Config { .. }
        | SlashCommand::Memory
        | SlashCommand::Init
        | SlashCommand::Diff
        | SlashCommand::Version
        | SlashCommand::Export { .. }
        | SlashCommand::Session { .. }
        | SlashCommand::Plugins { .. }
        | SlashCommand::Agents { .. }
        | SlashCommand::Skills { .. }
        | SlashCommand::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        discover_definition_roots_from, discover_skill_roots_from, handle_branch_slash_command,
        handle_commit_slash_command, handle_plugins_slash_command, handle_slash_command,
        handle_worktree_slash_command, load_agents_from_roots, load_skills_from_roots,
        preferred_user_home, render_agents_report, render_plugins_report, render_skills_report,
        render_slash_command_help, resume_supported_slash_commands, slash_command_specs,
        suggest_slash_commands, DefinitionSource, SkillOrigin, SkillRoot, SlashCommand,
    };
    #[cfg(unix)]
    use super::{handle_commit_push_pr_slash_command, CommitPushPrRequest};
    use plugins::{PluginKind, PluginManager, PluginManagerConfig, PluginMetadata, PluginSummary};
    use runtime::{CompactionConfig, ContentBlock, ConversationMessage, MessageRole, Session};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    use std::env;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("commands-plugin-{label}-{nanos}"))
    }

    /// 打印一行**不被 libtest 捕获**的输出（只给隔离子进程回报哨兵用，故 `#[cfg(unix)]`）。
    ///
    /// 直接写 `std::io::stdout()` 不经过 `print!` 的输出捕获通道，因此父进程无论是否加
    /// `--nocapture` 都能看到子进程真的执行到了断言体（防"过滤条件写错 ⇒ 0 个用例跑过"的假绿）。
    #[cfg(unix)]
    fn announce_env(line: &str) {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }

    /// **显式环境例外**：需要真实外部可执行文件（`git`）的用例必须先调用本函数。
    ///
    /// ## 本 crate 测试已声明的环境要求
    ///
    /// 1. `PATH` 上有可执行的 `git`（`git --version` 成功退出）——`branch_and_worktree_commands_manage_git_state`
    ///    与 `commit_command_stages_and_commits_changes` 需要它；`#[cfg(unix)]` 的
    ///    `commit_push_pr_command_commits_pushes_and_creates_pr` 也需要（它在隔离子进程里跑，
    ///    见该用例的环境隔离说明）；
    /// 2. `std::env::temp_dir()`（Windows 上是 `TEMP`/`TMP`）必须可写：所有用例的仓库、
    ///    插件目录与生产侧临时文件都建在它下面；
    /// 3. `git` 不支持 `init -b main` 时由 `init_git_repo` 走 `git init` + `branch -m main` 回退。
    ///
    /// **未**自动探测、但会影响结果的机器级 git 配置（满足不了时不得为了变绿而放宽断言）：
    /// `commit.gpgsign`、`init.templateDir`、`core.hooksPath`、`core.excludesfile`、
    /// `core.autocrlf` / `core.safecrlf`、`user.useConfigOnly`。它们导致的失败是**真实信号**：
    /// 该机器上这些用例无法按预期运行，要查的是机器配置，不是测试。
    ///
    /// ## 缺前置时为什么是**失败**而不是跳过（RPR-01b 裁决 §5.2）
    ///
    /// 改前这里是 `require_git_or_skip`＋`if !.. { return; }`：libtest 把"什么都没做就返回"
    /// 报成 **ok**，于是**没有 git 的机器**上 `cargo test -p coolzhu-command-router` 照样显示
    /// `19 passed`，门禁拿到的是一片绿，而"git 相关命令在本机一个断言都没跑"完全不可见 ——
    /// 这正是裁决禁止的"用 skip 令发布门禁全绿"：
    /// §5.2 表"该能力属**当前发布必验范围** ⇒ 缺前置 ⇒ 验收未完成，不能用 skip 令发布门禁全绿"；
    /// 组合用例表"新代码误将失败改标环境跳过 ⇒ 门禁**拒绝**这种分类变化"。
    ///
    /// 现在缺前置**明确失败**：真实探测得到的退出状态 / spawn 错误、被探测的程序名、
    /// 以及"这不是产品缺陷、而是本机前置缺失 ⇒ 该能力在本机验收未完成"都写进 panic 消息，
    /// 并以 `[env-missing]` 前缀开头，使其可被**独立统计**（数失败输出里的该前缀行）。
    /// 这里**保留**上一轮 RD4-10 有独立价值的部分：真实探测、环境要求声明、
    /// 以及"探针必须能判别缺失"的负向验证（见 [`missing_program_probe_is_discriminating`]）。
    fn require_program_or_fail(test_name: &str, program: &str) {
        if let Err(reason) = program_probe(program, &["--version"]) {
            panic!("{}", env_missing_message(test_name, program, &reason));
        }
    }

    /// 真实探测：启动 `program` 并检查退出状态。`Err` 带上**可读原因**（不是布尔）。
    fn program_probe(program: &str, args: &[&str]) -> Result<(), String> {
        match Command::new(program).args(args).output() {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(format!(
                "`{program} {}` 退出状态 {:?}",
                args.join(" "),
                output.status.code()
            )),
            Err(error) => Err(format!("无法执行 `{program}`（{error}）")),
        }
    }

    /// `[env-missing]` 文案即**分类契约**：门禁/独立统计按此前缀识别"前置缺失、未验证"，
    /// 与"断言失败"分开报告。单独成函数是为了让前缀与措辞可被用例断言钉住。
    fn env_missing_message(test_name: &str, program: &str, reason: &str) -> String {
        format!(
            "[env-missing] {test_name}: 本机未验证——{reason}。\
             本用例需要真实可用的 `{program}`，缺前置时它**不验证任何本 crate 的行为**，\
             因此**不是通过**：该能力在本机**验收未完成**（这也不是产品缺陷）。\
             独立统计口径：数失败输出里的 [env-missing] 行。"
        )
    }

    /// 判别性（RPR-01b）：前置探测**不是恒真**——一个不可能存在的可执行文件名必须被判为"不满足"，
    /// 且原因里必须带上被探测的程序名；同一个探测函数对**确实存在**的可执行文件必须判为"满足"。
    ///
    /// 这正是上一轮 RD4-10 手工做过的那次负向验证（"把探针程序名改成不存在的可执行文件"），
    /// 这里把它固化成**可执行断言**，同时把 `[env-missing]` 分类契约钉住 ——
    /// 而不是只留一段一次性手工记录。对照组用测试二进制自身＋`--list`
    /// （`env::current_exe()` 必然存在；**不能**用无参数调用，那会递归重跑整个测试套件）。
    #[test]
    fn missing_program_probe_is_discriminating() {
        let absent = "claw-rpr01b-absent-executable";
        let reason = program_probe(absent, &["--version"]).expect_err("不存在的程序不得判为满足");
        assert!(
            reason.contains(absent),
            "缺前置的原因必须指明被探测的程序名：{reason}"
        );

        let exe = std::env::current_exe().expect("current exe");
        assert!(
            program_probe(&exe.to_string_lossy(), &["--list"]).is_ok(),
            "同一探测对确实存在的可执行文件必须判为满足（对照组）"
        );

        let message = env_missing_message("some_test", "git", &reason);
        assert!(
            message.starts_with("[env-missing] some_test:"),
            "分类契约：前缀 + 用例名：{message}"
        );
        assert!(message.contains("git"), "必须指明缺的是什么程序：{message}");
        assert!(
            message.contains("验收未完成"),
            "必须如实说明这是未验证而不是产品缺陷：{message}"
        );
    }

    fn run_command(cwd: &Path, program: &str, args: &[&str]) -> String {
        let output = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("command should run");
        assert!(
            output.status.success(),
            "{} {} failed: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("stdout should be utf8")
    }

    fn init_git_repo(label: &str) -> PathBuf {
        let root = temp_dir(label);
        fs::create_dir_all(&root).expect("repo root");

        let init = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&root)
            .output()
            .expect("git init should run");
        if !init.status.success() {
            let fallback = Command::new("git")
                .arg("init")
                .current_dir(&root)
                .output()
                .expect("fallback git init should run");
            assert!(
                fallback.status.success(),
                "fallback git init should succeed"
            );
            let rename = Command::new("git")
                .args(["branch", "-m", "main"])
                .current_dir(&root)
                .output()
                .expect("git branch -m should run");
            assert!(rename.status.success(), "git branch -m main should succeed");
        }

        run_command(&root, "git", &["config", "user.name", "Claw Tests"]);
        run_command(&root, "git", &["config", "user.email", "claw@example.com"]);
        fs::write(root.join("README.md"), "seed\n").expect("seed file");
        run_command(&root, "git", &["add", "README.md"]);
        run_command(&root, "git", &["commit", "-m", "chore: seed repo"]);
        root
    }

    /// 隔离子进程的**显式**环境契约（父进程用 `Command::env` 提供，子进程只读）：
    /// 区分"子进程身份"、夹具位置与"断言体已执行"的哨兵。
    ///
    /// 这样做的好处正是裁决要求的"避免改变父测试进程的环境"：`PATH`/`SAFEUSER` 只出现在
    /// 子进程的环境块里，父测试进程全程不变（用例结尾直接断言）。
    #[cfg(unix)]
    const ISOLATED_CHILD_ENV: &str = "CLAW_RPR01B_ISOLATED_CHILD";
    #[cfg(unix)]
    const ISOLATED_REPO_ENV: &str = "CLAW_RPR01B_ISOLATED_REPO";
    #[cfg(unix)]
    const ISOLATED_GH_LOG_ENV: &str = "CLAW_RPR01B_ISOLATED_GH_LOG";
    /// 子进程在执行到断言体时回报的哨兵（绕过 libtest 捕获的裸 stdout 行）。
    #[cfg(unix)]
    const ISOLATED_BODY_SENTINEL: &str = "[rpr01b-isolated-body-ran]";

    #[cfg(unix)]
    fn init_bare_repo(label: &str) -> PathBuf {
        let root = temp_dir(label);
        let output = Command::new("git")
            .args(["init", "--bare"])
            .arg(&root)
            .output()
            .expect("bare repo should initialize");
        assert!(output.status.success(), "git init --bare should succeed");
        root
    }

    #[cfg(unix)]
    fn write_fake_gh(bin_dir: &Path, log_path: &Path, url: &str) {
        fs::create_dir_all(bin_dir).expect("bin dir");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo 'gh 1.0.0'\n  exit 0\nfi\nprintf '%s\\n' \"$*\" >> \"{}\"\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"create\" ]; then\n  echo '{}'\n  exit 0\nfi\nif [ \"$1\" = \"pr\" ] && [ \"$2\" = \"view\" ]; then\n  echo '{{\"url\":\"{}\"}}'\n  exit 0\nfi\nexit 0\n",
            log_path.display(),
            url,
            url,
        );
        let path = bin_dir.join("gh");
        fs::write(&path, script).expect("gh stub");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod");
    }

    fn write_external_plugin(root: &Path, name: &str, version: &str) {
        fs::create_dir_all(root.join(".claw-plugin")).expect("manifest dir");
        fs::write(
            root.join(".claw-plugin").join("plugin.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \"description\": \"commands plugin\"\n}}"
            ),
        )
        .expect("write manifest");
    }

    fn write_bundled_plugin(root: &Path, name: &str, version: &str, default_enabled: bool) {
        fs::create_dir_all(root.join(".claw-plugin")).expect("manifest dir");
        fs::write(
            root.join(".claw-plugin").join("plugin.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \"description\": \"bundled commands plugin\",\n  \"defaultEnabled\": {}\n}}",
                if default_enabled { "true" } else { "false" }
            ),
        )
        .expect("write bundled manifest");
    }

    fn write_agent(root: &Path, name: &str, description: &str, model: &str, reasoning: &str) {
        fs::create_dir_all(root).expect("agent root");
        fs::write(
            root.join(format!("{name}.toml")),
            format!(
                "name = \"{name}\"\ndescription = \"{description}\"\nmodel = \"{model}\"\nmodel_reasoning_effort = \"{reasoning}\"\n"
            ),
        )
        .expect("write agent");
    }

    fn write_skill(root: &Path, name: &str, description: &str) {
        let skill_root = root.join(name);
        fs::create_dir_all(&skill_root).expect("skill root");
        fs::write(
            skill_root.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .expect("write skill");
    }

    fn write_legacy_command(root: &Path, name: &str, description: &str) {
        fs::create_dir_all(root).expect("commands root");
        fs::write(
            root.join(format!("{name}.md")),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .expect("write command");
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn parses_supported_slash_commands() {
        assert_eq!(SlashCommand::parse("/help"), Some(SlashCommand::Help));
        assert_eq!(SlashCommand::parse(" /status "), Some(SlashCommand::Status));
        assert_eq!(
            SlashCommand::parse("/bughunter runtime"),
            Some(SlashCommand::Bughunter {
                scope: Some("runtime".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/branch create feature/demo"),
            Some(SlashCommand::Branch {
                action: Some("create".to_string()),
                target: Some("feature/demo".to_string()),
            })
        );
        assert_eq!(
            SlashCommand::parse("/worktree add ../demo wt-demo"),
            Some(SlashCommand::Worktree {
                action: Some("add".to_string()),
                path: Some("../demo".to_string()),
                branch: Some("wt-demo".to_string()),
            })
        );
        assert_eq!(SlashCommand::parse("/commit"), Some(SlashCommand::Commit));
        assert_eq!(
            SlashCommand::parse("/commit-push-pr ready for review"),
            Some(SlashCommand::CommitPushPr {
                context: Some("ready for review".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/pr ready for review"),
            Some(SlashCommand::Pr {
                context: Some("ready for review".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/issue flaky test"),
            Some(SlashCommand::Issue {
                context: Some("flaky test".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/ultraplan ship both features"),
            Some(SlashCommand::Ultraplan {
                task: Some("ship both features".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/teleport conversation.rs"),
            Some(SlashCommand::Teleport {
                target: Some("conversation.rs".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/debug-tool-call"),
            Some(SlashCommand::DebugToolCall)
        );
        assert_eq!(
            SlashCommand::parse("/model opus"),
            Some(SlashCommand::Model {
                model: Some("opus".to_string()),
            })
        );
        assert_eq!(
            SlashCommand::parse("/model"),
            Some(SlashCommand::Model { model: None })
        );
        assert_eq!(
            SlashCommand::parse("/permissions read-only"),
            Some(SlashCommand::Permissions {
                mode: Some("read-only".to_string()),
            })
        );
        assert_eq!(
            SlashCommand::parse("/clear"),
            Some(SlashCommand::Clear { confirm: false })
        );
        assert_eq!(
            SlashCommand::parse("/clear --confirm"),
            Some(SlashCommand::Clear { confirm: true })
        );
        assert_eq!(SlashCommand::parse("/cost"), Some(SlashCommand::Cost));
        assert_eq!(
            SlashCommand::parse("/resume session.json"),
            Some(SlashCommand::Resume {
                session_path: Some("session.json".to_string()),
            })
        );
        assert_eq!(
            SlashCommand::parse("/config"),
            Some(SlashCommand::Config { section: None })
        );
        assert_eq!(
            SlashCommand::parse("/config env"),
            Some(SlashCommand::Config {
                section: Some("env".to_string())
            })
        );
        assert_eq!(SlashCommand::parse("/memory"), Some(SlashCommand::Memory));
        assert_eq!(SlashCommand::parse("/init"), Some(SlashCommand::Init));
        assert_eq!(SlashCommand::parse("/diff"), Some(SlashCommand::Diff));
        assert_eq!(SlashCommand::parse("/version"), Some(SlashCommand::Version));
        assert_eq!(
            SlashCommand::parse("/export notes.txt"),
            Some(SlashCommand::Export {
                path: Some("notes.txt".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/session switch abc123"),
            Some(SlashCommand::Session {
                action: Some("switch".to_string()),
                target: Some("abc123".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/plugins install demo"),
            Some(SlashCommand::Plugins {
                action: Some("install".to_string()),
                target: Some("demo".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/plugins list"),
            Some(SlashCommand::Plugins {
                action: Some("list".to_string()),
                target: None
            })
        );
        assert_eq!(
            SlashCommand::parse("/plugins enable demo"),
            Some(SlashCommand::Plugins {
                action: Some("enable".to_string()),
                target: Some("demo".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/plugins disable demo"),
            Some(SlashCommand::Plugins {
                action: Some("disable".to_string()),
                target: Some("demo".to_string())
            })
        );
    }

    #[test]
    fn renders_help_from_shared_specs() {
        let help = render_slash_command_help();
        assert!(help.contains("available via coolzhu-cli --resume SESSION.json"));
        assert!(help.contains("Core flow"));
        assert!(help.contains("Workspace & memory"));
        assert!(help.contains("Sessions & output"));
        assert!(help.contains("Git & GitHub"));
        assert!(help.contains("Automation & discovery"));
        assert!(help.contains("/help"));
        assert!(help.contains("/status"));
        assert!(help.contains("/compact"));
        assert!(help.contains("/bughunter [scope]"));
        assert!(help.contains("/branch [list|create <name>|switch <name>]"));
        assert!(help.contains("/worktree [list|add <path> [branch]|remove <path>|prune]"));
        assert!(help.contains("/commit"));
        assert!(help.contains("/commit-push-pr [context]"));
        assert!(help.contains("/pr [context]"));
        assert!(help.contains("/issue [context]"));
        assert!(help.contains("/ultraplan [task]"));
        assert!(help.contains("/teleport <symbol-or-path>"));
        assert!(help.contains("/debug-tool-call"));
        assert!(help.contains("/model [model]"));
        assert!(help.contains("/permissions [read-only|workspace-write|danger-full-access]"));
        assert!(help.contains("/clear [--confirm]"));
        assert!(help.contains("/cost"));
        assert!(help.contains("/resume <session-path>"));
        assert!(help.contains("/config [env|hooks|model|agent|plugins]"));
        assert!(help.contains("/memory"));
        assert!(help.contains("/init"));
        assert!(help.contains("/diff"));
        assert!(help.contains("/version"));
        assert!(help.contains("/export [file]"));
        assert!(help.contains("/session [list|switch <session-id>]"));
        assert!(help.contains(
            "/plugin [list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]"
        ));
        assert!(help.contains("aliases: /plugins, /marketplace"));
        assert!(help.contains("/agents"));
        assert!(help.contains("/skills"));
        assert_eq!(slash_command_specs().len(), 28);
        assert_eq!(resume_supported_slash_commands().len(), 13);
    }

    #[test]
    fn suggests_close_slash_commands() {
        let suggestions = suggest_slash_commands("stats", 3);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0], "/status");
    }

    #[test]
    fn compacts_sessions_via_slash_command() {
        let session = Session {
            version: 1,
            messages: vec![
                ConversationMessage::user_text("a ".repeat(200)),
                ConversationMessage::assistant(vec![ContentBlock::Text {
                    text: "b ".repeat(200),
                }]),
                ConversationMessage::tool_result("1", "bash", "ok ".repeat(200), false),
                ConversationMessage::assistant(vec![ContentBlock::Text {
                    text: "recent".to_string(),
                }]),
            ],
        };

        let result = handle_slash_command(
            "/compact",
            &session,
            CompactionConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 1,
            },
        )
        .expect("slash command should be handled");

        assert!(result.message.contains("Compacted 2 messages"));
        assert_eq!(result.session.messages[0].role, MessageRole::System);
    }

    #[test]
    fn help_command_is_non_mutating() {
        let session = Session::new();
        let result = handle_slash_command("/help", &session, CompactionConfig::default())
            .expect("help command should be handled");
        assert_eq!(result.session, session);
        assert!(result.message.contains("Slash commands"));
    }

    #[test]
    fn ignores_unknown_or_runtime_bound_slash_commands() {
        let session = Session::new();
        assert!(handle_slash_command("/unknown", &session, CompactionConfig::default()).is_none());
        assert!(handle_slash_command("/status", &session, CompactionConfig::default()).is_none());
        assert!(
            handle_slash_command("/branch list", &session, CompactionConfig::default()).is_none()
        );
        assert!(
            handle_slash_command("/bughunter", &session, CompactionConfig::default()).is_none()
        );
        assert!(
            handle_slash_command("/worktree list", &session, CompactionConfig::default()).is_none()
        );
        assert!(handle_slash_command("/commit", &session, CompactionConfig::default()).is_none());
        assert!(handle_slash_command(
            "/commit-push-pr review notes",
            &session,
            CompactionConfig::default()
        )
        .is_none());
        assert!(handle_slash_command("/pr", &session, CompactionConfig::default()).is_none());
        assert!(handle_slash_command("/issue", &session, CompactionConfig::default()).is_none());
        assert!(
            handle_slash_command("/ultraplan", &session, CompactionConfig::default()).is_none()
        );
        assert!(
            handle_slash_command("/teleport foo", &session, CompactionConfig::default()).is_none()
        );
        assert!(
            handle_slash_command("/debug-tool-call", &session, CompactionConfig::default())
                .is_none()
        );
        assert!(
            handle_slash_command("/model sonnet", &session, CompactionConfig::default()).is_none()
        );
        assert!(handle_slash_command(
            "/permissions read-only",
            &session,
            CompactionConfig::default()
        )
        .is_none());
        assert!(handle_slash_command("/clear", &session, CompactionConfig::default()).is_none());
        assert!(
            handle_slash_command("/clear --confirm", &session, CompactionConfig::default())
                .is_none()
        );
        assert!(handle_slash_command("/cost", &session, CompactionConfig::default()).is_none());
        assert!(handle_slash_command(
            "/resume session.json",
            &session,
            CompactionConfig::default()
        )
        .is_none());
        assert!(handle_slash_command("/config", &session, CompactionConfig::default()).is_none());
        assert!(
            handle_slash_command("/config env", &session, CompactionConfig::default()).is_none()
        );
        assert!(handle_slash_command("/diff", &session, CompactionConfig::default()).is_none());
        assert!(handle_slash_command("/version", &session, CompactionConfig::default()).is_none());
        assert!(
            handle_slash_command("/export note.txt", &session, CompactionConfig::default())
                .is_none()
        );
        assert!(
            handle_slash_command("/session list", &session, CompactionConfig::default()).is_none()
        );
        assert!(
            handle_slash_command("/plugins list", &session, CompactionConfig::default()).is_none()
        );
    }

    #[test]
    fn renders_plugins_report_with_name_version_and_status() {
        let rendered = render_plugins_report(&[
            PluginSummary {
                metadata: PluginMetadata {
                    id: "demo@external".to_string(),
                    name: "demo".to_string(),
                    version: "1.2.3".to_string(),
                    description: "demo plugin".to_string(),
                    kind: PluginKind::External,
                    source: "demo".to_string(),
                    default_enabled: false,
                    root: None,
                },
                enabled: true,
            },
            PluginSummary {
                metadata: PluginMetadata {
                    id: "sample@external".to_string(),
                    name: "sample".to_string(),
                    version: "0.9.0".to_string(),
                    description: "sample plugin".to_string(),
                    kind: PluginKind::External,
                    source: "sample".to_string(),
                    default_enabled: false,
                    root: None,
                },
                enabled: false,
            },
        ]);

        assert!(rendered.contains("demo"));
        assert!(rendered.contains("v1.2.3"));
        assert!(rendered.contains("enabled"));
        assert!(rendered.contains("sample"));
        assert!(rendered.contains("v0.9.0"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn lists_agents_from_project_and_user_roots() {
        let workspace = temp_dir("agents-workspace");
        let project_agents = workspace.join(".codex").join("agents");
        let user_home = temp_dir("agents-home");
        let user_agents = user_home.join(".codex").join("agents");

        write_agent(
            &project_agents,
            "planner",
            "Project planner",
            "gpt-5.4",
            "medium",
        );
        write_agent(
            &user_agents,
            "planner",
            "User planner",
            "gpt-5.4-mini",
            "high",
        );
        write_agent(
            &user_agents,
            "verifier",
            "Verification agent",
            "gpt-5.4-mini",
            "high",
        );

        let roots = vec![
            (DefinitionSource::ProjectCodex, project_agents),
            (DefinitionSource::UserCodex, user_agents),
        ];
        let report =
            render_agents_report(&load_agents_from_roots(&roots).expect("agent roots should load"));

        assert!(report.contains("Agents"));
        assert!(report.contains("2 active agents"));
        assert!(report.contains("Project (.codex):"));
        assert!(report.contains("planner · Project planner · gpt-5.4 · medium"));
        assert!(report.contains("User (~/.codex):"));
        assert!(report.contains("(shadowed by Project (.codex)) planner · User planner"));
        assert!(report.contains("verifier · Verification agent · gpt-5.4-mini · high"));

        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(user_home);
    }

    #[test]
    fn lists_skills_from_project_and_user_roots() {
        let workspace = temp_dir("skills-workspace");
        let project_skills = workspace.join(".codex").join("skills");
        let project_commands = workspace.join(".claw").join("commands");
        let user_home = temp_dir("skills-home");
        let user_skills = user_home.join(".codex").join("skills");

        write_skill(&project_skills, "plan", "Project planning guidance");
        write_legacy_command(&project_commands, "deploy", "Legacy deployment guidance");
        write_skill(&user_skills, "plan", "User planning guidance");
        write_skill(&user_skills, "help", "Help guidance");

        let roots = vec![
            SkillRoot {
                source: DefinitionSource::ProjectCodex,
                path: project_skills,
                origin: SkillOrigin::SkillsDir,
            },
            SkillRoot {
                source: DefinitionSource::ProjectClaw,
                path: project_commands,
                origin: SkillOrigin::LegacyCommandsDir,
            },
            SkillRoot {
                source: DefinitionSource::UserCodex,
                path: user_skills,
                origin: SkillOrigin::SkillsDir,
            },
        ];
        let report =
            render_skills_report(&load_skills_from_roots(&roots).expect("skill roots should load"));

        assert!(report.contains("Skills"));
        assert!(report.contains("3 available skills"));
        assert!(report.contains("Project (.codex):"));
        assert!(report.contains("plan · Project planning guidance"));
        assert!(report.contains("Project (.claw):"));
        assert!(report.contains("deploy · Legacy deployment guidance · legacy /commands"));
        assert!(report.contains("User (~/.codex):"));
        assert!(report.contains("(shadowed by Project (.codex)) plan · User planning guidance"));
        assert!(report.contains("help · Help guidance"));

        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(user_home);
    }

    #[test]
    fn userprofile_is_used_when_home_is_missing() {
        let user_profile = temp_dir("windows-userprofile");
        assert_eq!(
            preferred_user_home(None, Some(user_profile.clone())),
            Some(user_profile.clone())
        );
        assert_eq!(
            preferred_user_home(Some(PathBuf::new()), Some(user_profile.clone())),
            Some(user_profile.clone())
        );

        let home = temp_dir("explicit-home");
        assert_eq!(
            preferred_user_home(Some(home.clone()), Some(user_profile)),
            Some(home)
        );
    }

    #[test]
    fn discovery_includes_codex_home_and_windows_userprofile_roots() {
        let workspace = temp_dir("discovery-workspace");
        let codex_home = temp_dir("discovery-codex-home");
        let user_profile = temp_dir("discovery-userprofile");
        write_agent(
            &codex_home.join("agents"),
            "codex-home-agent",
            "CODEX_HOME agent",
            "gpt-5.4",
            "high",
        );
        write_agent(
            &user_profile.join(".codex").join("agents"),
            "profile-agent",
            "USERPROFILE agent",
            "gpt-5.4-mini",
            "medium",
        );
        write_skill(
            &codex_home.join("skills"),
            "codex-home-skill",
            "CODEX_HOME skill",
        );
        write_skill(
            &user_profile.join(".codex").join("skills"),
            "profile-skill",
            "USERPROFILE skill",
        );

        let agent_roots = discover_definition_roots_from(
            &workspace,
            "agents",
            Some(&codex_home),
            Some(&user_profile),
        );
        let agents = load_agents_from_roots(&agent_roots).expect("agent roots should load");
        assert!(agents.iter().any(|agent| agent.name == "codex-home-agent"));
        assert!(agents.iter().any(|agent| agent.name == "profile-agent"));

        let skill_roots =
            discover_skill_roots_from(&workspace, Some(&codex_home), Some(&user_profile));
        let skills = load_skills_from_roots(&skill_roots).expect("skill roots should load");
        assert!(skills.iter().any(|skill| skill.name == "codex-home-skill"));
        assert!(skills.iter().any(|skill| skill.name == "profile-skill"));

        let _ = fs::remove_dir_all(workspace);
        let _ = fs::remove_dir_all(codex_home);
        let _ = fs::remove_dir_all(user_profile);
    }

    #[test]
    fn empty_discovery_reports_explain_cli_and_gui_domains() {
        let agents = render_agents_report(&[]);
        assert!(agents.contains("agent definition files"));
        assert!(agents.contains("not GUI model sessions"));

        let skills = render_skills_report(&[]);
        assert!(skills.contains("CLI-discoverable skills"));
        assert!(skills.contains("not GUI model-session configuration"));
    }

    #[test]
    fn agents_and_skills_usage_support_help_and_unexpected_args() {
        let cwd = temp_dir("slash-usage");

        let agents_help =
            super::handle_agents_slash_command(Some("help"), &cwd).expect("agents help");
        assert!(agents_help.contains("Usage            /agents"));
        assert!(agents_help.contains("Direct CLI       coolzhu-cli agents"));
        assert!(agents_help.contains("not GUI model sessions"));

        let agents_unexpected =
            super::handle_agents_slash_command(Some("show planner"), &cwd).expect("agents usage");
        assert!(agents_unexpected.contains("Unexpected       show planner"));

        let skills_help =
            super::handle_skills_slash_command(Some("--help"), &cwd).expect("skills help");
        assert!(skills_help.contains("Usage            /skills"));
        assert!(skills_help.contains("legacy /commands"));

        let skills_unexpected =
            super::handle_skills_slash_command(Some("show help"), &cwd).expect("skills usage");
        assert!(skills_unexpected.contains("Unexpected       show help"));

        let _ = fs::remove_dir_all(cwd);
    }

    #[test]
    fn parses_quoted_skill_frontmatter_values() {
        let contents = "---\nname: \"hud\"\ndescription: 'Quoted description'\n---\n";
        let (name, description) = super::parse_skill_frontmatter(contents);
        assert_eq!(name.as_deref(), Some("hud"));
        assert_eq!(description.as_deref(), Some("Quoted description"));
    }

    #[test]
    fn installs_plugin_from_path_and_lists_it() {
        let config_home = temp_dir("home");
        let source_root = temp_dir("source");
        write_external_plugin(&source_root, "demo", "1.0.0");

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let install = handle_plugins_slash_command(
            Some("install"),
            Some(source_root.to_str().expect("utf8 path")),
            &mut manager,
        )
        .expect("install command should succeed");
        assert!(install.reload_runtime);
        assert!(install.message.contains("installed demo@external"));
        assert!(install.message.contains("Name             demo"));
        assert!(install.message.contains("Version          1.0.0"));
        assert!(install.message.contains("Status           disabled"));

        let list = handle_plugins_slash_command(Some("list"), None, &mut manager)
            .expect("list command should succeed");
        assert!(!list.reload_runtime);
        assert!(list.message.contains("demo"));
        assert!(list.message.contains("v1.0.0"));
        assert!(list.message.contains("disabled"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn enables_and_disables_plugin_by_name() {
        let config_home = temp_dir("toggle-home");
        let source_root = temp_dir("toggle-source");
        write_external_plugin(&source_root, "demo", "1.0.0");

        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        handle_plugins_slash_command(
            Some("install"),
            Some(source_root.to_str().expect("utf8 path")),
            &mut manager,
        )
        .expect("install command should succeed");

        let disable = handle_plugins_slash_command(Some("disable"), Some("demo"), &mut manager)
            .expect("disable command should succeed");
        assert!(disable.reload_runtime);
        assert!(disable.message.contains("disabled demo@external"));
        assert!(disable.message.contains("Name             demo"));
        assert!(disable.message.contains("Status           disabled"));

        let list = handle_plugins_slash_command(Some("list"), None, &mut manager)
            .expect("list command should succeed");
        assert!(list.message.contains("demo"));
        assert!(list.message.contains("disabled"));

        let enable = handle_plugins_slash_command(Some("enable"), Some("demo"), &mut manager)
            .expect("enable command should succeed");
        assert!(enable.reload_runtime);
        assert!(enable.message.contains("enabled demo@external"));
        assert!(enable.message.contains("Name             demo"));
        assert!(enable.message.contains("Status           enabled"));

        let list = handle_plugins_slash_command(Some("list"), None, &mut manager)
            .expect("list command should succeed");
        assert!(list.message.contains("demo"));
        assert!(list.message.contains("enabled"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn lists_auto_installed_bundled_plugins_with_status() {
        let config_home = temp_dir("bundled-home");
        let bundled_root = temp_dir("bundled-root");
        let bundled_plugin = bundled_root.join("starter");
        write_bundled_plugin(&bundled_plugin, "starter", "0.1.0", false);

        let mut config = PluginManagerConfig::new(&config_home);
        config.bundled_root = Some(bundled_root.clone());
        let mut manager = PluginManager::new(config);

        let list = handle_plugins_slash_command(Some("list"), None, &mut manager)
            .expect("list command should succeed");
        assert!(!list.reload_runtime);
        assert!(list.message.contains("starter"));
        assert!(list.message.contains("v0.1.0"));
        assert!(list.message.contains("disabled"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(bundled_root);
    }

    #[test]
    fn branch_and_worktree_commands_manage_git_state() {
        // given
        // 显式环境例外：本用例需要真实 `git`（见 `require_program_or_fail` 的环境要求声明）。
        require_program_or_fail("branch_and_worktree_commands_manage_git_state", "git");
        let repo = init_git_repo("branch-worktree");
        let worktree_path = repo
            .parent()
            .expect("repo should have parent")
            .join("branch-worktree-linked");

        // when
        let branch_list =
            handle_branch_slash_command(Some("list"), None, &repo).expect("branch list succeeds");
        let created = handle_branch_slash_command(Some("create"), Some("feature/demo"), &repo)
            .expect("branch create succeeds");
        let switched = handle_branch_slash_command(Some("switch"), Some("main"), &repo)
            .expect("branch switch succeeds");
        let added = handle_worktree_slash_command(
            Some("add"),
            Some(worktree_path.to_str().expect("utf8 path")),
            Some("wt-demo"),
            &repo,
        )
        .expect("worktree add succeeds");
        let listed_worktrees =
            handle_worktree_slash_command(Some("list"), None, None, &repo).expect("list succeeds");
        let removed = handle_worktree_slash_command(
            Some("remove"),
            Some(worktree_path.to_str().expect("utf8 path")),
            None,
            &repo,
        )
        .expect("remove succeeds");

        // then
        assert!(branch_list.contains("main"));
        assert!(created.contains("feature/demo"));
        assert!(switched.contains("main"));
        assert!(added.contains("wt-demo"));
        let listed_worktrees_normalized = listed_worktrees.replace('\\', "/");
        let expected_worktree_path = worktree_path.to_string_lossy().replace('\\', "/");
        assert!(
            listed_worktrees_normalized.contains(&expected_worktree_path),
            "expected listed worktrees to contain {expected_worktree_path}, got {listed_worktrees}"
        );
        assert!(removed.contains("Result           removed"));

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(worktree_path);
    }

    #[test]
    fn commit_command_stages_and_commits_changes() {
        // given
        // 显式环境例外：本用例需要真实 `git`（见 `require_program_or_fail` 的环境要求声明）。
        require_program_or_fail("commit_command_stages_and_commits_changes", "git");
        let repo = init_git_repo("commit-command");
        fs::write(repo.join("notes.txt"), "hello\n").expect("write notes");

        // when
        let report =
            handle_commit_slash_command("feat: add notes", &repo).expect("commit succeeds");
        let status = run_command(&repo, "git", &["status", "--short"]);
        let message = run_command(&repo, "git", &["log", "-1", "--pretty=%B"]);

        // then
        assert!(report.contains("Result           created"));
        assert!(status.trim().is_empty());
        assert_eq!(message.trim(), "feat: add notes");

        let _ = fs::remove_dir_all(repo);
    }

    /// ## Unix 子项的环境隔离（RPR-01b 裁决：Unix 不得只加一把局部锁就宣称线程安全）
    ///
    /// `handle_commit_push_pr_slash_command` 会在**本进程**里解析 `PATH`（判断/启动 `gh`），
    /// `build_branch_name` 还会读 `SAFEUSER` / `USER` 拼分支名；生产 API **没有**环境注入接缝，
    /// 所以要在父测试进程里构造这个场景就只能 `set_var`。改前正是那样写的：
    /// `env::set_var("PATH"/"SAFEUSER")` + 末尾手写恢复，且它那把 `env_lock` **只有本用例持**，
    /// 别处按 `PATH` 找 `git`/`sh` 的用例并不持锁 —— 断言失败或提前返回时，
    /// **当前测试进程的后续执行**就会一直读到被写入的 PATH/SAFEUSER（直到恢复或进程结束）。
    /// 注意措辞：污染的是**本测试进程后续执行**，不是用户系统环境变量被永久改写。
    ///
    /// 裁决给的方向是"必须改进程级环境时，复用同一进程统一锁 + RAII；**Unix 尤其不能只加一把
    /// 局部锁就宣称线程安全完整**（其它线程与依赖库可能直接读环境）⇒ 这类测试**优先用隔离子进程**"。
    /// 因此这里不再改父进程环境：父用例只准备夹具，再用 [`Command::env`] 把 `PATH`/`SAFEUSER`
    /// **显式**交给一个只跑本用例的隔离子进程；子进程改的也只是它自己的环境。
    ///
    /// 假绿防护：父用例断言子进程退出成功、断言子进程输出了哨兵行、且断言子进程报告
    /// `1 passed` —— 过滤条件写错导致"0 个用例跑过"会被这三点抓住。
    #[cfg(unix)]
    #[test]
    fn commit_push_pr_command_commits_pushes_and_creates_pr() {
        // 子进程分支：环境已由父进程用 Command::env 显式给定，这里只做真实断言。
        // 必须**同时**看到身份标志与夹具位置才走断言体：万一外层环境漏进了身份标志而没有夹具，
        // 走父分支（自己建夹具）比拿着空路径去断言安全。
        if env::var_os(ISOLATED_CHILD_ENV).is_some() && env::var_os(ISOLATED_REPO_ENV).is_some() {
            run_commit_push_pr_body();
            return;
        }

        // 父分支：只准备夹具，**全程不改本进程任何环境变量**。
        let path_before = env::var_os("PATH");
        let safeuser_before = env::var_os("SAFEUSER");

        // 本用例需要真实 `git`（显式环境例外；缺前置 ⇒ 明确失败，不跳过）。
        require_program_or_fail(
            "commit_push_pr_command_commits_pushes_and_creates_pr",
            "git",
        );

        let repo = init_git_repo("commit-push-pr");
        let remote = init_bare_repo("commit-push-pr-remote");
        run_command(
            &repo,
            "git",
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().expect("utf8 remote"),
            ],
        );
        run_command(&repo, "git", &["push", "-u", "origin", "main"]);
        fs::write(repo.join("feature.txt"), "feature\n").expect("write feature file");

        let fake_bin = temp_dir("fake-gh-bin");
        let gh_log = fake_bin.join("gh.log");
        write_fake_gh(&fake_bin, &gh_log, "https://example.com/pr/123");

        // 子进程 PATH：桩目录 + 父进程原 PATH（父进程自身**不变**）。
        let mut isolated_path = fake_bin.display().to_string();
        if let Some(path) = &path_before {
            isolated_path.push(':');
            isolated_path.push_str(&path.to_string_lossy());
        }

        let exe = env::current_exe().expect("current exe");
        let output = Command::new(exe)
            .args([
                "--exact",
                "tests::commit_push_pr_command_commits_pushes_and_creates_pr",
                "--nocapture",
            ])
            .env(ISOLATED_CHILD_ENV, "1")
            .env(ISOLATED_REPO_ENV, &repo)
            .env(ISOLATED_GH_LOG_ENV, &gh_log)
            .env("PATH", &isolated_path)
            .env("SAFEUSER", "tester")
            .output()
            .expect("isolated child should run");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "隔离子进程必须成功：status={:?}\nstdout={stdout}\nstderr={stderr}",
            output.status.code()
        );
        assert!(
            stdout.contains(ISOLATED_BODY_SENTINEL),
            "隔离子进程必须真的执行到断言体（防过滤条件写错导致的假绿）：\nstdout={stdout}\nstderr={stderr}"
        );
        assert!(
            stdout.contains("1 passed"),
            "隔离子进程必须恰好跑过这一条用例：\nstdout={stdout}\nstderr={stderr}"
        );

        // 父进程环境全程未变——本用例不再需要任何锁，因为根本没有人改进程环境。
        assert_eq!(
            env::var_os("PATH"),
            path_before,
            "I6：不得改动父测试进程的 PATH"
        );
        assert_eq!(
            env::var_os("SAFEUSER"),
            safeuser_before,
            "I6：不得改动父测试进程的 SAFEUSER"
        );

        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(remote);
        let _ = fs::remove_dir_all(fake_bin);
    }

    /// 只在隔离子进程里执行的断言体：`gh` 由子进程 `PATH` 上的桩提供，`SAFEUSER=tester` 决定分支名。
    #[cfg(unix)]
    fn run_commit_push_pr_body() {
        let repo = PathBuf::from(env::var(ISOLATED_REPO_ENV).expect("isolated repo env"));
        let gh_log = PathBuf::from(env::var(ISOLATED_GH_LOG_ENV).expect("isolated gh log env"));

        // 先回报哨兵：父进程据此判定"断言体真的跑了"（直接写 stdout，绕过 libtest 捕获）。
        announce_env(ISOLATED_BODY_SENTINEL);

        let request = CommitPushPrRequest {
            commit_message: Some("feat: add feature file".to_string()),
            pr_title: "Add feature file".to_string(),
            pr_body: "## Summary\n- add feature file".to_string(),
            branch_name_hint: "Add feature file".to_string(),
        };

        // when
        let report =
            handle_commit_push_pr_slash_command(&request, &repo).expect("commit-push-pr succeeds");
        let branch = run_command(&repo, "git", &["branch", "--show-current"]);
        let message = run_command(&repo, "git", &["log", "-1", "--pretty=%B"]);
        let gh_invocations = fs::read_to_string(&gh_log).expect("gh log should exist");

        // then
        assert!(report.contains("Result           created"));
        assert!(report.contains("URL              https://example.com/pr/123"));
        assert_eq!(branch.trim(), "tester/add-feature-file");
        assert_eq!(message.trim(), "feat: add feature file");
        assert!(gh_invocations.contains("pr create"));
        assert!(gh_invocations.contains("--base main"));
    }
}
