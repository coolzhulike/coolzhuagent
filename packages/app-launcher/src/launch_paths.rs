//! PATH-01 / PATH-02：统一启动路径解析与既有工作区的保留/恢复。
//!
//! # 目录分类（术语固化）
//!
//! | 类别 | 政策 |
//! | --- | --- |
//! | 安装根 | 程序、随包资源与产品默认配置；**不存用户会话和秘密** |
//! | 工作区根 | 用户当前**明确选择**的工程/配置根（随包配置里的现行键名仍是 `runtime_dir`） |
//! | 业务数据根/数据库 | 按工作区配置（`<workspace>/coolzhu.toml`）解析；**保留既有 `paths.data_dir` 等覆盖** |
//! | 普通诊断日志根 | 默认 `%LOCALAPPDATA%\CoolzhuAgent\logs`，各子进程实际位置由快照列出 |
//! | 用户级启动选择 | `%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json`，**不随 MSI 覆盖**，**不从 `log_dir` 反推** |
//! | 跨工作区输入安全状态 | 用户级共享位置，**不随切换工作区移动或重置** |
//!
//! 裁决明确：**不要求同根**（工作区在用户选择位置、诊断日志默认在 LocalAppData），但必须把两者的
//! **来源、用途与实际路径公开呈现**；**不通过删除 `runtime_dir` 来"统一目录"**；**不再让日志目录变化
//! 隐式改变新版本的工作区**；**不在本轮重命名 `runtime_dir`**。
//!
//! # 解析优先级（冻结，①–⑤）
//!
//! 1. [`CandidateSource::UserSpecified`]：本次明确的用户/控制面工作区选择（只影响声明的本次启动或持久选择）
//! 2. [`CandidateSource::SavedSelection`]：已持久保存的用户级选择（正常启动默认使用；MSI 升级不得覆盖）
//! 3. [`CandidateSource::ImportedLegacySelection`] / [`CandidateSource::ConfigSnapshot`]：
//!    可确认的旧版用户覆盖或旧有效选择（按升级规则导入，保留来源与旧值）
//! 4. [`CandidateSource::PackagedDefault`]：随包 `runtime_dir` 默认值（**仅用于尚未建立选择的首次初始化**）
//! 5. [`CandidateSource::LegacyLogDirDerivation`]：兼容支路——旧配置缺键时从 `log_dir` 推导
//!    （**只识别旧行为与迁移候选，不再作为新版本常规的隐式数据源切换**）
//!
//! **高优先级来源无效时，不自动落到低优先级**：否则"用户磁盘暂时断开"会再次变成"系统创建了一个新空工作区"。
//!
//! # 副作用边界
//!
//! 候选发现与 [`resolve_launch_paths`] **只读**：不创建目录、不打开数据库、不升级 schema、
//! 不写任何默认配置，**不扫描整个用户磁盘**（只查已保存选择、已知历史默认目录、可信旧启动记录与用户指定路径）。
//! 真正的写盘集中在 [`apply_resolution_actions`]（首次初始化的显式创建、一次性采用旧候选后的选择落库）。

use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::fs;
use std::hash::Hasher;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

use crate::LaunchError;

/// 用户级启动选择的根目录名（位于 `%LOCALAPPDATA%` 下）。
pub const USER_STATE_DIR_NAME: &str = "CoolzhuAgent";
/// 用户级启动选择文件名：**只存**选择路径、稳定 workspace 身份、来源、schema 版本与 revision。
/// **不复制**模型参数 / Base URL / 密钥 / 整份 `coolzhu.toml`。
pub const LAUNCHER_USER_STATE_FILE: &str = "launcher-user.json";
/// 用户级启动选择文件的 schema 版本。
pub const LAUNCHER_USER_SCHEMA_VERSION: u64 = 1;
/// 随包配置当前支持的 schema 版本（`config/package-launcher.json` 的 `launcher_config_version`）。
pub const LAUNCHER_CONFIG_SCHEMA_VERSION: u64 = 2;
/// 保留的随包配置快照上限（升级前保留旧启动配置）。
pub const CONFIG_SNAPSHOT_LIMIT: usize = 16;
/// 选择文件互斥锁的等待上限（毫秒）；超时即报冲突，不覆盖别人的写。
pub const SELECTION_LOCK_WAIT_MS: u64 = 2_000;
/// 选择文件的临时文件后缀（`launcher-user.json.tmp`）。
pub const SELECTION_TMP_SUFFIX: &str = ".tmp";

/// 后台回读结果（**后台实际使用**的路径与构建身份）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObservedBackground {
    pub workspace: Option<PathBuf>,
    pub session_db: Option<PathBuf>,
    pub build_version: Option<String>,
    pub health_status: Option<String>,
    pub active_sessions: Option<u64>,
    pub port: Option<u16>,
}

/// 业务数据根的绑定方式。
///
/// 目前只有一种受支持的方式：**由工作区配置解析**。launcher 不注入 `COOLZHU_WEB_SESSION_DB`
/// 之类的覆盖，也不复制一份"数据根解析规则"（否则就成了第二套业务配置权威，会静默忽略
/// `paths.data_dir` 覆盖——见 P08）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataPathBinding {
    /// 由工作区配置（`<workspace>/coolzhu.toml` 的 `paths.data_dir` / `session.*` / `attachment.*`）解析。
    WorkspaceConfigAuthority,
}

impl DataPathBinding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceConfigAuthority => "workspace_config_authority",
        }
    }
}

/// 单个子进程的实际日志文件位置（诊断日志根默认 `%LOCALAPPDATA%\CoolzhuAgent\logs`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLogPath {
    pub process: String,
    pub path: PathBuf,
}

/// 本次启动**已解析结果的快照**（不是新增一套业务配置权威）。
///
/// launcher / Web / Tauri 与必要子进程共享**同一结果**，不能各自根据 cwd / 日志目录 /
/// 全局当前选择重新猜。**日志变化不得改变 workspace、数据库、权限根或输入安全状态的位置。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLaunchPaths {
    /// 本次启动标识（用于自检与复用判定）。
    pub launch_id: String,
    /// 包/构建身份（随包配置 schema 版本 + 启动器版本）。
    pub package_identity: String,
    /// 随包配置 schema 版本。
    pub config_schema_version: u64,
    /// 用户级选择的 revision（未持久化时为 0）。
    pub selection_revision: u64,
    /// 稳定 workspace 身份（工作区已存在时；与 web-console 的 `workspace_identity` 同构）。
    pub workspace_id: Option<String>,
    /// 工作区根（用户明确选择的工程/配置根）。
    pub workspace_root: PathBuf,
    /// 业务数据根绑定方式。
    pub data_dir_binding: DataPathBinding,
    /// **请求的**会话数据库路径；`None` = 由工作区配置决定（launcher 不固定）。
    pub requested_session_db: Option<PathBuf>,
    /// **后台实际使用的**会话数据库路径（健康就绪后回读；用于证明"配置值生效"）。
    pub observed_session_db: Option<PathBuf>,
    /// **后台实际使用的**工作区路径（健康就绪后回读）。
    pub observed_workspace: Option<PathBuf>,
    /// 后台自报的构建版本（复用判定与自检记录）。
    pub observed_build_version: Option<String>,
    /// 用户级启动选择与共享状态的根（`%LOCALAPPDATA%\CoolzhuAgent`）。
    pub user_state_root: PathBuf,
    /// 跨工作区输入安全状态的用户级共享根（**不随切换工作区移动或重置**）。
    pub input_safety_state_root: PathBuf,
    /// 各子进程实际日志位置。
    pub per_process_log_paths: Vec<ProcessLogPath>,
    /// 实际生效的解析来源。
    pub resolution_source: ResolutionSource,
}

impl ResolvedLaunchPaths {
    /// 业务数据根：只有后台回读成功时才能给出（否则就是猜）。
    #[must_use]
    pub fn effective_data_dir(&self) -> Option<PathBuf> {
        self.observed_session_db
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }

    /// 是否观察到"工作区配置覆盖了业务数据根"（即实际库不在 `<workspace>/.coolzhu` 下）。
    #[must_use]
    pub fn data_dir_override_observed(&self) -> bool {
        let Some(observed) = &self.observed_session_db else {
            return false;
        };
        let workspace_default = self
            .workspace_root
            .join(crate::DATA_DIR_NAME)
            .join("web-sessions.sqlite3");
        !same_canonical(observed, &workspace_default)
    }

    /// 带上后台回读结果。
    #[must_use]
    pub fn with_observations(&self, observed: &ObservedBackground) -> Self {
        let mut next = self.clone();
        if let Some(workspace) = &observed.workspace {
            next.observed_workspace = Some(workspace.clone());
        }
        if let Some(db) = &observed.session_db {
            next.observed_session_db = Some(db.clone());
        }
        if let Some(version) = &observed.build_version {
            next.observed_build_version = Some(version.clone());
        }
        next
    }

    /// 自检 / `--print-resolved-paths` 用的 JSON：同时给出**请求的路径**与**后台实际使用的路径**。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "launch_id": self.launch_id,
            "package_identity": self.package_identity,
            "config_schema_version": self.config_schema_version,
            "selection_revision": self.selection_revision,
            "workspace_id": self.workspace_id,
            "resolution_source": self.resolution_source.as_str(),
            "resolution_source_detail": self.resolution_source.detail(),
            "requested": {
                "workspace_root": self.workspace_root.to_string_lossy(),
                "session_db": self.requested_session_db.as_ref().map(|p| p.to_string_lossy().to_string()),
                "data_dir_binding": self.data_dir_binding.as_str(),
            },
            "observed": {
                "workspace": self.observed_workspace.as_ref().map(|p| p.to_string_lossy().to_string()),
                "session_db": self.observed_session_db.as_ref().map(|p| p.to_string_lossy().to_string()),
                "data_dir": self.effective_data_dir().map(|p| p.to_string_lossy().to_string()),
                "data_dir_override_observed": self.data_dir_override_observed(),
                "build_version": self.observed_build_version,
            },
            "user_state_root": self.user_state_root.to_string_lossy(),
            "input_safety_state_root": self.input_safety_state_root.to_string_lossy(),
            "per_process_log_paths": self.per_process_log_paths.iter().map(|entry| json!({
                "process": entry.process,
                "path": entry.path.to_string_lossy(),
            })).collect::<Vec<_>>(),
        })
    }

    /// 启动时打印的"公开呈现"文本：来源 + 用途 + 实际路径。
    #[must_use]
    pub fn display_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("launch_id={} package_identity={}", self.launch_id, self.package_identity),
            format!(
                "工作区根 request={} source={}{}",
                self.workspace_root.display(),
                self.resolution_source.as_str(),
                self.resolution_source.detail()
            ),
            format!(
                "用户级启动选择 root={} revision={}（不随 MSI 覆盖、不从 log_dir 反推）",
                self.user_state_root.display(),
                self.selection_revision
            ),
            format!(
                "跨工作区输入安全状态 root={}（用户级共享，不随切换工作区移动）",
                self.input_safety_state_root.display()
            ),
            format!("业务数据根绑定={}（保留工作区配置的 paths.data_dir 等覆盖）", self.data_dir_binding.as_str()),
        ];
        for entry in &self.per_process_log_paths {
            lines.push(format!("日志[{}]={}", entry.process, entry.path.display()));
        }
        match (&self.observed_workspace, &self.observed_session_db) {
            (Some(workspace), Some(db)) => lines.push(format!(
                "后台实际使用 workspace={} session_db={}（build={}）",
                workspace.display(),
                db.display(),
                self.observed_build_version.as_deref().unwrap_or("unknown")
            )),
            _ => lines.push("后台实际使用路径=尚未回读（健康就绪后写入自检）".to_string()),
        }
        lines
    }
}

/// 解析来源（对应冻结的 ①–⑤）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionSource {
    /// ① 本次明确的用户/控制面工作区选择。
    ExplicitThisLaunch,
    /// ② 已持久保存的用户级选择。
    PersistedUserSelection { revision: u64 },
    /// ③ 旧版用户覆盖 / 旧有效选择（一次性导入，保留来源与旧值）。
    ImportedLegacySelection { origin: String },
    /// ④ 随包 `runtime_dir` 默认值——**仅用于尚未建立选择的首次初始化**。
    PackagedDefaultFirstInit { config_key: String },
    /// ④ 随包默认值位置**已经**有用户数据（不是首次初始化，不能另建空库）。
    PackagedDefaultExistingData { config_key: String },
    /// 旧版配置自己声明的默认工作区（v1 兼容：行为与升级前一致）。
    LegacyConfigDeclaredDefault { origin: String },
}

impl ResolutionSource {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ExplicitThisLaunch => "explicit_this_launch",
            Self::PersistedUserSelection { .. } => "persisted_user_selection",
            Self::ImportedLegacySelection { .. } => "imported_legacy_selection",
            Self::PackagedDefaultFirstInit { .. } => "packaged_default_first_init",
            Self::PackagedDefaultExistingData { .. } => "packaged_default_existing_data",
            Self::LegacyConfigDeclaredDefault { .. } => "legacy_config_declared_default",
        }
    }

    /// 来源细节（revision / origin / 配置键），始终出现在启动日志与自检里。
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::ExplicitThisLaunch => " declared=this_launch".to_string(),
            Self::PersistedUserSelection { revision } => format!(" revision={revision}"),
            Self::ImportedLegacySelection { origin }
            | Self::LegacyConfigDeclaredDefault { origin } => format!(" origin={origin}"),
            Self::PackagedDefaultFirstInit { config_key }
            | Self::PackagedDefaultExistingData { config_key } => format!(" config_key={config_key}"),
        }
    }

    /// 优先级序号（①–⑤），用于诊断与断言。
    #[must_use]
    pub const fn priority(&self) -> u8 {
        match self {
            Self::ExplicitThisLaunch => 1,
            Self::PersistedUserSelection { .. } => 2,
            Self::ImportedLegacySelection { .. } => 3,
            Self::LegacyConfigDeclaredDefault { .. } => 3,
            Self::PackagedDefaultExistingData { .. } => 4,
            Self::PackagedDefaultFirstInit { .. } => 4,
        }
    }
}

/// 目录可访问性判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceAccess {
    /// 存在、是目录、可写。
    Writable,
    /// 存在且是目录，但不可写（无法承载会话库）。
    ReadOnly,
    /// 不存在（含所在磁盘/网络位置不可达）。
    Missing,
    /// 路径存在但不是目录。
    NotDirectory,
    /// 无法判定（未解析变量、权限拒绝对父路径的探测等）。
    Unresolved,
}

impl WorkspaceAccess {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Writable => "writable",
            Self::ReadOnly => "read_only",
            Self::Missing => "missing",
            Self::NotDirectory => "not_a_directory",
            Self::Unresolved => "unresolved",
        }
    }

    /// 中文说明（错误呈现里给用户看）。
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Writable => "目录存在且可写",
            Self::ReadOnly => "目录存在但不可写",
            Self::Missing => "目录不存在或所在磁盘/位置不可访问",
            Self::NotDirectory => "该路径存在但不是目录",
            Self::Unresolved => "无法判定（权限拒绝或路径无法解析）",
        }
    }

    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Writable)
    }
}

/// 目录探测接缝（测试可注入"权限拒绝/磁盘断开"等场景）。
pub trait WorkspaceAccessProbe {
    fn access(&self, path: &Path) -> WorkspaceAccess;
}

/// 真实探测：只做只读判定 + 一次写探针（**不创建目标目录**）。
#[derive(Debug, Clone, Copy, Default)]
pub struct RealWorkspaceAccess;

impl WorkspaceAccessProbe for RealWorkspaceAccess {
    fn access(&self, path: &Path) -> WorkspaceAccess {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return WorkspaceAccess::Missing,
            Err(_) => return WorkspaceAccess::Unresolved,
        };
        if !metadata.is_dir() {
            return WorkspaceAccess::NotDirectory;
        }
        match probe_dir_writable(path) {
            Ok(true) => WorkspaceAccess::Writable,
            Ok(false) => WorkspaceAccess::ReadOnly,
            Err(_) => WorkspaceAccess::Unresolved,
        }
    }
}

/// 在目标目录里写一个探针文件再删除：不创建目录、不改动既有文件、不留下残留。
fn probe_dir_writable(dir: &Path) -> io::Result<bool> {
    let probe = dir.join(format!(".coolzhu-launch-write-probe-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(mut file) => {
            let _ = file.write_all(b"probe");
            let _ = file.sync_all();
            drop(file);
            let _ = fs::remove_file(&probe);
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(false),
        Err(error) => Err(error),
    }
}

/// 只读的"这里有没有工作区数据"证据（不打开数据库、不升级 schema、不写盘）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspaceDataEvidence {
    pub session_db: bool,
    pub session_json: bool,
    pub workspace_config: bool,
    pub attachments: bool,
    /// 非隐藏的顶层条目数（"误建后已写入新消息/文件"也算数据）。
    pub other_entries: usize,
}

impl WorkspaceDataEvidence {
    #[must_use]
    pub const fn has_workspace_data(&self) -> bool {
        self.session_db
            || self.session_json
            || self.workspace_config
            || self.attachments
            || self.other_entries > 0
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.session_db {
            parts.push("会话库");
        }
        if self.session_json {
            parts.push("兼容会话 JSON");
        }
        if self.workspace_config {
            parts.push("coolzhu.toml");
        }
        if self.attachments {
            parts.push("附件目录");
        }
        if self.other_entries > 0 {
            parts.push("其它顶层条目");
        }
        if parts.is_empty() {
            "无".to_string()
        } else {
            parts.join("+")
        }
    }
}

/// 只读探测某个目录里的工作区数据。
///
/// **排除产品自己的用户级状态条目**（`%LOCALAPPDATA%\CoolzhuAgent` 既是"已知历史默认工作区"、
/// 又是启动选择/日志/浏览器桥 nonce 的用户级位置）：这些不是工作区数据，否则全新用户会被误判成
/// "已有工作区"，从而不再在 `%USERPROFILE%\coolzhuagent` 首次初始化（P01/P03 的分界）。
#[must_use]
pub fn probe_workspace_data(root: &Path) -> WorkspaceDataEvidence {
    let state_dir = root.join(crate::DATA_DIR_NAME);
    let other_entries = fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        return false;
                    }
                    !USER_LEVEL_STATE_ENTRY_NAMES
                        .iter()
                        .any(|known| name.eq_ignore_ascii_case(known))
                })
                .count()
        })
        .unwrap_or(0);
    WorkspaceDataEvidence {
        session_db: state_dir.join("web-sessions.sqlite3").is_file(),
        session_json: state_dir.join("web-sessions.json").is_file(),
        workspace_config: root.join(crate::CONFIG_FILE_NAME).is_file(),
        attachments: state_dir.join("attachments").is_dir(),
        other_entries,
    }
}

/// 产品自己的用户级状态条目名（不构成"工作区有数据"的证据）。
pub const USER_LEVEL_STATE_ENTRY_NAMES: [&str; 7] = [
    LAUNCHER_USER_STATE_FILE,
    "launcher-user.json.tmp",
    "launcher-user.json.lock",
    "logs",
    "runtime",
    "config-snapshots",
    "input-safety",
];

/// 候选来源（只使用已保存选择、已知历史默认目录、可信旧启动记录与用户指定路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSource {
    /// ② 已持久保存的用户级选择。
    SavedSelection { revision: u64 },
    /// ③ 一次性导入的旧版选择（保留 origin 与旧值）。
    ImportedLegacySelection { origin: String },
    /// ③ 可信旧启动记录：随包配置快照里记录的 `runtime_dir` 原值。
    ConfigSnapshot {
        snapshot: String,
        recorded_at_ms: u64,
    },
    /// ③ 已知历史默认目录（如 `%LOCALAPPDATA%\CoolzhuAgent`）。
    KnownHistoricalDefault { name: String },
    /// ⑤ 旧配置缺键时从 `log_dir` 推导的结果（**仅迁移候选**）。
    LegacyLogDirDerivation,
    /// ④ 随包 `runtime_dir` 默认值。
    PackagedDefault,
    /// 本次用户指定的路径。
    UserSpecified,
}

impl CandidateSource {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SavedSelection { .. } => "saved_selection",
            Self::ImportedLegacySelection { .. } => "imported_legacy_selection",
            Self::ConfigSnapshot { .. } => "config_snapshot",
            Self::KnownHistoricalDefault { .. } => "known_historical_default",
            Self::LegacyLogDirDerivation => "legacy_log_dir_derivation",
            Self::PackagedDefault => "packaged_default",
            Self::UserSpecified => "user_specified",
        }
    }

    /// 是否属于"③ 旧版用户覆盖/旧有效选择"这一档。
    #[must_use]
    pub const fn is_legacy(&self) -> bool {
        matches!(
            self,
            Self::ImportedLegacySelection { .. }
                | Self::ConfigSnapshot { .. }
                | Self::KnownHistoricalDefault { .. }
                | Self::LegacyLogDirDerivation
        )
    }

    /// 是否是"已确认的选择"（无效即阻断，不允许落到低优先级）。
    #[must_use]
    pub const fn is_confirmed_selection(&self) -> bool {
        matches!(self, Self::SavedSelection { .. })
    }

    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::SavedSelection { revision } => format!("revision={revision}"),
            Self::ImportedLegacySelection { origin } => format!("origin={origin}"),
            Self::ConfigSnapshot {
                snapshot,
                recorded_at_ms,
            } => format!("snapshot={snapshot} recorded_at_ms={recorded_at_ms}"),
            Self::KnownHistoricalDefault { name } => format!("name={name}"),
            Self::LegacyLogDirDerivation => "from_log_dir_compat_branch".to_string(),
            Self::PackagedDefault => format!("config_key={RUNTIME_DIR_KEY}"),
            Self::UserSpecified => "declared=this_launch".to_string(),
        }
    }
}

/// 随包配置里工作区默认值的现行键名（**本轮不改名**，只在契约中注明含义）。
pub const RUNTIME_DIR_KEY: &str = "runtime_dir";

/// 一个工作区候选（**只含只读证据**：不记录 mtime/容量，避免"按最新/最大自动选"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCandidate {
    pub path: PathBuf,
    pub source: CandidateSource,
    pub access: WorkspaceAccess,
    pub evidence: WorkspaceDataEvidence,
}

impl WorkspaceCandidate {
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{}（来源 {} {}；可访问性 {}；数据证据 {}）",
            self.path.display(),
            self.source.as_str(),
            self.source.detail(),
            self.access.as_str(),
            self.evidence.summary()
        )
    }

    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "path": self.path.to_string_lossy(),
            "source": self.source.as_str(),
            "source_detail": self.source.detail(),
            "access": self.access.as_str(),
            "has_workspace_data": self.evidence.has_workspace_data(),
            "evidence": self.evidence.summary(),
        })
    }
}

/// 用户级选择记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherUserSelection {
    pub workspace_root: PathBuf,
    pub workspace_id: Option<String>,
    /// 选择来源（如 `explicit_select` / `adopted_legacy_candidate` / `first_init_default`）。
    pub source: String,
    pub recorded_at_ms: u64,
}

/// 一次性导入的旧版选择（保留来源与旧值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySelectionRecord {
    pub workspace_root: PathBuf,
    pub origin: String,
    pub imported_at_ms: u64,
}

/// 非权威观测值（不参与选择优先级；只用于复用判定的构建身份/数据绑定核对）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LauncherObservations {
    pub last_background_build_version: Option<String>,
    pub last_launch_id: Option<String>,
    pub last_workspace_root: Option<PathBuf>,
    /// 上次本机观测到的会话数据库路径（与 `last_workspace_root` 配套使用）。
    pub last_observed_session_db: Option<PathBuf>,
}

/// 用户级启动选择文件的完整内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherUserState {
    pub schema_version: u64,
    pub revision: u64,
    pub selection: Option<LauncherUserSelection>,
    pub legacy_selections: Vec<LegacySelectionRecord>,
    pub observations: LauncherObservations,
}

impl Default for LauncherUserState {
    fn default() -> Self {
        Self {
            schema_version: LAUNCHER_USER_SCHEMA_VERSION,
            revision: 0,
            selection: None,
            legacy_selections: Vec::new(),
            observations: LauncherObservations::default(),
        }
    }
}

impl LauncherUserState {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "revision": self.revision,
            "selection": self.selection.as_ref().map(|selection| json!({
                "workspace_root": selection.workspace_root.to_string_lossy(),
                "workspace_id": selection.workspace_id,
                "source": selection.source,
                "recorded_at_ms": selection.recorded_at_ms,
            })),
            "legacy_selections": self.legacy_selections.iter().map(|record| json!({
                "workspace_root": record.workspace_root.to_string_lossy(),
                "origin": record.origin,
                "imported_at_ms": record.imported_at_ms,
            })).collect::<Vec<_>>(),
            "observations": {
                "last_background_build_version": self.observations.last_background_build_version,
                "last_launch_id": self.observations.last_launch_id,
                "last_workspace_root": self.observations.last_workspace_root.as_ref().map(|p| p.to_string_lossy().to_string()),
                "last_observed_session_db": self.observations.last_observed_session_db.as_ref().map(|p| p.to_string_lossy().to_string()),
            },
        })
    }

    fn from_json(value: &Value) -> Result<Self, String> {
        let schema_version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let revision = value.get("revision").and_then(Value::as_u64).unwrap_or(0);
        let selection = value.get("selection").and_then(|selection| {
            if selection.is_null() {
                return None;
            }
            selection
                .get("workspace_root")
                .and_then(Value::as_str)
                .map(|root| LauncherUserSelection {
                    workspace_root: PathBuf::from(root),
                    workspace_id: selection
                        .get("workspace_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    source: selection
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    recorded_at_ms: selection
                        .get("recorded_at_ms")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                })
        });
        let legacy_selections = value
            .get("legacy_selections")
            .and_then(Value::as_array)
            .map(|records| {
                records
                    .iter()
                    .filter_map(|record| {
                        record
                            .get("workspace_root")
                            .and_then(Value::as_str)
                            .map(|root| LegacySelectionRecord {
                                workspace_root: PathBuf::from(root),
                                origin: record
                                    .get("origin")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown")
                                    .to_string(),
                                imported_at_ms: record
                                    .get("imported_at_ms")
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0),
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let observations = value
            .get("observations")
            .map(|observations| LauncherObservations {
                last_background_build_version: observations
                    .get("last_background_build_version")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                last_launch_id: observations
                    .get("last_launch_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                last_workspace_root: observations
                    .get("last_workspace_root")
                    .and_then(Value::as_str)
                    .map(PathBuf::from),
                last_observed_session_db: observations
                    .get("last_observed_session_db")
                    .and_then(Value::as_str)
                    .map(PathBuf::from),
            })
            .unwrap_or_default();
        if schema_version > LAUNCHER_USER_SCHEMA_VERSION {
            return Err(format!(
                "选择文件 schema_version={schema_version} 高于本启动器支持的 {LAUNCHER_USER_SCHEMA_VERSION}"
            ));
        }
        Ok(Self {
            schema_version,
            revision,
            selection,
            legacy_selections,
            observations,
        })
    }
}

/// 随包配置快照（升级前保留旧启动配置及其原始发布基线）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSnapshot {
    pub file: PathBuf,
    pub recorded_at_ms: u64,
    pub config_path: PathBuf,
    pub declared_runtime_dir: Option<PathBuf>,
    pub declared_runtime_dir_text: Option<String>,
}

/// 用户级启动选择根：`%LOCALAPPDATA%\CoolzhuAgent`。
///
/// **不能从 `log_dir` 反推**——否则改日志设置又会移动启动选择的权威位置。
pub fn user_state_root_for(
    local_app_data: Option<&Path>,
    user_profile: Option<&Path>,
    explicit_override: Option<&Path>,
) -> Result<PathBuf, LaunchError> {
    if let Some(override_dir) = explicit_override {
        if !override_dir.is_absolute() {
            return Err(LaunchError::SelectionStore {
                path: override_dir.to_path_buf(),
                detail: "用户级选择根必须是绝对路径（--user-state-dir 只用于隔离测试/受控运维）".into(),
                remedy: "传入绝对路径，或去掉 --user-state-dir 使用 %LOCALAPPDATA%\\CoolzhuAgent".into(),
            });
        }
        return Ok(override_dir.to_path_buf());
    }
    if let Some(local) = local_app_data.filter(|path| path.is_absolute()) {
        return Ok(local.join(USER_STATE_DIR_NAME));
    }
    // `%LOCALAPPDATA%` 就是 `%USERPROFILE%\AppData\Local`；这是用户目录上下文内的推导，
    // 与日志目录无关（不得改成看 log_dir）。
    if let Some(profile) = user_profile.filter(|path| path.is_absolute()) {
        return Ok(profile.join("AppData").join("Local").join(USER_STATE_DIR_NAME));
    }
    Err(LaunchError::SelectionStore {
        path: PathBuf::new(),
        detail: "取不到 %LOCALAPPDATA% 或 %USERPROFILE%：无法确定用户级启动选择位置".into(),
        remedy: "以正常交互用户身份运行（用户级选择不随安装包覆盖，也不能由安装账号代选）".into(),
    })
}

/// 跨工作区输入安全状态的用户级共享根。
///
/// 用户级共享、**不随切换工作区移动或重置**、不能从 `log_dir` 反推。
#[must_use]
pub fn input_safety_state_root_for(user_state_root: &Path) -> PathBuf {
    user_state_root.join("input-safety")
}

/// 选择文件路径。
#[must_use]
pub fn user_state_file(user_state_root: &Path) -> PathBuf {
    user_state_root.join(LAUNCHER_USER_STATE_FILE)
}

/// 快照目录。
#[must_use]
pub fn config_snapshot_dir(user_state_root: &Path) -> PathBuf {
    user_state_root.join("config-snapshots")
}

/// 读取用户级选择：文件缺失 → 默认（无可确认选择）；损坏/版本过高 → 明确报错（不静默忽略）。
pub fn load_user_state(user_state_root: &Path) -> Result<LauncherUserState, LaunchError> {
    let path = user_state_file(user_state_root);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(LauncherUserState::default())
        }
        Err(error) => {
            return Err(LaunchError::SelectionStore {
                path: path.clone(),
                detail: format!("选择文件不可读：{error}"),
                remedy: "检查文件权限，或用 --select-workspace 重新建立选择（不会删除工作区数据）".into(),
            })
        }
    };
    let value: Value = serde_json::from_str(&text).map_err(|error| LaunchError::SelectionStore {
        path: path.clone(),
        detail: format!("选择文件不是合法 JSON：{error}"),
        remedy: format!(
            "修复或删除 {} 后用 --select-workspace 重新建立选择（不会删除工作区数据）",
            path.display()
        ),
    })?;
    LauncherUserState::from_json(&value).map_err(|detail| LaunchError::SelectionStore {
        path: path.clone(),
        detail,
        remedy: "本启动器不会猜测未知 schema 的含义；请用匹配版本的启动器，或用 --select-workspace 重新建立选择".into(),
    })
}

/// 选择文件互斥锁（跨进程 create_new 原子占位）。
struct SelectionLock {
    path: PathBuf,
}

impl Drop for SelectionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_selection_lock(user_state_root: &Path) -> Result<SelectionLock, LaunchError> {
    let path = user_state_root.join("launcher-user.json.lock");
    fs::create_dir_all(user_state_root).map_err(|error| LaunchError::SelectionStore {
        path: user_state_root.to_path_buf(),
        detail: format!("无法创建用户级选择目录：{error}"),
        remedy: "检查 %LOCALAPPDATA% 权限".into(),
    })?;
    let deadline = Instant::now() + std::time::Duration::from_millis(SELECTION_LOCK_WAIT_MS);
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let _ = file.write_all(format!("pid={}\n", std::process::id()).as_bytes());
                return Ok(SelectionLock { path });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    // 另一个启动器正在改选择：报冲突，绝不覆盖它的写。
                    return Err(LaunchError::SelectionLocked {
                        path: user_state_file(user_state_root),
                        lock: path,
                        waited_ms: SELECTION_LOCK_WAIT_MS,
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => {
                return Err(LaunchError::SelectionStore {
                    path: path.clone(),
                    detail: format!("无法占用选择文件锁：{error}"),
                    remedy: "检查 %LOCALAPPDATA%\\CoolzhuAgent 权限或移除残留的 launcher-user.json.lock".into(),
                })
            }
        }
    }
}

/// 更新用户级选择：**revision 冲突检测 + 原子发布**。
///
/// 失败时**保持旧选择**（不落盘、不在内存里先宣布切换成功），返回的 revision 才是新值。
pub fn store_user_state(
    user_state_root: &Path,
    state: &LauncherUserState,
    expected_revision: u64,
) -> Result<u64, LaunchError> {
    let path = user_state_file(user_state_root);
    let _guard = acquire_selection_lock(user_state_root)?;
    let current = load_user_state(user_state_root)?;
    if current.revision != expected_revision {
        return Err(LaunchError::SelectionConflict {
            path,
            expected_revision,
            actual_revision: current.revision,
        });
    }
    let mut next = state.clone();
    next.schema_version = LAUNCHER_USER_SCHEMA_VERSION;
    next.revision = expected_revision + 1;
    write_json_atomic(&path, &next.to_json(), SELECTION_TMP_SUFFIX)?;
    Ok(next.revision)
}

fn write_json_atomic(path: &Path, body: &Value, tmp_suffix: &str) -> Result<(), LaunchError> {
    let mut tmp = path.to_path_buf();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "launcher-user".to_string());
    tmp.set_file_name(format!("{file_name}{tmp_suffix}"));
    {
        let mut file = fs::File::create(&tmp).map_err(|error| LaunchError::Persistence(error))?;
        file.write_all(body.to_string().as_bytes())
            .map_err(LaunchError::Persistence)?;
        file.sync_all().map_err(LaunchError::Persistence)?;
    }
    fs::rename(&tmp, path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        LaunchError::Persistence(error)
    })
}

/// 记录/更新"非权威观测值"（构建版本、启动标识、上次工作区）。冲突与失败都只是告警。
pub fn record_observations(
    user_state_root: &Path,
    update: impl FnOnce(&mut LauncherObservations),
) -> Result<u64, LaunchError> {
    let current = load_user_state(user_state_root)?;
    let mut next = current.clone();
    update(&mut next.observations);
    store_user_state(user_state_root, &next, current.revision)
}

/// 显式建立/修订用户级选择（恢复入口）。
///
/// 路径必须绝对；不存在时**显式创建**（用户明确选择），不可写/不是目录/无法解析时明确报错。
pub fn select_workspace(
    user_state_root: &Path,
    path: &Path,
    probe: &dyn WorkspaceAccessProbe,
    now_ms: u64,
) -> Result<u64, LaunchError> {
    if !path.is_absolute() {
        return Err(LaunchError::WorkspaceUnavailable {
            role: "user_selection".into(),
            path: path.to_path_buf(),
            source: "explicit_select".into(),
            detail: "工作区选择必须是绝对路径（相对路径会被解释成当前工作目录，等于静默落到别的工作区）"
                .into(),
            remedy: "传入绝对路径，例如 D:\\work\\my-project".into(),
        });
    }
    let access = probe.access(path);
    match access {
        WorkspaceAccess::Writable => {}
        WorkspaceAccess::Missing => {
            fs::create_dir_all(path).map_err(|error| LaunchError::WorkspaceUnavailable {
                role: "user_selection".into(),
                path: path.to_path_buf(),
                source: "explicit_select".into(),
                detail: format!("目录不存在且无法创建：{error}"),
                remedy: "先手工创建该目录（或修好所在磁盘/网络位置）再重试".into(),
            })?;
            if !probe.access(path).is_usable() {
                return Err(LaunchError::WorkspaceUnavailable {
                    role: "user_selection".into(),
                    path: path.to_path_buf(),
                    source: "explicit_select".into(),
                    detail: format!("目录创建后仍不可用（{}）", probe.access(path).describe()),
                    remedy: "检查权限后重试；本启动器不会改用其它目录".into(),
                });
            }
        }
        other => {
            return Err(LaunchError::WorkspaceUnavailable {
                role: "user_selection".into(),
                path: path.to_path_buf(),
                source: "explicit_select".into(),
                detail: other.describe().to_string(),
                remedy: "修正该路径（或选择一个可写目录）后重试；本启动器不会改用其它目录".into(),
            })
        }
    }
    let current = load_user_state(user_state_root)?;
    let workspace_id = workspace_identity(path);
    let mut next = current.clone();
    let mut legacy_selections = current.legacy_selections.clone();
    if let Some(previous) = current.selection.as_ref() {
        if !same_canonical(&previous.workspace_root, path) {
            // 旧值必须保留为可解释记录（不是静默丢弃）。
            legacy_selections.push(LegacySelectionRecord {
                workspace_root: previous.workspace_root.clone(),
                origin: format!("replaced_by_explicit_select@r{}", current.revision),
                imported_at_ms: now_ms,
            });
        }
    }
    next.legacy_selections = legacy_selections;
    next.selection = Some(LauncherUserSelection {
        workspace_root: path.to_path_buf(),
        workspace_id,
        source: "explicit_select".into(),
        recorded_at_ms: now_ms,
    });
    store_user_state(user_state_root, &next, current.revision)
}

/// 保留随包配置的原始发布基线（每次内容变化写一份新快照）。
pub fn record_config_snapshot(
    user_state_root: &Path,
    config_path: &Path,
    config_text: &str,
    declared_runtime_dir: Option<&Path>,
    declared_runtime_dir_text: Option<&str>,
    now_ms: u64,
) -> Result<Option<PathBuf>, LaunchError> {
    let dir = config_snapshot_dir(user_state_root);
    fs::create_dir_all(&dir).map_err(LaunchError::Persistence)?;
    let mut hasher = DefaultHasher::new();
    hasher.write(config_text.as_bytes());
    let file = dir.join(format!("launcher-config-{:016x}.json", hasher.finish()));
    if file.is_file() {
        return Ok(None);
    }
    let body = json!({
        "schema_version": 1,
        "recorded_at_ms": now_ms,
        "config_path": config_path.to_string_lossy(),
        "declared_runtime_dir": declared_runtime_dir.map(|path| path.to_string_lossy().to_string()),
        "declared_runtime_dir_text": declared_runtime_dir_text,
        "config_text": config_text,
    });
    write_json_atomic(&file, &body, ".json.tmp")?;
    prune_config_snapshots(&dir, CONFIG_SNAPSHOT_LIMIT);
    Ok(Some(file))
}

fn prune_config_snapshots(dir: &Path, limit: usize) {
    let mut files: Vec<(u64, PathBuf)> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    if path.extension().and_then(|value| value.to_str()) != Some("json") {
                        return None;
                    }
                    let text = fs::read_to_string(&path).ok()?;
                    let value: Value = serde_json::from_str(&text).ok()?;
                    let recorded_at_ms = value.get("recorded_at_ms").and_then(Value::as_u64)?;
                    Some((recorded_at_ms, path))
                })
                .collect()
        })
        .unwrap_or_default();
    if files.len() <= limit {
        return;
    }
    files.sort_by_key(|(recorded_at_ms, _)| *recorded_at_ms);
    for (_, path) in files.iter().take(files.len() - limit) {
        let _ = fs::remove_file(path);
    }
}

/// 读取全部配置快照（按记录时间升序）。
#[must_use]
pub fn read_config_snapshots(user_state_root: &Path) -> Vec<ConfigSnapshot> {
    let dir = config_snapshot_dir(user_state_root);
    let mut snapshots: Vec<ConfigSnapshot> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let file = entry.path();
                    if file.extension().and_then(|value| value.to_str()) != Some("json") {
                        return None;
                    }
                    let value: Value =
                        serde_json::from_str(&fs::read_to_string(&file).ok()?).ok()?;
                    Some(ConfigSnapshot {
                        file,
                        recorded_at_ms: value
                            .get("recorded_at_ms")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        config_path: PathBuf::from(
                            value
                                .get("config_path")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                        ),
                        declared_runtime_dir: value
                            .get("declared_runtime_dir")
                            .and_then(Value::as_str)
                            .map(PathBuf::from),
                        declared_runtime_dir_text: value
                            .get("declared_runtime_dir_text")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    snapshots.sort_by_key(|snapshot| snapshot.recorded_at_ms);
    snapshots
}

/// 稳定 workspace 身份，与 web-console 的 `workspace_identity`
/// （`modules/gui-web/packages/web-console/src/main.rs`，只读引用）保持同构：
/// `ws-{:016x}`，输入为**规范化后小写、分隔符统一为 `/`** 的路径。
#[must_use]
pub fn workspace_identity(workspace: &Path) -> Option<String> {
    let metadata = fs::metadata(workspace).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let normalized = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let mut hasher = DefaultHasher::new();
    hasher.write(normalized.as_bytes());
    Some(format!("ws-{:016x}", hasher.finish()))
}

/// 规范化比较键（小写 + 统一分隔符 + 去尾部分隔符）。
#[must_use]
pub fn canonical_key(path: &Path) -> String {
    let key = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    key.trim_end_matches('/').to_string()
}

/// 两个路径是否指向同一位置（含别名/链接/大小写差异）。
#[must_use]
pub fn same_canonical(left: &Path, right: &Path) -> bool {
    canonical_key(left) == canonical_key(right)
}

/// 解析输入（全部显式注入，便于隔离测试与"解析只做一次"）。
#[derive(Debug, Clone)]
pub struct LaunchPathInputs {
    pub launch_id: String,
    pub package_identity: String,
    pub config_schema_version: u64,
    /// 随包配置声明的默认工作区（v2 的 `runtime_dir`；v1 存在该键时也在此）。
    pub packaged_default_workspace: Option<PathBuf>,
    /// v1 兼容：旧配置缺 `runtime_dir` 时从 `log_dir` 推导的结果（**仅迁移候选**）。
    pub legacy_log_dir_derived_workspace: Option<PathBuf>,
    /// v1 兼容：`runtime_dir` 键是否存在（缺失才是"旧配置缺键"）。
    pub config_declares_runtime_dir: bool,
    pub log_dir: PathBuf,
    pub user_state_root: PathBuf,
    /// 已加载的用户级选择（调用方负责读盘；本模块不读）。
    pub user_state: LauncherUserState,
    /// 可信旧启动记录（配置快照）。
    pub config_snapshots: Vec<ConfigSnapshot>,
    /// 本次明确的工作区选择。
    pub explicit_selection: Option<ExplicitSelection>,
    pub now_ms: u64,
}

/// 本次启动显式声明的选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitSelection {
    pub path: PathBuf,
    pub persist: bool,
    pub declared_by: String,
}

/// 解析动作（写盘集中在 [`apply_resolution_actions`]，解析本身无副作用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionAction {
    /// 首次初始化：显式创建随包默认工作区，并把它记为已建立的选择。
    InitializeWorkspaceAndPersistSelection {
        path: PathBuf,
        source: String,
        expected_revision: u64,
    },
    /// 一次性采用唯一可信旧候选（保留 origin），并落库。
    AdoptLegacyCandidate {
        path: PathBuf,
        origin: String,
        expected_revision: u64,
    },
    /// 采用随包默认值位置已有的用户数据，并落库。
    UseExistingPackagedDefault {
        path: PathBuf,
        config_key: String,
        expected_revision: u64,
    },
    /// 本次显式选择要持久化（`--select-workspace` 走独立入口，这里只处理声明的持久化）。
    PersistExplicitSelection {
        path: PathBuf,
        expected_revision: u64,
    },
    /// 旧版配置声明的默认值：沿用（与升级前行为一致）并落库，避免下次再变。
    PersistLegacyDeclaredDefault {
        path: PathBuf,
        origin: String,
        expected_revision: u64,
    },
}

impl ResolutionAction {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::InitializeWorkspaceAndPersistSelection { path, source, .. } => {
                format!("首次初始化工作区 {}（source={source}）并持久化选择", path.display())
            }
            Self::AdoptLegacyCandidate { path, origin, .. } => {
                format!("一次性采用旧候选 {}（origin={origin}）并持久化选择", path.display())
            }
            Self::UseExistingPackagedDefault { path, .. } => format!(
                "沿用随包默认值位置已有的工作区 {} 并持久化选择",
                path.display()
            ),
            Self::PersistExplicitSelection { path, .. } => {
                format!("持久化本次显式选择 {}", path.display())
            }
            Self::PersistLegacyDeclaredDefault { path, origin, .. } => format!(
                "沿用旧版配置声明的工作区 {}（origin={origin}）并持久化选择",
                path.display()
            ),
        }
    }

    fn expected_revision(&self) -> u64 {
        match self {
            Self::InitializeWorkspaceAndPersistSelection {
                expected_revision, ..
            }
            | Self::AdoptLegacyCandidate {
                expected_revision, ..
            }
            | Self::UseExistingPackagedDefault {
                expected_revision, ..
            }
            | Self::PersistExplicitSelection {
                expected_revision, ..
            }
            | Self::PersistLegacyDeclaredDefault {
                expected_revision, ..
            } => *expected_revision,
        }
    }

    fn selection(&self) -> (PathBuf, String) {
        match self {
            Self::InitializeWorkspaceAndPersistSelection { path, source, .. } => {
                (path.clone(), source.clone())
            }
            Self::AdoptLegacyCandidate { path, origin, .. } => {
                (path.clone(), format!("adopted_legacy_candidate:{origin}"))
            }
            Self::UseExistingPackagedDefault {
                path, config_key, ..
            } => (
                path.clone(),
                format!("packaged_default_existing_data:{config_key}"),
            ),
            Self::PersistExplicitSelection { path, .. } => (path.clone(), "explicit_select".into()),
            Self::PersistLegacyDeclaredDefault { path, origin, .. } => {
                (path.clone(), format!("legacy_config_declared_default:{origin}"))
            }
        }
    }
}

/// 解析诊断信息（迁移信息、被忽略的候选、配置来源变化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionNote {
    pub code: String,
    pub message: String,
}

impl ResolutionNote {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// 解析结果（含待执行动作与诊断）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPathDecision {
    pub resolved: ResolvedLaunchPaths,
    pub actions: Vec<ResolutionAction>,
    pub notes: Vec<ResolutionNote>,
}

/// **冻结的解析优先级**实现（①–⑤）。
///
/// 纯函数：不写盘、不创建目录、不打开数据库、不升级 schema。
/// **高优先级来源无效时直接阻断，不落到低优先级。**
pub fn resolve_launch_paths(
    inputs: &LaunchPathInputs,
    probe: &dyn WorkspaceAccessProbe,
) -> Result<LaunchPathDecision, LaunchError> {
    let mut notes = Vec::new();
    let user_state_root = inputs.user_state_root.clone();
    let input_safety_state_root = input_safety_state_root_for(&user_state_root);
    let mut actions = Vec::new();

    let finish = |workspace_root: PathBuf,
                      source: ResolutionSource,
                      revision: u64,
                      notes: Vec<ResolutionNote>,
                      actions: Vec<ResolutionAction>|
     -> LaunchPathDecision {
        let resolved = ResolvedLaunchPaths {
            launch_id: inputs.launch_id.clone(),
            package_identity: inputs.package_identity.clone(),
            config_schema_version: inputs.config_schema_version,
            selection_revision: revision,
            workspace_id: workspace_identity(&workspace_root),
            workspace_root,
            data_dir_binding: DataPathBinding::WorkspaceConfigAuthority,
            requested_session_db: None,
            observed_session_db: None,
            observed_workspace: None,
            observed_build_version: None,
            user_state_root: user_state_root.clone(),
            input_safety_state_root: input_safety_state_root.clone(),
            // 各子进程的实际日志位置都明确列出（默认根 %LOCALAPPDATA%\CoolzhuAgent\logs）。
            per_process_log_paths: process_log_paths(&[
                (
                    "launcher.selfcheck",
                    inputs.log_dir.join("package-selfcheck-last.json"),
                ),
                (
                    "web-console.stdout",
                    inputs.log_dir.join("web-console.stdout.log"),
                ),
                (
                    "web-console.stderr",
                    inputs.log_dir.join("web-console.stderr.log"),
                ),
                ("tauri.stdout", inputs.log_dir.join("tauri.stdout.log")),
                ("tauri.stderr", inputs.log_dir.join("tauri.stderr.log")),
            ]),
            resolution_source: source,
        };
        LaunchPathDecision {
            resolved,
            actions,
            notes,
        }
    };

    // 配置来源变化（升级替换随包配置）必须可见，不能悄悄改变工作区。
    note_config_drift(inputs, &mut notes);

    // ① 本次明确的用户/控制面工作区选择。
    if let Some(explicit) = &inputs.explicit_selection {
        if !explicit.path.is_absolute() {
            return Err(LaunchError::WorkspaceUnavailable {
                role: "explicit_this_launch".into(),
                path: explicit.path.clone(),
                source: format!("declared_by={}", explicit.declared_by),
                detail: "工作区必须是绝对路径（相对路径会被解释成当前工作目录，等于静默落到别的工作区）"
                    .into(),
                remedy: "传入绝对路径后重试；本启动器不会落到当前目录或系统临时目录".into(),
            });
        }
        let access = probe.access(&explicit.path);
        if !access.is_usable() {
            return Err(LaunchError::WorkspaceUnavailable {
                role: "explicit_this_launch".into(),
                path: explicit.path.clone(),
                source: format!("declared_by={}", explicit.declared_by),
                detail: access.describe().to_string(),
                remedy: "修正本次指定的工作区路径后重试；本启动器不会改用其它目录，也不会落到当前目录".into(),
            });
        }
        if explicit.persist {
            actions.push(ResolutionAction::PersistExplicitSelection {
                path: explicit.path.clone(),
                expected_revision: inputs.user_state.revision,
            });
        }
        return Ok(finish(
            explicit.path.clone(),
            ResolutionSource::ExplicitThisLaunch,
            inputs.user_state.revision,
            notes,
            actions,
        ));
    }

    // ② 已持久保存的用户级选择：正常启动默认使用；无效即阻断（绝不落到 ③/④/⑤）。
    if let Some(selection) = &inputs.user_state.selection {
        let access = probe.access(&selection.workspace_root);
        if !access.is_usable() {
            return Err(LaunchError::WorkspaceUnavailable {
                role: "saved_user_selection".into(),
                path: selection.workspace_root.clone(),
                source: format!(
                    "persisted_user_selection revision={} source={}",
                    inputs.user_state.revision, selection.source
                ),
                detail: access.describe().to_string(),
                remedy: format!(
                    "恢复该位置（接回磁盘/修正权限）后重试；若确实要改用别处，显式执行 --select-workspace <绝对路径>；\
                     本启动器不会创建替代工作区，也不会读其它目录来「补上」数据（选择文件：{}）",
                    user_state_file(&user_state_root).display()
                ),
            });
        }
        return Ok(finish(
            selection.workspace_root.clone(),
            ResolutionSource::PersistedUserSelection {
                revision: inputs.user_state.revision,
            },
            inputs.user_state.revision,
            notes,
            actions,
        ));
    }

    // 候选发现（只读；只用可信来源，不扫描整个用户磁盘）。
    let candidates = discover_candidates(inputs, probe, &mut notes);
    let holders: Vec<WorkspaceCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.evidence.has_workspace_data())
        .cloned()
        .collect();

    // 无法判定的候选（权限拒绝等）必须阻断：不能把"看不到"当成"没有数据"。
    for candidate in &candidates {
        if candidate.access == WorkspaceAccess::Unresolved {
            return Err(LaunchError::WorkspaceUnavailable {
                role: "workspace_candidate".into(),
                path: candidate.path.clone(),
                source: format!("{} {}", candidate.source.as_str(), candidate.source.detail()),
                detail: candidate.access.describe().to_string(),
                remedy: "修好该位置的权限/可达性后重试（该位置可能有工作区数据，不能当作不存在）".into(),
            });
        }
    }

    // ③/④ 采用规则：有数据的候选只能有一个；两个及以上必须由用户显式选择。
    if holders.len() > 1 {
        return Err(LaunchError::WorkspaceSelectionAmbiguous {
            candidates: holders,
            remedy: "分别确认哪个是你要的工作区，然后执行 --select-workspace <绝对路径>（本启动器不按时间/容量/数量自动选，不合并，也不删除任何一份）"
                .into(),
        });
    }

    if let Some(holder) = holders.first() {
        let source = match &holder.source {
            CandidateSource::PackagedDefault => ResolutionSource::PackagedDefaultExistingData {
                config_key: RUNTIME_DIR_KEY.to_string(),
            },
            CandidateSource::KnownHistoricalDefault { name } => {
                ResolutionSource::ImportedLegacySelection {
                    origin: format!("known_historical_default:{name}"),
                }
            }
            CandidateSource::LegacyLogDirDerivation => ResolutionSource::ImportedLegacySelection {
                origin: "legacy_config_missing_runtime_dir".to_string(),
            },
            CandidateSource::ConfigSnapshot {
                snapshot,
                recorded_at_ms,
            } => ResolutionSource::ImportedLegacySelection {
                origin: format!("config_snapshot:{snapshot}@recorded_at_ms={recorded_at_ms}"),
            },
            CandidateSource::ImportedLegacySelection { origin } => {
                ResolutionSource::ImportedLegacySelection {
                    origin: origin.clone(),
                }
            }
            CandidateSource::SavedSelection { revision } => ResolutionSource::PersistedUserSelection {
                revision: *revision,
            },
            CandidateSource::UserSpecified => ResolutionSource::ExplicitThisLaunch,
        };
        let origin = holder.source.detail();
        notes.push(ResolutionNote::new(
            "adopt_single_legacy_candidate",
            format!(
                "唯一可信候选 {} 含工作区数据，按升级规则一次性采用（{origin}）；已保留其余候选路径与旧值，不做合并/删除",
                holder.path.display()
            ),
        ));
        let action = if matches!(holder.source, CandidateSource::PackagedDefault) {
            ResolutionAction::UseExistingPackagedDefault {
                path: holder.path.clone(),
                config_key: RUNTIME_DIR_KEY.to_string(),
                expected_revision: inputs.user_state.revision,
            }
        } else {
            ResolutionAction::AdoptLegacyCandidate {
                path: holder.path.clone(),
                origin: origin.clone(),
                expected_revision: inputs.user_state.revision,
            }
        };
        actions.push(action);
        return Ok(finish(
            holder.path.clone(),
            source,
            inputs.user_state.revision,
            notes,
            actions,
        ));
    }

    // ④ 随包默认值：仅用于尚未建立选择的首次初始化。
    if let Some(default_dir) = &inputs.packaged_default_workspace {
        let access = probe.access(default_dir);
        match access {
            WorkspaceAccess::Missing => {
                notes.push(ResolutionNote::new(
                    "first_init_packaged_default",
                    format!(
                        "没有既有有效选择、也没有可信旧候选：按随包默认值首次初始化 {}（显式创建；不继承打包机/安装账号目录）",
                        default_dir.display()
                    ),
                ));
                actions.push(ResolutionAction::InitializeWorkspaceAndPersistSelection {
                    path: default_dir.clone(),
                    source: "first_init_packaged_default".to_string(),
                    expected_revision: inputs.user_state.revision,
                });
                return Ok(finish(
                    default_dir.clone(),
                    ResolutionSource::PackagedDefaultFirstInit {
                        config_key: RUNTIME_DIR_KEY.to_string(),
                    },
                    inputs.user_state.revision,
                    notes,
                    actions,
                ));
            }
            WorkspaceAccess::Writable => {
                // v1 兼容：旧配置本来就把这里当工作区（与其他候选都无数据）。
                let source = if inputs.config_schema_version >= LAUNCHER_CONFIG_SCHEMA_VERSION {
                    ResolutionSource::PackagedDefaultFirstInit {
                        config_key: RUNTIME_DIR_KEY.to_string(),
                    }
                } else {
                    ResolutionSource::LegacyConfigDeclaredDefault {
                        origin: "legacy_config_runtime_dir".to_string(),
                    }
                };
                notes.push(ResolutionNote::new(
                    "packaged_default_empty_init",
                    format!(
                        "随包默认值位置 {} 存在但无工作区数据：按该默认值初始化并记录选择",
                        default_dir.display()
                    ),
                ));
                actions.push(ResolutionAction::InitializeWorkspaceAndPersistSelection {
                    path: default_dir.clone(),
                    source: if inputs.config_declares_runtime_dir {
                        "first_init_packaged_default".to_string()
                    } else {
                        "legacy_config_declared_default".to_string()
                    },
                    expected_revision: inputs.user_state.revision,
                });
                return Ok(finish(
                    default_dir.clone(),
                    source,
                    inputs.user_state.revision,
                    notes,
                    actions,
                ));
            }
            other => {
                return Err(LaunchError::WorkspaceUnavailable {
                    role: "packaged_default".into(),
                    path: default_dir.clone(),
                    source: format!("config_key={RUNTIME_DIR_KEY}"),
                    detail: other.describe().to_string(),
                    remedy: "修正随包配置里的工作区默认值，或显式执行 --select-workspace <绝对路径>；不会回退到当前目录或临时目录".into(),
                })
            }
        }
    }

    // ⑤ 兼容支路（仅 v1 缺键）：输出旧推导结果作为迁移信息，要求确认一次。
    if let Some(derived) = &inputs.legacy_log_dir_derived_workspace {
        let access = probe.access(derived);
        notes.push(ResolutionNote::new(
            "legacy_log_dir_derivation",
            format!(
                "旧版配置缺 {RUNTIME_DIR_KEY}：兼容推导结果 {}（可访问性 {}）。该推导只用于迁移候选，不再作为新版本的隐式数据源",
                derived.display(),
                access.as_str()
            ),
        ));
        let mut shown = candidates;
        if !shown.iter().any(|c| same_canonical(&c.path, derived)) {
            shown.push(WorkspaceCandidate {
                path: derived.clone(),
                source: CandidateSource::LegacyLogDirDerivation,
                access,
                evidence: probe_workspace_data(derived),
            });
        }
        return Err(LaunchError::WorkspaceSelectionRequired {
            reason: format!(
                "旧版配置缺少 {RUNTIME_DIR_KEY}，且没有已保存的用户级选择：需要一次明确确认后才建立选择（此后不再随 log_dir 漂移）"
            ),
            candidates: shown,
            remedy: format!(
                "确认要使用的工作区后执行：COOLZHU-AGENT.exe --select-workspace \"{}\"（也可指定其它绝对路径）；本启动器不会自动采用推导结果",
                derived.display()
            ),
        });
    }

    // 无任何候选：给出默认建议与明确的首次初始化入口。
    Err(LaunchError::WorkspaceSelectionRequired {
        reason: "没有可确认的工作区选择，且随包配置未声明工作区默认值".into(),
        candidates,
        remedy: "执行 COOLZHU-AGENT.exe --select-workspace \"%USERPROFILE%\\coolzhuagent\" 建立首次选择".into(),
    })
}

fn note_config_drift(inputs: &LaunchPathInputs, notes: &mut Vec<ResolutionNote>) {
    let Some(current) = &inputs.packaged_default_workspace else {
        return;
    };
    // 快照按记录时间升序：最后一个带声明值的快照 = "升级前那一版随包配置"。
    let previous = inputs
        .config_snapshots
        .iter()
        .rev()
        .find(|snapshot| snapshot.declared_runtime_dir.is_some());
    if let Some(snapshot) = previous {
        if let Some(old_value) = &snapshot.declared_runtime_dir {
            if !same_canonical(old_value, current) {
                notes.push(ResolutionNote::new(
                    "packaged_config_workspace_changed",
                    format!(
                        "随包配置声明的工作区默认值已从 {} 变为 {}（旧配置快照 {}）。已保存的用户选择不受影响；本启动器不会因默认值变化而迁移或新建工作区",
                        old_value.display(),
                        current.display(),
                        snapshot.file.display()
                    ),
                ));
            }
        }
    }
}

fn discover_candidates(
    inputs: &LaunchPathInputs,
    probe: &dyn WorkspaceAccessProbe,
    notes: &mut Vec<ResolutionNote>,
) -> Vec<WorkspaceCandidate> {
    let mut candidates: Vec<WorkspaceCandidate> = Vec::new();
    let push = |path: PathBuf, source: CandidateSource, candidates: &mut Vec<WorkspaceCandidate>| {
        if let Some(existing) = candidates
            .iter_mut()
            .find(|candidate| same_canonical(&candidate.path, &path))
        {
            // 同一位置的多来源只保留更高优先级的那条（① > ② > ③ > ④ > ⑤）。
            if source_rank(&source) < source_rank(&existing.source) {
                existing.source = source;
            }
            return;
        }
        let access = probe.access(&path);
        let evidence = if access == WorkspaceAccess::Missing {
            WorkspaceDataEvidence::default()
        } else {
            probe_workspace_data(&path)
        };
        candidates.push(WorkspaceCandidate {
            path,
            source,
            access,
            evidence,
        });
    };

    for record in &inputs.user_state.legacy_selections {
        push(
            record.workspace_root.clone(),
            CandidateSource::ImportedLegacySelection {
                origin: record.origin.clone(),
            },
            &mut candidates,
        );
    }
    for snapshot in &inputs.config_snapshots {
        if let Some(declared) = &snapshot.declared_runtime_dir {
            push(
                declared.clone(),
                CandidateSource::ConfigSnapshot {
                    snapshot: snapshot
                        .file
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    recorded_at_ms: snapshot.recorded_at_ms,
                },
                &mut candidates,
            );
        }
    }
    // 已知历史默认目录：旧行为（`log_dir` 推导）落在这里，是 P03 必须保护的位置。
    if let Some(historical) = historical_default_workspace(&inputs.log_dir) {
        push(
            historical.clone(),
            CandidateSource::KnownHistoricalDefault {
                name: "local_app_data_coolzhuagent".to_string(),
            },
            &mut candidates,
        );
    }
    if let Some(derived) = &inputs.legacy_log_dir_derived_workspace {
        push(
            derived.clone(),
            CandidateSource::LegacyLogDirDerivation,
            &mut candidates,
        );
    }
    if let Some(default_dir) = &inputs.packaged_default_workspace {
        push(
            default_dir.clone(),
            CandidateSource::PackagedDefault,
            &mut candidates,
        );
    }

    for candidate in &candidates {
        if candidate.access == WorkspaceAccess::Missing && candidate.source.is_legacy() {
            notes.push(ResolutionNote::new(
                "candidate_missing",
                format!(
                    "候选 {}（{}）不存在或不可达，仅作迁移信息，不作为数据源",
                    candidate.path.display(),
                    candidate.source.as_str()
                ),
            ));
        }
    }
    candidates
}

fn source_rank(source: &CandidateSource) -> u8 {
    match source {
        CandidateSource::UserSpecified => 1,
        CandidateSource::SavedSelection { .. } => 2,
        CandidateSource::ImportedLegacySelection { .. } => 3,
        CandidateSource::ConfigSnapshot { .. } => 4,
        CandidateSource::KnownHistoricalDefault { .. } => 5,
        CandidateSource::LegacyLogDirDerivation => 6,
        CandidateSource::PackagedDefault => 7,
    }
}

/// 已知历史默认目录：`<log_dir 的 LocalAppData 根>\CoolzhuAgent`。
///
/// 只识别 `<...>\CoolzhuAgent\logs\package-launcher` 这一既有形态，**不做全盘搜索**。
#[must_use]
pub fn historical_default_workspace(log_dir: &Path) -> Option<PathBuf> {
    if !is_launcher_log_dir(log_dir) {
        return None;
    }
    let logs_dir = log_dir.parent()?;
    if !is_named(logs_dir, "logs") {
        return None;
    }
    logs_dir.parent().map(Path::to_path_buf)
}

fn is_launcher_log_dir(log_dir: &Path) -> bool {
    is_named(log_dir, "package-launcher")
}

fn is_named(path: &Path, name: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(name))
}

/// 动作执行结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    Applied { revision: u64 },
    Skipped { reason: String },
}

/// 执行解析动作（**唯一写盘入口**）。
///
/// 失败/冲突**不阻断本次启动**（保持在已解析的工作区上启动），但必须作为告警上报，
/// 且**不得**宣布"选择已保存"。真正需要 fail-closed 的显式选择走 [`select_workspace`]。
pub fn apply_resolution_actions(
    user_state_root: &Path,
    decision: &LaunchPathDecision,
    now_ms: u64,
) -> Vec<(String, ActionOutcome)> {
    let mut outcomes = Vec::new();
    for action in &decision.actions {
        let label = action.describe();
        let expected_revision = action.expected_revision();
        let result = (|| -> Result<u64, LaunchError> {
            let current = load_user_state(user_state_root)?;
            let (path, selection_source) = action.selection();
            if let ResolutionAction::InitializeWorkspaceAndPersistSelection { path, .. } = action {
                fs::create_dir_all(path).map_err(LaunchError::Persistence)?;
            }
            let mut next = current.clone();
            let mut legacy_selections = current.legacy_selections.clone();
            if let Some(previous) = current.selection.as_ref() {
                if !same_canonical(&previous.workspace_root, &path) {
                    legacy_selections.push(LegacySelectionRecord {
                        workspace_root: previous.workspace_root.clone(),
                        origin: format!("replaced@r{}", current.revision),
                        imported_at_ms: now_ms,
                    });
                }
            }
            next.legacy_selections = legacy_selections;
            next.selection = Some(LauncherUserSelection {
                workspace_root: path.clone(),
                workspace_id: workspace_identity(&path),
                source: selection_source,
                recorded_at_ms: now_ms,
            });
            store_user_state(user_state_root, &next, expected_revision)
        })();
        match result {
            Ok(revision) => outcomes.push((label, ActionOutcome::Applied { revision })),
            Err(error) => outcomes.push((
                label,
                ActionOutcome::Skipped {
                    reason: error.to_string(),
                },
            )),
        }
    }
    outcomes
}

/// 只读地列出工作区候选与来源（恢复入口 `--list-candidates`）。
///
/// 与解析共用同一套候选发现：**只用可信来源**，不扫描用户磁盘，
/// 不创建/删除任何目录，不打开数据库、不升级 schema。
#[must_use]
pub fn list_workspace_candidates(
    inputs: &LaunchPathInputs,
    probe: &dyn WorkspaceAccessProbe,
) -> (Vec<WorkspaceCandidate>, Vec<ResolutionNote>) {
    let mut notes = Vec::new();
    let candidates = discover_candidates(inputs, probe, &mut notes);
    (candidates, notes)
}

/// 诊断用的"来源 + 用途 + 实际路径"表（`--list-candidates` 与错误呈现共用）。
#[must_use]
pub fn render_candidates(candidates: &[WorkspaceCandidate]) -> String {
    if candidates.is_empty() {
        return "（未发现任何候选）".to_string();
    }
    let mut lines = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        lines.push(format!("  [{}] {}", index + 1, candidate.describe()));
    }
    lines.join("\n")
}

/// 供 main.rs 记录"日志路径"用的小工具：把 `(进程, 文件)` 收集成快照字段。
#[must_use]
pub fn process_log_paths(entries: &[(&str, PathBuf)]) -> Vec<ProcessLogPath> {
    entries
        .iter()
        .map(|(process, path)| ProcessLogPath {
            process: (*process).to_string(),
            path: path.clone(),
        })
        .collect()
}

/// 环境变量查找（`%VAR%` 展开）：给命令行传入的工作区路径用。
///
/// 模板按**实际运行用户**的目录上下文解析（不在构建机上替换成开发者绝对路径）；
/// 变量未解析 ⇒ 明确报错，**不回退到当前目录或临时目录**。
pub fn expand_user_path_text(
    raw: &str,
    env_lookup: &mut dyn FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push('%');
            out.push_str(after);
            rest = "";
            break;
        };
        let name = &after[..end];
        if name.is_empty() {
            out.push_str("%%");
        } else {
            let value = env_lookup(name)
                .ok_or_else(|| format!("环境变量 %{name}% 未设置：无法解析该路径（不会回退到当前目录）"))?;
            out.push_str(&value.to_string_lossy());
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    let path = PathBuf::from(out.trim());
    if path.as_os_str().is_empty() {
        return Err("路径为空".to_string());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        path.push(format!("coolzhu-launch-paths-{label}-{nanos}-{seq}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_workspace_data(root: &Path) {
        fs::create_dir_all(root.join(crate::DATA_DIR_NAME)).unwrap();
        fs::write(
            root.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3"),
            b"sqlite",
        )
        .unwrap();
    }

    /// 可注入的探测：把指定路径标成"权限拒绝/磁盘断开"，其余走真实判定。
    struct ScriptedProbe {
        overrides: RefCell<HashMap<String, WorkspaceAccess>>,
    }

    impl ScriptedProbe {
        fn new(overrides: &[(&Path, WorkspaceAccess)]) -> Self {
            Self {
                overrides: RefCell::new(
                    overrides
                        .iter()
                        .map(|(path, access)| (canonical_key(path), *access))
                        .collect(),
                ),
            }
        }
    }

    impl WorkspaceAccessProbe for ScriptedProbe {
        fn access(&self, path: &Path) -> WorkspaceAccess {
            if let Some(override_access) = self.overrides.borrow().get(&canonical_key(path)) {
                return *override_access;
            }
            RealWorkspaceAccess.access(path)
        }
    }

    fn base_inputs(label: &str) -> (LaunchPathInputs, PathBuf, PathBuf) {
        let root = temp_dir(label);
        let user_state_root = root.join("user-state");
        let log_dir = root.join("localappdata").join("CoolzhuAgent").join("logs").join("package-launcher");
        (
            LaunchPathInputs {
                launch_id: format!("launch-{label}"),
                package_identity: "coolzhu-app-launcher/0.2.0+test".to_string(),
                config_schema_version: LAUNCHER_CONFIG_SCHEMA_VERSION,
                packaged_default_workspace: None,
                legacy_log_dir_derived_workspace: None,
                config_declares_runtime_dir: true,
                log_dir,
                user_state_root,
                user_state: LauncherUserState::default(),
                config_snapshots: Vec::new(),
                explicit_selection: None,
                now_ms: 1_700_000_000_000,
            },
            root,
            PathBuf::new(),
        )
    }

    // P01：全新用户首次启动 ⇒ 默认建议 USERPROFILE、明确初始化、不继承打包机/安装账号目录。
    #[test]
    fn p01_fresh_user_initializes_packaged_default_without_inheriting_installer_context() {
        let (mut inputs, root, _) = base_inputs("p01");
        let profile = root.join("home").join("alice");
        let default_dir = profile.join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, default_dir);
        assert_eq!(
            decision.resolved.resolution_source,
            ResolutionSource::PackagedDefaultFirstInit {
                config_key: RUNTIME_DIR_KEY.to_string()
            }
        );
        assert!(matches!(
            decision.actions.as_slice(),
            [ResolutionAction::InitializeWorkspaceAndPersistSelection { .. }]
        ));

        // 未启动前不创建目录（解析零副作用），执行动作后才显式初始化。
        assert!(!default_dir.exists());
        let outcomes = apply_resolution_actions(&inputs.user_state_root, &decision, inputs.now_ms);
        assert!(matches!(outcomes[0].1, ActionOutcome::Applied { revision: 1 }));
        assert!(default_dir.is_dir());

        // 落库的选择就是该目录，且不再依赖任何"打包机/安装账号"路径。
        let stored = load_user_state(&inputs.user_state_root).unwrap();
        assert_eq!(
            stored.selection.as_ref().map(|s| s.workspace_root.clone()),
            Some(default_dir)
        );
        assert_eq!(stored.revision, 1);
    }

    // P02：既有自定义工作区升级 ⇒ 仍打开原选择。
    #[test]
    fn p02_persisted_custom_workspace_survives_upgrade_and_keeps_config_untouched() {
        let (mut inputs, root, _) = base_inputs("p02");
        let custom = root.join("work").join("my-project");
        write_workspace_data(&custom);
        let config_text = "[model]\nbase_url = \"http://localhost:9999\"\napi_key_ref = \"session\"\n";
        fs::write(custom.join(crate::CONFIG_FILE_NAME), config_text).unwrap();
        inputs.packaged_default_workspace = Some(root.join("home").join("coolzhuagent"));
        inputs.user_state.selection = Some(LauncherUserSelection {
            workspace_root: custom.clone(),
            workspace_id: workspace_identity(&custom),
            source: "explicit_select".into(),
            recorded_at_ms: 1,
        });
        inputs.user_state.revision = 4;
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, custom);
        assert_eq!(decision.resolved.selection_revision, 4);
        assert!(decision.actions.is_empty(), "已保存选择不需要任何写动作");
        assert_eq!(
            fs::read_to_string(custom.join(crate::CONFIG_FILE_NAME)).unwrap(),
            config_text,
            "升级路径不得改写工作区配置（模型参数/Base URL/密钥保持）"
        );
    }

    // P03：旧版实际使用 LocalAppData 工作区 ⇒ 不因新版默认值改变而迁走或另建空库。
    #[test]
    fn p03_legacy_local_app_data_workspace_is_not_abandoned_for_new_default() {
        let (mut inputs, root, _) = base_inputs("p03");
        let legacy = root
            .join("localappdata")
            .join("CoolzhuAgent");
        write_workspace_data(&legacy);
        let new_default = root.join("home").join("alice").join("coolzhuagent");
        inputs.packaged_default_workspace = Some(new_default.clone());
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, legacy);
        assert_eq!(
            decision.resolved.resolution_source,
            ResolutionSource::ImportedLegacySelection {
                origin: "known_historical_default:local_app_data_coolzhuagent".to_string()
            }
        );

        apply_resolution_actions(&inputs.user_state_root, &decision, inputs.now_ms);
        assert!(
            !new_default.exists(),
            "不得因为默认值改变而另建空工作区（P03）"
        );
    }

    // P04：已选磁盘断开/目录不存在/权限拒绝 ⇒ 明确阻断，不自动创建替代库。
    #[test]
    fn p04_inaccessible_saved_selection_blocks_without_creating_a_replacement() {
        let (mut inputs, root, _) = base_inputs("p04");
        let disconnected = root.join("removable").join("workspace");
        inputs.user_state.selection = Some(LauncherUserSelection {
            workspace_root: disconnected.clone(),
            workspace_id: None,
            source: "explicit_select".into(),
            recorded_at_ms: 1,
        });
        let default_dir = root.join("home").join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        let probe = ScriptedProbe::new(&[]);

        let error = resolve_launch_paths(&inputs, &probe).unwrap_err();
        match &error {
            LaunchError::WorkspaceUnavailable { role, path, remedy, .. } => {
                assert_eq!(role, "saved_user_selection");
                assert_eq!(path, &disconnected);
                assert!(remedy.contains("--select-workspace"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!default_dir.exists());
        assert!(error.to_string().contains("不会创建替代工作区"));
    }

    // P04（权限拒绝）：目录存在但不可写时同样阻断，不落到下一个候选。
    #[test]
    fn p04_permission_denied_saved_selection_blocks_instead_of_falling_through() {
        let (mut inputs, root, _) = base_inputs("p04-perm");
        let selected = root.join("locked").join("workspace");
        fs::create_dir_all(&selected).unwrap();
        let default_dir = root.join("home").join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        inputs.user_state.selection = Some(LauncherUserSelection {
            workspace_root: selected.clone(),
            workspace_id: None,
            source: "explicit_select".into(),
            recorded_at_ms: 1,
        });
        let probe = ScriptedProbe::new(&[(selected.as_path(), WorkspaceAccess::ReadOnly)]);

        let error = resolve_launch_paths(&inputs, &probe).unwrap_err();
        assert!(matches!(error, LaunchError::WorkspaceUnavailable { .. }));
        assert!(!default_dir.exists());
    }

    // P05：两个历史目录都有数据 ⇒ 不按 mtime/容量/数量自动选、不合并、不删除。
    #[test]
    fn p05_two_data_holding_candidates_require_explicit_choice_and_change_nothing() {
        let (mut inputs, root, _) = base_inputs("p05");
        let legacy = root.join("localappdata").join("CoolzhuAgent");
        write_workspace_data(&legacy);
        let other = root.join("work").join("second");
        write_workspace_data(&other);
        fs::write(other.join("notes.txt"), b"hello").unwrap();
        inputs.packaged_default_workspace = Some(root.join("home").join("alice").join("coolzhuagent"));
        inputs.user_state.legacy_selections = vec![LegacySelectionRecord {
            workspace_root: other.clone(),
            origin: "imported_from_previous_install".into(),
            imported_at_ms: 0,
        }];
        let probe = ScriptedProbe::new(&[]);

        let error = resolve_launch_paths(&inputs, &probe).unwrap_err();
        match &error {
            LaunchError::WorkspaceSelectionAmbiguous { candidates, remedy } => {
                assert_eq!(candidates.len(), 2);
                assert!(remedy.contains("不合并"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        // 两份数据都还在，且没有新建目录。
        assert!(legacy.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3").is_file());
        assert!(other.join("notes.txt").is_file());
        assert!(!inputs
            .packaged_default_workspace
            .as_ref()
            .unwrap()
            .exists());
    }

    // P06：修改 log_dir ⇒ 工作区、实际数据库绑定、共享输入安全路径不变。
    #[test]
    fn p06_changing_log_dir_does_not_move_workspace_or_shared_state() {
        let (mut inputs, root, _) = base_inputs("p06");
        let default_dir = root.join("home").join("alice").join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        let user_state_root = inputs.user_state_root.clone();
        let probe = ScriptedProbe::new(&[]);

        let first = resolve_launch_paths(&inputs, &probe).unwrap();
        let mut other = inputs.clone();
        other.log_dir = root.join("elsewhere").join("logs").join("package-launcher");
        let second = resolve_launch_paths(&other, &probe).unwrap();

        assert_eq!(first.resolved.workspace_root, second.resolved.workspace_root);
        assert_eq!(
            first.resolved.input_safety_state_root,
            second.resolved.input_safety_state_root
        );
        assert_eq!(first.resolved.user_state_root, second.resolved.user_state_root);
        assert_eq!(first.resolved.data_dir_binding, second.resolved.data_dir_binding);
        assert_eq!(first.resolved.user_state_root, user_state_root);
    }

    // P07：新版配置缺 runtime_dir ⇒ 明确报错；旧版缺键 ⇒ 可解释兼容流程；不混用。
    #[test]
    fn p07_missing_runtime_dir_semantics_differ_between_config_versions() {
        let (mut inputs, root, _) = base_inputs("p07");
        let probe = ScriptedProbe::new(&[]);

        // 新版（v2）解析层不会接受"缺键"：输入里没有默认值也没有任何可确认来源 → 阻断。
        inputs.packaged_default_workspace = None;
        inputs.legacy_log_dir_derived_workspace = None;
        let error = resolve_launch_paths(&inputs, &probe).unwrap_err();
        match &error {
            LaunchError::WorkspaceSelectionRequired { reason, remedy, .. } => {
                assert!(reason.contains("未声明工作区默认值"));
                assert!(remedy.contains("--select-workspace"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        // 旧版（v1）缺键且推导位置为空：走兼容支路，给出推导结果作为迁移信息并要求确认一次。
        let derived = root.join("localappdata").join("CoolzhuAgent");
        let mut legacy = inputs.clone();
        legacy.config_schema_version = 1;
        legacy.config_declares_runtime_dir = false;
        legacy.legacy_log_dir_derived_workspace = Some(derived.clone());
        let error = resolve_launch_paths(&legacy, &probe).unwrap_err();
        match &error {
            LaunchError::WorkspaceSelectionRequired { candidates, remedy, .. } => {
                assert!(candidates.iter().any(|c| c.path == derived));
                assert!(remedy.contains("不会自动采用推导结果"));
                assert!(remedy.contains("--select-workspace"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        // 旧版缺键但推导位置确实有数据 ⇒ 一次性采用（P03 场景），不新建、不要求确认。
        write_workspace_data(&derived);
        let decision = resolve_launch_paths(&legacy, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, derived);
        assert!(matches!(
            decision.resolved.resolution_source,
            ResolutionSource::ImportedLegacySelection { .. }
        ));
    }

    // P08：runtime_dir 正确但 paths.data_dir 有覆盖 ⇒ 不隐藏覆盖。
    #[test]
    fn p08_launcher_does_not_pin_business_data_paths_and_reports_observed_override() {
        let (mut inputs, root, _) = base_inputs("p08");
        let workspace = root.join("work").join("project");
        fs::create_dir_all(workspace.join("agent-data")).unwrap();
        inputs.packaged_default_workspace = Some(workspace.clone());
        let probe = ScriptedProbe::new(&[]);
        let decision = resolve_launch_paths(&inputs, &probe).unwrap();

        // launcher 不请求固定会话库 → 工作区配置的 paths.data_dir 覆盖才会生效。
        assert_eq!(
            decision.resolved.data_dir_binding,
            DataPathBinding::WorkspaceConfigAuthority
        );
        assert!(decision.resolved.requested_session_db.is_none());
        assert!(!decision.resolved.data_dir_override_observed());

        // 后台回读显示库在 agent-data 下：快照必须如实呈现（不"制造恢复成功"）。
        let observed = ObservedBackground {
            workspace: Some(workspace.clone()),
            session_db: Some(workspace.join("agent-data").join("web-sessions.sqlite3")),
            build_version: Some("abc1234 · 2026-09-25".into()),
            ..ObservedBackground::default()
        };
        let resolved = decision.resolved.with_observations(&observed);
        assert!(resolved.data_dir_override_observed());
        assert_eq!(
            resolved.effective_data_dir(),
            Some(workspace.join("agent-data"))
        );
        let json = resolved.to_json();
        assert_eq!(
            json["observed"]["data_dir_override_observed"],
            Value::Bool(true)
        );
        assert_eq!(
            json["observed"]["session_db"],
            Value::String(
                workspace
                    .join("agent-data")
                    .join("web-sessions.sqlite3")
                    .to_string_lossy()
                    .to_string()
            )
        );
    }

    // P10：中文/空格路径合法可用；相对路径/未解析变量明确报错，不落回 cwd。
    #[test]
    fn p10_unicode_and_space_paths_work_relative_or_unresolved_paths_error() {
        let (mut inputs, root, _) = base_inputs("p10");
        let workspace = root.join("我的 工作区").join("coolzhu agent");
        fs::create_dir_all(&workspace).unwrap();
        inputs.packaged_default_workspace = Some(workspace.clone());
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, workspace);
        assert!(decision.resolved.workspace_id.is_some());

        // 相对路径的显式选择：拒绝（否则等于静默落到"当前目录"）。
        let mut relative = inputs.clone();
        relative.explicit_selection = Some(ExplicitSelection {
            path: PathBuf::from("relative-workspace"),
            persist: false,
            declared_by: "--workspace".into(),
        });
        let error = resolve_launch_paths(&relative, &probe).unwrap_err();
        assert!(matches!(error, LaunchError::WorkspaceUnavailable { role, .. } if role == "explicit_this_launch"));

        // 未解析变量：展开失败必须报错（不能变成当前目录下的字面量目录）。
        let mut lookup = |name: &str| -> Option<OsString> {
            (name == "USERPROFILE").then(|| OsString::from("C:/Users/me"))
        };
        assert!(expand_user_path_text("%NOPE%\\ws", &mut lookup).is_err());
        assert_eq!(
            expand_user_path_text("%USERPROFILE%\\coolzhuagent", &mut lookup).unwrap(),
            PathBuf::from("C:/Users/me/coolzhuagent")
        );
    }

    // P11：提权安装 + 普通用户启动 + 多用户 ⇒ 各自解析自己的用户目录，不共享他人默认选择。
    #[test]
    fn p11_multi_user_resolution_is_per_user_context() {
        let root = temp_dir("p11");
        let user_a_state = user_state_root_for(
            Some(&root.join("user-a").join("AppData").join("Local")),
            Some(&root.join("user-a")),
            None,
        )
        .unwrap();
        let user_b_state = user_state_root_for(
            Some(&root.join("user-b").join("AppData").join("Local")),
            Some(&root.join("user-b")),
            None,
        )
        .unwrap();
        assert_ne!(user_a_state, user_b_state);

        // A 建立选择后，B 完全看不到（各自的用户级选择文件）。
        write_workspace_data(&root.join("user-a").join("coolzhuagent"));
        select_workspace(
            &user_a_state,
            &root.join("user-a").join("coolzhuagent"),
            &RealWorkspaceAccess,
            10,
        )
        .unwrap();
        let state_b = load_user_state(&user_b_state).unwrap();
        assert!(state_b.selection.is_none());

        // 安装根不能出现在任何解析结果里（用户级选择不在安装目录）。
        assert!(!user_a_state.starts_with(root.join("Program Files")));
    }

    // P12：选择保存中断 / 并发改选择 ⇒ revision 冲突或原子恢复，不出现两个矛盾的"已确认选择"。
    #[test]
    fn p12_selection_writes_are_atomic_and_revision_checked() {
        let (inputs, root, _) = base_inputs("p12");
        let user_state_root = inputs.user_state_root.clone();
        let workspace = root.join("ws");
        fs::create_dir_all(&workspace).unwrap();

        let revision = select_workspace(&user_state_root, &workspace, &RealWorkspaceAccess, 1).unwrap();
        assert_eq!(revision, 1);
        assert!(!user_state_root
            .join(format!("{LAUNCHER_USER_STATE_FILE}{SELECTION_TMP_SUFFIX}"))
            .exists());
        assert!(!user_state_root.join("launcher-user.json.lock").exists());

        // 并发者拿着过期 revision 提交 → 冲突，且旧选择不被覆盖。
        let mut stale = load_user_state(&user_state_root).unwrap();
        stale.selection = Some(LauncherUserSelection {
            workspace_root: root.join("other"),
            workspace_id: None,
            source: "stale".into(),
            recorded_at_ms: 2,
        });
        let error = store_user_state(&user_state_root, &stale, 0).unwrap_err();
        match error {
            LaunchError::SelectionConflict {
                expected_revision,
                actual_revision,
                ..
            } => {
                assert_eq!(expected_revision, 0);
                assert_eq!(actual_revision, 1);
            }
            other => panic!("unexpected error: {other}"),
        }
        let after = load_user_state(&user_state_root).unwrap();
        assert_eq!(
            after.selection.as_ref().map(|s| s.workspace_root.clone()),
            Some(workspace)
        );
        assert_eq!(after.revision, 1);

        // 损坏的选择文件不得被静默忽略（否则等于悄悄换工作区）。
        fs::write(user_state_file(&user_state_root), b"{ not json").unwrap();
        let error = load_user_state(&user_state_root).unwrap_err();
        assert!(matches!(error, LaunchError::SelectionStore { .. }));
    }

    // P13：错目录已产生新消息后恢复旧目录 ⇒ 两份数据都保留，恢复入口不暗中删除"误建"目录。
    #[test]
    fn p13_recovery_keeps_both_datasets_and_never_deletes_the_mis_created_one() {
        let (mut inputs, root, _) = base_inputs("p13");
        // 场景：随包默认值位置曾被误建为空目录，此后用户又在里面产生了新消息。
        let mis_created = root.join("home").join("alice").join("coolzhuagent");
        write_workspace_data(&mis_created);
        inputs.packaged_default_workspace = Some(mis_created.clone());
        let original = root.join("work").join("original");
        write_workspace_data(&original);
        inputs.user_state.legacy_selections = vec![LegacySelectionRecord {
            workspace_root: original.clone(),
            origin: "imported_from_previous_install".into(),
            imported_at_ms: 0,
        }];
        let probe = ScriptedProbe::new(&[]);

        // 两个都有数据 → 阻断，绝不自动选，也绝不删除。
        let error = resolve_launch_paths(&inputs, &probe).unwrap_err();
        assert!(matches!(error, LaunchError::WorkspaceSelectionAmbiguous { .. }));
        assert!(mis_created.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3").is_file());
        assert!(original.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3").is_file());

        // 恢复入口：显式选回旧目录后，误建目录仍在（作为 legacy 记录保留）。
        let revision = select_workspace(
            &inputs.user_state_root,
            &original,
            &RealWorkspaceAccess,
            7,
        )
        .unwrap();
        assert_eq!(revision, 1);
        assert!(mis_created.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3").is_file());
        let state = load_user_state(&inputs.user_state_root).unwrap();
        assert_eq!(
            state.selection.as_ref().map(|s| s.workspace_root.clone()),
            Some(original)
        );
    }

    // P14：切换 workspace 时输入隔离继续按资源生效，不因新目录获得新空安全库。
    #[test]
    fn p14_input_safety_state_root_is_shared_and_workspace_independent() {
        let (mut inputs, root, _) = base_inputs("p14");
        let user_state_root = inputs.user_state_root.clone();
        let workspace_a = root.join("ws-a");
        let workspace_b = root.join("ws-b");
        fs::create_dir_all(&workspace_a).unwrap();
        fs::create_dir_all(&workspace_b).unwrap();
        inputs.packaged_default_workspace = Some(workspace_a.clone());
        let probe = ScriptedProbe::new(&[]);

        let first = resolve_launch_paths(&inputs, &probe).unwrap();
        let mut second_inputs = inputs.clone();
        second_inputs.packaged_default_workspace = Some(workspace_b.clone());
        let second = resolve_launch_paths(&second_inputs, &probe).unwrap();

        assert_eq!(
            first.resolved.input_safety_state_root,
            second.resolved.input_safety_state_root
        );
        assert_eq!(
            first.resolved.input_safety_state_root,
            input_safety_state_root_for(&user_state_root)
        );
        assert!(
            !second
                .resolved
                .input_safety_state_root
                .starts_with(&workspace_b),
            "输入安全状态不能落到工作区里（否则切换工作区 = 新空安全库）"
        );
    }

    // P15：MSI repair/升级/卸载再装 ⇒ 用户选择与运行数据按政策保留（选择不在安装根）。
    #[test]
    fn p15_selection_and_config_baseline_live_outside_the_install_root() {
        let (inputs, root, _) = base_inputs("p15");
        let install_root = root.join("Program Files").join("CoolzhuAgent");
        fs::create_dir_all(&install_root).unwrap();
        let user_state_root = inputs.user_state_root.clone();
        assert!(!user_state_root.starts_with(&install_root));

        let config_text = r#"{"runtime_dir": "%USERPROFILE%\\coolzhuagent"}"#;
        let snapshot = record_config_snapshot(
            &user_state_root,
            &install_root.join("config").join("package-launcher.json"),
            config_text,
            Some(&root.join("old-workspace")),
            Some("%USERPROFILE%\\coolzhuagent"),
            5,
        )
        .unwrap();
        assert!(snapshot.is_some());
        // 同一内容只留一份（升级反复启动不会堆快照）。
        assert!(record_config_snapshot(
            &user_state_root,
            &install_root.join("config").join("package-launcher.json"),
            config_text,
            Some(&root.join("old-workspace")),
            Some("%USERPROFILE%\\coolzhuagent"),
            6,
        )
        .unwrap()
        .is_none());

        // 卸载再装：随包配置被替换后，旧基线与用户选择仍在用户级位置。
        let workspace = root.join("home").join("alice").join("coolzhuagent");
        fs::create_dir_all(&workspace).unwrap();
        select_workspace(&user_state_root, &workspace, &RealWorkspaceAccess, 9).unwrap();
        let snapshots = read_config_snapshots(&user_state_root);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].declared_runtime_dir.as_deref(),
            Some(root.join("old-workspace").as_path())
        );
        let state = load_user_state(&user_state_root).unwrap();
        assert!(state.selection.is_some());
    }

    // 升级替换随包配置时，"旧声明 → 新声明"必须可见（不能悄悄改变工作区）。
    #[test]
    fn config_drift_between_releases_is_reported_but_not_acted_on() {
        let (mut inputs, root, _) = base_inputs("drift");
        let old_workspace = root.join("old");
        inputs.config_snapshots = vec![ConfigSnapshot {
            file: root.join("snap.json"),
            recorded_at_ms: 1,
            config_path: root.join("installed").join("config").join("package-launcher.json"),
            declared_runtime_dir: Some(old_workspace.clone()),
            declared_runtime_dir_text: Some("%LOCALAPPDATA%\\CoolzhuAgent".into()),
        }];
        let new_workspace = root.join("new-default");
        fs::create_dir_all(&new_workspace).unwrap();
        inputs.packaged_default_workspace = Some(new_workspace.clone());
        // 用户已保存选择在别处：必须继续用用户选择，不受默认值变化影响。
        let selected = root.join("work");
        fs::create_dir_all(&selected).unwrap();
        inputs.user_state.selection = Some(LauncherUserSelection {
            workspace_root: selected.clone(),
            workspace_id: workspace_identity(&selected),
            source: "explicit_select".into(),
            recorded_at_ms: 1,
        });
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, selected);
        assert!(decision
            .notes
            .iter()
            .any(|note| note.code == "packaged_config_workspace_changed"));
    }

    // 用户级选择根不能从 log_dir 反推。
    #[test]
    fn user_state_root_never_derives_from_log_dir() {
        let root = temp_dir("state-root");
        let local = root.join("Local");
        let state_root = user_state_root_for(Some(&local), None, None).unwrap();
        assert_eq!(state_root, local.join(USER_STATE_DIR_NAME));
        // 只有 %USERPROFILE% 时按用户目录上下文推导（仍是用户级位置）。
        let profile = user_state_root_for(None, Some(&root), None).unwrap();
        assert_eq!(profile, root.join("AppData").join("Local").join(USER_STATE_DIR_NAME));
        // 显式覆盖只允许绝对路径。
        assert!(user_state_root_for(None, Some(&root), Some(Path::new("relative"))).is_err());
    }

    // 选择文件只存选择/身份/来源/schema/revision，不复制模型参数与密钥。
    #[test]
    fn selection_file_carries_only_selection_identity_and_revision() {
        let (inputs, root, _) = base_inputs("shape");
        let workspace = root.join("ws");
        fs::create_dir_all(&workspace).unwrap();
        select_workspace(&inputs.user_state_root, &workspace, &RealWorkspaceAccess, 3).unwrap();
        let text = fs::read_to_string(user_state_file(&inputs.user_state_root)).unwrap();
        for forbidden in ["api_key", "base_url", "api-key", "secret", "token"] {
            assert!(
                !text.to_ascii_lowercase().contains(forbidden),
                "选择文件不应包含 {forbidden}: {text}"
            );
        }
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["schema_version"], LAUNCHER_USER_SCHEMA_VERSION);
        assert_eq!(value["revision"], 1);
        assert_eq!(value["selection"]["source"], "explicit_select");
        assert!(value["selection"]["workspace_id"].is_string());
    }

    // P01 前提：产品自己的用户级状态条目（logs/runtime/config-snapshots/…）不算工作区数据，
    // 否则全新用户会被误判成"已有工作区"而不再在 USERPROFILE 下首次初始化。
    #[test]
    fn user_level_state_entries_are_not_workspace_data() {
        let root = temp_dir("state-not-data");
        for name in ["logs", "runtime", "config-snapshots", "input-safety"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join(LAUNCHER_USER_STATE_FILE), b"{}").unwrap();
        fs::write(root.join("launcher-user.json.lock"), b"").unwrap();
        let evidence = probe_workspace_data(&root);
        assert!(!evidence.has_workspace_data(), "{evidence:?}");
        assert_eq!(evidence.other_entries, 0);

        // 真正的工作区数据（会话库 / 兼容 JSON / coolzhu.toml / 附件 / 普通文件）仍然被识别。
        fs::create_dir_all(root.join(crate::DATA_DIR_NAME).join("attachments")).unwrap();
        fs::write(
            root.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3"),
            b"sqlite",
        )
        .unwrap();
        assert!(probe_workspace_data(&root).has_workspace_data());

        let plain = temp_dir("state-plain-file");
        fs::write(plain.join("notes.txt"), b"hello").unwrap();
        assert!(probe_workspace_data(&plain).has_workspace_data());
    }

    // 全新用户：状态目录里只有产品状态 ⇒ 仍然在随包默认值处首次初始化（P01）。
    #[test]
    fn fresh_user_first_init_ignores_state_only_dirs_and_does_not_inherit_installer_context() {
        let (mut inputs, root, _) = base_inputs("p01-fixed");
        let user_state_root = inputs.user_state_root.clone();
        fs::create_dir_all(user_state_root.join("logs").join("package-launcher")).unwrap();
        fs::create_dir_all(user_state_root.join("runtime")).unwrap();
        let profile = root.join("home").join("alice");
        let default_dir = profile.join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, default_dir);
        assert_eq!(
            decision.resolved.resolution_source,
            ResolutionSource::PackagedDefaultFirstInit {
                config_key: RUNTIME_DIR_KEY.to_string()
            }
        );
        apply_resolution_actions(&inputs.user_state_root, &decision, inputs.now_ms);
        assert!(default_dir.is_dir());
    }

    // 候选发现必须只读：不创建、不删除、不触发 schema 升级。
    #[test]
    fn candidate_discovery_is_read_only() {
        let (mut inputs, root, _) = base_inputs("readonly");
        let legacy = root.join("localappdata").join("CoolzhuAgent");
        write_workspace_data(&legacy);
        let legacy_db = legacy.join(crate::DATA_DIR_NAME).join("web-sessions.sqlite3");
        let before = fs::read(&legacy_db).unwrap();
        let default_dir = root.join("home").join("alice").join("coolzhuagent");
        inputs.packaged_default_workspace = Some(default_dir.clone());
        let probe = ScriptedProbe::new(&[]);

        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        assert_eq!(decision.resolved.workspace_root, legacy);
        assert_eq!(fs::read(&legacy_db).unwrap(), before);
        assert!(!default_dir.exists());
        assert_eq!(
            fs::read_dir(root.join("localappdata")).unwrap().count(),
            1,
            "候选发现不得在历史默认根旁边留下任何新条目"
        );
    }

    // 解析只做一次：同一份快照被 launcher/子进程共享（不同 log_dir 不改变它）。
    #[test]
    fn resolved_snapshot_exposes_sources_uses_and_paths() {
        let (mut inputs, root, _) = base_inputs("snapshot");
        let workspace = root.join("ws");
        fs::create_dir_all(&workspace).unwrap();
        inputs.packaged_default_workspace = Some(workspace.clone());
        inputs.explicit_selection = Some(ExplicitSelection {
            path: workspace.clone(),
            persist: false,
            declared_by: "--workspace".into(),
        });
        let probe = ScriptedProbe::new(&[]);
        let decision = resolve_launch_paths(&inputs, &probe).unwrap();
        let json = decision.resolved.to_json();
        assert_eq!(json["resolution_source"], "explicit_this_launch");
        assert_eq!(json["requested"]["workspace_root"], Value::String(workspace.to_string_lossy().to_string()));
        assert_eq!(json["observed"]["session_db"], Value::Null);
        assert!(json["user_state_root"].is_string());
        assert!(json["input_safety_state_root"].is_string());
        assert!(decision.resolved.display_lines().iter().any(|line| line.contains("日志[")));
    }

    // 分享/呈现：候选渲染包含来源、用途与数据证据，但不含时间/容量排序信息。
    #[test]
    fn candidate_rendering_lists_sources_without_size_or_time_ranking() {
        let (mut inputs, root, _) = base_inputs("render");
        let workspace = root.join("ws");
        fs::create_dir_all(&workspace).unwrap();
        inputs.packaged_default_workspace = Some(workspace.clone());
        let probe = ScriptedProbe::new(&[]);
        let candidates = discover_candidates(&inputs, &probe, &mut Vec::new());
        let rendered = render_candidates(&candidates);
        assert!(rendered.contains("packaged_default"));
        assert!(!rendered.contains("mtime"));
        assert!(!rendered.contains("size="));
    }
}
