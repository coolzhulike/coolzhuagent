mod action_evidence;
mod action_injection;
mod agent_event;
mod bash;
mod bootstrap;
mod compact;
mod config;
mod conversation;
mod fact_store;
mod file_ops;
mod hooks;
mod input_safety;
mod json;
mod late_facts;
mod mcp;
mod mcp_client;
mod mcp_stdio;
mod memory;
mod oauth;
mod permission_gate;
mod permissions;
mod prompt;
mod recovery;
mod remote;
mod run_contract;
pub mod sandbox;
mod semantic;
mod submission_dedup;
mod session;
mod tool;
mod usage;

pub use agent_event::{AgentEvent, AgentEventKind, ContextSnapshot, ItemId, ThreadId, TurnId};
pub use bash::{execute_bash, BashCommandInput, BashCommandOutput};
pub use bootstrap::{BootstrapPhase, BootstrapPlan};
pub use compact::{
    compact_session, estimate_session_tokens, format_compact_summary,
    get_compact_continuation_message, should_compact, CompactionConfig, CompactionResult,
};
pub use config::{
    active_profile_name, ConfigEntry, ConfigError, ConfigLoader, ConfigSource, ContextEngineMode,
    McpConfigCollection, McpManagedProxyServerConfig, McpOAuthConfig, McpRemoteServerConfig,
    McpSdkServerConfig, McpServerConfig, McpStdioServerConfig, McpTransport,
    McpWebSocketServerConfig, OAuthConfig, ResolvedPermissionMode, RuntimeAgentConfig,
    RuntimeConfig, RuntimeFeatureConfig, RuntimeHookConfig, RuntimePluginConfig,
    ScopedMcpServerConfig, CLAW_SETTINGS_SCHEMA_NAME,
};
pub use conversation::{
    ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, RuntimeError, StaticToolExecutor,
    ToolError, ToolExecutor, TurnSummary,
};
pub use action_evidence::{
    ActionEvidence, ActionSurface, EvidenceBasis, EvidenceReference, EvidenceReferenceKind,
};
pub use fact_store::{
    ActionFact, AppendOnlyFactStore, AttemptDecision, FactLogBackend, FactLogRecord, FactLookup,
    FactSnapshot, FactStore, FactStoreError, InMemoryFactLog, JsonlFactLog, ReceiptDecision,
    RecoveryDecision, RecoveryFact, RecoveryFactOutcome, RecoveryLink, RunAttemptFact,
    TerminalControlDecision, UsageAttemptDecision, UsageFactSummary,
};
// RD4-02A（第七轮裁决 §1）：共享输入安全存储的**领域契约**（纯类型；SQLite 实现在 Web/宿主适配边界，
// core-runtime 不引入 rusqlite）。权威正文见 round7-rulings-and-gates.md §1。
pub use input_safety::{
    decide_permit_reuse, disposition_allows_automatic_privilege_escalation, disposition_for,
    parse_input_safety_store_id, resolve_intake_close_race, DescendantStopEvidence,
    ExecutorDisposition, ExecutorEvidenceSource, ExecutorInstanceState, ExecutorObservation,
    classify_executor_instance, ExecutionAttemptId, ExecutorRegistration,
    ExecutorVerification, HelperExecutionState, HelperHandshake, HelperLifecycleError,
    preflight_agreement, AwaitingPermitOutcome, HelperIdentity, HelperLifecycleException,
    HelperPermitSignal, PreflightRefusal, ProtocolVersions, TWO_PHASE_HELPER_PROTOCOL_VERSION,
    HelperReadySignal, InvalidHelperSignal, ProcessInstanceEvidence, resolve_awaiting_permit,
    InvalidExecutionAttemptId, InvalidExecutorRegistration,
    TerminatePhase,
    BlockingRefRejection, IncidentState, InputPermit, InputPermitState, InputSafetyEvent,
    InputSafetyEventKind, InputSafetyIncident, InputSafetyRecoveryOperation,
    InputSafetyResourceScope, InputSafetyResourceState, InputSafetyStoreId, IntakeCloseRaceOutcome,
    InvalidInputPermit, InvalidInputSafetyStoreId, InvalidResourceScope, PermitAnomalyKind,
    PermitReuseDecision, PermitTransitionError, RecoveryDisposition, RecoveryStage,
    ReleaseIsolationDecision, ResourceSafetyState, TightenOnlyAction, VerifiedBlockingRef,
    INPUT_SAFETY_SCHEMA_VERSION, INPUT_SAFETY_STORE_ID_PREFIX,
};
// RD4-01（第五轮裁决 A-1）：遗留 CU 运行收敛的契约与规则（additive 导出，语义见各自文档）。
pub use fact_store::{
    latest_legacy_cu_run_convergence, reconcile_legacy_cu_run_convergence,
    LegacyCuRunConvergenceRefusal, LegacyCuRunConvergenceRuleDecision, LegacyCuRunObservedState,
};
pub use run_contract::{
    legacy_cu_run_convergence_block_precedes_terminal_write,
    legacy_cu_run_convergence_order_is_safe, legacy_cu_run_convergence_reopen_is_allowed,
    legacy_cu_run_explanation_claims_forbidden_wording, CurrentResourceCandidateBasis,
    CurrentResourceSafetyCheck, LegacyCuRunConvergenceDecision, LegacyCuRunConvergenceEvidence,
    LegacyCuRunConvergenceFact, LegacyCuRunConvergenceIntent, LegacyCuRunConvergenceKey,
    LegacyCuRunConvergenceStep, LegacyCuRunMissingDimensions, LegacyCuRunOriginalFacts,
    LegacyCuRunRecoveryOperator, LegacyCuRunSideEffectLimits, LegacyCuRunSubject,
    LegacyResourceScope, LegacyRunControlState, LegacyRunHistoricalOutcome,
    LegacyRunInputResourceState, FORBIDDEN_LEGACY_CU_RUN_CONVERGENCE_ORDER,
    LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION, LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE,
    LEGACY_CU_RUN_CONVERGENCE_RULE_VERSION, LEGACY_CU_RUN_CONVERGENCE_STEPS,
    LEGACY_CU_RUN_FORBIDDEN_CLAIM_WORDINGS,
    LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION,
};
pub use file_ops::{
    edit_file, glob_search, grep_search, read_file, write_file, EditFileOutput, GlobSearchOutput,
    GrepSearchInput, GrepSearchOutput, ReadFileOutput, StructuredPatchHunk, TextFilePayload,
    WriteFileOutput,
};
pub use hooks::{
    HookEvent, HookRunResult, HookRunner, HOOK_CAPABILITY_POST_TOOL_USE,
    HOOK_CAPABILITY_PRE_TOOL_USE,
};
pub use lsp::{
    FileDiagnostics, LspContextEnrichment, LspError, LspManager, LspServerConfig, SymbolLocation,
    WorkspaceDiagnostics,
};
pub use mcp::{
    mcp_server_signature, mcp_tool_name, mcp_tool_prefix, normalize_name_for_mcp,
    scoped_mcp_config_hash, unwrap_ccr_proxy_url,
};
pub use mcp_client::{
    McpClientAuth, McpClientBootstrap, McpClientTransport, McpManagedProxyTransport,
    McpRemoteTransport, McpSdkTransport, McpStdioTransport,
};
pub use mcp_stdio::{
    spawn_mcp_stdio_process, JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse,
    ManagedMcpTool, McpInitializeClientInfo, McpInitializeParams, McpInitializeResult,
    McpInitializeServerInfo, McpListResourcesParams, McpListResourcesResult, McpListToolsParams,
    McpListToolsResult, McpReadResourceParams, McpReadResourceResult, McpResource,
    McpResourceContents, McpServerManager, McpServerManagerError, McpStdioProcess, McpTool,
    McpToolCallContent, McpToolCallParams, McpToolCallResult, UnsupportedMcpServer,
};
pub use memory::{
    build_similarity_edges, cluster_by_similarity, decide_memory_write, effective_recall_score,
    filter_active, filter_unexpired, find_near_duplicates, find_supersede_target,
    is_low_value_memory, is_memory_bead_active, is_memory_bead_expired, memory_bead_matches_query,
    memory_bead_signature, memory_layer_for_kind, memory_layer_rank, normalize_memory_layer,
    prune_memory_beads_to, query_memory_beads, rank_beads_by_query, recall_at_k,
    render_prompt_memory_context, rule_based_valid_until, select_prompt_memory_beads,
    summarize_memory_beads, MemoryBeadQueryOptions, MemoryBeadView, MemoryBeadsSummary, MemoryEdge,
    MemoryLayer, MemoryWriteDecision,
};
pub use oauth::{
    clear_oauth_credentials, code_challenge_s256, credentials_path, generate_pkce_pair,
    generate_state, load_oauth_credentials, loopback_redirect_uri, parse_oauth_callback_query,
    parse_oauth_callback_request_target, save_oauth_credentials, OAuthAuthorizationRequest,
    OAuthCallbackParams, OAuthRefreshRequest, OAuthTokenExchangeRequest, OAuthTokenSet,
    PkceChallengeMethod, PkceCodePair,
};
pub use permission_gate::{
    default_protected_rules, evaluate_permission, is_path_inside_workspace, match_protected_rule,
    normalize_for_match, PathAccess, PathTarget, ProtectedRule, SessionGrantView,
};
pub use permissions::{
    PermissionMode, PermissionOutcome, PermissionPolicy, PermissionProfile,
    PermissionPromptDecision, PermissionPrompter, PermissionRequest,
};
pub use prompt::{
    load_system_prompt, prepend_bullets, ContextFile, ProjectContext, PromptBuildError,
    SystemPromptBuilder, FRONTIER_MODEL_NAME, SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
};
pub use remote::{
    inherited_upstream_proxy_env, no_proxy_list, read_token, upstream_proxy_ws_url,
    RemoteSessionContext, UpstreamProxyBootstrap, UpstreamProxyState, DEFAULT_REMOTE_BASE_URL,
    DEFAULT_SESSION_TOKEN_PATH, DEFAULT_SYSTEM_CA_BUNDLE, NO_PROXY_HOSTS, UPSTREAM_PROXY_ENV_KEYS,
};
pub use run_contract::{
    admit_action_fact, admit_action_origin, is_placeholder_identity_value, ActionContext,
    ActionIdentityAdmission, ActionOrigin, ActionOriginAdmission, ActionOriginAuthority,
    ActionReceipt, ActionSource, CancelOrigin, ContextKind, ControlPlaneContext,
    ControlPlaneOperations, ConversationActionContext, EffectStatus, EffectiveRunIdentityScope,
    GoalVerdict, HostCausalMetadata, HostRunOutcome, IdentityAnomaly, IdentityDimension,
    InputDelivery, InputOwnerEpoch, InputReleaseStatus, ParentRunLink, PartialObservation,
    PlannedActionEnvelope, PlannedAttemptRegistry, PlannedRequestAttempt, RetryOwner, RunBudget,
    RunClaimToken, RunContractError, RunIdentity, RunIdentityScope, RunParentRelation,
    RunScopeContext, RunTerminalStatus, SessionWriterEpoch, StablePlannedAttemptId,
    TrustedOriginContext, RUN_IDENTITY_SCHEMA_VERSION,
    // PR-02A：`ActionOriginAuthority` 的**实现方**必须能命名这些记录类型，否则 trait 在
    // crate 之外根本无法实现（事实：在此之前生产里一次都没调用过 `admit_action_origin`，
    // 唯一实现是 crate 内的测试替身）。这里只是把它们导出，不改任何语义。
    CleanupIncidentRecord, CleanupRelation, ControlOperationRecord, HostOperationRecord,
    RunRelationRecord, ToolCallRelation,
};
pub use semantic::{
    cosine_similarity, decode_vector, encode_vector, hash_embed, BruteForceCosineIndex,
    SemanticIndex, DEFAULT_EMBED_DIM,
};
pub use session::{ContentBlock, ConversationMessage, MessageRole, Session, SessionError};
pub use tool::{
    runtime_tool_execute, PermissionDecision, PermissionGateReport, RuntimeToolContext,
    ToolCallContext, ToolCaller, ToolInvocationExecutor, ToolInvoke, ToolOutcome,
    ToolOutcomeStatus,
};
pub use action_injection::{ActionInjectionRegistry, InjectionDecision};
pub use late_facts::{append_late_fact, LateFact, LateFactAppend, LateFactError, LateFactKind};
pub use recovery::{RecoveryAttempt, RecoveryError};
pub use submission_dedup::{
    MessageSubmissionKey, MessageSubmissionRegistry, SubmissionDecision, SubmissionRecord,
};
pub use usage::{
    format_usd, pricing_for_model, ModelPricing, ReportedUsage, TokenUsage, UsageAttempt,
    UsageAttemptOutcome, UsageCostEstimate, UsageLedger, UsageTracker,
};

#[cfg(test)]
fn test_env_mutex() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// 本 crate **唯一**的进程状态锁：测试里改 cwd 与改进程环境共用这一把
/// （RPR-01b 裁决第 1 条：不得每个模块各建一把锁）。
///
/// 取锁是 poison-tolerant 的（持锁用例 panic 后后续用例仍能继续）；
/// 但"环境恢复失败"这一信号不再由此吞掉，见 [`test_env::record_restore_failure`]。
#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    test_env_mutex()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 测试用的进程环境变量收口（RPR-01b）：统一锁 + RAII guard。
///
/// 本 crate 不引用其它 crate 的同类辅助（各 crate 独立、辅助函数各有一份），
/// 该模块也不对外暴露，仅 `#[cfg(test)]`。
#[cfg(test)]
pub(crate) mod test_env {
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

    /// 恢复环境变量失败时的全局留痕（裁决第 5 条：不得被 poison-tolerant 取锁静默吞掉）。
    static RESTORE_FAILED: AtomicBool = AtomicBool::new(false);

    fn restore_failures() -> &'static Mutex<Vec<String>> {
        static FAILURES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
        FAILURES.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn record_restore_failure(details: &[String]) {
        RESTORE_FAILED.store(true, Ordering::SeqCst);
        restore_failures()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(details.iter().cloned());
        // 正确措辞（裁决第 6 条）：污染的是**当前测试进程的后续执行**，直到恢复（本进程内无法恢复时即进程结束）。
        // 不是"用户系统环境变量被永久修改"。
        eprintln!(
            "RPR-01b：恢复进程环境变量失败（本进程后续执行可能仍读到被写入的值，直到恢复或进程结束）：{}",
            details.join("; ")
        );
    }

    /// 前序有恢复失败时**响亮地**失败：拒绝在可能被污染的环境上继续跑依赖进程级环境解析的用例。
    fn assert_no_restore_failure() {
        if !RESTORE_FAILED.load(Ordering::SeqCst) {
            return;
        }
        let details = restore_failures()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .join("; ");
        panic!(
            "RPR-01b：前序用例恢复环境变量失败（{details}），本进程环境可能仍被污染；\
             拒绝在脏环境下继续（poison-tolerant 取锁不得把该信号吞掉）。"
        );
    }

    /// 测试专用：清除「恢复失败」标志。调用方必须**已持有**统一锁令牌 ——
    /// 标志检查在真锁内进行，此时其它线程都阻塞在锁上，故清标志不存在竞争窗口。
    pub(crate) fn clear_restore_failure_for_test() {
        RESTORE_FAILED.store(false, Ordering::SeqCst);
        restore_failures()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub(crate) fn restore_failure_recorded() -> bool {
        RESTORE_FAILED.load(Ordering::SeqCst)
    }

    thread_local! {
        /// 当前线程是否已持有统一锁（用于识别嵌套 guard，避免二次加锁自锁）。
        static LOCK_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// 统一锁的持有令牌（裁决第 4 条：嵌套 guard 复用已持有的锁令牌）。
    ///
    /// `owned == None` 表示复用了**本线程**外层已持有的锁：不再加锁、也不负责释放 ——
    /// `std::sync::Mutex` 不可重入，嵌套时第二次加锁会直接自锁。
    ///
    /// 注意：嵌套识别靠令牌自己的线程局部标记，**裸 `test_env_lock()` 不会被识别**；
    /// 同一线程里不要"令牌 + 裸取锁"混用（会自锁）。
    pub(crate) struct EnvLockToken {
        owned: Option<MutexGuard<'static, ()>>,
    }

    impl EnvLockToken {
        pub(crate) fn acquire() -> Self {
            if LOCK_HELD.with(std::cell::Cell::get) {
                // 嵌套：复用外层令牌。仍检查恢复失败标志 —— 此时本线程持有真锁，
                // 其它线程都阻塞在锁上，故不存在"并发观察到标志"的竞争窗口。
                assert_no_restore_failure();
                return Self { owned: None };
            }
            // 先拿锁，再检查标志：panic 时线程局部状态（LOCK_HELD）尚未置位，保持干净。
            let guard = super::test_env_lock();
            assert_no_restore_failure();
            LOCK_HELD.with(|held| held.set(true));
            Self { owned: Some(guard) }
        }

        /// 是否复用了本线程外层已持有的锁令牌（嵌套场景为 `true`）。
        pub(crate) fn is_reentrant(&self) -> bool {
            self.owned.is_none()
        }
    }

    impl Drop for EnvLockToken {
        fn drop(&mut self) {
            if self.owned.is_some() {
                LOCK_HELD.with(|held| held.set(false));
            }
            // `owned` 的 MutexGuard 随后自动析构释放锁；poison 语义与旧用例一致。
        }
    }

    /// 设置/移除一个环境变量，把 std 在非法名字（空 / 含 '=' / 含 NUL）、值含 NUL 时的 panic
    /// 收敛成 `Err`：恢复路径运行在 `Drop` 里，绝不能让 std 的 panic 直接逃逸
    /// （正在 unwind 时二次 panic 会 abort 整个测试进程）。
    fn apply_env(name: &str, target: Option<&OsString>) -> Result<(), String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match target {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }));
        result.map_err(|_| format!("{name}: set/remove 环境变量时 panic（名字非法或值含 NUL）"))
    }

    pub(crate) fn env_set(
        name: &'static str,
        value: impl AsRef<std::ffi::OsStr>,
    ) -> (&'static str, Option<OsString>) {
        (name, Some(value.as_ref().to_os_string()))
    }

    /// 目标状态 = 「不存在」（注意与「存在但为空串」不同，后者用 `env_set(name, "")`）。
    pub(crate) fn env_unset(name: &'static str) -> (&'static str, Option<OsString>) {
        (name, None)
    }

    /// RAII 版**进程级**环境变量作用域。
    ///
    /// 用法：`let _env = ScopedEnv::new(vec![env_set("HOME", &home), env_unset("CLAW_CONFIG_HOME")]);`
    ///
    /// 保证：
    /// 1. **持锁区间**：从「读原值之前」到「全部恢复之后」一直持有统一锁令牌
    ///    （`ScopedEnv::drop` 体先恢复，字段随后才析构释放锁）；
    /// 2. **三态无损**（裁决第 2 条）：原值用 `Option<OsString>` 保存，区分「原来不存在」(`None`)、
    ///    「原来为空串」(`Some(OsString::new())`)、「原来有值」（不用空串冒充"不存在"）；
    /// 3. **部分失败回滚**（裁决第 3 条）：多变量构造时先把原值登记进 `self` 再改值，
    ///    中途失败时 `self` 已是构造函数的局部变量，析构会恢复**已改动**的部分；
    /// 4. **嵌套复用令牌**（裁决第 4 条）：见 [`EnvLockToken`]；
    /// 5. **恢复失败不静默**（裁决第 5 条）：见 [`record_restore_failure`]。
    ///
    /// 边界（裁决第 7 条）：RAII 只覆盖**栈展开（unwind）**路径。`std::process::abort`、
    /// `libc::_exit` / `pthread_exit` 等不展开栈的终止方式**不会**运行析构函数，
    /// 本 guard 不承诺"任何情况下都必定恢复"。
    pub(crate) struct ScopedEnv {
        /// 先声明锁令牌：`ScopedEnv::drop` 体（恢复）跑完后字段才析构释放锁。
        lock: EnvLockToken,
        originals: Vec<(&'static str, Option<OsString>)>,
    }

    impl ScopedEnv {
        pub(crate) fn new(targets: Vec<(&'static str, Option<OsString>)>) -> Self {
            // 先登记原值再改值：二者之间没有可 panic 的语句，
            // 因此"已登记但未改值"的条目恢复时只是把原值写回一遍，幂等无害。
            let mut scope = Self {
                lock: EnvLockToken::acquire(),
                originals: Vec::with_capacity(targets.len()),
            };
            for (name, target) in targets {
                let original = std::env::var_os(name);
                scope.originals.push((name, original));
                if let Err(error) = apply_env(name, target.as_ref()) {
                    // apply 失败时该变量状态未变（std 在真正改动前就 panic 了），
                    // 撤销刚登记的恢复项，避免对非法名字做一次注定失败的"恢复"。
                    scope.originals.pop();
                    panic!("ScopedEnv 设置环境变量失败：{error}");
                }
            }
            scope
        }

        /// 测试专用：直接注入「原值」而不做任何 set —— 用于构造"恢复必然失败"的路径。
        pub(crate) fn with_injected_originals(originals: Vec<(&'static str, Option<OsString>)>) -> Self {
            Self {
                lock: EnvLockToken::acquire(),
                originals,
            }
        }

        /// 本 guard 是否嵌套复用了外层锁令牌。
        pub(crate) fn is_reentrant(&self) -> bool {
            self.lock.is_reentrant()
        }

        /// 已保存的原值（`None` = 该变量没有登记；`Some(None)` = 原来不存在）。
        pub(crate) fn original_of(&self, name: &str) -> Option<&Option<OsString>> {
            self.originals
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, original)| original)
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            let mut failures = Vec::new();
            // 逆序恢复：后设的先回退。
            for (name, original) in self.originals.iter().rev() {
                if let Err(error) = apply_env(name, original.as_ref()) {
                    failures.push(error);
                }
            }
            if failures.is_empty() {
                return;
            }
            record_restore_failure(&failures);
            // 非 unwind 路径可以安全 panic：立刻让用例失败，绝不静默；
            // unwind 路径不能二次 panic（会 abort 整个测试进程），故只"留痕 + 标志 + 阻塞后续"。
            if !std::thread::panicking() {
                panic!(
                    "ScopedEnv 恢复环境变量失败：{}（进程环境与本进程后续执行可能仍被污染）",
                    failures.join("; ")
                );
            }
        }
    }
}

/// 测试用：RAII 版进程 cwd 切换（只负责恢复 cwd，不负责删目录 —— 临时目录的清理仍由用例自己决定）。
///
/// 背景（RPR-01）：手写 `let previous = current_dir(); set_current_dir(root); ...;
/// set_current_dir(previous)` 一旦用例中途 panic（断言/expect 失败），末尾的恢复语句执行不到，
/// 进程 cwd 就永久留在临时目录里。又因为 `test_env_lock` 是 poison-tolerant 的，
/// 后续用例不会报"锁被毒化"，而是**静默**跑在错误 cwd 下。
/// 交给 Drop 恢复可覆盖 unwind 路径。本 crate 不引用其它 crate 的同类辅助（各 crate 独立）。
#[cfg(test)]
pub(crate) struct ScopedCurrentDir {
    original: std::path::PathBuf,
}

#[cfg(test)]
impl ScopedCurrentDir {
    pub(crate) fn enter(path: impl AsRef<std::path::Path>) -> Self {
        let original = std::env::current_dir().expect("current dir");
        let path = path.as_ref();
        if let Err(error) = std::env::set_current_dir(path) {
            panic!("set current dir to {}: {error}", path.display());
        }
        Self { original }
    }
}

#[cfg(test)]
impl Drop for ScopedCurrentDir {
    fn drop(&mut self) {
        // Drop 里不能 panic：用例正在 unwind 时二次 panic 会 abort 整个测试进程。
        // 恢复失败只能尽力而为并留痕（原始目录被删等）。
        if let Err(error) = std::env::set_current_dir(&self.original) {
            eprintln!(
                "ScopedCurrentDir 恢复进程 cwd 失败 -> {}：{error}",
                self.original.display()
            );
        }
    }
}

#[cfg(test)]
mod scoped_current_dir_tests {
    use super::ScopedCurrentDir;
    use std::path::PathBuf;

    fn missing_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("runtime-scoped-cwd-missing-{name}"))
    }

    /// 回归（RPR-01）：Drop 不得 panic —— 原目录不可达时只能尽力而为。
    /// 若改回 `.expect(...)`，本用例会 panic 失败（改前即如此）。
    #[test]
    fn drop_never_panics_when_original_is_gone() {
        let _guard = super::test_env_lock();
        let before = std::env::current_dir().expect("cwd before");
        // 直接构造"原目录不存在"的 guard，模拟恢复失败的路径。
        {
            let _cwd = ScopedCurrentDir {
                original: missing_dir("original"),
            };
        }
        assert_eq!(
            std::env::current_dir().expect("cwd after drop"),
            before,
            "恢复失败时不得改动 cwd"
        );
    }
}

#[cfg(test)]
mod scoped_env_tests {
    use super::test_env::{
        clear_restore_failure_for_test, env_set, env_unset, restore_failure_recorded, EnvLockToken,
        ScopedEnv,
    };
    use std::ffi::OsString;

    /// 回归（RPR-01b）：正常返回时三态原值（不存在 / 空串 / 有值）无损恢复。
    #[test]
    fn scoped_env_restores_all_three_original_states_on_normal_drop() {
        let outer = ScopedEnv::new(vec![
            env_unset("CLAW_RPR01B_RT_MISSING"),
            env_set("CLAW_RPR01B_RT_EMPTY", ""),
            env_set("CLAW_RPR01B_RT_VALUE", "original"),
        ]);
        assert!(!outer.is_reentrant(), "最外层 guard 必须真正持有统一锁");

        {
            let inner = ScopedEnv::new(vec![
                env_set("CLAW_RPR01B_RT_MISSING", "now-set"),
                env_unset("CLAW_RPR01B_RT_EMPTY"),
                env_set("CLAW_RPR01B_RT_VALUE", "changed"),
            ]);
            assert!(inner.is_reentrant(), "嵌套 guard 必须复用外层令牌（不再加锁）");
            assert_eq!(inner.original_of("CLAW_RPR01B_RT_MISSING"), Some(&None));
            assert_eq!(
                inner.original_of("CLAW_RPR01B_RT_EMPTY"),
                Some(&Some(OsString::from(""))),
                "原来为空串必须记为 Some(\"\")，不得退化成 None"
            );
            assert_eq!(
                inner.original_of("CLAW_RPR01B_RT_VALUE"),
                Some(&Some(OsString::from("original")))
            );
        }

        assert_eq!(std::env::var_os("CLAW_RPR01B_RT_MISSING"), None);
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_RT_EMPTY"),
            Some(OsString::from(""))
        );
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_RT_VALUE"),
            Some(OsString::from("original"))
        );
        assert!(!restore_failure_recorded());
    }

    /// 回归（RPR-01b）：panic（unwind）后必须恢复，且**后续执行**看到的环境与进入作用域前一致。
    /// 判别性：改前手写 restore 在 panic 时被跳过，后续用例会读到作用域内的测试值。
    #[test]
    fn scoped_env_restores_on_panic_and_keeps_later_readers_consistent() {
        let _holder = ScopedEnv::new(vec![env_unset("CLAW_CONFIG_HOME")]);
        let before = std::env::var_os("CLAW_CONFIG_HOME");
        assert_eq!(before, None);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _env = ScopedEnv::new(vec![env_set("CLAW_CONFIG_HOME", "rpr01b-dirty-home")]);
            assert_eq!(
                std::env::var_os("CLAW_CONFIG_HOME"),
                Some(OsString::from("rpr01b-dirty-home"))
            );
            panic!("模拟作用域内断言失败");
        }));
        assert!(panicked.is_err(), "作用域内的 panic 必须继续向外传播");

        assert_eq!(
            std::env::var_os("CLAW_CONFIG_HOME"),
            before,
            "panic（unwind）后必须恢复到进入作用域前的三态"
        );
        // 生产读取路径（config::default_config_home 优先读 CLAW_CONFIG_HOME）不得再看到作用域内的脏值。
        assert_ne!(
            crate::config::default_config_home(),
            std::path::PathBuf::from("rpr01b-dirty-home"),
            "panic 后生产读取路径不得仍读到作用域内写入的值"
        );
        assert!(!restore_failure_recorded());
    }

    /// 回归（P-08）：作用域内**提前返回**（`return` / `?` 传播，非 panic）同样必须恢复。
    ///
    /// 补齐同类回归覆盖面：tool-registry 侧已有同名用例，core-runtime 侧此前只有
    /// "正常 Drop / panic / 构造失败"三条，缺"提前返回"这条最容易踩的路径。
    /// 判别性：改前的手写 restore 写在函数末尾，提前返回根本走不到。
    #[test]
    fn scoped_env_restores_on_early_return() {
        fn probe() -> Result<(), String> {
            let _env = ScopedEnv::new(vec![env_set("CLAW_RPR01B_RT_EARLY", "in-scope")]);
            assert_eq!(
                std::env::var_os("CLAW_RPR01B_RT_EARLY"),
                Some(OsString::from("in-scope"))
            );
            return Err(String::from("提前返回"));
        }

        let _holder = ScopedEnv::new(vec![env_unset("CLAW_RPR01B_RT_EARLY")]);
        assert_eq!(std::env::var_os("CLAW_RPR01B_RT_EARLY"), None);

        assert_eq!(probe(), Err(String::from("提前返回")));

        assert_eq!(
            std::env::var_os("CLAW_RPR01B_RT_EARLY"),
            None,
            "提前返回后必须恢复到作用域前的状态"
        );
        assert!(!restore_failure_recorded());
    }

    /// 回归（RPR-01b 裁决第 3 条）：多变量构造中途失败时，已修改的部分也要恢复。
    #[test]
    fn scoped_env_rolls_back_applied_vars_when_later_construction_fails() {
        let _holder = ScopedEnv::new(vec![
            env_unset("CLAW_RPR01B_RT_PARTIAL_A"),
            env_unset("CLAW_RPR01B_RT_PARTIAL_B"),
        ]);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _env = ScopedEnv::new(vec![
                env_set("CLAW_RPR01B_RT_PARTIAL_A", "applied-before-failure"),
                env_set("CLAW_RPR01B_RT_PARTIAL_B", "also-applied"),
                env_set("CLAW_RPR01B=BAD", "invalid"),
            ]);
        }));
        assert!(panicked.is_err(), "非法名字必须让构造 panic（不得静默跳过）");
        assert_eq!(std::env::var_os("CLAW_RPR01B_RT_PARTIAL_A"), None);
        assert_eq!(std::env::var_os("CLAW_RPR01B_RT_PARTIAL_B"), None);
    }

    /// 回归（RPR-01b 裁决第 5 条）：恢复失败不得被 poison-tolerant 取锁静默吞掉。
    /// 注入"原值无法恢复"的作用域（名字非法）→ Drop 立刻响亮失败 + 置全局标志 +
    /// 下一次取锁（含嵌套复用路径）拒绝在脏环境上继续。
    #[test]
    fn env_restore_failure_is_not_swallowed_by_poison_tolerant_lock() {
        let token = EnvLockToken::acquire();
        clear_restore_failure_for_test();
        assert!(!restore_failure_recorded());

        let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _broken = ScopedEnv::with_injected_originals(vec![(
                "CLAW_RPR01B=BAD",
                Some(OsString::from("x")),
            )]);
        }));
        assert!(dropped.is_err(), "非 unwind 路径下恢复失败必须立刻失败（不得静默）");
        assert!(restore_failure_recorded(), "恢复失败必须留下全局记录");

        let reused =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(EnvLockToken::acquire));
        assert!(reused.is_err(), "存在未处理恢复失败时取锁必须响亮失败");

        clear_restore_failure_for_test();
        assert!(!restore_failure_recorded());
        drop(token);
    }
}
