//! PR-02A／P0-2：**生产**动作来源核对（`ActionOriginAuthority` 的组合实现）。
//!
//! 为什么需要它：`admit_action_origin` 的第二级（核对真实父对象）必须消费**可信上下文**。
//! 在此之前生产里一次都没调用过它（唯一实现是 crate 内的测试替身 `TrustedOriginContext`），
//! 于是"这条动作的来源"只是一句话、不是约束——系统看起来全绿，事实链却不是生产约束。
//!
//! 三条口径（照裁决 PR-02A）：
//!
//! 1. **不伪造**：查不到的关联一律 `None`（契约随即拒绝），**绝不**返回"看起来像"的近似值
//!    （provider trace、外层 `computer_use_perform` 的 call id、"最近一次请求"都不行）；
//! 2. **不越权判定**：跨 run 是否合法由契约的 `ensure_runs_are_related` 判定；本实现只如实
//!    给出记录（或如实给出"没有"）；
//! 3. **快照语义**：结构体持有的是**构造时刻从库里读到**的记录，不是调用方传进来的字符串，
//!    因此 `&self` 返回引用是安全的——记录的真实性由"来源是库"保证。
//!
//! 工具调用归属（`tool_call_id`）当前**没有登记表**（裁决把 `tool_calls` 登记推迟到 PR-02B）：
//! 因此这里留空。契约的两个分支正好表达所需语义——`(None, None)` 通过（类型 B：模型规划但
//! 非工具链动作），`(None, Some)` 拒绝（声称工具归属却无登记 ＝ 伪造工具归属）。

use std::collections::BTreeMap;
use std::path::Path;

use runtime::{
    ActionOriginAuthority, CleanupIncidentRecord, ControlOperationRecord, HostOperationRecord,
    PlannedRequestAttempt, RunRelationRecord, RunScopeContext, ToolCallRelation,
};

use crate::computer_use_store::ComputerUseRunStore;

/// 组合来源（裁决 §四）：规划请求登记（落库）+ 运行关联（会话库）+ 会话上下文（冻结上下文）。
pub(crate) struct ProductionActionOriginAuthority {
    /// 冻结的会话上下文：来自接纳时的冻结值，**不是**"当前工作区"。
    conversation_scope: Option<RunScopeContext>,
    /// 运行关联（按 run_id）。工作区未记录的历史行**不构造**记录——不拿当前工作区顶替。
    run_relations: BTreeMap<String, RunRelationRecord>,
    /// 产生某动作执行计划的规划请求（按 action_id）：`plan_producer`。
    plan_producers: BTreeMap<String, PlannedRequestAttempt>,
    /// 已登记的规划请求（按稳定复合键）：`known_request`（附加因果引用核对用）。
    known_requests: BTreeMap<String, PlannedRequestAttempt>,
    /// 工具调用归属（PR-02B）：按 `action_id` 存"这个动作所属的工具调用"。
    ///
    /// 只在**登记表里真有这一行**时才有条目：查不到就不填，让契约走 `(None, None)`
    /// （不声称工具归属）或对伪造的声称报"工具归属不得虚构"。
    tool_relations: BTreeMap<String, ToolCallRelation>,
    /// 宿主辅助动作的父操作登记：暂无 ⇒ `HostIncidental` 一律拒绝（fail-closed）。
    host_operations: BTreeMap<String, HostOperationRecord>,
    /// 清理事故登记（`SafetyCleanup` 来源核对的依据）。
    ///
    /// 只有**登记表里真有**、且 `recovery_eligible = true` 的事故才会进这个快照——
    /// 没有资格的事故不该被当成"可以清理的来源"（契约会据此拒绝）。
    cleanup_incidents: BTreeMap<String, CleanupIncidentRecord>,
    /// 控制面操作登记：暂无 ⇒ `UserDirect` 一律拒绝（模型输入里声称"用户点击"不构成来源）。
    control_operations: BTreeMap<String, ControlOperationRecord>,
}

impl ProductionActionOriginAuthority {
    /// 为**一条具体动作**构造快照。
    ///
    /// 只接受会话库路径与两侧身份（run／action），其余记录一律来自查询：调用方无法"声明"关联。
    pub(crate) fn for_action(
        session_db_path: &Path,
        call_id: &str,
        action_id: &str,
        conversation_scope: Option<RunScopeContext>,
    ) -> Result<Self, String> {
        let store = ComputerUseRunStore::open(session_db_path).map_err(|error| error.to_string())?;
        Self::for_store(&store, call_id, action_id, conversation_scope)
    }

    /// **执行器用的构造**：复用**同一条** store（不再为每个动作新开连接/重复迁移）。
    ///
    /// 复用不只是省开销：动作来源核对必须看到"刚刚登记的那条 attempt"，同一连接下的读写顺序
    /// 是确定的；另开连接会引入 WAL 下的可见性/忙等噪声，让"查不到"与"还没提交"混为一谈。
    pub(crate) fn for_store(
        store: &ComputerUseRunStore,
        call_id: &str,
        action_id: &str,
        conversation_scope: Option<RunScopeContext>,
    ) -> Result<Self, String> {
        let mut authority = Self {
            conversation_scope,
            run_relations: BTreeMap::new(),
            plan_producers: BTreeMap::new(),
            known_requests: BTreeMap::new(),
            tool_relations: BTreeMap::new(),
            host_operations: BTreeMap::new(),
            cleanup_incidents: BTreeMap::new(),
            control_operations: BTreeMap::new(),
        };
        // 运行关联：只在该运行**真的**记录了归属与房间时构造（否则如实为"查不到"）。
        if let Some(run) = store.load(call_id).map_err(|error| error.to_string())? {
            if let (Some(workspace_id), Some(room_id)) = (
                run.workspace.workspace_id(),
                run.chat_room_id.as_deref().filter(|value| !value.trim().is_empty()),
            ) {
                authority.run_relations.insert(
                    call_id.to_string(),
                    RunRelationRecord::new(call_id, workspace_id, room_id),
                );
            }
        }
        // 工具调用归属（PR-02B）：本 CU 运行是由哪次工具调用承载的——运行行里有真实的
        // `provider_tool_call_id`，但**只有在登记表里查得到**时才算"有真实工具调用关系"。
        // 查不到就什么都不填：既不自造，也不把"外层 id 存在"当成"关系成立"。
        if let Some(run_row) = store.load(call_id).map_err(|error| error.to_string())? {
            if let Some(provider_tool_call_id) = run_row
                .provider_tool_call_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                if store
                    .tool_call_is_registered(provider_tool_call_id)
                    .map_err(|error| error.to_string())?
                {
                    authority.tool_relations.insert(
                        action_id.to_string(),
                        ToolCallRelation::new(action_id, provider_tool_call_id),
                    );
                }
            }
        }
        // 清理事故（`SafetyCleanup` 的核对来源）：预载**本运行**的事故，按 incident_id 提供查询
        // ——契约的 `cleanup_incident(incident_id)` 就是按 id 查，因此这里按 run 预载最贴合。
        // 只载入**具备恢复资格**的事故：没有资格的不得被当成"可以清理的来源"。
        for incident in store
            .cleanup_incidents_for_run(call_id)
            .map_err(|error| error.to_string())?
        {
            if incident.recovery_eligible {
                authority.cleanup_incidents.insert(
                    incident.incident_id.clone(),
                    CleanupIncidentRecord {
                        incident_id: incident.incident_id,
                        run_id: incident.run_id,
                        original_action_id: incident.original_action_id,
                        original_tool_call_id: incident.original_tool_call_id,
                        recovery_eligible: true,
                    },
                );
            }
        }
        // 规划请求：按动作读"真正产生它的请求"；同一请求同时进 `known_requests`。
        if let Some(attempt) = store
            .plan_attempt_for_action(call_id, action_id)
            .map_err(|error| error.to_string())?
        {
            authority
                .known_requests
                .insert(attempt.stable_key(), attempt.clone());
            authority
                .plan_producers
                .insert(action_id.to_string(), attempt);
        }
        Ok(authority)
    }

    /// 只读探测：本快照里有没有"该动作所属的工具调用"。
    ///
    /// 调用方（执行器）据此决定**是否声称** `tool_call_id`：契约对"有真实工具调用关系却漏传"
    /// 是拒绝的，因此"有就必填、无就不填"必须由证据决定，而不是由调用方挑。
    #[must_use]
    pub(crate) fn has_tool_call_relation(&self, action_id: &str) -> bool {
        self.tool_relations.contains_key(action_id)
    }

    /// 该动作所属工具调用的 id（`None` = 没有可核对的关系 ⇒ **不得声称**工具归属）。
    #[must_use]
    pub(crate) fn tool_call_id_for(&self, action_id: &str) -> Option<&str> {
        self.tool_relations
            .get(action_id)
            .map(|relation| relation.tool_call_id.as_str())
    }

    /// 只读探测：本快照里有没有"产生该动作的规划请求"（供调用方区分"查不到"与"不符"）。
    #[must_use]
    pub(crate) fn has_plan_producer(&self, action_id: &str) -> bool {
        self.plan_producers.contains_key(action_id)
    }
}

impl ActionOriginAuthority for ProductionActionOriginAuthority {
    fn conversation_scope(&self) -> Option<&RunScopeContext> {
        self.conversation_scope.as_ref()
    }

    fn run_relation(&self, run_id: &str) -> Option<&RunRelationRecord> {
        self.run_relations.get(run_id)
    }

    fn plan_producer(&self, action_id: &str) -> Option<&PlannedRequestAttempt> {
        self.plan_producers.get(action_id)
    }

    fn known_request(&self, stable_key: &str) -> Option<&PlannedRequestAttempt> {
        self.known_requests.get(stable_key)
    }

    fn tool_call_relation(&self, action_id: &str) -> Option<&ToolCallRelation> {
        self.tool_relations.get(action_id)
    }

    fn host_operation(&self, operation_id: &str) -> Option<&HostOperationRecord> {
        self.host_operations.get(operation_id)
    }

    fn cleanup_incident(&self, incident_id: &str) -> Option<&CleanupIncidentRecord> {
        self.cleanup_incidents.get(incident_id)
    }

    fn control_operation(&self, control_operation_id: &str) -> Option<&ControlOperationRecord> {
        self.control_operations.get(control_operation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_store() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let db = directory.path().join("web-sessions.sqlite3");
        {
            let connection = crate::open_session_connection(&db).expect("open");
            crate::initialize_session_schema(&connection).expect("schema");
        }
        (directory, db)
    }

    fn scope() -> RunScopeContext {
        RunScopeContext::new("ws-0123456789abcdef", "room-1", "session-1", "turn-1")
    }

    /// 登记一条运行归属（真实行），使 `run_relation` 有据可查。
    fn seed_run(db: &Path, call_id: &str) {
        let workspace =
            crate::canonical_workspace_identity("ws-0123456789abcdef").expect("canonical workspace");
        let store = ComputerUseRunStore::open(db).expect("store");
        let run = crate::computer_use_store::NewComputerUseRun {
            call_id: call_id.to_string(),
            provider_tool_call_id: Some("provider-call".to_string()),
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            chat_room_id: Some("room-1".to_string()),
            idempotency_key: format!("idem-{call_id}"),
            objective_json: "{}".to_string(),
            surface: computer_use::ComputerUseSurface::Desktop,
            deadline_ms: 60_000,
            created_at_ms: 1,
            workspace: crate::computer_use_store::CuWorkspaceAttribution::from_parent_run(&workspace),
        };
        assert!(store.create_run(&run).expect("create"), "运行行必须写入");
    }

    /// **CU-F05-1 的前半**：真实 attempt 落库后，`plan_producer` 查得到、且与声明一致。
    #[test]
    fn plan_producer_comes_from_the_store_not_from_the_caller() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        let attempt =
            runtime::PlannedRequestAttempt::new("cu-run-1", "computer_use_planning:step-0", "attempt-1")
                .expect("合法复合键");
        {
            let store = ComputerUseRunStore::open(&db).expect("store");
            store
                .record_plan_attempt("cu-run-1", &attempt, 0)
                .expect("登记 attempt");
            store
                .bind_plan_attempt_action(&attempt.stable_key(), "action-0")
                .expect("绑定动作");
        }
        let authority = ProductionActionOriginAuthority::for_action(
            &db,
            "cu-run-1",
            "action-0",
            Some(scope()),
        )
        .expect("构造快照");
        assert_eq!(
            authority.plan_producer("action-0").map(|value| value.stable_key()),
            Some(attempt.stable_key()),
            "产生该动作的规划请求必须来自落库登记"
        );
        assert_eq!(
            authority.known_request(&attempt.stable_key()).map(|value| value.attempt_id()),
            Some("attempt-1")
        );
        // 别的动作：**查不到**（不得用"最近一次请求"顶替）。
        assert!(authority.plan_producer("action-1").is_none());
        assert!(!authority.has_plan_producer("action-1"));
        // 运行关联来自真实行（工作区 + 房间都在）。
        assert_eq!(
            authority.run_relation("cu-run-1").map(|value| value.room_id.clone()),
            Some("room-1".to_string())
        );
        // 未登记的 run 如实为"查不到"（跨 run 声明因此会被契约拒绝）。
        assert!(authority.run_relation("cu-run-unknown").is_none());
    }

    /// **PR-02B**：工具归属只在**登记表真有这一行**时成立；未登记一律不成立（不得伪造）。
    #[test]
    fn tool_relation_requires_a_registered_tool_call() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        {
            let store = ComputerUseRunStore::open(&db).expect("store");
            // 该 CU 运行的承载工具调用：真实 provider id，但**尚未登记**。
            store
                .register_tool_call("toolu-cu-1", Some("run-1"), None, "computer_use_perform", "d", "dispatched")
                .expect("另一个工具调用的登记（与本 run 无关）");
        }
        let authority = ProductionActionOriginAuthority::for_action(
            &db,
            "cu-run-1",
            "action-0",
            Some(scope()),
        )
        .expect("快照");
        assert!(
            !authority.has_tool_call_relation("action-0"),
            "provider id 存在但**未登记** ⇒ 关系不成立（外层 id 不等于真实关系）"
        );
        assert_eq!(authority.tool_call_id_for("action-0"), None);

        // 登记该工具调用之后，关系才成立。
        let provider_id = ComputerUseRunStore::open(&db)
            .expect("store")
            .load("cu-run-1")
            .expect("load")
            .expect("run row")
            .provider_tool_call_id
            .expect("运行行必须有 provider 工具调用 id");
        ComputerUseRunStore::open(&db)
            .expect("store")
            .register_tool_call(&provider_id, Some("run-1"), None, "computer_use_perform", "d", "dispatched")
            .expect("登记");
        let authority = ProductionActionOriginAuthority::for_action(
            &db,
            "cu-run-1",
            "action-0",
            Some(scope()),
        )
        .expect("快照");
        assert!(authority.has_tool_call_relation("action-0"));
        assert_eq!(authority.tool_call_id_for("action-0"), Some(provider_id.as_str()));
    }

    /// **工具归属不伪造**：当前没有工具调用登记表 ⇒ 一律 `None`。
    ///
    /// 这正是裁决要求的口径：`(None, Some)` 会被契约判为伪造工具归属，而不是"看起来通过"。
    #[test]
    fn tool_relations_are_never_fabricated() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        let authority =
            ProductionActionOriginAuthority::for_action(&db, "cu-run-1", "action-0", Some(scope()))
                .expect("构造快照");
        assert!(authority.tool_call_relation("action-0").is_none());
        assert!(authority.cleanup_incident("incident-1").is_none());
        assert!(authority.control_operation("control-1").is_none());
        assert!(authority.host_operation("operation-1").is_none());
    }

    /// **CU-F05-3**：工具归属**不得虚构**——没有登记却声称 `tool_call_id` ⇒ 拒绝；
    /// 没有工具链关系时 `tool_call_id = None` ⇒ 通过（类型 B：模型规划但非工具链动作）。
    #[test]
    fn cu_f05_3_tool_ownership_cannot_be_fabricated() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        let attempt =
            runtime::PlannedRequestAttempt::new("cu-run-1", "computer_use_planning:step-0", "attempt-1")
                .expect("合法复合键");
        {
            let store = ComputerUseRunStore::open(&db).expect("store");
            store.record_plan_attempt("cu-run-1", &attempt, 0).expect("登记");
            store
                .bind_plan_attempt_action(&attempt.stable_key(), "action-0")
                .expect("绑定");
        }
        let authority = ProductionActionOriginAuthority::for_action(
            &db,
            "cu-run-1",
            "action-0",
            Some(scope()),
        )
        .expect("快照");
        let identity = scope()
            .step_action_fact(
                "cu-run-1",
                "step-0",
                attempt.stable_key(),
                "action-0",
            )
            .with_tool_call_id("provider-call");
        let context = runtime::ConversationActionContext::new(identity).expect("上下文");
        let origin = |tool_call_id: Option<&str>| runtime::ActionOrigin {
            action_id: "action-0".to_string(),
            source: runtime::ActionSource::ModelPlanned,
            context: runtime::ActionContext::Conversation(context.clone()),
            request_attempt_id: Some(attempt.stable_key()),
            tool_call_id: tool_call_id.map(str::to_string),
            parent_step_operation: None,
            host_algorithm_version: None,
            resource_scope: None,
            cleanup: None,
            user_direct: None,
            host_transform: None,
            additional_causal_refs: Vec::new(),
        };
        // ① 不声称工具归属 ⇒ 通过（当前没有工具调用登记表，类型 B 合法）。
        runtime::admit_action_origin(&origin(None), &authority).expect("类型 B 必须通过");
        // ② 声称一个没有登记的工具调用 ⇒ 拒绝（不得伪造工具归属）。
        let forged = runtime::admit_action_origin(&origin(Some("computer_use_perform-forged")), &authority)
            .err()
            .expect("伪造工具归属必须被拒");
        assert!(
            forged.message.contains("工具归属不得虚构"),
            "拒绝理由必须点明伪造：{}",
            forged.message
        );
    }

    /// **CU-F05-5（正半）**：具备恢复资格的事故 ⇒ 清理动作**成立**（来源可核对）。
    #[test]
    fn cu_f05_5_eligible_cleanup_incident_is_accepted_as_a_source() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        {
            let store = ComputerUseRunStore::open(&db).expect("store");
            store
                .register_cleanup_incident(
                    "incident-cleanup-1",
                    "cu-run-1",
                    "action-0",
                    Some("toolu-1"),
                )
                .expect("登记事故");
            // 前提状态：资格由**解决路径**给出（`resolve_unconfirmed_release` 的翻转）；
            // 本用例只验证"来源核对"，因此直接置位（真实链路见 store 侧的资格用例）。
            store
                .mark_cleanup_incident_recovery_eligible("incident-cleanup-1")
                .expect("给出恢复资格");
        }
        let authority = ProductionActionOriginAuthority::for_action(
            &db,
            "cu-run-1",
            "action-cleanup",
            Some(scope()),
        )
        .expect("快照");
        let identity = scope()
            .step_action_fact("cu-run-1", "step-0", "attempt-1", "action-cleanup")
            .with_tool_call_id("provider-call");
        let context = runtime::ConversationActionContext::new(identity).expect("上下文");
        let origin = |original_action_id: &str| runtime::ActionOrigin {
            action_id: "action-cleanup".to_string(),
            source: runtime::ActionSource::SafetyCleanup,
            context: runtime::ActionContext::Conversation(context.clone()),
            request_attempt_id: None,
            tool_call_id: None,
            parent_step_operation: None,
            host_algorithm_version: None,
            resource_scope: None,
            cleanup: Some(runtime::CleanupRelation {
                incident_id: "incident-cleanup-1".to_string(),
                original_action_id: original_action_id.to_string(),
                recovery_eligible: true,
            }),
            user_direct: None,
            host_transform: None,
            additional_causal_refs: Vec::new(),
        };
        // ① 原动作与事故记录一致 ⇒ 成立。
        runtime::admit_action_origin(&origin("action-0"), &authority)
            .expect("有资格且原动作一致 ⇒ 清理来源成立");
        // ② 原动作不一致 ⇒ 拒绝（不得借别的事故给自己做来源）。
        let mismatched = runtime::admit_action_origin(&origin("action-other"), &authority)
            .err()
            .expect("原动作不一致必须被拒");
        assert!(
            mismatched.message.contains("不一致"),
            "拒绝理由必须指向原动作不一致：{}",
            mismatched.message
        );
        // ③ 未登记的事故 ⇒ 拒绝。
        let unknown = runtime::admit_action_origin(
            &runtime::ActionOrigin {
                cleanup: Some(runtime::CleanupRelation {
                    incident_id: "incident-unknown".to_string(),
                    original_action_id: "action-0".to_string(),
                    recovery_eligible: true,
                }),
                ..origin("action-0")
            },
            &authority,
        )
        .err()
        .expect("未登记的事故必须被拒");
        assert!(unknown.message.contains("incident"), "{}", unknown.message);
    }

    /// **CU-F05-5（负半）**：清理动作没有 incident 登记 ⇒ **拒绝**（fail-closed）。
    ///
    /// 正面那一半（有 incident + 恢复资格即通过）需要 `CleanupIncidentRecord` 形态的事故登记，
    /// 生产里尚不存在 ⇒ 归 PR-02B／独立工单（见台账 §B-84）。
    #[test]
    fn cu_f05_5_cleanup_without_an_incident_registry_is_refused() {
        let (_directory, db) = seed_store();
        seed_run(&db, "cu-run-1");
        let authority =
            ProductionActionOriginAuthority::for_action(&db, "cu-run-1", "action-cleanup", Some(scope()))
                .expect("快照");
        let identity = scope()
            .step_action_fact("cu-run-1", "step-0", "attempt-1", "action-cleanup")
            .with_tool_call_id("provider-call");
        let context = runtime::ConversationActionContext::new(identity).expect("上下文");
        let origin = runtime::ActionOrigin {
            action_id: "action-cleanup".to_string(),
            source: runtime::ActionSource::SafetyCleanup,
            context: runtime::ActionContext::Conversation(context),
            request_attempt_id: None,
            tool_call_id: None,
            parent_step_operation: None,
            host_algorithm_version: None,
            resource_scope: None,
            cleanup: Some(runtime::CleanupRelation {
                incident_id: "incident-1".to_string(),
                original_action_id: "action-0".to_string(),
                // 声明"具备恢复资格"：拒绝必须来自"查不到 incident"，而不是来自资格字段。
                recovery_eligible: true,
            }),
            user_direct: None,
            host_transform: None,
            additional_causal_refs: Vec::new(),
        };
        let refused = runtime::admit_action_origin(&origin, &authority)
            .err()
            .expect("没有 incident 登记的清理动作必须被拒");
        assert!(
            refused.message.contains("incident") || refused.message.contains("清理"),
            "拒绝理由必须指向缺失的事故登记：{}",
            refused.message
        );
    }

    /// 历史行（工作区未记录）**不得**被当前工作区顶替：运行关联如实缺失。
    #[test]
    fn legacy_rows_without_recorded_workspace_have_no_run_relation() {
        let (_directory, db) = seed_store();
        {
            // 直接写一条"归属未记录"的历史行（迁移前的形态）。
            let connection = crate::open_session_connection(&db).expect("open");
            connection
                .execute_batch(
                    r#"
                    INSERT INTO computer_use_runs(
                        call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
                        idempotency_key, objective_json, surface, state, state_version,
                        deadline_ms, created_at_ms, updated_at_ms,
                        workspace_id, workspace_context_version)
                    VALUES ('cu-legacy', 'provider-legacy', 'turn-1', 'session-1', 'room-1',
                            'idem-legacy', '{}', 'desktop', 'succeeded', 1, 1, 1, 1, NULL, NULL);
                    "#,
                )
                .expect("legacy row");
        }
        let authority =
            ProductionActionOriginAuthority::for_action(&db, "cu-legacy", "action-0", Some(scope()))
                .expect("构造快照");
        assert!(
            authority.run_relation("cu-legacy").is_none(),
            "历史行没有归属记录 ⇒ 不得用当前工作区顶替"
        );
    }
}
