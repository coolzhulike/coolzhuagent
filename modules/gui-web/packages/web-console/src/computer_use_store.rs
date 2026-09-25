use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use computer_use::{
    ComputerUseResult, ComputerUseRunState, ComputerUseSurface, ComputerUseTerminalStatus,
};
use runtime::{
    legacy_cu_run_convergence_order_is_safe, reconcile_legacy_cu_run_convergence,
    CurrentResourceSafetyCheck, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus,
    LegacyCuRunConvergenceEvidence, LegacyCuRunConvergenceFact, LegacyCuRunConvergenceIntent,
    LegacyCuRunConvergenceKey, LegacyCuRunConvergenceRefusal,
    LegacyCuRunConvergenceRuleDecision, LegacyCuRunConvergenceStep, LegacyCuRunMissingDimensions,
    LegacyCuRunObservedState, LegacyCuRunOriginalFacts, LegacyCuRunRecoveryOperator,
    LegacyCuRunSideEffectLimits, LegacyCuRunSubject, LegacyResourceScope, LegacyRunControlState,
    LegacyRunHistoricalOutcome, LegacyRunInputResourceState,
};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn apply_session_migration_v11(connection: &Connection) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS computer_use_runs (
            call_id TEXT PRIMARY KEY,
            provider_tool_call_id TEXT,
            turn_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            chat_room_id TEXT,
            idempotency_key TEXT NOT NULL,
            objective_json TEXT NOT NULL,
            surface TEXT NOT NULL,
            state TEXT NOT NULL,
            state_version INTEGER NOT NULL DEFAULT 0,
            risk_class TEXT NOT NULL DEFAULT 'observe',
            approval_state TEXT NOT NULL DEFAULT 'not_required',
            approval_deadline_ms INTEGER,
            action_count INTEGER NOT NULL DEFAULT 0,
            replan_count INTEGER NOT NULL DEFAULT 0,
            no_progress_count INTEGER NOT NULL DEFAULT 0,
            current_observation_generation INTEGER NOT NULL DEFAULT 0,
            deadline_ms INTEGER NOT NULL,
            terminal_result_json TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_session_turn
            ON computer_use_runs(session_id, turn_id);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_turn_idempotency
            ON computer_use_runs(session_id, turn_id, idempotency_key, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_state
            ON computer_use_runs(state);
        CREATE INDEX IF NOT EXISTS idx_computer_use_runs_updated
            ON computer_use_runs(updated_at_ms);

        CREATE TABLE IF NOT EXISTS computer_use_steps (
            run_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            observation_generation INTEGER NOT NULL,
            action_type TEXT NOT NULL,
            normalized_target TEXT NOT NULL,
            action_fingerprint TEXT NOT NULL,
            status TEXT NOT NULL,
            error_code TEXT,
            before_evidence_ref TEXT,
            after_evidence_ref TEXT,
            visible_progress INTEGER NOT NULL DEFAULT 0,
            input_delivery TEXT,
            partial INTEGER,
            path_completed INTEGER,
            confirmed_point_count INTEGER,
            effect_status TEXT,
            goal_verdict TEXT,
            input_release_status TEXT,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            PRIMARY KEY (run_id, step_index),
            FOREIGN KEY (run_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_steps_run
            ON computer_use_steps(run_id, step_index);
        CREATE INDEX IF NOT EXISTS idx_computer_use_steps_status
            ON computer_use_steps(status);

        CREATE TABLE IF NOT EXISTS computer_use_step_details (
            run_id TEXT NOT NULL, step_index INTEGER NOT NULL, action_json TEXT NOT NULL,
            PRIMARY KEY(run_id, step_index),
            FOREIGN KEY(run_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS computer_use_planner_diagnostics (
            id INTEGER PRIMARY KEY AUTOINCREMENT, call_id TEXT NOT NULL,
            turn_id TEXT NOT NULL, room_id TEXT, session_id TEXT NOT NULL,
            request_kind TEXT NOT NULL, observation_generation INTEGER NOT NULL,
            model TEXT NOT NULL, provider_response_id TEXT,
            response_json TEXT NOT NULL, error_code TEXT,
            started_at_ms INTEGER NOT NULL, completed_at_ms INTEGER NOT NULL,
            FOREIGN KEY(call_id) REFERENCES computer_use_runs(call_id) ON DELETE CASCADE
        );

        -- RPR-05b-1：未确认释放的**人工解除事实**。
        --
        -- 历史事实只追加：`computer_use_steps.input_release_status='unknown'`
        -- 永不被改写成 not_sent/released。解除不是"改写历史"，而是追加一条
        -- 带操作者来源、前置/后置检查、理由与 epoch 的记录，声明"这批 run 的残留
        -- 输入已由人处理完毕"。互锁判据因此是"存在未解除的未确认释放"。
        CREATE TABLE IF NOT EXISTS computer_use_release_resolutions (
            resolution_id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            previous_epoch INTEGER NOT NULL,
            operator_source TEXT NOT NULL,
            operator_id TEXT,
            reason TEXT NOT NULL,
            input_owner_epoch_before INTEGER,
            input_owner_epoch_after INTEGER,
            covered_run_count INTEGER NOT NULL,
            covered_step_count INTEGER NOT NULL,
            precheck_json TEXT NOT NULL,
            postcheck_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_computer_use_release_resolutions_scope_epoch
            ON computer_use_release_resolutions(session_id, turn_id, epoch);
        CREATE INDEX IF NOT EXISTS idx_computer_use_release_resolutions_scope_created
            ON computer_use_release_resolutions(session_id, turn_id, created_at_ms);
        -- 解除覆盖到的 run 清单：判据靠这张表排除"已解除"的 run，
        -- 不需要改写 computer_use_steps / computer_use_runs 的任何一个既有事实列。
        CREATE TABLE IF NOT EXISTS computer_use_release_resolution_runs (
            resolution_id INTEGER NOT NULL,
            run_id TEXT NOT NULL,
            PRIMARY KEY (resolution_id, run_id),
            FOREIGN KEY (resolution_id)
                REFERENCES computer_use_release_resolutions(resolution_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_release_resolution_runs_run
            ON computer_use_release_resolution_runs(run_id);
        "#,
    )?;
    for definition in [
        "input_delivery TEXT",
        "partial INTEGER",
        "path_completed INTEGER",
        "confirmed_point_count INTEGER",
        "effect_status TEXT",
        "goal_verdict TEXT",
        "input_release_status TEXT",
    ] {
        ensure_computer_use_steps_column(connection, definition)?;
    }
    if current < 12 {
        connection.execute_batch("PRAGMA user_version = 12;")?;
    }
    // 解除事实表刻意**不**推进 `user_version`：这条版本阶梯由 main.rs 的
    // v12..v20 迁移按序掌管，这里抢号会让别人"版本已推进但工作没做"。
    // 建表本身是 `IF NOT EXISTS`，与 v16/v17/v18 的先例一致，每次初始化都安全补齐。
    Ok(())
}

/// CU 工作区归属的**上下文版本**：说明 `workspace_id` 是按哪一版归属契约写下来的。
///
/// 它不是"工作区版本"、也不是"当前工作区代次"——只是让将来改变归属来源语义时，读回方能
/// 区分"旧契约写下的归属"与"新契约写下的归属"，而不是靠猜。
pub(crate) const CU_WORKSPACE_CONTEXT_VERSION: i64 = 1;

/// 迁移**前置规则**：两个打开入口（`main.rs` 的 `initialize_session_schema` 与
/// [`ComputerUseRunStore::open`]）必须共用这一处判定，否则会漂移成两套阶梯（第六轮 §6 边界 3）。
///
/// 规则：库的 `user_version` **超前**于本二进制所知终点 ⇒ **明确拒绝**。
/// 不得"把版本号降下来修复"：写一个旧版本号会让后续写入落在一个自己并不认识其 schema 的库上，
/// 从而静默产生错位数据。版本号保持原样，由调用方 fail-closed 处理。
pub(crate) fn ensure_session_schema_not_from_the_future(
    connection: &Connection,
) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let supported = crate::SESSION_SCHEMA_VERSION;
    if current > supported {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "会话库 schema 比本二进制更新：user_version={current}，本构建只支持到 {supported}；\
             拒绝在不认识的 schema 上继续操作（不得降版本号\"修复\"）"
        )));
    }
    Ok(())
}

/// v22：`computer_use_runs` 增加**可空**工作区归属列（裁决 §5.1）。
///
/// - 可空是给**历史行**留的：迁移之前写入的运行没有归属记录，读回时必须呈现"历史归属未记录"，
///   **不得**按当前工作区批量回填（因此这里只加列，不写任何 UPDATE）。
/// - 新运行永不写 NULL：写入侧由 [`NewComputerUseRun::workspace`] 的类型（非 `Option`）保证。
/// - 与 v11 同例：本文件拥有的表由本文件补列，`IF NOT EXISTS` 风格、每次初始化都安全补齐，
///   并推进 `user_version`（阶梯由 main.rs 按序掌管，这里只推进到本步终点）。
pub(crate) fn apply_session_migration_v22(connection: &Connection) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for definition in [
        "workspace_id TEXT",
        "workspace_context_version INTEGER",
    ] {
        ensure_computer_use_runs_column(connection, definition)?;
    }
    if current < 22 {
        connection.execute_batch("PRAGMA user_version = 22;")?;
    }
    Ok(())
}

/// v22 只追加可空归属列；历史运行没有归属记录，因此必须保留 NULL。
fn ensure_computer_use_runs_column(
    connection: &Connection,
    definition: &str,
) -> rusqlite::Result<()> {
    let column = definition
        .split_whitespace()
        .next()
        .expect("静态迁移定义必须含列名");
    let mut statement = connection.prepare("PRAGMA table_info(computer_use_runs)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|existing| existing == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE computer_use_runs ADD COLUMN {definition}"
        ))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RD4-01（第五轮裁决 A-1）：遗留 CU 运行收敛的存储映射与编解码
// ---------------------------------------------------------------------------
//
// 契约（事实形状、允许说什么、三个维度分开）在 `runtime::run_contract` 的"遗留 CU 运行的
// 收敛契约"一节；规则（幂等、不覆盖）在 `runtime::fact_store` 的
// `reconcile_legacy_cu_run_convergence`。本节只做**存储**：列/表、编解码、查询、单事务写入。
//
// 三条存储口径：
//
// 1. **不改写历史列**：`state` / `state_version` / `terminal_result_json` / `updated_at_ms`
//    / `workspace_id` 一行都不动（`workspace_id` 永远是 NULL：历史归属不猜造、不回填）。
//    收敛后的运行控制状态落在**本文件新增的列** `legacy_convergence_state` 上，
//    值只可能是 [`LEGACY_RUN_CONVERGENCE_CONTROL_STATE`]。
//    这样做的理由：CU 状态机（`computer_use::ComputerUseRunState`）的词表里没有
//    `Interrupted`，而 `state` 是那个状态机的列；把 `state` 改写成 `'interrupted'`
//    会让整行读不回来（`parse_run_state` 报错），也会丢掉"原状态"这个必须原样保存的维度。
//    **不得**为了不改约束就写 `'succeeded'`（或任何成功/失败/取消结论）。
// 2. **不改写历史列的另一面**：旧 SQL（`terminal_result_json IS NULL AND state NOT IN …`）
//    在收敛后**仍然命中**这个 run，因此该 scope 的旧活动运行互锁与未确认释放判据
//    都不会被收敛清零——这正是裁决要求的"输入资源状态不随 run 终态自动清零"。
// 3. **单事务**：`computer_use_legacy_run_convergences`（收敛事实）与旧 run 行的收敛列
//    在**同一笔** Immediate 事务里写入；会话库与输入安全库不是同一事务，因此本文件
//    **不宣称跨库原子性**，只提供"可重入的收敛意图 + 幂等关联"（见意图表与
//    `pending_legacy_run_convergence_intents`）。

/// 收敛后的运行控制状态字面量（**本 store 的口径**，不是 CU 状态机的 `state` 值）。
pub(crate) const LEGACY_RUN_CONVERGENCE_CONTROL_STATE: &str = "interrupted";

/// CU 状态机里"已收尾"的状态字面量（`computer_use::ComputerUseRunState` 的终态变体）。
///
/// 判据 SQL 由本常量拼出：同一条列表在文件里只存一份，避免"哪份才是真的"。
const TERMINAL_RUN_STATE_LITERALS: [&str; 5] = [
    "succeeded",
    "failed",
    "blocked",
    "cancelled",
    "timed_out",
];

/// 终态字面量的 SQL 列表（`'succeeded','failed',…`）。
fn terminal_run_state_sql_list() -> String {
    TERMINAL_RUN_STATE_LITERALS
        .iter()
        .map(|state| format!("'{state}'"))
        .collect::<Vec<_>>()
        .join(",")
}

/// 遗留运行收敛的第二步迁移（列 + 表）。
///
/// **已登记**：阶梯末步在 `main.rs` 的 `apply_session_migration_v22` 之后有**恰好一行**
/// `computer_use_store::apply_session_migration_v23_legacy_run_convergence(connection)?;`，
/// 且 `SESSION_SCHEMA_VERSION` = `23`；`ComputerUseRunStore::open()` 与 v22 同例也调用它
/// （本文件拥有的列/表在独立打开时同样必须可用）。
///
/// 兼容性：只追加**可空**列与非唯一/唯一索引、`IF NOT EXISTS` 建表，不重建
/// `computer_use_runs`，因此旧行、旧二进制（`user_version` 已到 22 的库）都仍然可读。
pub(crate) fn apply_session_migration_v23_legacy_run_convergence(
    connection: &Connection,
) -> rusqlite::Result<()> {
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for definition in [
        "legacy_convergence_id INTEGER",
        "legacy_convergence_state TEXT",
        "legacy_convergence_reconciled_at_ms INTEGER",
    ] {
        ensure_computer_use_runs_column(connection, definition)?;
    }
    connection.execute_batch(
        r#"
        -- 收敛事实：一条遗留运行至多一条（对账键唯一）。
        CREATE TABLE IF NOT EXISTS computer_use_legacy_run_convergences (
            convergence_id INTEGER PRIMARY KEY AUTOINCREMENT,
            reconciliation_key TEXT NOT NULL UNIQUE,
            -- 被处理对象
            source_database_identity TEXT NOT NULL,
            source_database_identity_registered_now INTEGER NOT NULL,
            source_database_identity_not_past_attribution TEXT,
            original_run_id TEXT NOT NULL,
            -- 原始信息（原样保存，收敛不改写它们）
            original_state TEXT NOT NULL,
            original_state_version INTEGER NOT NULL,
            session_id TEXT,
            turn_id TEXT,
            -- 缺失维度
            workspace_id_unrecorded INTEGER NOT NULL,
            other_gaps_json TEXT NOT NULL,
            -- 本次决定
            terminal_status TEXT NOT NULL,
            reason_code TEXT NOT NULL,
            explanation TEXT NOT NULL,
            rule_version INTEGER NOT NULL,
            recovery_operation_id TEXT NOT NULL,
            -- 依据（五组引用；空组表示"本次没有取得该组依据"）
            evidence_json TEXT NOT NULL,
            -- 本次执行者
            recovery_service_instance TEXT NOT NULL,
            recovery_control_authority TEXT NOT NULL,
            operated_at_ms INTEGER NOT NULL,
            -- 维度①运行控制状态
            control_continuation_allowed INTEGER NOT NULL,
            -- 维度②历史执行结果（与①③分开列，互不推导）
            historical_outcome_kind TEXT NOT NULL,
            historical_goal_verdict TEXT,
            historical_execution_ended_at_ms INTEGER,
            historical_evidence_refs_json TEXT NOT NULL,
            -- 维度③输入资源状态（独立检查与恢复，不随①清零）
            input_old_executor_may_be_present INTEGER,
            input_unconfirmed_release_obligations INTEGER NOT NULL,
            input_blocking_event_refs_json TEXT NOT NULL,
            input_safe_for_new_input INTEGER NOT NULL,
            -- 资源 scope：历史未记录 + 本次"当前可能受影响"的新检查记录
            historical_resource_scope_kind TEXT NOT NULL,
            historical_resource_scope_id TEXT,
            current_resource_basis TEXT NOT NULL,
            current_resource_session_id TEXT NOT NULL,
            current_resource_turn_id TEXT NOT NULL,
            current_resource_checked_at_ms INTEGER NOT NULL,
            -- 副作用限制（三项恒为 0）
            side_effect_limits_json TEXT NOT NULL,
            -- 时间：本次对账时刻；原执行结束时刻只在有证据时才有值
            reconciled_at_ms INTEGER NOT NULL,
            original_execution_ended_at_ms INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_legacy_convergences_run
            ON computer_use_legacy_run_convergences(original_run_id);

        -- 可重入的收敛意图：跨库不可原子，靠"来源对象 + 原状态·revision"对账。
        CREATE TABLE IF NOT EXISTS computer_use_legacy_run_convergence_intents (
            recovery_operation_id TEXT PRIMARY KEY,
            reconciliation_key TEXT NOT NULL UNIQUE,
            source_database_identity TEXT NOT NULL,
            original_run_id TEXT NOT NULL,
            original_state TEXT NOT NULL,
            original_state_version INTEGER NOT NULL,
            recorded_at_ms INTEGER NOT NULL,
            reconciled INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_computer_use_legacy_convergence_intents_run
            ON computer_use_legacy_run_convergence_intents(original_run_id);
        "#,
    )?;
    if current < 23 {
        connection.execute_batch("PRAGMA user_version = 23;")?;
    }
    Ok(())
}

/// 旧 run 行上的收敛控制状态（**读回**口径）。
///
/// 与 `state` 列是两件事：`state` 是 CU 状态机的历史状态（收敛不改写），
/// 本枚举是"该 run 的运行控制状态是否已被遗留收敛终止"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredLegacyRunControlState {
    /// 未收敛：没有收敛事实（控制状态仍是历史状态本身，未由本次恢复终止）。
    NotConverged,
    /// 已由遗留收敛终止继续执行资格（值只可能是 `'interrupted'`）。
    Interrupted,
}

impl StoredLegacyRunControlState {
    /// 该 run 是否仍允许继续规划 / 派发 / 执行。
    ///
    /// 只有 `NotConverged` 才可能"仍允许"——注意这只回答控制维度，
    /// **不回答**输入资源是否安全（那要独立检查）。
    pub(crate) const fn continuation_allowed(self) -> bool {
        matches!(self, Self::NotConverged)
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NotConverged => "not_converged",
            Self::Interrupted => LEGACY_RUN_CONVERGENCE_CONTROL_STATE,
        }
    }
}

/// 从列值解码收敛控制状态：NULL = 未收敛，只认 `'interrupted'`，其它值**报错**
/// （宁可响亮地失败，也不把不认识的终态读成"未收敛"或任何默认值）。
fn legacy_run_control_state_from_column(
    value: Option<String>,
) -> rusqlite::Result<StoredLegacyRunControlState> {
    match value.as_deref() {
        None => Ok(StoredLegacyRunControlState::NotConverged),
        Some(LEGACY_RUN_CONVERGENCE_CONTROL_STATE) => Ok(StoredLegacyRunControlState::Interrupted),
        Some(other) => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(InvalidLegacyConvergenceColumn(format!(
                "不认识的遗留收敛控制状态 `{other}`：不得读成未收敛或任何默认值"
            ))),
        )),
    }
}

/// 一条**尚未收敛**的遗留运行（A-1 的"被处理对象"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyUnconvergedRun {
    pub call_id: String,
    pub session_id: String,
    pub turn_id: String,
    /// 收敛前的历史状态字面量（**原状态**，收敛会原样记进事实）。
    pub state: String,
    /// 收敛前的原 revision。
    pub state_version: u64,
    pub created_at_ms: u64,
}

/// 收敛事实在存储里的投影：`convergence_id` + 解码后的契约事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredLegacyRunConvergence {
    pub convergence_id: i64,
    pub fact: LegacyCuRunConvergenceFact,
}

/// 意图 upsert 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyRunConvergenceIntentUpsert {
    /// 首次登记。
    Inserted,
    /// 同一个对账键上已有**完全相同**的意图：幂等，不追加。
    AlreadyRecorded,
    /// 同一个对账键上已有**不同**的意图：拒绝（不覆盖）。
    Conflicting { existing: LegacyCuRunConvergenceIntent },
}

/// 收敛请求的前置条件不满足：**不得**进入写入（fail-closed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyRunConvergencePrecondition {
    /// A-1.6 步骤①没做：新输入接纳尚未暂停 / 没有记录恢复控制资格。
    NewInputNotPausedOrUnauthorized,
    /// A-1.6 步骤③没做：资源不安全的场景下没有给出"已建立的资源阻断"引用。
    ResourceBlockNotEstablished,
    /// 写入顺序不对（事务内顺序闸门拒绝）：危险顺序在结构上执行不下去。
    WriteOrderViolated {
        attempted: LegacyCuRunConvergenceStep,
    },
    /// 行不存在：没有可收敛的对象（不做"凭空建一条"）。
    UnknownRun,
    /// 该行不是本判据范围内的对象：它的工作区归属**已记录**（本类事实只处理归属未记录的
    /// 迁移前遗留行）。"已收尾"不在这里判——那条走规则的"已有真实终态"拒绝。
    WorkspaceAttributionRecorded,
}

impl LegacyRunConvergencePrecondition {
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::NewInputNotPausedOrUnauthorized => "legacy_convergence_new_input_not_paused",
            Self::ResourceBlockNotEstablished => "legacy_convergence_resource_block_missing",
            Self::WriteOrderViolated { .. } => "legacy_convergence_write_order_violated",
            Self::UnknownRun => "legacy_convergence_unknown_run",
            Self::WorkspaceAttributionRecorded => "legacy_convergence_workspace_recorded",
        }
    }
}

/// 收敛写入的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyRunConvergenceOutcome {
    /// 首次收敛，已在同一事务里写入收敛事实与旧 run 的收敛列。
    Converged {
        convergence_id: i64,
        reconciled_at_ms: u64,
        /// 是否允许就此开放新输入。**只**由维度③的独立检查结果决定
        /// （见 `LegacyCuRunConvergenceFact::reopen_new_input_is_allowed`）。
        may_reopen_new_input: bool,
    },
    /// 同一次收敛已存在：幂等，**没有写第二份**，旧 run 行也未被改写。
    AlreadyConverged { convergence_id: i64 },
    /// 规则拒绝（已有真实终态 / 提交候选 / 冲突 / 原 revision 变了 / 操作 ID 复用）。
    Refused(LegacyCuRunConvergenceRefusal),
    /// 前置条件不满足：没有写任何东西。
    PreconditionUnmet(LegacyRunConvergencePrecondition),
}

/// 一次遗留收敛请求（调用方读到的期望值 + 本次要写入的内容）。
pub(crate) struct LegacyRunConvergenceRequest<'a> {
    /// 来源数据库标识（本次恢复读到的库身份）。
    pub source_database_identity: &'a str,
    /// 该标识是否由本次维护新登记。
    pub source_database_identity_registered_now: bool,
    /// 原 CU run ID。
    pub call_id: &'a str,
    /// 调用方读到的原状态（必须与存储实际读到的一致）。
    pub expected_original_state: &'a str,
    /// 调用方读到的原 revision（同上）。
    pub expected_original_state_version: u64,
    /// 调用方**独立核对**到的"待确认终态提交候选"（例如事实日志里该 run 的待提交事实）。
    pub observed_commit_candidate: bool,
    /// 本次恢复操作 ID。
    pub recovery_operation_id: &'a str,
    /// 实际恢复服务实例。
    pub recovery_service_instance: &'a str,
    /// 本次取得的恢复控制资格。
    pub recovery_control_authority: &'a str,
    /// A-1.6 步骤①：新输入接纳是否**已经**暂停。
    pub new_input_intake_paused: bool,
    /// 本次操作时刻。
    pub operated_at_ms: u64,
    /// 本次对账时刻（写入时刻）。
    pub reconciled_at_ms: u64,
    /// A-1.6 步骤③：已建立的资源共享事故 / 恢复待决阻断引用
    /// （会话库与输入安全库不是同一事务：这里只引用，不宣称跨库原子）。
    pub resource_blocking_refs: &'a [String],
    /// 依据（五组引用）。
    pub evidence: &'a LegacyCuRunConvergenceEvidence,
    /// 维度②历史执行结果（没有证据就 `Unknown`）。
    pub historical_outcome: &'a LegacyRunHistoricalOutcome,
    /// 维度③输入资源状态（独立检查结果）。
    pub input_resource: &'a LegacyRunInputResourceState,
    /// 历史资源 scope 的记录状态（遗留行是 `Unrecorded`）。
    pub historical_resource_scope: &'a LegacyResourceScope,
    /// 本次建立的新安全检查记录。
    pub current_resource_safety_check: &'a CurrentResourceSafetyCheck,
}

/// 事务内**顺序闸门**：把 A-1.6 的顺序做成"顺序不对就写不进去"。
///
/// 每一步都必须在前一步完成之后才允许标记；`require_ready_for_terminal_write` 是
/// 步骤④（写旧 run 非成功终态 + 收敛事实）的唯一入口条件。危险顺序
/// （先改终态 → 旧 SQL 不再命中 → 尚未建立资源阻断 → 新动作开始输入）在这套结构下
/// **执行不下去**：终态写入拿不到放行，而"开放新输入"要求对账已完成。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct LegacyConvergenceOrderGate {
    paused_and_authorized: bool,
    intent_recorded: bool,
    resource_block_established: bool,
    terminal_and_fact_written: bool,
    reconciled: bool,
}

impl LegacyConvergenceOrderGate {
    /// 步骤①：暂停新输入接纳 + 取得恢复协调权。
    fn mark_paused_and_authorized(
        &mut self,
        paused: bool,
        control_authority: &str,
    ) -> Result<(), LegacyRunConvergencePrecondition> {
        if !paused || control_authority.trim().is_empty() {
            return Err(LegacyRunConvergencePrecondition::NewInputNotPausedOrUnauthorized);
        }
        self.paused_and_authorized = true;
        Ok(())
    }

    /// 步骤②：记录可重入的收敛意图（必须先完成步骤①）。
    fn mark_intent_recorded(&mut self) -> Result<(), LegacyRunConvergencePrecondition> {
        if !self.paused_and_authorized {
            return Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted: LegacyCuRunConvergenceStep::RecordReentrantConvergenceIntent,
            });
        }
        self.intent_recorded = true;
        Ok(())
    }

    /// 步骤③：必要时先建立共享资源事故 / 恢复待决阻断（必须先完成步骤②）。
    ///
    /// 资源已可安全接纳新输入时无需阻断；否则必须有已建立的阻断引用。
    fn mark_resource_block_established(
        &mut self,
        blocking_refs: &[String],
        resource_safe_for_new_input: bool,
    ) -> Result<(), LegacyRunConvergencePrecondition> {
        if !self.intent_recorded {
            return Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted:
                    LegacyCuRunConvergenceStep::EstablishSharedResourceIncidentOrRecoveryPendingBlock,
            });
        }
        let has_refs = blocking_refs
            .iter()
            .any(|reference| !reference.trim().is_empty());
        if !resource_safe_for_new_input && !has_refs {
            return Err(LegacyRunConvergencePrecondition::ResourceBlockNotEstablished);
        }
        self.resource_block_established = true;
        Ok(())
    }

    /// 步骤④的放行条件：步骤①②③都已完成。
    ///
    /// **这是危险顺序在结构上被挡住的点**：没有资源阻断（步骤③）就写不了旧 run 的终态。
    fn require_ready_for_terminal_write(&self) -> Result<(), LegacyRunConvergencePrecondition> {
        if !self.paused_and_authorized || !self.intent_recorded || !self.resource_block_established {
            return Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted: LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
            });
        }
        Ok(())
    }

    /// 步骤④完成。
    fn mark_terminal_and_fact_written(&mut self) {
        self.terminal_and_fact_written = true;
    }

    /// 步骤⑤：完成意图对账。
    fn mark_reconciled(&mut self) -> Result<(), LegacyRunConvergencePrecondition> {
        if !self.terminal_and_fact_written {
            return Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted: LegacyCuRunConvergenceStep::ReconcileConvergenceIntent,
            });
        }
        self.reconciled = true;
        Ok(())
    }

    /// 步骤⑥：**仅在独立安全条件满足后**开放新输入。
    ///
    /// 条件里**没有**"运行已终止"这一项——控制终态不构成资源安全的理由。
    fn may_reopen_new_input(&self, resource_safe_for_new_input: bool) -> bool {
        self.reconciled && resource_safe_for_new_input
    }

    /// 本闸门走过的步骤序列（按完成顺序），用于与契约的规范顺序核对。
    fn steps_taken(&self) -> Vec<LegacyCuRunConvergenceStep> {
        let mut steps = Vec::new();
        if self.paused_and_authorized {
            steps.push(LegacyCuRunConvergenceStep::PauseNewInputIntakeAndTakeRecoveryAuthority);
        }
        if self.intent_recorded {
            steps.push(LegacyCuRunConvergenceStep::RecordReentrantConvergenceIntent);
        }
        if self.resource_block_established {
            steps.push(
                LegacyCuRunConvergenceStep::EstablishSharedResourceIncidentOrRecoveryPendingBlock,
            );
        }
        if self.terminal_and_fact_written {
            steps.push(
                LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
            );
        }
        if self.reconciled {
            steps.push(LegacyCuRunConvergenceStep::ReconcileConvergenceIntent);
        }
        steps
    }
}

/// 收敛事实的行编码列清单（**写侧与读侧只在这一处对齐**，避免两条 SQL 各自漂移）。
const LEGACY_CONVERGENCE_SELECT_SQL: &str = "SELECT convergence_id, source_database_identity, \
     source_database_identity_registered_now, source_database_identity_not_past_attribution, \
     original_run_id, original_state, original_state_version, session_id, turn_id, \
     workspace_id_unrecorded, other_gaps_json, terminal_status, reason_code, explanation, \
     rule_version, recovery_operation_id, evidence_json, recovery_service_instance, \
     recovery_control_authority, operated_at_ms, control_continuation_allowed, \
     historical_outcome_kind, historical_goal_verdict, historical_execution_ended_at_ms, \
     historical_evidence_refs_json, input_old_executor_may_be_present, \
     input_unconfirmed_release_obligations, input_blocking_event_refs_json, \
     input_safe_for_new_input, historical_resource_scope_kind, historical_resource_scope_id, \
     current_resource_basis, current_resource_session_id, current_resource_turn_id, \
     current_resource_checked_at_ms, side_effect_limits_json, reconciled_at_ms, \
     original_execution_ended_at_ms \
     FROM computer_use_legacy_run_convergences";

/// 由"来源对象 + 原始信息 + 本次请求"构造收敛事实。
///
/// 决定**只有一种**：`LegacyCuRunConvergenceDecision::interrupted`（唯一原因码 + 唯一解释
/// 文案 + 契约的规则版本）。缺失维度里的"其它真实缺口"由**已记录的事实**推导：只列真的
/// 缺了的东西（历史资源 scope 未记录、旧执行者是否存在未知、原执行结束时刻未知），
/// 不凭空列，也不因为"没列"而暗示"没有缺口"。
fn legacy_convergence_fact_from_request(
    subject: LegacyCuRunSubject,
    original: LegacyCuRunOriginalFacts,
    request: &LegacyRunConvergenceRequest<'_>,
) -> LegacyCuRunConvergenceFact {
    let mut other_gaps = Vec::new();
    if request.historical_resource_scope.is_unrecorded() {
        other_gaps.push("historical_input_resource_scope_unrecorded".to_string());
    }
    if request.input_resource.old_executor_may_be_present.is_none() {
        other_gaps.push("old_executor_presence_unknown".to_string());
    }
    if request.historical_outcome.is_unknown() {
        other_gaps.push("original_execution_ended_at_unknown".to_string());
    }
    LegacyCuRunConvergenceFact {
        subject,
        original,
        missing: LegacyCuRunMissingDimensions {
            workspace_id_unrecorded: true,
            other_gaps,
        },
        decision: runtime::LegacyCuRunConvergenceDecision::interrupted(
            request.recovery_operation_id,
        ),
        evidence: request.evidence.clone(),
        operator: LegacyCuRunRecoveryOperator {
            recovery_service_instance: request.recovery_service_instance.to_string(),
            recovery_control_authority: request.recovery_control_authority.to_string(),
            operated_at_unix_ms: request.operated_at_ms,
        },
        control: LegacyRunControlState::continuation_terminated_by_recovery(),
        historical_outcome: request.historical_outcome.clone(),
        input_resource: request.input_resource.clone(),
        historical_resource_scope: request.historical_resource_scope.clone(),
        current_resource_safety_check: request.current_resource_safety_check.clone(),
        side_effects: LegacyCuRunSideEffectLimits::none(),
        reconciled_at_unix_ms: request.reconciled_at_ms,
    }
}

/// 写入一条收敛事实，返回 `convergence_id`。
///
/// 唯一索引 `reconciliation_key` 是幂等的持久化依据：同一次收敛的第二份插入会**报错**并让
/// 整个事务回滚，而不是静默写重（幂等由调用方在插入前用规则判定）。
fn insert_legacy_convergence_fact_on(
    transaction: &rusqlite::Transaction<'_>,
    fact: &LegacyCuRunConvergenceFact,
) -> rusqlite::Result<i64> {
    let evidence_json = serde_json::to_string(&fact.evidence)
        .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?;
    let side_effect_limits_json = serde_json::to_string(&fact.side_effects)
        .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?;
    let (outcome_kind, goal_verdict, ended_at, evidence_refs) = match &fact.historical_outcome {
        LegacyRunHistoricalOutcome::Unknown => ("unknown", None, None, Vec::new()),
        LegacyRunHistoricalOutcome::Evidenced {
            evidence_refs,
            goal_verdict,
            execution_ended_at_unix_ms,
        } => (
            "evidenced",
            Some(goal_verdict_name(*goal_verdict)),
            *execution_ended_at_unix_ms,
            evidence_refs.clone(),
        ),
    };
    let (scope_kind, scope_id) = match &fact.historical_resource_scope {
        LegacyResourceScope::Unrecorded => ("unrecorded", None),
        LegacyResourceScope::Recorded { scope_id } => ("recorded", Some(scope_id.clone())),
    };
    transaction.execute(
        "INSERT INTO computer_use_legacy_run_convergences(
            reconciliation_key, source_database_identity,
            source_database_identity_registered_now,
            source_database_identity_not_past_attribution, original_run_id,
            original_state, original_state_version, session_id, turn_id,
            workspace_id_unrecorded, other_gaps_json,
            terminal_status, reason_code, explanation, rule_version, recovery_operation_id,
            evidence_json,
            recovery_service_instance, recovery_control_authority, operated_at_ms,
            control_continuation_allowed,
            historical_outcome_kind, historical_goal_verdict, historical_execution_ended_at_ms,
            historical_evidence_refs_json,
            input_old_executor_may_be_present, input_unconfirmed_release_obligations,
            input_blocking_event_refs_json, input_safe_for_new_input,
            historical_resource_scope_kind, historical_resource_scope_id,
            current_resource_basis, current_resource_session_id, current_resource_turn_id,
            current_resource_checked_at_ms,
            side_effect_limits_json, reconciled_at_ms, original_execution_ended_at_ms
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
                  ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38)",
        params![
            legacy_convergence_reconciliation_key_literal(&fact.reconciliation_key()),
            fact.subject.source_database_identity,
            fact.subject.source_database_identity_registered_now,
            fact.subject.source_database_identity_not_past_attribution,
            fact.subject.original_run_id,
            fact.original.original_state,
            fact.original.original_state_version,
            fact.original.session_id,
            fact.original.turn_id,
            fact.missing.workspace_id_unrecorded,
            json_string_list(&fact.missing.other_gaps),
            fact.decision.terminal_status.as_str(),
            fact.decision.reason_code,
            fact.decision.explanation,
            fact.decision.rule_version,
            fact.decision.recovery_operation_id,
            evidence_json,
            fact.operator.recovery_service_instance,
            fact.operator.recovery_control_authority,
            fact.operator.operated_at_unix_ms,
            fact.control.continuation_allowed,
            outcome_kind,
            goal_verdict,
            ended_at,
            json_string_list(&evidence_refs),
            fact.input_resource.old_executor_may_be_present,
            fact.input_resource.unconfirmed_release_obligations,
            json_string_list(&fact.input_resource.blocking_event_refs),
            fact.input_resource.safe_for_new_input,
            scope_kind,
            scope_id,
            "current_may_be_affected_not_historical_scope",
            fact.current_resource_safety_check.session_id,
            fact.current_resource_safety_check.turn_id,
            fact.current_resource_safety_check.checked_at_unix_ms,
            side_effect_limits_json,
            fact.reconciled_at_unix_ms,
            fact.original_execution_ended_at_unix_ms(),
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

/// 读回一条收敛事实（列 -> 契约事实）。
///
/// 读回**不做任何推断**：缺失就是缺失（`NULL` -> 缺省），且解码后必须通过
/// `LegacyCuRunConvergenceFact::validate()`——被外部改写成"成功"的收敛事实会在读回时报错，
/// 而不是被当成正常记录喂给消费者。
fn stored_legacy_convergence_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredLegacyRunConvergence> {
    let terminal_status: String = row.get(11)?;
    let historical_outcome_kind: String = row.get(21)?;
    let historical_outcome = match historical_outcome_kind.as_str() {
        "unknown" => LegacyRunHistoricalOutcome::Unknown,
        "evidenced" => LegacyRunHistoricalOutcome::Evidenced {
            evidence_refs: parse_string_list(&row.get::<_, String>(24)?)?,
            goal_verdict: goal_verdict_from_name(row.get(22)?)?.ok_or_else(|| {
                invalid_legacy_convergence_column(
                    "有证据的历史结论缺目标结论：不得读成任何默认值".to_string(),
                )
            })?,
            execution_ended_at_unix_ms: row.get(23)?,
        },
        other => {
            return Err(invalid_legacy_convergence_column(format!(
                "不认识的历史执行结果种类 `{other}`：不得读成未知或任何默认结论"
            )));
        }
    };
    let historical_resource_scope = match row.get::<_, String>(29)?.as_str() {
        "unrecorded" => LegacyResourceScope::Unrecorded,
        "recorded" => LegacyResourceScope::Recorded {
            scope_id: row.get::<_, Option<String>>(30)?.ok_or_else(|| {
                invalid_legacy_convergence_column(
                    "历史资源 scope 声明已记录却没有标识：不得用它当 scope".to_string(),
                )
            })?,
        },
        other => {
            return Err(invalid_legacy_convergence_column(format!(
                "不认识的历史资源 scope 种类 `{other}`"
            )));
        }
    };
    let side_effects: LegacyCuRunSideEffectLimits =
        serde_json::from_str(&row.get::<_, String>(35)?)
            .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?;
    let fact = LegacyCuRunConvergenceFact {
        subject: LegacyCuRunSubject {
            source_database_identity: row.get(1)?,
            source_database_identity_registered_now: row.get(2)?,
            original_run_id: row.get(4)?,
            source_database_identity_not_past_attribution: row.get(3)?,
        },
        original: LegacyCuRunOriginalFacts {
            original_state: row.get(5)?,
            original_state_version: row.get(6)?,
            session_id: row.get(7)?,
            turn_id: row.get(8)?,
        },
        missing: LegacyCuRunMissingDimensions {
            workspace_id_unrecorded: row.get(9)?,
            other_gaps: parse_string_list(&row.get::<_, String>(10)?)?,
        },
        decision: runtime::LegacyCuRunConvergenceDecision {
            terminal_status: runtime::RunTerminalStatus::parse(&terminal_status).ok_or_else(|| {
                invalid_legacy_convergence_column(format!(
                    "不认识的运行终态 `{terminal_status}`：不得读成成功或任何默认值"
                ))
            })?,
            reason_code: row.get(12)?,
            explanation: row.get(13)?,
            rule_version: row.get(14)?,
            recovery_operation_id: row.get(15)?,
        },
        evidence: serde_json::from_str(&row.get::<_, String>(16)?)
            .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?,
        operator: LegacyCuRunRecoveryOperator {
            recovery_service_instance: row.get(17)?,
            recovery_control_authority: row.get(18)?,
            operated_at_unix_ms: row.get(19)?,
        },
        control: LegacyRunControlState {
            continuation_allowed: row.get(20)?,
        },
        historical_outcome,
        input_resource: LegacyRunInputResourceState {
            old_executor_may_be_present: row.get(25)?,
            unconfirmed_release_obligations: row.get(26)?,
            blocking_event_refs: parse_string_list(&row.get::<_, String>(27)?)?,
            safe_for_new_input: row.get(28)?,
        },
        historical_resource_scope,
        current_resource_safety_check: CurrentResourceSafetyCheck {
            basis: runtime::CurrentResourceCandidateBasis::CurrentMayBeAffectedNotHistoricalScope,
            session_id: row.get(32)?,
            turn_id: row.get(33)?,
            checked_at_unix_ms: row.get(34)?,
        },
        side_effects,
        reconciled_at_unix_ms: row.get(36)?,
    };
    // 读回校验：被改写过的收敛事实必须在读回时响亮失败（不得静默当成正常记录）。
    fact.validate()
        .map_err(|error| invalid_legacy_convergence_column(format!(
            "收敛事实读回校验失败（{}）：{}",
            error.code, error.message
        )))?;
    // 行里的 `original_execution_ended_at_ms` 是冗余列，必须与事实一致（防止两处漂移）。
    let column_ended_at: Option<u64> = row.get(37)?;
    if column_ended_at != fact.original_execution_ended_at_unix_ms() {
        return Err(invalid_legacy_convergence_column(
            "收敛事实的原执行结束时刻列与事实内容不一致".to_string(),
        ));
    }
    Ok(StoredLegacyRunConvergence {
        convergence_id: row.get(0)?,
        fact,
    })
}

/// 收敛列 / 事实的存储值不合法（内部错误类型：`store` 的错误出口是 `rusqlite::Result`）。
#[derive(Debug)]
struct InvalidLegacyConvergenceColumn(String);

impl std::fmt::Display for InvalidLegacyConvergenceColumn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for InvalidLegacyConvergenceColumn {}

fn invalid_legacy_convergence_column(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(InvalidLegacyConvergenceColumn(message)),
    )
}

/// 对账键的存储字面量：**来源对象 + 原状态·revision**（不是时间、不是随机 ID）。
///
/// 用 `\u{1f}` 分隔：它是控制字符，不可能出现在工作区/状态/run id 里，因此不会有两组
/// 不同的字段拼出同一个键（`original_state` 自带分隔符的情况也被排除）。
fn legacy_convergence_reconciliation_key_literal(key: &LegacyCuRunConvergenceKey) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        key.source_database_identity, key.original_run_id, key.original_state, key.original_state_version
    )
}

fn json_string_list(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

fn parse_string_list(value: &str) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(value).map_err(|error| invalid_legacy_convergence_column(error.to_string()))
}

fn goal_verdict_from_name(value: Option<String>) -> rusqlite::Result<Option<GoalVerdict>> {
    match value.as_deref() {
        None => Ok(None),
        Some("not_checked") => Ok(Some(GoalVerdict::NotChecked)),
        Some("passed") => Ok(Some(GoalVerdict::Passed)),
        Some("failed") => Ok(Some(GoalVerdict::Failed)),
        Some("inconclusive") => Ok(Some(GoalVerdict::Inconclusive)),
        Some(other) => Err(invalid_legacy_convergence_column(format!(
            "不认识的历史目标结论 `{other}`：不得读成任何默认结论"
        ))),
    }
}

/// 接纳时**冻结**的 CU 工作区归属（不可变）。///
/// 语义：
/// 1. 值只来自**已授权父运行**在接纳时使用的规范工作区标识（工具调度层传入，见
///    `execute_with_current_runtime` 的 `workspace_id` 参数），**不是**执行时再读
///    "当前工作区"；
/// 2. 一旦构造完成，运行期间不再重新解析：运行中切换 UI 工作区不会改变本运行的归属；
/// 3. 禁止值（数据库路径、当前目录、窗口名、模型提供字符串、临时占位值）在构造处即被拒绝，
///    不做"近似归一"、不补默认值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CuWorkspaceAttribution {
    workspace_id: String,
    context_version: i64,
}

impl CuWorkspaceAttribution {
    /// 从**已由源头解析器接受**的工作区标识构造归属（A-2 生效执行口径 §2.2/§10.4 决定一）。
    ///
    /// 参数类型是 `CanonicalWorkspaceId` 而不是 `&str`：本层因此**在类型上**不可能收到
    /// 未解析值（模型提供字符串、窗口名、路径都在源头解析器处就被拒绝），
    /// 所以这里**没有也不得有**形状/占位值/路径形状的第二份判定——那些规则只此一份，在源头。
    pub(crate) fn from_parent_run(workspace_id: &crate::CanonicalWorkspaceId) -> Self {
        Self {
            workspace_id: workspace_id.as_str().to_string(),
            context_version: CU_WORKSPACE_CONTEXT_VERSION,
        }
    }

    pub(crate) fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub(crate) fn context_version(&self) -> i64 {
        self.context_version
    }

    /// **仅供测试替身**：显式固定的测试归属，走与生产完全相同的写入路径。
    ///
    /// 它只存在于 `#[cfg(test)]`，生产代码无法取得未归属的执行器（见
    /// `ComputerUseExecutor::new_for_test`）。
    #[cfg(test)]
    pub(crate) fn test_fixture() -> Self {
        Self {
            workspace_id: "ws-00000000000000ff".to_string(),
            context_version: CU_WORKSPACE_CONTEXT_VERSION,
        }
    }
}

// A-2：这里原先有 CU 自己的 `MAX_WORKSPACE_ID_CHARS` / `is_canonical_workspace_identity` /
// `InvalidWorkspaceAttribution`（六类拒绝码）。它们**整体搬到源头** `main.rs` 的
// `canonical_workspace_identity` + `InvalidWorkspaceIdentity`（错误码逐字保留），
// CU 侧不再持有第二份格式规则——这正是 A-2 要删除的"复制来的硬编码形状"。


/// 已落库的工作区归属的**读回**视图。
///
/// `Unrecorded` 是给迁移之前的历史运行用的：那时这一列还不存在。它表示"归属未记录"，
/// **不等于**任何工作区——尤其不得被解读为"当前工作区"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredWorkspaceAttribution {
    Recorded {
        workspace_id: String,
        context_version: Option<i64>,
    },
    Unrecorded,
}

// 读回/呈现面目前只有测试在用：真要把"历史归属未记录"显示到界面，需要 CU 运行的对外读取
// 入口（列表/诊断 API）来接——那不在本工单的允许改动范围内。这里显式放行 dead_code，
// 而不是把这层删掉：裁决 §5.1 要求读回时**呈现**未记录，而不是"读不到就算了"。
#[allow(dead_code)]
impl StoredWorkspaceAttribution {
    /// 历史行的呈现文本（读回方直接使用，不要各自另造说法）。
    pub(crate) const UNRECORDED_TEXT: &'static str = "历史归属未记录";

    pub(crate) fn from_columns(workspace_id: Option<String>, context_version: Option<i64>) -> Self {
        match workspace_id {
            Some(workspace_id) => Self::Recorded {
                workspace_id,
                context_version,
            },
            None => Self::Unrecorded,
        }
    }

    pub(crate) fn workspace_id(&self) -> Option<&str> {
        match self {
            Self::Recorded { workspace_id, .. } => Some(workspace_id),
            Self::Unrecorded => None,
        }
    }

    pub(crate) fn context_version(&self) -> Option<i64> {
        match self {
            Self::Recorded {
                context_version, ..
            } => *context_version,
            Self::Unrecorded => None,
        }
    }

    /// 呈现文本：已记录则给出标识（版本缺失时如实说明），未记录则给出历史口径。
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Recorded {
                workspace_id,
                context_version: Some(version),
            } => format!("{workspace_id}（上下文版本 {version}）"),
            Self::Recorded {
                workspace_id,
                context_version: None,
            } => format!("{workspace_id}（上下文版本未记录）"),
            Self::Unrecorded => Self::UNRECORDED_TEXT.to_string(),
        }
    }
}

/// 未确认释放的现场事实：`has_unconfirmed_release` 的证据载体。
///
/// 判据（列名与取值以本文件的实际写入为准）：
/// - `computer_use_steps.input_release_status = 'unknown'`
///   （`InputReleaseStatus::Unknown` 经 `input_release_status_name` 落盘的字面量）；
/// - 且 `computer_use_steps.input_delivery` 不是 `'not_sent'`
///   （NULL 也算"非 not_sent"：读不到投递事实绝不放行，判据只对**可证明零输入**放行）；
/// - 且该 step 所属 run 未被任何本 scope 的解除记录覆盖；
/// - 归属由 `computer_use_runs.session_id / turn_id` 决定。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UnconfirmedReleaseFacts {
    pub session_id: String,
    pub turn_id: String,
    /// 仍未被人工解除的 run（去重、升序）——"未解除的未确认释放"。
    pub run_ids: Vec<String>,
    /// 命中的 step 行数（同一个 run 可能有多个）。
    pub step_count: usize,
    pub oldest_step_started_at_ms: Option<u64>,
    pub newest_step_started_at_ms: Option<u64>,
}

impl UnconfirmedReleaseFacts {
    pub(crate) fn is_empty(&self) -> bool {
        self.run_ids.is_empty()
    }
}

/// 人工解除的操作者来源。只认显式来源，避免"系统自动解除"混进审计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseResolutionOperator {
    /// 原生 UI 入口（RPR-05c 的 Tauri 侧）。
    NativeUi,
    /// 命令行 / 运维脚本。
    Cli,
    /// 测试。
    Test,
}

impl ReleaseResolutionOperator {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NativeUi => "native_ui",
            Self::Cli => "cli",
            Self::Test => "test",
        }
    }
}

/// 一次人工解除的输入。`store` 只落库，不做任何"自动判定已安全"的推断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseResolutionRequest<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub operator_source: ReleaseResolutionOperator,
    /// 操作者标识（Tauri 窗口身份、运维账号等）；读不到就 None，不得填猜测值。
    pub operator_id: Option<&'a str>,
    /// 必填：为什么可以判定残留输入已被处理。
    pub reason: &'a str,
    /// 操作者现场检查说明（例如"已在桌面确认鼠标左键已抬起"）。
    pub operator_check_note: &'a str,
    /// 前置检查：解除前读到的桌面输入所有者 epoch；读不到就 None。
    pub input_owner_epoch_before: Option<u64>,
    /// 后置检查：解除后重新读到的桌面输入所有者 epoch。
    pub input_owner_epoch_after: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseResolutionOutcome {
    /// 是否真的落库了一条解除事实。本 scope 没有可解除事实时是幂等空操作（false）。
    pub recorded: bool,
    pub resolution_id: Option<i64>,
    /// 旧 epoch：本 scope 上一条解除的 epoch（从未解除过为 0）。
    pub previous_epoch: u64,
    /// 新 epoch：本次解除的 epoch（未落库时等于 `previous_epoch`）。
    pub epoch: u64,
    pub covered_run_ids: Vec<String>,
    pub covered_step_count: usize,
    /// 后置检查：落库后本 scope 仍未确认释放的 run 数（正常必须为 0）。
    pub remaining_unconfirmed_runs: usize,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredReleaseResolution {
    pub resolution_id: i64,
    pub session_id: String,
    pub turn_id: String,
    pub epoch: u64,
    pub previous_epoch: u64,
    pub operator_source: String,
    pub operator_id: Option<String>,
    pub reason: String,
    pub operator_check_note: Option<String>,
    pub input_owner_epoch_before: Option<u64>,
    pub input_owner_epoch_after: Option<u64>,
    pub covered_run_ids: Vec<String>,
    pub covered_step_count: usize,
    pub covered_run_count: usize,
    pub precheck_json: String,
    pub postcheck_json: String,
    pub created_at_ms: u64,
}

/// 未确认释放的判据 SQL：`?1`=session_id，`?2`=turn_id，
/// `?3`=InputReleaseStatus::Unknown 的字面量，`?4`=InputDelivery::NotSent 的字面量。
///
/// 两个取值都从命名函数传入，避免 SQL 里出现与源码脱节的魔法字符串。
const UNCONFIRMED_RELEASE_SQL: &str = "
    SELECT s.run_id, s.started_at_ms
    FROM computer_use_steps AS s
    JOIN computer_use_runs AS r ON r.call_id = s.run_id
    WHERE r.session_id = ?1 AND r.turn_id = ?2
      AND s.input_release_status = ?3
      AND (s.input_delivery IS NULL OR s.input_delivery <> ?4)
      AND NOT EXISTS (
          SELECT 1
          FROM computer_use_release_resolution_runs AS rr
          JOIN computer_use_release_resolutions AS res
            ON res.resolution_id = rr.resolution_id
          WHERE rr.run_id = s.run_id
            AND res.session_id = ?1 AND res.turn_id = ?2
      )
    ORDER BY s.run_id, s.step_index
";

fn unconfirmed_release_facts_on(
    connection: &Connection,
    session_id: &str,
    turn_id: &str,
) -> rusqlite::Result<UnconfirmedReleaseFacts> {
    let mut statement = connection.prepare(UNCONFIRMED_RELEASE_SQL)?;
    let rows = statement.query_map(
        params![
            session_id,
            turn_id,
            input_release_status_name(InputReleaseStatus::Unknown),
            input_delivery_name(InputDelivery::NotSent),
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
    )?;
    let mut facts = UnconfirmedReleaseFacts {
        session_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
        ..UnconfirmedReleaseFacts::default()
    };
    for row in rows {
        let (run_id, started_at_ms) = row?;
        if facts.run_ids.last() != Some(&run_id) {
            facts.run_ids.push(run_id);
        }
        facts.step_count += 1;
        facts.oldest_step_started_at_ms = Some(
            facts
                .oldest_step_started_at_ms
                .map_or(started_at_ms, |value| value.min(started_at_ms)),
        );
        facts.newest_step_started_at_ms = Some(
            facts
                .newest_step_started_at_ms
                .map_or(started_at_ms, |value| value.max(started_at_ms)),
        );
    }
    Ok(facts)
}

fn scoped_epoch_on(
    connection: &Connection,
    session_id: &str,
    turn_id: &str,
) -> rusqlite::Result<u64> {
    connection.query_row(
        "SELECT COALESCE(MAX(epoch),0) FROM computer_use_release_resolutions
         WHERE session_id = ?1 AND turn_id = ?2",
        params![session_id, turn_id],
        |row| row.get(0),
    )
}

fn covered_run_ids_on(
    connection: &Connection,
    resolution_id: i64,
) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT run_id FROM computer_use_release_resolution_runs
         WHERE resolution_id = ?1 ORDER BY run_id",
    )?;
    let run_ids = statement
        .query_map([resolution_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(run_ids)
}

/// 解除请求本身不合法（lib 内错误类型：`store` 的错误出口是 `rusqlite::Result`）。
#[derive(Debug)]
struct InvalidResolutionRequest(&'static str);

impl std::fmt::Display for InvalidResolutionRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for InvalidResolutionRequest {}

fn invalid_resolution_request(message: &'static str) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(InvalidResolutionRequest(message)))
}

/// v12 只追加可空事实列；已有审计行没有原始回执，因此必须保留 NULL。
fn ensure_computer_use_steps_column(
    connection: &Connection,
    definition: &str,
) -> rusqlite::Result<()> {
    let column = definition
        .split_whitespace()
        .next()
        .expect("静态迁移定义必须含列名");
    let mut statement = connection.prepare("PRAGMA table_info(computer_use_steps)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|existing| existing == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE computer_use_steps ADD COLUMN {definition}"
        ))?;
    }
    Ok(())
}

const fn input_delivery_name(value: InputDelivery) -> &'static str {
    match value {
        InputDelivery::NotSent => "not_sent",
        InputDelivery::MayHaveBeenSent => "may_have_been_sent",
        InputDelivery::Sent => "sent",
    }
}

const fn effect_status_name(value: EffectStatus) -> &'static str {
    match value {
        EffectStatus::NotObserved => "not_observed",
        EffectStatus::EffectObserved => "effect_observed",
        EffectStatus::NoEffectObserved => "no_effect_observed",
        EffectStatus::Inconclusive => "inconclusive",
    }
}

const fn goal_verdict_name(value: GoalVerdict) -> &'static str {
    match value {
        GoalVerdict::NotChecked => "not_checked",
        GoalVerdict::Passed => "passed",
        GoalVerdict::Failed => "failed",
        GoalVerdict::Inconclusive => "inconclusive",
    }
}

const fn input_release_status_name(value: InputReleaseStatus) -> &'static str {
    match value {
        InputReleaseStatus::NotNeeded => "not_needed",
        InputReleaseStatus::Released => "released",
        InputReleaseStatus::Unknown => "unknown",
    }
}

/// **仅测试接缝**：造一条迁移之前那种"归属未记录"的运行行（`workspace_id IS NULL`）。
///
/// 写入侧**不可能**产生这种行（`NewComputerUseRun::workspace` 非 `Option`），因此历史行的
/// 读回口径与"旧活动运行"互锁只能靠这个接缝覆盖。它只插入，生产代码没有任何调用路径。
#[cfg(test)]
pub(crate) fn seed_legacy_unrecorded_run_for_test(
    store: &ComputerUseRunStore,
    call_id: &str,
    session_id: &str,
    turn_id: &str,
    open: bool,
) {
    let state = if open { "executing" } else { "blocked" };
    let terminal_json = (!open).then_some("{\"status\":\"blocked\"}");
    let connection = store.connection.lock().expect("computer-use store lock");
    connection
        .execute(
            "INSERT INTO computer_use_runs(
                 call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
                 idempotency_key, objective_json, surface, state, state_version,
                 deadline_ms, created_at_ms, updated_at_ms, terminal_result_json
             ) VALUES (?1, 'toolu-legacy', ?2, ?3, 'room-1', ?4,
                 '{\"objective\":\"legacy\"}', 'desktop', ?5, 1, 60_000, 1, 1, ?6)",
            params![
                call_id,
                turn_id,
                session_id,
                format!("legacy-key-{call_id}"),
                state,
                terminal_json
            ],
        )
        .expect("legacy run row");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewComputerUseRun {
    pub call_id: String,
    pub provider_tool_call_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    pub chat_room_id: Option<String>,
    pub idempotency_key: String,
    pub objective_json: String,
    pub surface: ComputerUseSurface,
    pub deadline_ms: u64,
    pub created_at_ms: u64,
    /// 接纳时冻结的工作区归属。**非 `Option`**：新运行在类型层面就不允许"没有归属"——
    /// 历史行的 NULL 只出现在**读回**侧（[`StoredWorkspaceAttribution::Unrecorded`]）。
    pub workspace: CuWorkspaceAttribution,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredComputerUseRun {
    pub call_id: String,
    pub provider_tool_call_id: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    pub chat_room_id: Option<String>,
    pub state: ComputerUseRunState,
    pub state_version: u64,
    pub surface: ComputerUseSurface,
    pub terminal_result: Option<ComputerUseResult>,
    pub updated_at_ms: u64,
    /// 该运行落库时的归属；迁移之前的历史行为 `Unrecorded`。
    pub workspace: StoredWorkspaceAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComputerUseStepRecord {
    pub run_id: String,
    pub step_index: usize,
    pub observation_generation: u64,
    pub action_type: String,
    pub normalized_target: String,
    pub action_fingerprint: String,
    pub status: String,
    pub error_code: Option<String>,
    pub before_evidence_ref: Option<String>,
    pub after_evidence_ref: Option<String>,
    pub visible_progress: bool,
    pub input_delivery: Option<InputDelivery>,
    pub partial: Option<bool>,
    pub path_completed: Option<bool>,
    pub confirmed_point_count: Option<u32>,
    pub effect_status: Option<EffectStatus>,
    pub goal_verdict: Option<GoalVerdict>,
    pub input_release_status: Option<InputReleaseStatus>,
    pub started_at_ms: u64,
    pub completed_at_ms: Option<u64>,
}

pub(crate) struct PlannerDiagnostic<'a> {
    pub call_id: &'a str,
    pub turn_id: &'a str,
    pub room_id: Option<&'a str>,
    pub session_id: &'a str,
    pub request_kind: &'a str,
    pub observation_generation: u64,
    pub model: &'a str,
    pub provider_response_id: Option<&'a str>,
    pub response_json: &'a str,
    pub error_code: Option<&'a str>,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
}

/// 诊断保留动作结构和几何，不记录输入正文、URL、未知字符串或模型思考。
pub(crate) fn sanitized_action_json(raw: &str) -> String {
    use serde_json::{json, Value};
    fn clean(value: &Value, key: &str) -> Value {
        match value {
            Value::Object(object) => {
                let allowed = [
                    "done",
                    "action",
                    "kind",
                    "target",
                    "arguments",
                    "points",
                    "duration_ms",
                    "x",
                    "y",
                    "button",
                    "keys",
                    "direction",
                    "amount",
                    "drop_target",
                    "value",
                    "checked",
                    "activate",
                    "tab_id",
                ];
                let mut result = serde_json::Map::new();
                for (name, value) in object {
                    if allowed.contains(&name.as_str()) {
                        result.insert(name.clone(), clean(value, name));
                    } else {
                        result.insert("redacted_fields".into(), Value::Bool(true));
                    }
                }
                Value::Object(result)
            }
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .take(256)
                    .map(|value| clean(value, key))
                    .collect(),
            ),
            Value::String(text) => {
                let safe = match key {
                    "kind" | "action" => [
                        "click",
                        "double_click",
                        "text_input",
                        "scroll",
                        "key_combination",
                        "drag",
                        "slider_drag",
                        "navigate",
                        "select",
                        "check",
                        "submit",
                        "history_back",
                        "history_forward",
                        "open_tab",
                        "close_tab",
                        "activate_tab",
                    ]
                    .contains(&text.as_str()),
                    "target" | "drop_target" => {
                        text.len() <= 128
                            && (((text.starts_with("uia-")
                                || text.starts_with("dom-")
                                || text == "browser-tabs")
                                && text
                                    .chars()
                                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
                                || text.strip_prefix("window-canvas:").is_some_and(|handle| {
                                    !handle.is_empty()
                                        && handle.chars().all(|ch| ch.is_ascii_hexdigit())
                                }))
                    }
                    "direction" => ["up", "down", "left", "right"].contains(&text.as_str()),
                    "button" => ["left", "right"].contains(&text.as_str()),
                    "keys" => [
                        "ctrl", "shift", "alt", "enter", "escape", "tab", "home", "end", "a", "c",
                        "l", "v", "x", "y", "z",
                    ]
                    .contains(&text.as_str()),
                    "tab_id" => text.len() <= 32 && text.chars().all(|ch| ch.is_ascii_digit()),
                    _ => false,
                };
                if safe {
                    value.clone()
                } else {
                    Value::String("[redacted]".into())
                }
            }
            _ => value.clone(),
        }
    }
    if raw.len() > 16 * 1024 {
        return json!({"omitted":"response_too_large","bytes":raw.len()}).to_string();
    }
    let result = serde_json::from_str::<Value>(raw)
        .map(|value| clean(&value, ""))
        .unwrap_or_else(|_| json!({"omitted":"invalid_json","bytes":raw.len()}))
        .to_string();
    if result.len() > 16 * 1024 {
        json!({"omitted":"sanitized_response_too_large","bytes":raw.len()}).to_string()
    } else {
        result
    }
}

pub(crate) struct ComputerUseRunStore {
    connection: Mutex<Connection>,
}

impl ComputerUseRunStore {
    pub(crate) fn from_connection(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }

    pub(crate) fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
        )?;
        // 与主入口共用的前置规则（超前版本拒绝），必须在任何补列/建表之前。
        ensure_session_schema_not_from_the_future(&connection)?;
        apply_session_migration_v11(&connection)?;
        // 与 v11 同例：本文件拥有的表由本文件补齐归属列，独立打开 store（不经 main.rs 阶梯）
        // 时也必须可用，否则接纳处会因为"没有那一列"而写不进真实归属。
        // 注意：这三步与 main.rs 阶梯调用的是**同一批函数**（不是另写一份），前置规则也共用
        // `ensure_session_schema_not_from_the_future`，避免两套阶梯逐渐漂移。
        apply_session_migration_v22(&connection)?;
        // 同例：收敛列/表也由本文件拥有，独立打开 store（不经 main.rs 阶梯）时也必须可用；
        // 否则收敛写入会以 `no such column` fail-closed（登记前后行为一致，不留半套事实）。
        apply_session_migration_v23_legacy_run_convergence(&connection)?;
        Ok(Self::from_connection(connection))
    }

    /// 写入一次接纳事实。归属列**必然**随行写入（类型层面不可缺省）；
    /// 返回 `false` 表示这一行没有按预期写入（已存在或写入失败），调用方**不得**在此之后开始输入。
    pub(crate) fn create_run(&self, run: &NewComputerUseRun) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            INSERT OR IGNORE INTO computer_use_runs(
                call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
                idempotency_key, objective_json, surface, state, state_version,
                deadline_ms, created_at_ms, updated_at_ms,
                workspace_id, workspace_context_version
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'requested', 0, ?9, ?10, ?10, ?11, ?12)
            "#,
            params![
                run.call_id,
                run.provider_tool_call_id,
                run.turn_id,
                run.session_id,
                run.chat_room_id,
                run.idempotency_key,
                run.objective_json,
                surface_name(run.surface),
                run.deadline_ms,
                run.created_at_ms,
                run.workspace.workspace_id(),
                run.workspace.context_version(),
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn transition(
        &self,
        call_id: &str,
        expected_version: u64,
        state: ComputerUseRunState,
        updated_at_ms: u64,
    ) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            UPDATE computer_use_runs
            SET state = ?1, state_version = state_version + 1, updated_at_ms = ?2
            WHERE call_id = ?3 AND state_version = ?4 AND terminal_result_json IS NULL
            "#,
            params![
                run_state_name(state),
                updated_at_ms,
                call_id,
                expected_version
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn finish(
        &self,
        call_id: &str,
        expected_version: u64,
        result: &ComputerUseResult,
    ) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let result_json = serde_json::to_string(result)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let changed = connection.execute(
            r#"
            UPDATE computer_use_runs
            SET state = ?1, state_version = state_version + 1,
                terminal_result_json = ?2, updated_at_ms = ?3
            WHERE call_id = ?4 AND state_version = ?5 AND terminal_result_json IS NULL
            "#,
            params![
                run_state_name(terminal_run_state(result.status)),
                result_json,
                now_ms(),
                call_id,
                expected_version,
            ],
        )?;
        Ok(changed == 1)
    }

    pub(crate) fn append_step(&self, step: &ComputerUseStepRecord) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let changed = connection.execute(
            r#"
            INSERT OR IGNORE INTO computer_use_steps(
                run_id, step_index, observation_generation, action_type, normalized_target,
                action_fingerprint, status, error_code, before_evidence_ref, after_evidence_ref,
                visible_progress, input_delivery, partial, path_completed, confirmed_point_count,
                effect_status, goal_verdict, input_release_status, started_at_ms, completed_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            "#,
            params![
                step.run_id,
                step.step_index,
                step.observation_generation,
                step.action_type,
                step.normalized_target,
                step.action_fingerprint,
                step.status,
                step.error_code,
                step.before_evidence_ref,
                step.after_evidence_ref,
                step.visible_progress,
                step.input_delivery.map(input_delivery_name),
                step.partial,
                step.path_completed,
                step.confirmed_point_count,
                step.effect_status.map(effect_status_name),
                step.goal_verdict.map(goal_verdict_name),
                step.input_release_status.map(input_release_status_name),
                step.started_at_ms,
                step.completed_at_ms,
            ],
        )?;
        Ok(changed == 1)
    }

    /// 实际输入前写 executing，输入/观察后更新同一步；不事后伪造动作与时间。
    pub(crate) fn record_step(
        &self,
        step: &ComputerUseStepRecord,
        action_json: &str,
    ) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("computer-use store lock");
        let transaction = connection.transaction()?;
        Self::write_step(&transaction, step, action_json)?;
        transaction.commit()
    }

    /// 步骤行 + 动作详情写入（`record_step` 与 `record_step_with_fact` 的**唯一**实现，
    /// 避免两条写入路径各自漂移）。
    fn write_step(
        transaction: &rusqlite::Transaction<'_>,
        step: &ComputerUseStepRecord,
        action_json: &str,
    ) -> rusqlite::Result<()> {
        transaction.execute("INSERT INTO computer_use_steps
            (run_id,step_index,observation_generation,action_type,normalized_target,action_fingerprint,status,error_code,before_evidence_ref,after_evidence_ref,visible_progress,input_delivery,partial,path_completed,confirmed_point_count,effect_status,goal_verdict,input_release_status,started_at_ms,completed_at_ms)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
            ON CONFLICT(run_id,step_index) DO UPDATE SET status=excluded.status,error_code=excluded.error_code,
            after_evidence_ref=excluded.after_evidence_ref,visible_progress=excluded.visible_progress,
            input_delivery=excluded.input_delivery,partial=excluded.partial,path_completed=excluded.path_completed,
            confirmed_point_count=excluded.confirmed_point_count,effect_status=excluded.effect_status,
            goal_verdict=excluded.goal_verdict,input_release_status=excluded.input_release_status,
            completed_at_ms=excluded.completed_at_ms",
            params![step.run_id,step.step_index,step.observation_generation,step.action_type,step.normalized_target,
                step.action_fingerprint,step.status,step.error_code,step.before_evidence_ref,step.after_evidence_ref,
                step.visible_progress,step.input_delivery.map(input_delivery_name),step.partial,step.path_completed,
                step.confirmed_point_count,step.effect_status.map(effect_status_name),step.goal_verdict.map(goal_verdict_name),
                step.input_release_status.map(input_release_status_name),step.started_at_ms,step.completed_at_ms])?;
        transaction.execute("INSERT OR IGNORE INTO computer_use_step_details(run_id,step_index,action_json) VALUES(?1,?2,?3)",
            params![step.run_id,step.step_index,action_json])?;
        Ok(())
    }

    /// 同一次业务提交里写步骤 + 由调用方在**同一事务内**追加事实（第二轮裁决第 2.2 项）。
    ///
    /// 存在的意义是让这种形态在结构上不可能出现：
    /// `先提交步骤成功 → 再独立连接写回执 → 第二步失败 → 仍宣布事实已完整保存`。
    ///
    /// 事实**必须**经 `AppendOnlyFactStore`（规则引擎：契约校验、first-wins、身份异常分流）
    /// 写入，因此这里把事务交给调用方自己去跑规则引擎，而不是替它拼一个 `FactLogRecord`——
    /// 绕过规则引擎的写入路径正是裁决要禁掉的东西。调用方返回 `Err` 或规则引擎报错都会
    /// **回滚整个事务**，步骤更新一并撤销。
    pub(crate) fn record_step_with_facts(
        &self,
        step: &ComputerUseStepRecord,
        action_json: &str,
        append_facts: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "computer-use store lock".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        Self::write_step(&transaction, step, action_json).map_err(|error| error.to_string())?;
        append_facts(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn record_planner_diagnostic(
        &self,
        value: &PlannerDiagnostic<'_>,
    ) -> rusqlite::Result<()> {
        self.connection.lock().expect("computer-use store lock").execute(
            "INSERT INTO computer_use_planner_diagnostics(call_id,turn_id,room_id,session_id,request_kind,observation_generation,
             model,provider_response_id,response_json,error_code,started_at_ms,completed_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![value.call_id,value.turn_id,value.room_id,value.session_id,value.request_kind,value.observation_generation,
                value.model,value.provider_response_id,value.response_json,value.error_code,value.started_at_ms,value.completed_at_ms])?;
        Ok(())
    }

    pub(crate) fn record_step_verification(
        &self,
        call_id: &str,
        before_generation: u64,
        verification: &computer_use::Verification,
    ) -> rusqlite::Result<()> {
        self.connection.lock().expect("computer-use store lock").execute(
            "UPDATE computer_use_steps SET visible_progress=?1,status=?2,after_evidence_ref=?3,
                effect_status=?4,goal_verdict=?5
             WHERE run_id=?6 AND step_index=(SELECT MAX(step_index) FROM computer_use_steps WHERE run_id=?6 AND observation_generation=?7)
             AND status='input_sent_observed'",
            params![verification.visible_progress,if verification.achieved {"verified"} else {"completed_unverified"},
                serde_json::to_string(&verification.evidence).unwrap_or_default(),
                if verification.visible_progress { effect_status_name(EffectStatus::EffectObserved) } else { effect_status_name(EffectStatus::Inconclusive) },
                if verification.achieved { goal_verdict_name(GoalVerdict::Passed) } else { goal_verdict_name(GoalVerdict::Failed) },
                call_id,before_generation])?;
        Ok(())
    }

    pub(crate) fn action_counts(&self, call_id: &str) -> rusqlite::Result<(usize, usize)> {
        self.connection.lock().expect("computer-use store lock").query_row(
            "SELECT COUNT(*),COALESCE(SUM(CASE WHEN input_delivery='sent' OR (input_delivery IS NULL AND status IN ('input_sent','input_sent_observed','verified','completed_unverified','observation_failed')) THEN 1 ELSE 0 END),0) FROM computer_use_steps WHERE run_id=?1",
            [call_id], |row| Ok((row.get(0)?,row.get(1)?)))
    }

    pub(crate) fn load(&self, call_id: &str) -> rusqlite::Result<Option<StoredComputerUseRun>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        connection
            .query_row(
                &format!("{STORED_RUN_SELECT_SQL} WHERE call_id = ?1"),
                [call_id],
                stored_run_from_row,
            )
            .optional()
    }

    pub(crate) fn load_by_idempotency_key(
        &self,
        session_id: &str,
        turn_id: &str,
        idempotency_key: &str,
    ) -> rusqlite::Result<Option<StoredComputerUseRun>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        connection
            .query_row(
                &format!(
                    "{STORED_RUN_SELECT_SQL} \
                     WHERE session_id = ?1 AND turn_id = ?2 AND idempotency_key = ?3 \
                     ORDER BY updated_at_ms DESC, created_at_ms DESC LIMIT 1"
                ),
                params![session_id, turn_id, idempotency_key],
                stored_run_from_row,
            )
            .optional()
    }

    /// 同一 scope 内**仍未收尾**且工作区归属未记录（历史行）的运行。
    ///
    /// 判据刻意取"两个终态信号都必须说结束"（`terminal_result_json IS NULL` 且 state 非终态）：
    /// 只要有一处显示已收尾，就不算"旧活动运行"，避免把已经收尾的行当成阻断理由。
    /// 它只查证、不改写：历史行的 NULL 归属永远不会被回填。
    pub(crate) fn unrecorded_workspace_active_runs(
        &self,
        session_id: &str,
        turn_id: &str,
        exclude_call_id: &str,
    ) -> rusqlite::Result<Vec<String>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare(&format!(
            "SELECT call_id FROM computer_use_runs \
             WHERE session_id = ?1 AND turn_id = ?2 AND call_id <> ?3 \
               AND workspace_id IS NULL AND terminal_result_json IS NULL \
               AND state NOT IN ({}) \
             ORDER BY created_at_ms ASC",
            terminal_run_state_sql_list()
        ))?;
        let rows = statement.query_map(params![session_id, turn_id, exclude_call_id], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect()
    }

    pub(crate) fn count_runs_for_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<usize> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM computer_use_runs WHERE session_id = ?1 AND turn_id = ?2",
            params![session_id, turn_id],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as usize)
    }

    /// 只有确定在参数校验阶段、尚无输入的拒绝才使用独立纠错额度。
    /// 观察/规划/执行失败和运行中的调用仍消耗原执行额度，不能藉改参数无限重入。
    pub(crate) fn turn_budget_counts(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<(usize, usize)> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare(
            "SELECT terminal_result_json FROM computer_use_runs WHERE session_id=?1 AND turn_id=?2",
        )?;
        let rows = statement.query_map(params![session_id, turn_id], |row| {
            row.get::<_, Option<String>>(0)
        })?;
        let (mut execution, mut invalid) = (0, 0);
        for row in rows {
            let result: Option<ComputerUseResult> =
                row?.map(|value| parse_json_column(&value)).transpose()?;
            let is_invalid = result.as_ref().is_some_and(|result| {
                result.stage == computer_use::ComputerUseStage::IntentGuard
                    && result.attempts == 0
                    && result.steps_completed == 0
                    && result.error.as_ref().is_some_and(|error| {
                        matches!(
                            error.code.as_str(),
                            "invalid_tool_input"
                                | "invalid_objective"
                                | "invalid_success_criteria"
                                | "input_correction_budget_exhausted"
                        )
                    })
            });
            if is_invalid {
                invalid += 1;
            } else {
                execution += 1;
            }
        }
        Ok((execution, invalid))
    }

    /// 本 session/turn 内是否存在**未解除的未确认释放**（RPR-05b-1 跨 run 互锁判据）。
    ///
    /// 纯持久化查询：事实来自 sqlite 行，因此进程重启不会清空。
    pub(crate) fn has_unconfirmed_release(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<bool> {
        Ok(!self
            .unconfirmed_release_facts(session_id, turn_id)?
            .is_empty())
    }

    /// 互锁判据的完整现场事实（供执行器阻断、以及解除入口展示前置检查）。
    pub(crate) fn unconfirmed_release_facts(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<UnconfirmedReleaseFacts> {
        let connection = self.connection.lock().expect("computer-use store lock");
        unconfirmed_release_facts_on(&connection, session_id, turn_id)
    }

    /// 追加一条人工解除事实，并返回它覆盖的 run 清单与新旧 epoch。
    ///
    /// 语义边界：
    /// - **不改写历史**：`computer_use_steps / computer_use_runs` 一行不碰，
    ///   旧的 `unknown` 解除后仍然是 `unknown`；
    /// - **不复活旧 run**：解除只影响互锁判据，旧 run 的终态与 state 原样保留；
    /// - 本 scope 没有可解除事实时是幂等空操作（不落库、不推进 epoch）。
    pub(crate) fn resolve_unconfirmed_release(
        &self,
        request: &ReleaseResolutionRequest<'_>,
    ) -> rusqlite::Result<ReleaseResolutionOutcome> {
        if request.session_id.trim().is_empty() || request.turn_id.trim().is_empty() {
            return Err(invalid_resolution_request(
                "解除必须绑定 session_id 与 turn_id",
            ));
        }
        if request.reason.trim().is_empty() {
            return Err(invalid_resolution_request("解除必须给出人工理由"));
        }
        if request.operator_check_note.trim().is_empty() {
            return Err(invalid_resolution_request("解除必须给出前置现场检查说明"));
        }
        let mut connection = self.connection.lock().expect("computer-use store lock");
        let transaction = connection.transaction()?;
        let previous_epoch = scoped_epoch_on(&transaction, request.session_id, request.turn_id)?;
        let facts = unconfirmed_release_facts_on(&transaction, request.session_id, request.turn_id)?;
        if facts.is_empty() {
            // 没有可解除的事实：不落库、不推进 epoch——审计里不该出现"解除了一条不存在的隔离"。
            transaction.commit()?;
            return Ok(ReleaseResolutionOutcome {
                recorded: false,
                resolution_id: None,
                previous_epoch,
                epoch: previous_epoch,
                covered_run_ids: Vec::new(),
                covered_step_count: 0,
                remaining_unconfirmed_runs: 0,
                created_at_ms: now_ms(),
            });
        }
        let created_at_ms = now_ms();
        let epoch = previous_epoch.saturating_add(1);
        let precheck_json = serde_json::json!({
            "detected_run_ids": &facts.run_ids,
            "detected_step_count": facts.step_count,
            "oldest_step_started_at_ms": facts.oldest_step_started_at_ms,
            "newest_step_started_at_ms": facts.newest_step_started_at_ms,
            "input_owner_epoch_before": request.input_owner_epoch_before,
            "operator_check_note": request.operator_check_note,
        })
        .to_string();
        transaction.execute(
            "INSERT INTO computer_use_release_resolutions(
                session_id, turn_id, epoch, previous_epoch, operator_source, operator_id,
                reason, input_owner_epoch_before, input_owner_epoch_after,
                covered_run_count, covered_step_count, precheck_json, postcheck_json, created_at_ms
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'{}',?13)",
            params![
                request.session_id,
                request.turn_id,
                epoch,
                previous_epoch,
                request.operator_source.as_str(),
                request.operator_id,
                request.reason,
                request.input_owner_epoch_before,
                request.input_owner_epoch_after,
                facts.run_ids.len(),
                facts.step_count,
                precheck_json,
                created_at_ms,
            ],
        )?;
        let resolution_id = transaction.last_insert_rowid();
        for run_id in &facts.run_ids {
            transaction.execute(
                "INSERT INTO computer_use_release_resolution_runs(resolution_id, run_id)
                 VALUES (?1, ?2)",
                params![resolution_id, run_id],
            )?;
        }
        // 后置检查必须在覆盖行写入之后才算得出：若在预检与写入之间有别的进程追加了
        // 新的未确认释放，这里的重查会把它抓出来（remaining > 0）。同一事务内先插入
        // 再把最终检查结果补齐，行在提交前就已是完整事实，不构成对历史的改写。
        let after = unconfirmed_release_facts_on(&transaction, request.session_id, request.turn_id)?;
        let postcheck_json = serde_json::json!({
            "remaining_run_ids": &after.run_ids,
            "remaining_step_count": after.step_count,
            "input_owner_epoch_after": request.input_owner_epoch_after,
            "clear": after.is_empty(),
        })
        .to_string();
        transaction.execute(
            "UPDATE computer_use_release_resolutions SET postcheck_json = ?1
             WHERE resolution_id = ?2",
            params![postcheck_json, resolution_id],
        )?;
        transaction.commit()?;
        Ok(ReleaseResolutionOutcome {
            recorded: true,
            resolution_id: Some(resolution_id),
            previous_epoch,
            epoch,
            covered_run_ids: facts.run_ids,
            covered_step_count: facts.step_count,
            remaining_unconfirmed_runs: after.run_ids.len(),
            created_at_ms,
        })
    }

    /// 本 scope 的解除事实（按 epoch 升序），供审计与解除入口回显。
    pub(crate) fn release_resolutions(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> rusqlite::Result<Vec<StoredReleaseResolution>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare(
            "SELECT resolution_id, session_id, turn_id, epoch, previous_epoch, operator_source,
                    operator_id, reason, input_owner_epoch_before, input_owner_epoch_after,
                    covered_run_count, covered_step_count, precheck_json, postcheck_json,
                    created_at_ms
             FROM computer_use_release_resolutions
             WHERE session_id = ?1 AND turn_id = ?2 ORDER BY epoch",
        )?;
        let rows = statement
            .query_map(params![session_id, turn_id], |row| {
                Ok(StoredReleaseResolution {
                    resolution_id: row.get(0)?,
                    session_id: row.get(1)?,
                    turn_id: row.get(2)?,
                    epoch: row.get(3)?,
                    previous_epoch: row.get(4)?,
                    operator_source: row.get(5)?,
                    operator_id: row.get(6)?,
                    reason: row.get(7)?,
                    operator_check_note: None,
                    input_owner_epoch_before: row.get(8)?,
                    input_owner_epoch_after: row.get(9)?,
                    covered_run_count: row.get(10)?,
                    covered_step_count: row.get(11)?,
                    covered_run_ids: Vec::new(),
                    precheck_json: row.get(12)?,
                    postcheck_json: row.get(13)?,
                    created_at_ms: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut resolutions = Vec::with_capacity(rows.len());
        for mut resolution in rows {
            resolution.covered_run_ids = covered_run_ids_on(&connection, resolution.resolution_id)?;
            resolution.operator_check_note = serde_json::from_str::<serde_json::Value>(
                &resolution.precheck_json,
            )
            .ok()
            .and_then(|value| {
                value
                    .get("operator_check_note")
                    .and_then(|note| note.as_str())
                    .map(str::to_string)
            });
            resolutions.push(resolution);
        }
        Ok(resolutions)
    }

    // -----------------------------------------------------------------------
    // RD4-01：遗留 CU 运行的收敛（读回 / 意图 / 单事务写入）
    // -----------------------------------------------------------------------

    /// 读回某个 run 的收敛控制状态（`NULL` = 未收敛）。
    ///
    /// 它回答**只**控制维度："该 run 是否仍允许继续规划 / 派发 / 执行"。
    /// **不回答**输入资源是否安全、**不回答**历史结果是什么。
    pub(crate) fn legacy_run_control_state(
        &self,
        call_id: &str,
    ) -> rusqlite::Result<StoredLegacyRunControlState> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let value: Option<String> = connection
            .query_row(
                "SELECT legacy_convergence_state FROM computer_use_runs WHERE call_id = ?1",
                [call_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        legacy_run_control_state_from_column(value)
    }

    /// 本库里**尚未收敛**的遗留运行（迁移 v22 之前写入、归属未记录、未收尾）。
    ///
    /// 它就是 A-1 的"被处理对象"判据的读回面：**四个条件同时成立**才算候选——
    /// `workspace_id IS NULL`（历史归属未记录）、`terminal_result_json IS NULL`、
    /// `state` 非终态（未收尾）、`legacy_convergence_id IS NULL`（尚未收敛）。
    /// 它只读不改：既不会回填归属，也不会顺手改状态。
    pub(crate) fn legacy_unconverged_runs(
        &self,
    ) -> rusqlite::Result<Vec<LegacyUnconvergedRun>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare(&format!(
            "SELECT call_id, session_id, turn_id, state, state_version, created_at_ms
             FROM computer_use_runs
             WHERE workspace_id IS NULL
               AND terminal_result_json IS NULL
               AND legacy_convergence_id IS NULL
               AND state NOT IN ({})
             ORDER BY created_at_ms, call_id",
            terminal_run_state_sql_list()
        ))?;
        let rows = statement.query_map([], |row| {
            Ok(LegacyUnconvergedRun {
                call_id: row.get(0)?,
                session_id: row.get(1)?,
                turn_id: row.get(2)?,
                state: row.get(3)?,
                state_version: row.get(4)?,
                created_at_ms: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// 某个遗留 run 的**最新**一条收敛事实（没有就 `None`）。
    pub(crate) fn latest_legacy_run_convergence(
        &self,
        call_id: &str,
    ) -> rusqlite::Result<Option<StoredLegacyRunConvergence>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        connection
            .query_row(
                &format!("{LEGACY_CONVERGENCE_SELECT_SQL} WHERE original_run_id = ?1 ORDER BY convergence_id DESC LIMIT 1"),
                [call_id],
                stored_legacy_convergence_from_row,
            )
            .optional()
    }

    /// 本库全部收敛事实（按 `convergence_id` 升序）。规则对账与审计用。
    pub(crate) fn legacy_run_convergences(
        &self,
    ) -> rusqlite::Result<Vec<StoredLegacyRunConvergence>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement =
            connection.prepare(&format!("{LEGACY_CONVERGENCE_SELECT_SQL} ORDER BY convergence_id"))?;
        let rows = statement.query_map([], stored_legacy_convergence_from_row)?;
        rows.collect()
    }

    /// 登记（或幂等读回）一条**可重入的**收敛意图。
    ///
    /// 语义：
    /// - 同一个对账键（来源对象 + 原状态·revision）上没有任何意图 → 登记（`Inserted`）；
    /// - 已有**完全相同**的意图（同一操作 ID、同一登记时刻）→ `AlreadyRecorded`（不追加）；
    /// - 已有意图但操作 ID / 登记时刻不同 → `Conflicting`：**不覆盖**。重复启动 / 双实例
    ///   应当从 [`Self::pending_legacy_run_convergence_intents`] 读到既有意图并**重新进入**
    ///   那次收敛（用记录里的操作 ID），而不是另起一次。
    pub(crate) fn record_legacy_run_convergence_intent(
        &self,
        intent: &LegacyCuRunConvergenceIntent,
    ) -> rusqlite::Result<LegacyRunConvergenceIntentUpsert> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let key = legacy_convergence_reconciliation_key_literal(&intent.reconciliation_key());
        let existing: Option<(String, u64, String, String, u64, bool)> = connection
            .query_row(
                "SELECT recovery_operation_id, recorded_at_ms, source_database_identity,
                        original_run_id, original_state_version, reconciled
                 FROM computer_use_legacy_run_convergence_intents
                 WHERE reconciliation_key = ?1",
                [&key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        if let Some((
            operation_id,
            recorded_at_ms,
            source_database_identity,
            original_run_id,
            original_state_version,
            reconciled,
        )) = existing
        {
            if operation_id == intent.recovery_operation_id
                && recorded_at_ms == intent.recorded_at_unix_ms
                && source_database_identity == intent.source_database_identity
                && original_run_id == intent.original_run_id
                && original_state_version == intent.original_state_version
            {
                return Ok(LegacyRunConvergenceIntentUpsert::AlreadyRecorded);
            }
            return Ok(LegacyRunConvergenceIntentUpsert::Conflicting {
                existing: LegacyCuRunConvergenceIntent {
                    recovery_operation_id: operation_id,
                    source_database_identity,
                    original_run_id,
                    original_state: intent.original_state.clone(),
                    original_state_version,
                    recorded_at_unix_ms: recorded_at_ms,
                    reconciled,
                },
            });
        }
        connection.execute(
            "INSERT INTO computer_use_legacy_run_convergence_intents(
                recovery_operation_id, reconciliation_key, source_database_identity,
                original_run_id, original_state, original_state_version, recorded_at_ms, reconciled
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                intent.recovery_operation_id,
                key,
                intent.source_database_identity,
                intent.original_run_id,
                intent.original_state,
                intent.original_state_version,
                intent.recorded_at_unix_ms,
                intent.reconciled,
            ],
        )?;
        Ok(LegacyRunConvergenceIntentUpsert::Inserted)
    }

    /// **未完成对账**的收敛意图（`reconciled = 0`）。
    ///
    /// 它表示"意图已登记、收敛事实尚未写入"——崩溃或响应丢失后的**可重入**入口：
    /// 调用方据此重新进入那次收敛（用记录里的操作 ID），而不是另起一次。
    /// 意图存在**不代表**收敛已发生，更不代表运行已被终止。
    pub(crate) fn pending_legacy_run_convergence_intents(
        &self,
    ) -> rusqlite::Result<Vec<LegacyCuRunConvergenceIntent>> {
        let connection = self.connection.lock().expect("computer-use store lock");
        let mut statement = connection.prepare(
            "SELECT recovery_operation_id, source_database_identity, original_run_id,
                    original_state, original_state_version, recorded_at_ms
             FROM computer_use_legacy_run_convergence_intents
             WHERE reconciled = 0 ORDER BY recorded_at_ms, recovery_operation_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(LegacyCuRunConvergenceIntent {
                recovery_operation_id: row.get(0)?,
                source_database_identity: row.get(1)?,
                original_run_id: row.get(2)?,
                original_state: row.get(3)?,
                original_state_version: row.get(4)?,
                recorded_at_unix_ms: row.get(5)?,
                reconciled: false,
            })
        })?;
        rows.collect()
    }

    /// 收敛一条遗留 CU 运行：**唯一**写入入口。
    ///
    /// 顺序（A-1.6，与 `runtime::LEGACY_CU_RUN_CONVERGENCE_STEPS` 逐项一致；顺序由
    /// [`LegacyConvergenceOrderGate`] 在结构上强制，写不到"危险顺序"上去）：
    ///
    /// 1. 暂停新输入接纳 + 取得恢复协调权（由 `request` 声明；没做就 `PreconditionUnmet`）；
    /// 2. 登记**可重入的**收敛意图（自己的事务，先于写入；便于崩溃后对账）；
    /// 3. 必要时先建立共享资源事故 / 恢复待决阻断（资源不安全却没有任何阻断引用 → 拒绝）；
    /// 4. 在**同一笔** Immediate 事务里：写旧 run 的收敛列（非成功控制状态）+ 收敛事实；
    /// 5. 在同一事务里把意图标记为已对账；
    /// 6. 返回"是否允许开放新输入"——**只**由维度③的独立检查结果决定。
    ///
    /// **不宣称跨库原子性**：会话库（本 store）与输入安全库不是同一事务，因此
    /// `resource_blocking_refs` 引用的是**另一个库**里已建立的阻断记录。
    /// **不碰历史列**：`state` / `state_version` / `terminal_result_json` / `updated_at_ms` /
    /// `workspace_id` 一律不改写，因此旧 SQL 在收敛后仍命中该 run（资源维度不被清零）。
    pub(crate) fn converge_legacy_run(
        &self,
        request: &LegacyRunConvergenceRequest<'_>,
    ) -> rusqlite::Result<LegacyRunConvergenceOutcome> {
        let mut gate = LegacyConvergenceOrderGate::default();
        // 步骤①：暂停新输入 + 恢复协调权。
        if let Err(precondition) = gate.mark_paused_and_authorized(
            request.new_input_intake_paused,
            request.recovery_control_authority,
        ) {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(precondition));
        }
        // 读旧 run 现状（本类事实的对象判据 + 规则要用的观测值）。
        let observed_row = {
            let connection = self.connection.lock().expect("computer-use store lock");
            connection
                .query_row(
                    "SELECT state, state_version, terminal_result_json, workspace_id,
                            session_id, turn_id, legacy_convergence_id
                     FROM computer_use_runs WHERE call_id = ?1",
                    [request.call_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, u64>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<i64>>(6)?,
                        ))
                    },
                )
                .optional()?
        };
        let Some((
            observed_state,
            observed_state_version,
            terminal_result_json,
            recorded_workspace,
            session_id,
            turn_id,
            linked_convergence_id,
        )) = observed_row
        else {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::UnknownRun,
            ));
        };
        if recorded_workspace.is_some() {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::WorkspaceAttributionRecorded,
            ));
        }
        let has_real_terminal = terminal_result_json.is_some()
            || TERMINAL_RUN_STATE_LITERALS.contains(&observed_state.as_str());
        let subject = LegacyCuRunSubject {
            source_database_identity: request.source_database_identity.to_string(),
            source_database_identity_registered_now: request
                .source_database_identity_registered_now,
            original_run_id: request.call_id.to_string(),
            source_database_identity_not_past_attribution: request
                .source_database_identity_registered_now
                .then(|| {
                    runtime::LEGACY_CU_RUN_SOURCE_DATABASE_IDENTITY_NOT_PAST_ATTRIBUTION.to_string()
                }),
        };
        // 原始信息取**调用方读到的那一份**（不是本次重读的）：这样"两读之间该行被别的提交
        // 更新过"会被规则判成 `OriginalRevisionChanged` 并拒绝覆盖。
        let original = LegacyCuRunOriginalFacts {
            original_state: request.expected_original_state.to_string(),
            original_state_version: request.expected_original_state_version,
            session_id,
            turn_id,
        };
        let candidate = legacy_convergence_fact_from_request(subject, original, request);
        if let Err(error) = candidate.validate() {
            return Err(invalid_legacy_convergence_column(format!(
                "收敛事实不成立（{}）：{}",
                error.code, error.message
            )));
        }
        // 步骤③的**输入**先做一次纯校验：资源不安全却没有任何阻断引用时，**一个字节都不写**
        // （顺序上的“先建立阻断”由下面的顺序闸门在真正的位置再强制一次）。
        if !candidate.input_resource.safe_for_new_input
            && request
                .resource_blocking_refs
                .iter()
                .all(|reference| reference.trim().is_empty())
        {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::ResourceBlockNotEstablished,
            ));
        }
        // 已经收敛过的行：直接给出幂等 / 冲突判决（不再写第二份）。
        if let Some(convergence_id) = linked_convergence_id {
            return match self.latest_legacy_run_convergence(request.call_id)? {
                Some(existing) if existing.fact.same_convergence_as(&candidate) => {
                    Ok(LegacyRunConvergenceOutcome::AlreadyConverged { convergence_id })
                }
                Some(existing) => Ok(LegacyRunConvergenceOutcome::Refused(
                    LegacyCuRunConvergenceRefusal::ConflictingConvergence {
                        existing: Box::new(existing.fact),
                    },
                )),
                // 列上有链接却读不到事实：不一致的存储状态，fail-closed。
                None => Err(invalid_legacy_convergence_column(
                    "旧 run 行指向一条读不到的收敛事实".to_string(),
                )),
            };
        }
        let observed = LegacyCuRunObservedState {
            state: observed_state,
            state_version: observed_state_version,
            has_real_terminal,
            has_commit_candidate: request.observed_commit_candidate,
        };
        // 与"既有收敛事实"无关的拒绝（已有真实终态 / 提交候选 / 原 revision 变了）在登记意图
        // **之前**判定：被拒的请求不该留下一个待对账的意图。
        match reconcile_legacy_cu_run_convergence(&[], &observed, &candidate)
            .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?
        {
            LegacyCuRunConvergenceRuleDecision::Refused(refusal) => {
                return Ok(LegacyRunConvergenceOutcome::Refused(refusal));
            }
            LegacyCuRunConvergenceRuleDecision::Recorded
            | LegacyCuRunConvergenceRuleDecision::AlreadyConverged { .. } => {}
        }
        // 步骤②：登记可重入的收敛意图（自己的事务：先于写入落盘，崩溃后可对账）。
        let intent = LegacyCuRunConvergenceIntent {
            recovery_operation_id: candidate.decision.recovery_operation_id.clone(),
            source_database_identity: candidate.subject.source_database_identity.clone(),
            original_run_id: candidate.subject.original_run_id.clone(),
            original_state: candidate.original.original_state.clone(),
            original_state_version: candidate.original.original_state_version,
            recorded_at_unix_ms: candidate.operator.operated_at_unix_ms,
            reconciled: false,
        };
        match self.record_legacy_run_convergence_intent(&intent)? {
            LegacyRunConvergenceIntentUpsert::Inserted
            | LegacyRunConvergenceIntentUpsert::AlreadyRecorded => {}
            LegacyRunConvergenceIntentUpsert::Conflicting { existing } => {
                return Ok(LegacyRunConvergenceOutcome::Refused(
                    LegacyCuRunConvergenceRefusal::ConflictingIntent {
                        existing_operation_id: existing.recovery_operation_id,
                        existing_recorded_at_unix_ms: existing.recorded_at_unix_ms,
                    },
                ));
            }
        }
        if let Err(precondition) = gate.mark_intent_recorded() {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(precondition));
        }
        // 步骤③：必要时先建立资源阻断（资源不安全却没有任何阻断引用 → 拒绝写入）。
        if let Err(precondition) = gate.mark_resource_block_established(
            request.resource_blocking_refs,
            candidate.input_resource.safe_for_new_input,
        ) {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(precondition));
        }
        // 顺序自检：走过的步骤必须与契约的规范顺序逐项一致（重排即拒绝）。
        if !legacy_cu_run_convergence_order_is_safe(&gate.steps_taken()) {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::WriteOrderViolated {
                    attempted: LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
                },
            ));
        }
        if let Err(precondition) = gate.require_ready_for_terminal_write() {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(precondition));
        }
        // 步骤④⑤：同一笔 Immediate 事务里写旧 run 的收敛列 + 收敛事实 + 意图对账。
        let mut connection = self.connection.lock().expect("computer-use store lock");
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let existing_facts = {
            let mut statement = transaction.prepare(&format!(
                "{LEGACY_CONVERGENCE_SELECT_SQL} ORDER BY convergence_id"
            ))?;
            let rows = statement.query_map([], stored_legacy_convergence_from_row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let existing_facts: Vec<LegacyCuRunConvergenceFact> = existing_facts
            .into_iter()
            .map(|stored| stored.fact)
            .collect();
        match reconcile_legacy_cu_run_convergence(&existing_facts, &observed, &candidate)
            .map_err(|error| invalid_legacy_convergence_column(error.to_string()))?
        {
            LegacyCuRunConvergenceRuleDecision::AlreadyConverged { .. } => {
                // 幂等：不写第二份，也不改写旧 run 行。
                transaction.rollback()?;
                return Ok(LegacyRunConvergenceOutcome::AlreadyConverged {
                    convergence_id: existing_convergence_id_on(&connection, &candidate)?,
                });
            }
            LegacyCuRunConvergenceRuleDecision::Refused(refusal) => {
                transaction.rollback()?;
                return Ok(LegacyRunConvergenceOutcome::Refused(refusal));
            }
            LegacyCuRunConvergenceRuleDecision::Recorded => {}
        }
        let convergence_id = insert_legacy_convergence_fact_on(&transaction, &candidate)?;
        gate.mark_terminal_and_fact_written();
        // 旧 run 行：只写**新增的收敛列**（CAS：未收敛 + 未收尾才写）。
        let changed = transaction.execute(
            "UPDATE computer_use_runs
                SET legacy_convergence_id = ?1,
                    legacy_convergence_state = ?2,
                    legacy_convergence_reconciled_at_ms = ?3
              WHERE call_id = ?4
                AND state_version = ?5
                AND legacy_convergence_id IS NULL
                AND workspace_id IS NULL
                AND terminal_result_json IS NULL",
            params![
                convergence_id,
                LEGACY_RUN_CONVERGENCE_CONTROL_STATE,
                candidate.reconciled_at_unix_ms,
                candidate.subject.original_run_id,
                candidate.original.original_state_version,
            ],
        )?;
        if changed != 1 {
            // 两读之间该行被别的提交改过（或有并发收敛）：整笔回滚，不留半套事实。
            // 拒绝里报告**当下实际**读到的状态/版本，而不是本次开始时的那一份快照。
            let current: Option<(String, u64)> = transaction
                .query_row(
                    "SELECT state, state_version FROM computer_use_runs WHERE call_id = ?1",
                    [candidate.subject.original_run_id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (refused_state, refused_version) =
                current.unwrap_or((observed.state.clone(), observed.state_version));
            transaction.rollback()?;
            return Ok(LegacyRunConvergenceOutcome::Refused(
                LegacyCuRunConvergenceRefusal::OriginalRevisionChanged {
                    observed_state: refused_state,
                    observed_state_version: refused_version,
                },
            ));
        }
        let reconciled = transaction.execute(
            "UPDATE computer_use_legacy_run_convergence_intents SET reconciled = 1
              WHERE reconciliation_key = ?1 AND recovery_operation_id = ?2",
            params![
                legacy_convergence_reconciliation_key_literal(&candidate.reconciliation_key()),
                candidate.decision.recovery_operation_id,
            ],
        )?;
        if reconciled != 1 {
            return Err(invalid_legacy_convergence_column(
                "收敛意图缺失或不是本次操作：拒绝提交不一致的收敛".to_string(),
            ));
        }
        if let Err(precondition) = gate.mark_reconciled() {
            return Ok(LegacyRunConvergenceOutcome::PreconditionUnmet(precondition));
        }
        let may_reopen_new_input =
            gate.may_reopen_new_input(candidate.reopen_new_input_is_allowed());
        transaction.commit()?;
        Ok(LegacyRunConvergenceOutcome::Converged {
            convergence_id,
            reconciled_at_ms: candidate.reconciled_at_unix_ms,
            may_reopen_new_input,
        })
    }
}

/// 已收敛行的 `convergence_id`（幂等分支用；读不到就是存储不一致，fail-closed）。
fn existing_convergence_id_on(
    connection: &Connection,
    candidate: &LegacyCuRunConvergenceFact,
) -> rusqlite::Result<i64> {
    connection
        .query_row(
            "SELECT convergence_id FROM computer_use_legacy_run_convergences
              WHERE reconciliation_key = ?1",
            [legacy_convergence_reconciliation_key_literal(
                &candidate.reconciliation_key(),
            )],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            invalid_legacy_convergence_column(
                "幂等分支找不到既有的收敛事实（存储不一致）".to_string(),
            )
        })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn terminal_run_state(status: ComputerUseTerminalStatus) -> ComputerUseRunState {
    match status {
        ComputerUseTerminalStatus::Succeeded => ComputerUseRunState::Succeeded,
        ComputerUseTerminalStatus::Failed => ComputerUseRunState::Failed,
        ComputerUseTerminalStatus::Blocked => ComputerUseRunState::Blocked,
        ComputerUseTerminalStatus::Cancelled => ComputerUseRunState::Cancelled,
        ComputerUseTerminalStatus::TimedOut => ComputerUseRunState::TimedOut,
    }
}

fn run_state_name(state: ComputerUseRunState) -> &'static str {
    match state {
        ComputerUseRunState::Requested => "requested",
        ComputerUseRunState::Classified => "classified",
        ComputerUseRunState::Observing => "observing",
        ComputerUseRunState::Planning => "planning",
        ComputerUseRunState::PolicyCheck => "policy_check",
        ComputerUseRunState::AwaitingApproval => "awaiting_approval",
        ComputerUseRunState::Executing => "executing",
        ComputerUseRunState::Verifying => "verifying",
        ComputerUseRunState::Succeeded => "succeeded",
        ComputerUseRunState::Failed => "failed",
        ComputerUseRunState::Blocked => "blocked",
        ComputerUseRunState::Cancelled => "cancelled",
        ComputerUseRunState::TimedOut => "timed_out",
    }
}

fn surface_name(surface: ComputerUseSurface) -> &'static str {
    surface.as_str()
}

fn parse_run_state(value: &str) -> rusqlite::Result<ComputerUseRunState> {
    parse_json_column(&format!("\"{value}\""))
}

fn parse_surface(value: &str) -> rusqlite::Result<ComputerUseSurface> {
    parse_json_column(&format!("\"{value}\""))
}

/// 读回运行的**唯一**列清单：写侧与读侧只在这一处对齐，避免两条 SELECT 各自漂移。
const STORED_RUN_SELECT_SQL: &str = "SELECT call_id, provider_tool_call_id, session_id, turn_id, chat_room_id, \
     state, state_version, surface, terminal_result_json, updated_at_ms, \
     workspace_id, workspace_context_version \
     FROM computer_use_runs";

/// 运行行的读回投影：历史行的 `workspace_id = NULL` 呈现为"历史归属未记录"，
/// 绝不在此处（或任何读路径）按当前工作区补值。
fn stored_run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredComputerUseRun> {
    let state: String = row.get(5)?;
    let surface: String = row.get(7)?;
    let terminal_json: Option<String> = row.get(8)?;
    Ok(StoredComputerUseRun {
        call_id: row.get(0)?,
        provider_tool_call_id: row.get(1)?,
        session_id: row.get(2)?,
        turn_id: row.get(3)?,
        chat_room_id: row.get(4)?,
        state: parse_run_state(&state)?,
        state_version: row.get(6)?,
        surface: parse_surface(&surface)?,
        terminal_result: terminal_json
            .map(|json| parse_json_column(&json))
            .transpose()?,
        updated_at_ms: row.get(9)?,
        workspace: StoredWorkspaceAttribution::from_columns(row.get(10)?, row.get(11)?),
    })
}

fn parse_json_column<T: serde::de::DeserializeOwned>(value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

/// 测试用规范工作区标识：**必须**经源头解析器取得（与生产同一条路径，没有测试旁路）。
#[cfg(test)]
pub(crate) fn source_workspace_id(value: &str) -> crate::CanonicalWorkspaceId {
    crate::canonical_workspace_identity(value).expect("测试用规范工作区标识必须能被源头解析")
}

#[cfg(test)]
mod tests {
    use computer_use::{
        ComputerUseError, ComputerUseResult, ComputerUseRetryOwner, ComputerUseStage,
        ComputerUseSurface, ComputerUseTerminalStatus, SupervisorSnapshot,
    };
    use rusqlite::Connection;
    use serde::Deserialize;

    use super::*;
    use crate::ConfigComputerUse;

    fn failed_result(code: &str) -> ComputerUseResult {
        ComputerUseResult {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("toolu-1".into()),
            status: ComputerUseTerminalStatus::Failed,
            stage: ComputerUseStage::Verification,
            goal_achieved: false,
            surface: ComputerUseSurface::Browser,
            summary: format!("failed: {code}"),
            error: Some(ComputerUseError::blocked(
                code,
                "failed",
                ComputerUseRetryOwner::None,
            )),
            attempts: 1,
            steps_completed: 1,
            evidence: vec!["after.png".into()],
            supervisor: SupervisorSnapshot::default(),
            // 仅补新字段以让测试夹具继续编译：本文件不参与 CU 预算事实的写入，
            // 运行记录里的 `cu_budget` / `cleanup` 由结果 JSON 原样序列化。
            cu_budget: None,
            cleanup: None,
        }
    }

    fn run() -> NewComputerUseRun {
        NewComputerUseRun {
            call_id: "cu-1".into(),
            provider_tool_call_id: Some("toolu-1".into()),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
            chat_room_id: Some("room-1".into()),
            idempotency_key: "stable-key".into(),
            objective_json: r#"{"objective":"submit"}"#.into(),
            surface: ComputerUseSurface::Browser,
            deadline_ms: 121_000,
            created_at_ms: 1_000,
            workspace: CuWorkspaceAttribution::test_fixture(),
        }
    }

    fn temp_store() -> ComputerUseRunStore {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();
        apply_session_migration_v22(&connection).unwrap();
        // RD4-01：收敛列/表由 v23 的函数体提供。本工单**不**自行抢占迁移版本号，也不动
        // `main.rs` 的阶梯，因此测试显式应用它；生产侧由统一维护人在阶梯里登记
        // （见 `apply_session_migration_v23_legacy_run_convergence` 的文档）。
        apply_session_migration_v23_legacy_run_convergence(&connection).unwrap();
        ComputerUseRunStore::from_connection(connection)
    }

    /// **只经 SQL** 造一条迁移之前那种"归属未记录"的行（`workspace_id IS NULL`）。
    ///
    /// 写入侧**不可能**造出这种行（`NewComputerUseRun::workspace` 非 `Option`），所以要覆盖
    /// 历史行的读回与互锁，只能这样替身。
    fn seed_legacy_run_row(store: &ComputerUseRunStore, call_id: &str, open: bool) {
        seed_legacy_unrecorded_run_for_test(store, call_id, "session-1", "turn-1", open);
    }

    #[test]
    fn diagnostic_redaction_keeps_invalid_action_shape_without_credentials_images_or_text() {
        let raw = r#"{"done":false,"action":"click","api_key":"SECRET","reasoning":"FULL-THOUGHT","image":{"data_url":"data:image/png;base64,IMAGE"},"arguments":{"text":"PRIVATE-TYPING","points":[[0,0],[1,1]]}}"#;
        let redacted = sanitized_action_json(raw);
        assert!(redacted.contains("\"action\":\"click\""));
        for secret in [
            "SECRET",
            "FULL-THOUGHT",
            "IMAGE",
            "PRIVATE-TYPING",
            "data:image",
        ] {
            assert!(!redacted.contains(secret));
        }
        assert!(serde_json::from_str::<serde_json::Value>(&redacted).is_ok());
        let invalid = sanitized_action_json("not-json PASSWORD");
        assert!(!invalid.contains("PASSWORD"));
        assert!(sanitized_action_json(&"x".repeat(20_000)).len() < 100);
        assert!(sanitized_action_json(r#"{"target":"window-canvas:1234"}"#)
            .contains("window-canvas:1234"));
        assert!(
            sanitized_action_json(r#"{"target":"window-canvas:abc123"}"#)
                .contains("window-canvas:abc123")
        );
        assert!(!sanitized_action_json(r#"{"target":"window-canvas:SECRET"}"#).contains("SECRET"));
    }

    #[test]
    fn config_defaults_match_the_controller_safety_contract() {
        let config = ConfigComputerUse::default();
        let budgets = config.budgets();

        assert!(config.enabled);
        assert_eq!(config.tool_mode, "task-controller");
        assert_eq!(config.approval_ttl_seconds, 180);
        assert_eq!(config.evidence_retention_days, 7);
        assert_eq!(budgets.max_actions, 12);
        assert_eq!(budgets.max_replans, 2);
        assert_eq!(budgets.max_same_signature, 2);
        assert_eq!(budgets.max_no_progress_steps, 2);
        assert_eq!(budgets.timeout_ms, 120_000);
        assert_eq!(budgets.max_calls_per_turn, 2);
        assert!(config.desktop.require_window_identity);
        assert!(config.desktop.block_webview2_surface_conflict);
        assert!(config.browser.allow_drag);
        assert!(config.browser.allow_key_combinations);
        assert!(config.browser.allow_multiple_tabs);
    }

    #[test]
    fn toml_overrides_are_loaded_and_unsafe_limits_are_clamped() {
        #[derive(Deserialize)]
        struct Wrapper {
            computer_use: ConfigComputerUse,
        }

        let wrapper: Wrapper = toml::from_str(
            r#"
                [computer_use]
                enabled = false
                tool_mode = "shadow"
                approval_ttl_seconds = 30
                evidence_retention_days = 2

                [computer_use.controller]
                max_actions = 0
                max_replans = 99
                max_same_signature = 0
                max_no_progress_steps = 99
                timeout_seconds = 9999
                max_calls_per_turn = 99

                [computer_use.desktop]
                enabled = false
                require_window_identity = false
                block_webview2_surface_conflict = false

                [computer_use.browser]
                enabled = true
                allow_drag = true
                allow_key_combinations = true
                allow_multiple_tabs = true
            "#,
        )
        .unwrap();
        let config = wrapper.computer_use;
        let budgets = config.budgets();

        assert!(!config.enabled);
        assert_eq!(config.tool_mode, "shadow");
        assert_eq!(config.approval_ttl_seconds, 30);
        assert_eq!(config.evidence_retention_days, 2);
        assert_eq!(budgets.max_actions, 1);
        assert_eq!(budgets.max_replans, 5);
        assert_eq!(budgets.max_same_signature, 1);
        assert_eq!(budgets.max_no_progress_steps, 3);
        assert_eq!(budgets.timeout_ms, 300_000);
        assert_eq!(budgets.max_calls_per_turn, 2);
        assert!(!config.desktop.enabled);
        assert!(config.browser.allow_drag);
    }

    #[test]
    fn migration_v11_creates_run_step_tables_and_indexes() {
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 11);
        for table in ["computer_use_runs", "computer_use_steps"] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing table {table}");
        }
        for index in [
            "idx_computer_use_runs_session_turn",
            "idx_computer_use_runs_turn_idempotency",
            "idx_computer_use_runs_state",
            "idx_computer_use_runs_updated",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing index {index}");
        }
    }

    #[test]
    fn migration_v12_adds_nullable_receipt_columns_without_rewriting_legacy_rows() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE computer_use_steps (
                    run_id TEXT NOT NULL,
                    step_index INTEGER NOT NULL,
                    observation_generation INTEGER NOT NULL,
                    action_type TEXT NOT NULL,
                    normalized_target TEXT NOT NULL,
                    action_fingerprint TEXT NOT NULL,
                    status TEXT NOT NULL,
                    error_code TEXT,
                    before_evidence_ref TEXT,
                    after_evidence_ref TEXT,
                    visible_progress INTEGER NOT NULL DEFAULT 0,
                    started_at_ms INTEGER NOT NULL,
                    completed_at_ms INTEGER,
                    PRIMARY KEY (run_id, step_index)
                );
                INSERT INTO computer_use_steps(
                    run_id, step_index, observation_generation, action_type, normalized_target,
                    action_fingerprint, status, visible_progress, started_at_ms
                ) VALUES ('legacy-run', 0, 0, 'click', 'target', 'fingerprint', 'completed_unverified', 0, 1);
                PRAGMA user_version = 11;
                "#,
            )
            .unwrap();

        apply_session_migration_v11(&connection).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 12);
        let values: (Option<String>, Option<bool>, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT input_delivery, partial, effect_status, input_release_status FROM computer_use_steps WHERE run_id='legacy-run'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(values, (None, None, None, None));
    }

    /// CU-F03（裁决 §5.1 / T13 的读回面）：v22 只加**可空**归属列；迁移之前写入的历史行
    /// 读回时必须呈现"历史归属未记录"，而且**不得**被回填成当前工作区。
    #[test]
    fn migration_v22_adds_nullable_workspace_columns_without_backfilling_legacy_rows() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE computer_use_runs (
                    call_id TEXT PRIMARY KEY,
                    provider_tool_call_id TEXT,
                    turn_id TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    chat_room_id TEXT,
                    idempotency_key TEXT NOT NULL,
                    objective_json TEXT NOT NULL,
                    surface TEXT NOT NULL,
                    state TEXT NOT NULL,
                    state_version INTEGER NOT NULL DEFAULT 0,
                    risk_class TEXT NOT NULL DEFAULT 'observe',
                    approval_state TEXT NOT NULL DEFAULT 'not_required',
                    approval_deadline_ms INTEGER,
                    action_count INTEGER NOT NULL DEFAULT 0,
                    replan_count INTEGER NOT NULL DEFAULT 0,
                    no_progress_count INTEGER NOT NULL DEFAULT 0,
                    current_observation_generation INTEGER NOT NULL DEFAULT 0,
                    deadline_ms INTEGER NOT NULL,
                    terminal_result_json TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                INSERT INTO computer_use_runs(
                    call_id, turn_id, session_id, chat_room_id, idempotency_key, objective_json,
                    surface, state, deadline_ms, created_at_ms, updated_at_ms
                ) VALUES ('legacy-run', 'turn-1', 'session-1', 'room-1', 'legacy-key',
                    '{"objective":"legacy"}', 'desktop', 'executing', 60_000, 1, 1);
                PRAGMA user_version = 21;
                "#,
            )
            .unwrap();

        apply_session_migration_v22(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 22);
        let store = ComputerUseRunStore::from_connection(connection);
        let stored = store.load("legacy-run").unwrap().expect("legacy row");
        assert_eq!(stored.workspace, StoredWorkspaceAttribution::Unrecorded);
        assert_eq!(stored.workspace.text(), "历史归属未记录");
        assert_eq!(stored.workspace.workspace_id(), None);
        assert_eq!(stored.workspace.context_version(), None);

        // 读回不是回填：原始列仍然是 NULL（没有任何路径能把它写成"当前工作区"）。
        let columns: (Option<String>, Option<i64>) = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT workspace_id, workspace_context_version FROM computer_use_runs \
                 WHERE call_id='legacy-run'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(columns, (None, None));

        // 这条历史行仍然是"未收尾 + 归属未记录"⇒ 属于 §5.1 第 6 条要阻断的旧活动运行。
        assert_eq!(
            store
                .unrecorded_workspace_active_runs("session-1", "turn-1", "another-call")
                .unwrap(),
            vec!["legacy-run".to_string()]
        );
    }

    /// 归属互锁只认"未收尾 **且** 归属未记录"的行：已记录归属的行、已收尾的行都不算。
    #[test]
    fn unrecorded_workspace_active_runs_ignores_recorded_and_finished_rows() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());
        seed_legacy_run_row(&store, "legacy-open", true);
        seed_legacy_run_row(&store, "legacy-finished", false);

        assert_eq!(
            store
                .unrecorded_workspace_active_runs("session-1", "turn-1", "cu-1")
                .unwrap(),
            vec!["legacy-open".to_string()]
        );
        // 新运行自己（归属已记录）永远不会被算成阻断理由。
        assert!(!store
            .unrecorded_workspace_active_runs("session-1", "turn-1", "cu-1")
            .unwrap()
            .contains(&"cu-1".to_string()));
    }

    /// 新写入的运行**必然**带归属与上下文版本；归属是写入侧的类型要求，不是可选项。
    #[test]
    fn created_runs_always_record_the_frozen_workspace_and_context_version() {
        let store = temp_store();
        let mut record = run();
        record.workspace =
            CuWorkspaceAttribution::from_parent_run(&source_workspace_id("ws-0123456789abcdef"));
        assert!(store.create_run(&record).unwrap());

        let stored = store.load("cu-1").unwrap().unwrap();
        assert_eq!(
            stored.workspace,
            StoredWorkspaceAttribution::Recorded {
                workspace_id: "ws-0123456789abcdef".to_string(),
                context_version: Some(CU_WORKSPACE_CONTEXT_VERSION),
            }
        );
        assert_eq!(
            stored.workspace.text(),
            format!("ws-0123456789abcdef（上下文版本 {CU_WORKSPACE_CONTEXT_VERSION}）")
        );
        // 幂等：同一条 run 再写一次不会命中，也就不会改写既有归属。
        assert!(!store.create_run(&record).unwrap());
        assert_eq!(
            store.load("cu-1").unwrap().unwrap().workspace.workspace_id(),
            Some("ws-0123456789abcdef")
        );
    }

    /// 独立打开 store（不经 main.rs 阶梯）也必须能补齐归属列：否则接纳处会因为缺列而写不进归属。
    #[test]
    fn store_open_upgrades_a_v11_database_and_records_the_workspace() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("web-sessions.sqlite3");
        {
            let connection = Connection::open(&path).unwrap();
            apply_session_migration_v11(&connection).unwrap();
            let version: i64 = connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(version, 12, "前置条件：v11 只把版本推到 12");
        }

        let store = ComputerUseRunStore::open(&path).unwrap();
        let mut record = run();
        record.workspace =
            CuWorkspaceAttribution::from_parent_run(&source_workspace_id("ws-0123456789abcdef"));
        assert!(store.create_run(&record).unwrap());
        drop(store);

        let reopened = ComputerUseRunStore::open(&path).unwrap();
        let stored = reopened.load("cu-1").unwrap().unwrap();
        assert_eq!(
            stored.workspace.workspace_id(),
            Some("ws-0123456789abcdef")
        );
        let version: i64 = reopened
            .connection
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 22);
    }

    /// A-2 后 CU 侧的职责只剩"原样记录已解析身份"：格式类拒绝与分类测试已搬到源头
    /// （`main.rs` 的 A2-T1/A2-T4），此处**不得**再出现"CU 自己判定形状"的用例。
    #[test]
    fn workspace_attribution_records_the_source_parsed_identity_verbatim() {
        let accepted = CuWorkspaceAttribution::from_parent_run(&source_workspace_id(
            "ws-0123456789abcdef",
        ));
        assert_eq!(accepted.workspace_id(), "ws-0123456789abcdef");
        assert_eq!(accepted.context_version(), CU_WORKSPACE_CONTEXT_VERSION);
        // 已解析值不做任何加工：解析只在源头发生一次。
        assert!(!accepted.workspace_id().contains(' '));
    }


    /// 步骤 + 事实必须同事务，且事实必须经**规则引擎**写入：规则引擎报错时整个事务回滚，
    /// 步骤更新不得生效（第二轮裁决第 2.2 项禁止"步骤已提交、回执另开连接写、第二步失败仍
    /// 宣布已保存"）。
    #[test]
    fn step_and_facts_commit_together_and_a_rule_engine_error_rolls_back_the_step() {
        use runtime::FactStore as _;
        let connection = Connection::open_in_memory().unwrap();
        apply_session_migration_v11(&connection).unwrap();
        apply_session_migration_v22(&connection).unwrap();
        crate::apply_session_migration_v21(&connection).unwrap();
        let store = ComputerUseRunStore::from_connection(connection);
        let mut record = run();
        record.call_id = "cu-fact".into();
        assert!(store.create_run(&record).unwrap());

        let step = ComputerUseStepRecord {
            run_id: "cu-fact".into(),
            step_index: 0,
            observation_generation: 1,
            action_type: "click".into(),
            normalized_target: "target".into(),
            action_fingerprint: "fingerprint".into(),
            status: "input_sent".into(),
            error_code: None,
            before_evidence_ref: None,
            after_evidence_ref: None,
            visible_progress: false,
            input_delivery: Some(InputDelivery::Sent),
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect_status: Some(EffectStatus::EffectObserved),
            goal_verdict: Some(GoalVerdict::Passed),
            input_release_status: Some(InputReleaseStatus::Released),
            started_at_ms: 10,
            completed_at_ms: Some(20),
        };
        let scope = runtime::RunScopeContext::new("ws", "room", "session", "turn");
        let turn_identity = scope.turn_fact("cu-fact");

        // ① 正常路径：步骤与终态事实同一事务提交（事实经 AppendOnlyFactStore 规则引擎）。
        store
            .record_step_with_facts(&step, "{}", |transaction| {
                let mut facts = runtime::AppendOnlyFactStore::open(
                    crate::fact_log_sqlite::SqliteFactLog::new(transaction),
                )
                .map_err(|error| error.to_string())?;
                facts
                    .record_host_outcome(&turn_identity, &runtime::HostRunOutcome::Completed)
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect("步骤与终态事实同事务提交");

        // ② 规则引擎报错（自相矛盾的回执：not_sent 与 partial=true 不可同时成立）
        //    → 整个事务回滚，步骤更新不得生效。
        let mut rolled_back_step = step.clone();
        rolled_back_step.status = "input_not_sent".into();
        let action_identity = scope.step_action_fact("cu-fact", "step-0", "attempt-0", "action-0");
        let contradictory = runtime::ActionReceipt {
            action_id: "action-0".into(),
            input_delivery: InputDelivery::NotSent,
            partial: Some(true),
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::NotNeeded,
        };
        let error = store
            .record_step_with_facts(&rolled_back_step, "{}", |transaction| {
                let mut facts = runtime::AppendOnlyFactStore::open(
                    crate::fact_log_sqlite::SqliteFactLog::new(transaction),
                )
                .map_err(|error| error.to_string())?;
                facts
                    .record_action_receipt(&action_identity, &contradictory)
                    .map_err(|error| error.to_string())?;
                Ok(())
            })
            .expect_err("规则引擎必须拒绝自相矛盾的回执");
        assert!(
            !error.is_empty(),
            "失败必须带出规则引擎的原因，而不是空错误"
        );

        let connection = store.connection.lock().unwrap();
        let status: Option<String> = connection
            .query_row(
                "SELECT status FROM computer_use_steps WHERE run_id='cu-fact' AND step_index=0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            status.as_deref(),
            Some("input_sent"),
            "规则引擎报错回滚后，步骤更新不得生效"
        );
        let facts: i64 = connection
            .query_row("SELECT COUNT(*) FROM fact_log_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(facts, 1, "回滚不得留下多余的步骤或事实记录");
    }

    #[test]
    fn step_receipt_projection_records_only_explicit_input_fact() {
        let store = temp_store();
        let mut run = run();
        run.call_id = "cu-receipt".into();
        assert!(store.create_run(&run).unwrap());
        let step = ComputerUseStepRecord {
            run_id: "cu-receipt".into(),
            step_index: 0,
            observation_generation: 1,
            action_type: "click".into(),
            normalized_target: "target".into(),
            action_fingerprint: "fingerprint".into(),
            status: "input_sent".into(),
            error_code: None,
            before_evidence_ref: None,
            after_evidence_ref: None,
            visible_progress: false,
            input_delivery: Some(InputDelivery::Sent),
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect_status: None,
            goal_verdict: None,
            input_release_status: None,
            started_at_ms: 1,
            completed_at_ms: Some(2),
        };
        assert!(store.append_step(&step).unwrap());
        let values: (String, Option<bool>, Option<String>, Option<String>) = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT input_delivery, partial, effect_status, input_release_status FROM computer_use_steps WHERE run_id='cu-receipt'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(values, ("sent".to_string(), None, None, None));
    }

    #[test]
    fn failed_verification_marks_effect_inconclusive_instead_of_claiming_no_effect() {
        let store = temp_store();
        let mut run = run();
        run.call_id = "cu-inconclusive".into();
        assert!(store.create_run(&run).unwrap());
        let step = ComputerUseStepRecord {
            run_id: "cu-inconclusive".into(),
            step_index: 0,
            observation_generation: 1,
            action_type: "click".into(),
            normalized_target: "target".into(),
            action_fingerprint: "fingerprint".into(),
            status: "input_sent_observed".into(),
            error_code: None,
            before_evidence_ref: None,
            after_evidence_ref: Some("[]".into()),
            visible_progress: false,
            input_delivery: Some(InputDelivery::Sent),
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect_status: None,
            goal_verdict: None,
            input_release_status: None,
            started_at_ms: 1,
            completed_at_ms: Some(2),
        };
        assert!(store.append_step(&step).unwrap());
        store
            .record_step_verification(
                "cu-inconclusive",
                1,
                &computer_use::Verification {
                    achieved: false,
                    visible_progress: false,
                    summary: "缺少充分观察".into(),
                    evidence: vec!["verification-inconclusive".into()],
                },
            )
            .unwrap();
        let values: (String, String) = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT effect_status, goal_verdict FROM computer_use_steps WHERE run_id='cu-inconclusive'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(values, ("inconclusive".to_string(), "failed".to_string()));
    }

    #[test]
    fn session_schema_initialization_applies_migration_v11() {
        let connection = Connection::open_in_memory().unwrap();
        crate::initialize_session_schema(&connection).unwrap();

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 11);
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='computer_use_runs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    #[test]
    fn transition_rejects_an_outdated_state_version() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());

        assert!(store
            .transition(
                "cu-1",
                0,
                computer_use::ComputerUseRunState::Observing,
                1_001
            )
            .unwrap());
        assert!(!store
            .transition(
                "cu-1",
                0,
                computer_use::ComputerUseRunState::Planning,
                1_002
            )
            .unwrap());

        let stored = store.load("cu-1").unwrap().unwrap();
        assert_eq!(stored.state, computer_use::ComputerUseRunState::Observing);
        assert_eq!(stored.state_version, 1);
        assert_eq!(stored.chat_room_id.as_deref(), Some("room-1"));
    }

    #[test]
    fn turn_run_count_supports_recursive_call_watchdog() {
        let store = temp_store();
        assert_eq!(store.count_runs_for_turn("session-1", "turn-1").unwrap(), 0);
        assert!(store.create_run(&run()).unwrap());
        assert_eq!(store.count_runs_for_turn("session-1", "turn-1").unwrap(), 1);
        assert_eq!(
            store
                .count_runs_for_turn("session-1", "turn-other")
                .unwrap(),
            0
        );
    }

    #[test]
    fn load_by_idempotency_key_returns_existing_terminal_run_for_same_turn_task() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());
        assert!(store
            .finish("cu-1", 0, &failed_result("target_not_found"))
            .unwrap());

        let existing = store
            .load_by_idempotency_key("session-1", "turn-1", "stable-key")
            .unwrap()
            .expect("same task should be found by idempotency key");

        assert_eq!(existing.call_id, "cu-1");
        assert_eq!(
            existing.terminal_result.unwrap().error.unwrap().code,
            "target_not_found"
        );
    }

    #[test]
    fn terminal_result_is_written_once() {
        let store = temp_store();
        assert!(store.create_run(&run()).unwrap());
        assert!(store.finish("cu-1", 0, &failed_result("x")).unwrap());
        assert!(!store.finish("cu-1", 0, &failed_result("y")).unwrap());

        let stored = store.load("cu-1").unwrap().unwrap();
        assert_eq!(stored.state_version, 1);
        assert_eq!(stored.state, computer_use::ComputerUseRunState::Failed);
        assert_eq!(stored.terminal_result.unwrap().error.unwrap().code, "x");
    }

    // ---- RPR-05b-1：未确认释放的判据与人工解除事实 ----

    fn seed_release_step(
        store: &ComputerUseRunStore,
        session_id: &str,
        turn_id: &str,
        run_id: &str,
        delivery: Option<InputDelivery>,
        release: Option<InputReleaseStatus>,
    ) {
        assert!(store
            .create_run(&NewComputerUseRun {
                call_id: run_id.into(),
                provider_tool_call_id: Some(format!("tool-{run_id}")),
                session_id: session_id.into(),
                turn_id: turn_id.into(),
                chat_room_id: Some("room-1".into()),
                idempotency_key: format!("seeded-{run_id}"),
                objective_json: "{\"objective\":\"seeded\"}".into(),
                surface: ComputerUseSurface::Desktop,
                deadline_ms: 60_000,
                created_at_ms: 1,
                workspace: CuWorkspaceAttribution::test_fixture(),
            })
            .unwrap());
        assert!(store
            .append_step(&ComputerUseStepRecord {
                run_id: run_id.into(),
                step_index: 0,
                observation_generation: 1,
                action_type: "click".into(),
                normalized_target: "seeded-target".into(),
                action_fingerprint: "0000000000000000".into(),
                status: "failed".into(),
                error_code: Some("mouse_release_failed".into()),
                before_evidence_ref: None,
                after_evidence_ref: None,
                visible_progress: false,
                input_delivery: delivery,
                partial: None,
                path_completed: None,
                confirmed_point_count: None,
                effect_status: None,
                goal_verdict: None,
                input_release_status: release,
                started_at_ms: 5,
                completed_at_ms: Some(6),
            })
            .unwrap());
    }

    /// step 行的历史快照：解除**不得**改写其中任何一个字段。
    fn step_snapshot(
        store: &ComputerUseRunStore,
        run_id: &str,
    ) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
        let connection = store.connection.lock().unwrap();
        let mut statement = connection
            .prepare(
                "SELECT status, error_code, input_delivery, input_release_status
                 FROM computer_use_steps WHERE run_id = ?1 ORDER BY step_index",
            )
            .unwrap();
        statement
            .query_map([run_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn run_snapshot(store: &ComputerUseRunStore, run_id: &str) -> (String, Option<String>, u64) {
        store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT state, terminal_result_json, state_version FROM computer_use_runs
                 WHERE call_id = ?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    }

    fn resolution<'a>(
        session_id: &'a str,
        turn_id: &'a str,
    ) -> ReleaseResolutionRequest<'a> {
        ReleaseResolutionRequest {
            session_id,
            turn_id,
            operator_source: ReleaseResolutionOperator::NativeUi,
            operator_id: Some("local-operator"),
            reason: "已在桌面确认残留按键已抬起并释放",
            operator_check_note: "肉眼确认鼠标左键与键盘修饰键均未处于按下状态",
            input_owner_epoch_before: Some(7),
            input_owner_epoch_after: Some(8),
        }
    }

    /// 判据矩阵：只有"释放 unknown 且投递不是 not_sent"的 step 才构成未确认释放。
    ///
    /// 同时锁死判据与源码字面量的一致性：SQL 里的两个取值都来自
    /// `input_release_status_name` / `input_delivery_name`（也就是 append_step
    /// 真正写库时用的同一对函数），不允许出现手工维护的魔法字符串。
    #[test]
    fn unconfirmed_release_judgement_only_matches_unknown_release_with_unproven_input() {
        let cases = [
            (
                Some(InputDelivery::MayHaveBeenSent),
                Some(InputReleaseStatus::Unknown),
                true,
            ),
            (Some(InputDelivery::Sent), Some(InputReleaseStatus::Unknown), true),
            // 投递事实读不出来（NULL）不比 may_have_been_sent 更安全：按保守方向阻断。
            (None, Some(InputReleaseStatus::Unknown), true),
            (
                Some(InputDelivery::NotSent),
                Some(InputReleaseStatus::Unknown),
                false,
            ),
            (
                Some(InputDelivery::NotSent),
                Some(InputReleaseStatus::NotNeeded),
                false,
            ),
            (
                Some(InputDelivery::Sent),
                Some(InputReleaseStatus::Released),
                false,
            ),
            // 旧行根本没有释放事实：NULL != 'unknown'，不构成"未确认释放"。
            (Some(InputDelivery::Sent), None, false),
            (
                Some(InputDelivery::MayHaveBeenSent),
                Some(InputReleaseStatus::Released),
                false,
            ),
        ];
        for (index, (delivery, release, expected)) in cases.into_iter().enumerate() {
            let store = temp_store();
            let run_id = format!("cu-case-{index}");
            seed_release_step(&store, "session-1", "turn-1", &run_id, delivery, release);
            assert_eq!(
                store.has_unconfirmed_release("session-1", "turn-1").unwrap(),
                expected,
                "case {index}: {delivery:?}/{release:?}"
            );
            let facts = store.unconfirmed_release_facts("session-1", "turn-1").unwrap();
            assert_eq!(facts.run_ids.len(), usize::from(expected));
            assert_eq!(facts.step_count, usize::from(expected));
            assert_eq!(facts.oldest_step_started_at_ms, expected.then_some(5));
            // scope 隔离：别的 session / turn 不受影响。
            assert!(!store.has_unconfirmed_release("session-2", "turn-1").unwrap());
            assert!(!store.has_unconfirmed_release("session-1", "turn-2").unwrap());
        }
        // 判据引用的字面量必须与源码写入的取值一致。
        assert_eq!(
            input_release_status_name(InputReleaseStatus::Unknown),
            "unknown"
        );
        assert_eq!(input_delivery_name(InputDelivery::NotSent), "not_sent");
    }

    /// 审计字段齐全：操作者来源、时间、前置/后置检查、理由、旧/新 epoch 一个不少，
    /// 且历史事实（step 与 run 行）在解除前后**逐字段相同**。
    #[test]
    fn release_resolution_records_complete_audit_facts_without_rewriting_history() {
        let store = temp_store();
        // 两个未确认释放的 run（其一有 partial 之类无关字段也不影响），外加一条对照行。
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-unresolved-a",
            Some(InputDelivery::MayHaveBeenSent),
            Some(InputReleaseStatus::Unknown),
        );
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-unresolved-b",
            Some(InputDelivery::Sent),
            Some(InputReleaseStatus::Unknown),
        );
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-control-not-sent",
            Some(InputDelivery::NotSent),
            Some(InputReleaseStatus::NotNeeded),
        );
        let steps_before = [
            "cu-unresolved-a",
            "cu-unresolved-b",
            "cu-control-not-sent",
        ]
        .map(|run_id| step_snapshot(&store, run_id));
        let runs_before = run_snapshot(&store, "cu-unresolved-a");

        let outcome = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap();
        assert!(outcome.recorded);
        assert_eq!(outcome.previous_epoch, 0);
        assert_eq!(outcome.epoch, 1);
        assert_eq!(
            outcome.covered_run_ids,
            vec!["cu-unresolved-a".to_string(), "cu-unresolved-b".to_string()]
        );
        assert_eq!(outcome.covered_step_count, 2);
        assert_eq!(outcome.remaining_unconfirmed_runs, 0);
        assert!(outcome.created_at_ms > 0);
        assert!(!store.has_unconfirmed_release("session-1", "turn-1").unwrap());

        let stored = store.release_resolutions("session-1", "turn-1").unwrap();
        assert_eq!(stored.len(), 1);
        let record = &stored[0];
        assert_eq!(record.resolution_id, outcome.resolution_id.unwrap());
        assert_eq!(record.operator_source, "native_ui");
        assert_eq!(record.operator_id.as_deref(), Some("local-operator"));
        assert_eq!(record.reason, "已在桌面确认残留按键已抬起并释放");
        assert_eq!(
            record.operator_check_note.as_deref(),
            Some("肉眼确认鼠标左键与键盘修饰键均未处于按下状态")
        );
        assert_eq!(record.input_owner_epoch_before, Some(7));
        assert_eq!(record.input_owner_epoch_after, Some(8));
        assert_eq!(record.covered_run_count, 2);
        assert_eq!(record.covered_step_count, 2);
        assert_eq!(record.covered_run_ids, outcome.covered_run_ids);
        let precheck: serde_json::Value = serde_json::from_str(&record.precheck_json).unwrap();
        assert_eq!(
            precheck["detected_run_ids"],
            serde_json::json!(["cu-unresolved-a", "cu-unresolved-b"])
        );
        assert_eq!(precheck["detected_step_count"], 2);
        assert_eq!(precheck["input_owner_epoch_before"], 7);
        assert_eq!(precheck["oldest_step_started_at_ms"], 5);
        let postcheck: serde_json::Value = serde_json::from_str(&record.postcheck_json).unwrap();
        assert_eq!(postcheck["clear"], true);
        assert_eq!(postcheck["remaining_run_ids"], serde_json::json!([]));
        assert_eq!(postcheck["input_owner_epoch_after"], 8);

        // 历史从未被改写：step 行与 run 行逐字段相同，unknown 仍是 unknown。
        for (run_id, before) in [
            "cu-unresolved-a",
            "cu-unresolved-b",
            "cu-control-not-sent",
        ]
        .into_iter()
        .zip(steps_before)
        {
            assert_eq!(step_snapshot(&store, run_id), before, "{run_id} 的 step 行被改写了");
        }
        assert_eq!(step_snapshot(&store, "cu-unresolved-a")[0].3.as_deref(), Some("unknown"));
        assert_eq!(run_snapshot(&store, "cu-unresolved-a"), runs_before);
    }

    /// 解除按 scope 隔离，epoch 在同一 scope 内单调递增：新事实解除后是下一个 epoch，
    /// 别的 session/turn 的解除不会清掉本 scope。
    #[test]
    fn release_resolutions_are_scoped_and_epochs_are_monotonic() {
        let store = temp_store();
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-epoch-1",
            Some(InputDelivery::MayHaveBeenSent),
            Some(InputReleaseStatus::Unknown),
        );
        seed_release_step(
            &store,
            "session-1",
            "turn-2",
            "cu-other-turn",
            Some(InputDelivery::MayHaveBeenSent),
            Some(InputReleaseStatus::Unknown),
        );

        let first = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap();
        assert_eq!((first.previous_epoch, first.epoch), (0, 1));
        // 另一个 turn 的解除不影响本 scope 的事实。
        let other = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-2"))
            .unwrap();
        assert_eq!((other.previous_epoch, other.epoch), (0, 1));
        assert_eq!(other.covered_run_ids, vec!["cu-other-turn".to_string()]);
        assert!(!store.has_unconfirmed_release("session-1", "turn-1").unwrap());

        // 解除后又有新的未确认释放 → 新 epoch，且旧覆盖清单不重复计算。
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-epoch-2",
            Some(InputDelivery::Sent),
            Some(InputReleaseStatus::Unknown),
        );
        assert!(store.has_unconfirmed_release("session-1", "turn-1").unwrap());
        let second = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap();
        assert_eq!((second.previous_epoch, second.epoch), (1, 2));
        assert_eq!(second.covered_run_ids, vec!["cu-epoch-2".to_string()]);
        let epochs: Vec<u64> = store
            .release_resolutions("session-1", "turn-1")
            .unwrap()
            .iter()
            .map(|record| record.epoch)
            .collect();
        assert_eq!(epochs, vec![1, 2]);
        assert!(!store.has_unconfirmed_release("session-1", "turn-1").unwrap());
    }

    /// 没有可解除的事实时不落库、不推进 epoch：审计里不该出现"解除了一条不存在的隔离"。
    #[test]
    fn release_resolution_is_a_noop_when_nothing_is_unresolved() {
        let store = temp_store();
        let outcome = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap();
        assert!(!outcome.recorded);
        assert_eq!(outcome.resolution_id, None);
        assert_eq!(outcome.previous_epoch, 0);
        assert_eq!(outcome.epoch, 0);
        assert!(outcome.covered_run_ids.is_empty());
        assert!(store.release_resolutions("session-1", "turn-1").unwrap().is_empty());

        // 只有 not_sent 的对照行同样不构成可解除事实。
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-nothing",
            Some(InputDelivery::NotSent),
            Some(InputReleaseStatus::NotNeeded),
        );
        let outcome = store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap();
        assert!(!outcome.recorded);
        assert!(store.release_resolutions("session-1", "turn-1").unwrap().is_empty());
    }

    /// 解除必须带人工理由与前置现场检查说明：缺失即拒绝，不写半条事实。
    #[test]
    fn release_resolution_requires_a_reason_and_an_operator_check_note() {        let store = temp_store();
        seed_release_step(
            &store,
            "session-1",
            "turn-1",
            "cu-requires-reason",
            Some(InputDelivery::MayHaveBeenSent),
            Some(InputReleaseStatus::Unknown),
        );
        let mut request = resolution("session-1", "turn-1");
        request.reason = "   ";
        assert!(store.resolve_unconfirmed_release(&request).is_err());
        let mut request = resolution("session-1", "turn-1");
        request.operator_check_note = "";
        assert!(store.resolve_unconfirmed_release(&request).is_err());
        let mut request = resolution("  ", "turn-1");
        request.reason = "理由";
        assert!(store.resolve_unconfirmed_release(&request).is_err());
        // 拒绝之后事实仍在：不产生"半条解除记录"。
        assert!(store.has_unconfirmed_release("session-1", "turn-1").unwrap());
        assert!(store.release_resolutions("session-1", "turn-1").unwrap().is_empty());
    }

    /// 升级路径：已有 `computer_use_runs/steps` 的旧库补建解除事实表之后，
    /// 互锁判据必须立即可用（不出现 `no such table`），历史行保持原样。
    #[test]
    fn release_resolution_tables_are_added_to_an_existing_database_without_rewriting_rows() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE computer_use_runs (
                    call_id TEXT PRIMARY KEY, provider_tool_call_id TEXT, turn_id TEXT NOT NULL,
                    session_id TEXT NOT NULL, chat_room_id TEXT, idempotency_key TEXT NOT NULL,
                    objective_json TEXT NOT NULL, surface TEXT NOT NULL, state TEXT NOT NULL,
                    state_version INTEGER NOT NULL DEFAULT 0,
                    risk_class TEXT NOT NULL DEFAULT 'observe',
                    approval_state TEXT NOT NULL DEFAULT 'not_required',
                    approval_deadline_ms INTEGER, action_count INTEGER NOT NULL DEFAULT 0,
                    replan_count INTEGER NOT NULL DEFAULT 0,
                    no_progress_count INTEGER NOT NULL DEFAULT 0,
                    current_observation_generation INTEGER NOT NULL DEFAULT 0,
                    deadline_ms INTEGER NOT NULL, terminal_result_json TEXT,
                    created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE computer_use_steps (
                    run_id TEXT NOT NULL, step_index INTEGER NOT NULL,
                    observation_generation INTEGER NOT NULL, action_type TEXT NOT NULL,
                    normalized_target TEXT NOT NULL, action_fingerprint TEXT NOT NULL,
                    status TEXT NOT NULL, error_code TEXT, before_evidence_ref TEXT,
                    after_evidence_ref TEXT, visible_progress INTEGER NOT NULL DEFAULT 0,
                    started_at_ms INTEGER NOT NULL, completed_at_ms INTEGER,
                    PRIMARY KEY (run_id, step_index)
                );
                INSERT INTO computer_use_runs(
                    call_id, turn_id, session_id, idempotency_key, objective_json, surface,
                    state, deadline_ms, created_at_ms, updated_at_ms
                ) VALUES ('cu-legacy', 'turn-1', 'session-1', 'legacy-key', '{}', 'desktop',
                          'blocked', 1, 1, 1);
                INSERT INTO computer_use_steps(
                    run_id, step_index, observation_generation, action_type, normalized_target,
                    action_fingerprint, status, visible_progress, started_at_ms
                ) VALUES ('cu-legacy', 0, 1, 'click', 'legacy-target', 'fp', 'failed', 0, 1);
                PRAGMA user_version = 12;
                "#,
            )
            .unwrap();
        let has_release_table = |name: &str| -> bool {
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [name],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert!(
            !has_release_table("computer_use_release_resolutions"),
            "前置条件：升级前不该已存在解除事实表"
        );

        apply_session_migration_v11(&connection).unwrap();

        for table in [
            "computer_use_release_resolutions",
            "computer_use_release_resolution_runs",
        ] {
            assert!(has_release_table(table), "升级必须补建 {table}");
        }
        for index in [
            "idx_computer_use_release_resolutions_scope_epoch",
            "idx_computer_use_release_resolutions_scope_created",
            "idx_computer_use_release_resolution_runs_run",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "升级必须补建索引 {index}");
        }
        // 历史行保持 NULL：没有原始回执就不许倒灌猜测值。
        let legacy: (Option<String>, Option<String>) = connection
            .query_row(
                "SELECT input_delivery, input_release_status FROM computer_use_steps
                 WHERE run_id='cu-legacy' AND step_index=0",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(legacy, (None, None));

        // 升级后的旧库上写入一条未确认释放：判据立即可用。
        connection
            .execute(
                "INSERT INTO computer_use_steps(
                    run_id, step_index, observation_generation, action_type, normalized_target,
                    action_fingerprint, status, input_delivery, input_release_status, started_at_ms
                ) VALUES ('cu-legacy', 1, 1, 'click', 'legacy-target', 'fp', 'failed',
                          'may_have_been_sent', 'unknown', 2)",
                [],
            )
            .unwrap();
        let store = ComputerUseRunStore::from_connection(connection);
        let facts = store
            .unconfirmed_release_facts("session-1", "turn-1")
            .unwrap();
        assert_eq!(facts.run_ids, vec!["cu-legacy".to_string()]);
        assert_eq!(facts.step_count, 1, "只算真正未确认释放的那一行");
        assert!(store.has_unconfirmed_release("session-1", "turn-1").unwrap());
        // 解除之后判据清空，而历史行仍然原样。
        assert!(store
            .resolve_unconfirmed_release(&resolution("session-1", "turn-1"))
            .unwrap()
            .recorded);
        assert!(!store.has_unconfirmed_release("session-1", "turn-1").unwrap());
        let step_snapshot = step_snapshot(&store, "cu-legacy");
        assert_eq!(step_snapshot[0].3, None);
        assert_eq!(step_snapshot[1].3.as_deref(), Some("unknown"));
    }

    // -----------------------------------------------------------------------
    // RD4-01：遗留 CU 运行收敛（存储编解码 / 幂等 / 不覆盖 / 三维度分离）
    // -----------------------------------------------------------------------

    /// 收敛请求的固定输入：控制=已终止（由 store 写）、历史=未知、资源=仍隔离。
    struct LegacyConvergenceFixture {
        evidence: LegacyCuRunConvergenceEvidence,
        historical_outcome: LegacyRunHistoricalOutcome,
        input_resource: LegacyRunInputResourceState,
        historical_resource_scope: LegacyResourceScope,
        current_resource_safety_check: CurrentResourceSafetyCheck,
        resource_blocking_refs: Vec<String>,
    }

    impl LegacyConvergenceFixture {
        /// 资源**仍隔离**：存在未结清释放义务、旧执行者是否存在未知、已有资源事故阻断。
        fn resource_isolated() -> Self {
            Self {
                evidence: LegacyCuRunConvergenceEvidence {
                    owner_checks: vec!["owner-lease: none".into()],
                    ..LegacyCuRunConvergenceEvidence::default()
                },
                historical_outcome: LegacyRunHistoricalOutcome::Unknown,
                input_resource: LegacyRunInputResourceState {
                    old_executor_may_be_present: None,
                    unconfirmed_release_obligations: 1,
                    blocking_event_refs: vec!["input-safety:incident-9".into()],
                    safe_for_new_input: false,
                },
                historical_resource_scope: LegacyResourceScope::Unrecorded,
                current_resource_safety_check: CurrentResourceSafetyCheck {
                    basis:
                        runtime::CurrentResourceCandidateBasis::CurrentMayBeAffectedNotHistoricalScope,
                    session_id: "session-1".into(),
                    turn_id: "turn-1".into(),
                    checked_at_unix_ms: 4_900,
                },
                resource_blocking_refs: vec!["input-safety:incident-9".into()],
            }
        }

        /// 资源已独立检查为可安全接纳新输入（维度③的**另一次**观测）。
        fn resource_clear() -> Self {
            let mut fixture = Self::resource_isolated();
            fixture.input_resource.unconfirmed_release_obligations = 0;
            fixture.input_resource.old_executor_may_be_present = Some(false);
            fixture.input_resource.blocking_event_refs.clear();
            fixture.input_resource.safe_for_new_input = true;
            fixture.resource_blocking_refs.clear();
            fixture
        }

        fn request<'a>(
            &'a self,
            call_id: &'a str,
            recovery_operation_id: &'a str,
            expected_original_state: &'a str,
            expected_original_state_version: u64,
            reconciled_at_ms: u64,
        ) -> LegacyRunConvergenceRequest<'a> {
            LegacyRunConvergenceRequest {
                source_database_identity: "session-db:sessions.sqlite3",
                source_database_identity_registered_now: false,
                call_id,
                expected_original_state,
                expected_original_state_version,
                observed_commit_candidate: false,
                recovery_operation_id,
                recovery_service_instance: "recovery-1",
                recovery_control_authority: "single-writer-claim:recovery-1",
                new_input_intake_paused: true,
                operated_at_ms: reconciled_at_ms.saturating_sub(100),
                reconciled_at_ms,
                resource_blocking_refs: &self.resource_blocking_refs,
                evidence: &self.evidence,
                historical_outcome: &self.historical_outcome,
                input_resource: &self.input_resource,
                historical_resource_scope: &self.historical_resource_scope,
                current_resource_safety_check: &self.current_resource_safety_check,
            }
        }
    }

    /// 往遗留行上追加一条"未确认释放"的 step（资源维度的独立事实来源）。
    fn append_legacy_release_step(store: &ComputerUseRunStore, run_id: &str) {
        let step = ComputerUseStepRecord {
            run_id: run_id.into(),
            step_index: 0,
            observation_generation: 1,
            action_type: "click".into(),
            normalized_target: "legacy-target".into(),
            action_fingerprint: "0000000000000000".into(),
            status: "input_sent".into(),
            error_code: None,
            before_evidence_ref: None,
            after_evidence_ref: None,
            visible_progress: false,
            input_delivery: Some(InputDelivery::Sent),
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect_status: None,
            goal_verdict: None,
            input_release_status: Some(InputReleaseStatus::Unknown),
            started_at_ms: 5,
            completed_at_ms: None,
        };
        // 走 `record_step`（步骤行 + 动作回执一起写），这样"回执原样保留"也被覆盖。
        store
            .record_step(&step, r#"{"action":"click","target":"legacy-target"}"#)
            .unwrap();
    }

    /// 历史事实的**行数快照**：收敛不得新增/删除/改写任何一张历史表。
    fn history_row_counts(store: &ComputerUseRunStore) -> Vec<(String, i64)> {
        let connection = store.connection.lock().unwrap();
        [
            "computer_use_steps",
            "computer_use_step_details",
            "computer_use_release_resolutions",
            "computer_use_release_resolution_runs",
            "computer_use_planner_diagnostics",
        ]
        .iter()
        .map(|table| {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                .unwrap();
            ((*table).to_string(), count)
        })
        .collect()
    }

    /// 收敛后：控制维度被终止，而**历史列与资源维度一个字都不动**。
    #[test]
    fn legacy_convergence_terminates_control_state_without_touching_history_or_resources() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-run", true);
        append_legacy_release_step(&store, "legacy-run");
        let run_before = run_snapshot(&store, "legacy-run");
        let steps_before = step_snapshot(&store, "legacy-run");
        let history_before = history_row_counts(&store);
        let release_before = store
            .unconfirmed_release_facts("session-1", "turn-1")
            .unwrap();
        assert!(!release_before.is_empty(), "前置：资源处于未确认释放状态");
        assert!(store
            .unrecorded_workspace_active_runs("session-1", "turn-1", "other")
            .unwrap()
            .contains(&"legacy-run".to_string()));

        let fixture = LegacyConvergenceFixture::resource_isolated();
        let request = fixture.request("legacy-run", "recovery-op-1", "executing", 1, 5_000);
        match store.converge_legacy_run(&request).unwrap() {
            LegacyRunConvergenceOutcome::Converged {
                may_reopen_new_input,
                reconciled_at_ms,
                ..
            } => {
                assert_eq!(reconciled_at_ms, 5_000);
                assert!(
                    !may_reopen_new_input,
                    "资源仍隔离时不得开放新输入（控制终态不构成理由）"
                );
            }
            other => panic!("预期 Converged，实际 {other:?}"),
        }

        // 维度①：控制状态被终止。
        assert_eq!(
            store.legacy_run_control_state("legacy-run").unwrap(),
            StoredLegacyRunControlState::Interrupted
        );
        // 历史列一行不动（state / terminal_result_json / state_version）。
        assert_eq!(run_snapshot(&store, "legacy-run"), run_before);
        // 历史步骤原样保留。
        assert_eq!(step_snapshot(&store, "legacy-run"), steps_before);
        // 历史表（步骤 / 动作回执 / 解除事实 / 诊断）行数与内容一处不差：
        // 收敛只**追加**一条收敛事实与一行收敛列，不改写任何历史。
        assert_eq!(history_row_counts(&store), history_before);
        // 工作区归属仍是 NULL（未回填）。
        let stored = store.load("legacy-run").unwrap().unwrap();
        assert_eq!(stored.workspace, StoredWorkspaceAttribution::Unrecorded);
        assert_eq!(stored.workspace.workspace_id(), None);
        // 维度③：资源状态**不因**终态清零。
        assert_eq!(
            store
                .unconfirmed_release_facts("session-1", "turn-1")
                .unwrap(),
            release_before
        );
        assert!(store
            .unrecorded_workspace_active_runs("session-1", "turn-1", "other")
            .unwrap()
            .contains(&"legacy-run".to_string()));

        // 收敛事实逐项核对（读回路径）。
        let stored_fact = store
            .latest_legacy_run_convergence("legacy-run")
            .unwrap()
            .expect("收敛事实必须能读回");
        let fact = &stored_fact.fact;
        assert_eq!(
            fact.decision.reason_code,
            runtime::LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE
        );
        assert_eq!(
            fact.decision.explanation,
            runtime::LEGACY_CU_RUN_CLOSED_AT_RECOVERY_EXPLANATION
        );
        assert_eq!(
            fact.decision.terminal_status,
            runtime::RunTerminalStatus::Interrupted
        );
        for wording in runtime::LEGACY_CU_RUN_FORBIDDEN_CLAIM_WORDINGS {
            assert!(
                !fact.decision.explanation.contains(wording),
                "解释文案不得包含 `{wording}`"
            );
        }
        // 时间：本次对账时刻是本次时间；原实际执行结束时刻**仍为未知**。
        assert_eq!(fact.reconciled_at_unix_ms, 5_000);
        assert_eq!(fact.original_execution_ended_at_unix_ms(), None);
        // 缺失维度与三维度各自独立。
        assert!(fact.missing.workspace_id_unrecorded);
        assert_eq!(fact.original.original_state, "executing");
        assert_eq!(fact.original.original_state_version, 1);
        assert!(fact.historical_outcome.is_unknown());
        assert!(fact.historical_resource_scope.is_unrecorded());
        assert_eq!(fact.input_resource.unconfirmed_release_obligations, 1);
        assert!(!fact.reopen_new_input_is_allowed());
        // 副作用限制：不复活、不重放、不推断目标成功。
        assert!(!fact.side_effects.revives_run());
        assert!(!fact.side_effects.claims_goal_completion());
        // 收敛意图已经对账完成。
        assert!(store
            .pending_legacy_run_convergence_intents()
            .unwrap()
            .is_empty());
    }

    /// 幂等：同一条收敛重复请求不写第二份，也不改写旧 run 行。
    #[test]
    fn legacy_convergence_is_idempotent_and_never_writes_a_second_fact() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-run", true);
        let fixture = LegacyConvergenceFixture::resource_isolated();
        let request = fixture.request("legacy-run", "recovery-op-1", "executing", 1, 5_000);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged { .. }
        ));
        let run_after_first = run_snapshot(&store, "legacy-run");
        assert_eq!(store.legacy_run_convergences().unwrap().len(), 1);

        // 重复启动 / 双实例：换一个恢复操作 ID、换一个对账时刻，同一条收敛。
        let repeat = fixture.request("legacy-run", "recovery-op-2", "executing", 1, 6_000);
        assert!(matches!(
            store.converge_legacy_run(&repeat).unwrap(),
            LegacyRunConvergenceOutcome::AlreadyConverged { .. }
        ));
        // 逐字节相同的重复请求同样幂等。
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::AlreadyConverged { .. }
        ));
        assert_eq!(
            store.legacy_run_convergences().unwrap().len(),
            1,
            "不得写第二份收敛事实"
        );
        assert_eq!(
            run_snapshot(&store, "legacy-run"),
            run_after_first,
            "幂等请求不得改写旧 run 行"
        );
    }

    /// 不覆盖：已有真实终态提交 / 待确认提交候选的行不被合成 `Interrupted`。
    #[test]
    fn legacy_convergence_never_overwrites_a_real_terminal_or_a_commit_candidate() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-terminal", true);
        let fixture = LegacyConvergenceFixture::resource_isolated();
        // 真实终态提交：把这一行正常收尾成失败。
        assert!(store
            .finish("legacy-terminal", 1, &failed_result("execution_failed"))
            .unwrap());
        let terminal_snapshot = run_snapshot(&store, "legacy-terminal");
        let request = fixture.request("legacy-terminal", "recovery-op-1", "executing", 1, 5_000);
        match store.converge_legacy_run(&request).unwrap() {
            LegacyRunConvergenceOutcome::Refused(
                LegacyCuRunConvergenceRefusal::AlreadyHasRealTerminal { .. },
            ) => {}
            other => panic!("预期已有真实终态的拒绝，实际 {other:?}"),
        }
        assert!(store.legacy_run_convergences().unwrap().is_empty());
        assert_eq!(
            store.legacy_run_control_state("legacy-terminal").unwrap(),
            StoredLegacyRunControlState::NotConverged
        );
        assert_eq!(
            run_snapshot(&store, "legacy-terminal"),
            terminal_snapshot,
            "被拒绝时一行都不许改"
        );

        // 待确认的终态提交候选：同样不得合成 `Interrupted`。
        seed_legacy_run_row(&store, "legacy-pending-commit", true);
        let mut pending = fixture.request(
            "legacy-pending-commit",
            "recovery-op-2",
            "executing",
            1,
            5_100,
        );
        pending.observed_commit_candidate = true;
        assert_eq!(
            store.converge_legacy_run(&pending).unwrap(),
            LegacyRunConvergenceOutcome::Refused(
                LegacyCuRunConvergenceRefusal::CommitCandidatePending
            )
        );
        assert!(store.legacy_run_convergences().unwrap().is_empty());
        assert_eq!(
            store.legacy_run_control_state("legacy-pending-commit").unwrap(),
            StoredLegacyRunControlState::NotConverged
        );
        // 被拒绝的请求不留待对账的意图。
        assert!(store
            .pending_legacy_run_convergence_intents()
            .unwrap()
            .is_empty());
    }

    /// 前置条件 fail-closed：没暂停新输入 / 资源不安全却没有阻断引用 / 对象不对，一律不写。
    #[test]
    fn legacy_convergence_preconditions_fail_closed_without_writing() {
        let store = temp_store();
        let fixture = LegacyConvergenceFixture::resource_isolated();

        // 对象不存在。
        let absent = fixture.request("absent-run", "recovery-op-1", "executing", 1, 5_000);
        assert_eq!(
            store.converge_legacy_run(&absent).unwrap(),
            LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::UnknownRun
            )
        );

        // 没有暂停新输入接纳：连读都不该继续（更不许写）。
        seed_legacy_run_row(&store, "legacy-run", true);
        let mut unpaused = fixture.request("legacy-run", "recovery-op-2", "executing", 1, 5_000);
        unpaused.new_input_intake_paused = false;
        assert_eq!(
            store.converge_legacy_run(&unpaused).unwrap(),
            LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::NewInputNotPausedOrUnauthorized
            )
        );

        // 资源不安全（有未结清释放义务）却没有已建立的资源阻断引用。
        let mut no_block = fixture.request("legacy-run", "recovery-op-3", "executing", 1, 5_000);
        no_block.resource_blocking_refs = &[];
        assert_eq!(
            store.converge_legacy_run(&no_block).unwrap(),
            LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::ResourceBlockNotEstablished
            )
        );
        assert!(store.legacy_run_convergences().unwrap().is_empty());
        assert_eq!(
            store.legacy_run_control_state("legacy-run").unwrap(),
            StoredLegacyRunControlState::NotConverged
        );
        assert_eq!(
            run_snapshot(&store, "legacy-run").1,
            None,
            "未收敛的行不得出现终态结果"
        );
        assert!(store
            .pending_legacy_run_convergence_intents()
            .unwrap()
            .is_empty());

        // 归属**已记录**的新运行不是本类事实的对象（没有"缺 workspace 就放行"的反向口子）。
        assert!(store.create_run(&run()).unwrap());
        let new_run = fixture.request("cu-1", "recovery-op-4", "requested", 0, 5_000);
        assert_eq!(
            store.converge_legacy_run(&new_run).unwrap(),
            LegacyRunConvergenceOutcome::PreconditionUnmet(
                LegacyRunConvergencePrecondition::WorkspaceAttributionRecorded
            )
        );
        assert_eq!(
            store.legacy_run_control_state("cu-1").unwrap(),
            StoredLegacyRunControlState::NotConverged
        );
    }

    /// 该行已被别的提交更新过（调用方读的是旧快照）：拒绝覆盖，且一行都不写。
    #[test]
    fn legacy_convergence_refuses_a_stale_original_state_or_revision() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-run", true);
        let fixture = LegacyConvergenceFixture::resource_isolated();
        let before = run_snapshot(&store, "legacy-run");

        // 状态对不上（行已经走到别的状态）。
        let moved_state = fixture.request("legacy-run", "recovery-op-1", "planning", 1, 5_000);
        assert_eq!(
            store.converge_legacy_run(&moved_state).unwrap(),
            LegacyRunConvergenceOutcome::Refused(
                LegacyCuRunConvergenceRefusal::OriginalRevisionChanged {
                    observed_state: "executing".into(),
                    observed_state_version: 1,
                }
            )
        );
        // revision 对不上（行已经被另一次提交推进过）。
        let moved_revision = fixture.request("legacy-run", "recovery-op-2", "executing", 2, 5_000);
        assert_eq!(
            store.converge_legacy_run(&moved_revision).unwrap(),
            LegacyRunConvergenceOutcome::Refused(
                LegacyCuRunConvergenceRefusal::OriginalRevisionChanged {
                    observed_state: "executing".into(),
                    observed_state_version: 1,
                }
            )
        );
        assert!(store.legacy_run_convergences().unwrap().is_empty());
        assert_eq!(run_snapshot(&store, "legacy-run"), before);
        assert_eq!(
            store.legacy_run_control_state("legacy-run").unwrap(),
            StoredLegacyRunControlState::NotConverged
        );
    }

    /// 候选读回面：只列出"归属未记录 + 未收尾 + 尚未收敛"的遗留行。
    #[test]
    fn legacy_unconverged_runs_lists_only_the_legacy_unclosed_rows() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-open", true);
        seed_legacy_run_row(&store, "legacy-closed", false);
        assert!(store.create_run(&run()).unwrap());
        assert_eq!(
            store
                .legacy_unconverged_runs()
                .unwrap()
                .into_iter()
                .map(|candidate| candidate.call_id)
                .collect::<Vec<_>>(),
            vec!["legacy-open".to_string()],
            "已收尾 / 归属已记录的行都不是候选"
        );
        let candidate = store.legacy_unconverged_runs().unwrap().remove(0);
        assert_eq!(candidate.session_id, "session-1");
        assert_eq!(candidate.turn_id, "turn-1");
        assert_eq!(candidate.state, "executing");
        assert_eq!(candidate.state_version, 1);

        // 收敛之后它就不再是候选（但仍然不是"已收尾"）。
        let fixture = LegacyConvergenceFixture::resource_isolated();
        let request = fixture.request("legacy-open", "recovery-op-1", "executing", 1, 5_000);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged { .. }
        ));
        assert!(store.legacy_unconverged_runs().unwrap().is_empty());
        assert_eq!(
            store.legacy_run_control_state("legacy-open").unwrap(),
            StoredLegacyRunControlState::Interrupted
        );
    }

    /// A-1.6 的顺序闸门：危险顺序在结构上执行不下去。
    #[test]
    fn legacy_convergence_order_gate_blocks_the_dangerous_order() {
        let mut gate = LegacyConvergenceOrderGate::default();
        // 没暂停新输入就不可能"登记意图"。
        assert_eq!(
            gate.mark_intent_recorded(),
            Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted: LegacyCuRunConvergenceStep::RecordReentrantConvergenceIntent,
            })
        );
        assert!(gate.mark_paused_and_authorized(false, "authority").is_err());
        assert!(gate.mark_paused_and_authorized(true, "  ").is_err());
        gate.mark_paused_and_authorized(true, "single-writer-claim:recovery-1")
            .unwrap();
        gate.mark_intent_recorded().unwrap();
        // 资源不安全却没有任何阻断引用：建立不了阻断。
        assert_eq!(
            gate.mark_resource_block_established(&[], false),
            Err(LegacyRunConvergencePrecondition::ResourceBlockNotEstablished)
        );
        // **危险顺序被挡住**：阻断还没建立，就写不了旧 run 的终态。
        assert_eq!(
            gate.require_ready_for_terminal_write(),
            Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted:
                    LegacyCuRunConvergenceStep::WriteRunNonSuccessTerminalAndConvergenceFactInSourceTransaction,
            })
        );
        gate.mark_resource_block_established(&["input-safety:incident-9".into()], false)
            .unwrap();
        gate.require_ready_for_terminal_write().unwrap();
        assert!(legacy_cu_run_convergence_order_is_safe(&gate.steps_taken()));
        // 没写终态 / 没对账之前不得开放新输入。
        assert!(!gate.may_reopen_new_input(true));
        assert_eq!(
            gate.mark_reconciled(),
            Err(LegacyRunConvergencePrecondition::WriteOrderViolated {
                attempted: LegacyCuRunConvergenceStep::ReconcileConvergenceIntent,
            })
        );
        gate.mark_terminal_and_fact_written();
        gate.mark_reconciled().unwrap();
        assert!(!gate.may_reopen_new_input(false));
        assert!(gate.may_reopen_new_input(true));
        // 走过的顺序就是契约的规范顺序前缀，逐项一致。
        assert_eq!(
            gate.steps_taken(),
            runtime::LEGACY_CU_RUN_CONVERGENCE_STEPS[..5].to_vec()
        );
    }

    /// 意图可重入：意图存在而事实缺失时可重新进入同一次收敛；完成后不再挂起。
    #[test]
    fn legacy_convergence_intent_is_reentrant_and_reconciled_with_the_fact() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-run", true);
        let fixture = LegacyConvergenceFixture::resource_isolated();
        let intent = LegacyCuRunConvergenceIntent {
            recovery_operation_id: "recovery-op-1".into(),
            source_database_identity: "session-db:sessions.sqlite3".into(),
            original_run_id: "legacy-run".into(),
            original_state: "executing".into(),
            original_state_version: 1,
            recorded_at_unix_ms: 4_900,
            reconciled: false,
        };
        assert_eq!(
            store
                .record_legacy_run_convergence_intent(&intent)
                .unwrap(),
            LegacyRunConvergenceIntentUpsert::Inserted
        );
        assert_eq!(
            store.pending_legacy_run_convergence_intents().unwrap(),
            vec![intent.clone()],
            "意图存在、事实缺失 = 可重入状态"
        );
        // 完全相同内容的重复登记是幂等的。
        assert_eq!(
            store
                .record_legacy_run_convergence_intent(&intent)
                .unwrap(),
            LegacyRunConvergenceIntentUpsert::AlreadyRecorded
        );
        // 同一个对账键上换个操作 ID：不覆盖既有意图。
        let mut other = intent.clone();
        other.recovery_operation_id = "recovery-op-2".into();
        other.recorded_at_unix_ms = 4_950;
        assert!(matches!(
            store
                .record_legacy_run_convergence_intent(&other)
                .unwrap(),
            LegacyRunConvergenceIntentUpsert::Conflicting { .. }
        ));

        // 用**记录里的**操作 ID 重新进入那次收敛 → 事实落库、意图对账完成。
        let request = fixture.request("legacy-run", "recovery-op-1", "executing", 1, 5_000);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged { .. }
        ));
        assert!(store
            .pending_legacy_run_convergence_intents()
            .unwrap()
            .is_empty());
        assert_eq!(store.legacy_run_convergences().unwrap().len(), 1);
    }

    /// "是否开放新输入"只跟资源维度的独立检查有关：控制状态相同的两条收敛可以给出不同结论。
    #[test]
    fn legacy_convergence_reopen_follows_only_the_independent_resource_check() {
        let store = temp_store();
        let isolated = LegacyConvergenceFixture::resource_isolated();
        seed_legacy_run_row(&store, "legacy-isolated", true);
        let request = isolated.request("legacy-isolated", "recovery-op-1", "executing", 1, 5_000);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged {
                may_reopen_new_input: false,
                ..
            }
        ));

        let clear = LegacyConvergenceFixture::resource_clear();
        seed_legacy_run_row(&store, "legacy-clear", true);
        let request = clear.request("legacy-clear", "recovery-op-2", "executing", 1, 5_100);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged {
                may_reopen_new_input: true,
                ..
            }
        ));

        // 两行的**控制状态完全相同**（都被终止），差别只在资源维度的独立检查结果。
        for run_id in ["legacy-isolated", "legacy-clear"] {
            assert_eq!(
                store.legacy_run_control_state(run_id).unwrap(),
                StoredLegacyRunControlState::Interrupted
            );
        }
        assert!(store
            .latest_legacy_run_convergence("legacy-isolated")
            .unwrap()
            .unwrap()
            .fact
            .historical_outcome
            .is_unknown());
        assert!(store
            .latest_legacy_run_convergence("legacy-clear")
            .unwrap()
            .unwrap()
            .fact
            .historical_outcome
            .is_unknown());
    }

    /// 读回不得比写入更确定：被改写过的收敛行必须在读回时响亮失败。
    #[test]
    fn legacy_convergence_decoder_rejects_rewritten_rows() {
        let store = temp_store();
        seed_legacy_run_row(&store, "legacy-run", true);
        let fixture = LegacyConvergenceFixture::resource_isolated();
        let request = fixture.request("legacy-run", "recovery-op-1", "executing", 1, 5_000);
        assert!(matches!(
            store.converge_legacy_run(&request).unwrap(),
            LegacyRunConvergenceOutcome::Converged { .. }
        ));

        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE computer_use_legacy_run_convergences SET reason_code = 'user_cancelled'",
                    [],
                )
                .unwrap();
        }
        assert!(
            store.latest_legacy_run_convergence("legacy-run").is_err(),
            "被改写成'用户取消'的收敛事实必须读回失败"
        );
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE computer_use_legacy_run_convergences
                        SET reason_code = ?1, terminal_status = 'succeeded'",
                    [runtime::LEGACY_CU_RUN_CLOSED_AT_RECOVERY_REASON_CODE],
                )
                .unwrap();
        }
        assert!(
            store.latest_legacy_run_convergence("legacy-run").is_err(),
            "被改写成'成功'的收敛事实必须读回失败"
        );
        // 枚举列取不认识的值同样不得读成"未收敛"。
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE computer_use_runs SET legacy_convergence_state = 'succeeded'",
                    [],
                )
                .unwrap();
        }
        assert!(store.legacy_run_control_state("legacy-run").is_err());
    }
}
