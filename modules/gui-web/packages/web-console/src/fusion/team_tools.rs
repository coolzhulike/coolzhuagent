//! team_* 工具：模型可见的团队协调接口。执行只读写 fusion_* 台账，
//! 真正的角色运行由宿主编排器驱动——模型不能在这里递归启动其它角色。
use super::super::*;
use super::*;
use serde_json::{json, Value as JsonValue};

fn def(name: &str, description: &str, schema: JsonValue) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: Some(description.to_string()),
        input_schema: schema,
    }
}

fn props(entries: &[(&str, JsonValue)], required: &[&str]) -> JsonValue {
    let map = entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    json!({"type":"object","properties":map,"required":required,"additionalProperties":false})
}

fn s(desc: &str) -> JsonValue {
    json!({"type":"string","description":desc})
}

fn arr(item: JsonValue, desc: &str) -> JsonValue {
    json!({"type":"array","items":item,"description":desc})
}

/// 按角色的 team_* 工具表；与 §5.2 派发矩阵一致（叶子查询者不能再派发）。
pub(super) fn definitions_for(role: FusionRole, querier_is_separate: bool) -> Vec<ToolDefinition> {
    let mut defs = Vec::new();
    match role {
        FusionRole::Host => {
            defs.push(def(
                "team_plan",
                "任务型会话：登记/替换本任务的工作项计划（方案由你设计）。每项含 title、可选 depends_on、acceptance_criteria（验收标准）、verification、max_retries。调用后会生成计划卡片展示给用户。",
                props(
                    &[
                        ("title", s("任务标题，可省略")),
                        (
                            "work_items",
                            arr(
                                json!({
                                    "type":"object",
                                    "properties":{
                                        "id":{"type":"string"},
                                        "title":{"type":"string"},
                                        "depends_on":{"type":"array","items":{"type":"string"}},
                                        "acceptance_criteria":{"type":"array","items":{"type":"string"}},
                                        "verification":{"type":"object"},
                                        "max_retries":{"type":"integer","minimum":0,"maximum":10}
                                    },
                                    "required":["title"]
                                }),
                                "工作项列表",
                            ),
                        ),
                    ],
                    &["work_items"],
                ),
            ));
            defs.push(def(
                "team_dispatch",
                "向执行者或查询者派发一个 brief。to: executor|querier；kind: exec_task|rework|fix_guidance（默认按 to 推断）；work_item_id 可关联计划项；objective 必填；可附 constraints、acceptance_criteria、context_refs。立即返回 brief_id，结果稍后以事件形式送达。",
                props(
                    &[
                        ("to", json!({"type":"string","enum":["executor","querier"]})),
                        ("kind", json!({"type":"string","enum":["exec_task","rework","fix_guidance","query"]})),
                        ("work_item_id", s("可选：关联的工作项 id")),
                        ("objective", s("要对方完成什么")),
                        ("constraints", arr(s(""), "约束条件")),
                        ("acceptance_criteria", arr(s(""), "验收标准")),
                        ("context_refs", arr(s(""), "引用：消息id / artifact id / file:路径:行")),
                    ],
                    &["to", "objective"],
                ),
            ));
            defs.push(def(
                "team_query",
                "快捷查询：向查询者派发一条 kind=query 的 brief，返回 brief_id。用于代码、文件或网络检索。",
                props(
                    &[
                        ("question", s("要查询的问题")),
                        ("scope", json!({"type":"string","enum":["code","files","web","mixed"]})),
                        ("refs", arr(s(""), "相关引用")),
                    ],
                    &["question"],
                ),
            ));
            defs.push(def(
                "team_wait",
                "查看若干 brief 的当前状态（不阻塞等待；返回状态与已有结果摘要）。",
                props(&[("brief_ids", arr(s(""), "brief id 列表"))], &["brief_ids"]),
            ));
            defs.push(def(
                "team_ask_user",
                "需要用户拍板时发起决策卡片；任务将挂起(awaiting_user)直到用户回复。仅在有任务时可调用；不可逆操作、范围扩大或成本显著差异必须用它。",
                props(
                    &[
                        ("question", s("问题")),
                        ("options", arr(json!({"type":"object","properties":{"id":{"type":"string"},"label":{"type":"string"},"detail":{"type":"string"},"recommended":{"type":"boolean"}},"required":["label"]}), "候选选项")),
                        ("allow_free_text", json!({"type":"boolean"})),
                    ],
                    &["question"],
                ),
            ));
            defs.push(def(
                "team_accept",
                "对照验收标准审核执行者完成的工作项：verdict=accept|rework；rework 必须给 reason，会自动计入重试次数，超过上限工作项转 blocked 并需要你改方案或问用户。",
                props(
                    &[
                        ("work_item_id", s("工作项 id")),
                        ("verdict", json!({"type":"string","enum":["accept","rework"]})),
                        ("reason", s("不通过原因（rework 必填）")),
                    ],
                    &["work_item_id", "verdict"],
                ),
            ));
        }
        FusionRole::Executor => {
            defs.push(def(
                "team_report",
                "汇报当前 brief 的结果：status=done|partial|blocked|failed；summary 必填（≤400字）；可附 evidence{files_changed,commands}, open_issues, details（长内容自动归档为 artifact）。每个 brief 结束前必须调用。",
                props(
                    &[
                        ("brief_id", s("默认当前 brief")),
                        ("status", json!({"type":"string","enum":["done","partial","blocked","failed"]})),
                        ("summary", s("结果摘要")),
                        ("evidence", json!({"type":"object"})),
                        ("open_issues", arr(s(""), "遗留问题")),
                        ("details", s("长详情，超出摘要部分")),
                    ],
                    &["status", "summary"],
                ),
            ));
            defs.push(def(
                "team_escalate",
                "受阻时上报主持：problem 必填，附 attempts（已尝试方案）与 suggested_options（建议）。上报后当前 brief 转 blocked，由主持决定修复指导/改方案/问用户。不要反复重试同一失败。",
                props(
                    &[
                        ("brief_id", s("默认当前 brief")),
                        ("problem", s("遇到的问题")),
                        ("attempts", arr(s(""), "已尝试过的做法")),
                        ("suggested_options", arr(s(""), "建议的处理方式")),
                    ],
                    &["problem"],
                ),
            ));
            if querier_is_separate {
                defs.push(def(
                    "team_query",
                    "把网络检索或大范围搜索交给查询者（返回 brief_id，结果稍后送达）；正在修改的代码应自己读。",
                    props(
                        &[
                            ("question", s("要查询的问题")),
                            ("scope", json!({"type":"string","enum":["code","files","web","mixed"]})),
                            ("refs", arr(s(""), "相关引用")),
                        ],
                        &["question"],
                    ),
                ));
            }
        }
        FusionRole::Querier => {
            defs.push(def(
                "team_report",
                "汇报查询结果：status=done|partial|blocked|failed；summary 给结论；evidence.citations 给引用（文件:行号 或 URL）；open_issues 列不确定项。",
                props(
                    &[
                        ("brief_id", s("默认当前 brief")),
                        ("status", json!({"type":"string","enum":["done","partial","blocked","failed"]})),
                        ("summary", s("结论摘要")),
                        ("evidence", json!({"type":"object"})),
                        ("open_issues", arr(s(""), "遗留问题")),
                        ("details", s("长详情")),
                    ],
                    &["status", "summary"],
                ),
            ));
        }
    }
    // 三角色共有：按 id 召回长内容。
    defs.push(def(
        "team_recall",
        "按 id 召回完整内容（artifact / brief payload / brief result）。brief 与结果只内联摘要，长内容需用此工具按需取回。",
        props(&[("id", s("artifact/brief/result id"))], &["id"]),
    ));
    defs
}

fn text_response(status: &str, text: String) -> ApiResult<ToolDispatchResponse> {
    Ok(ToolDispatchResponse {
        status: status.to_string(),
        route: "fusion-team".to_string(),
        scenario: None,
        closed_loop: None,
        dispatch_plan: None,
        tool_result_text: Some(text),
        notes: Vec::new(),
    })
}

fn new_id(prefix: &str) -> String {
    let suffix = random_hex_identifier(8, "fusion id")
        .unwrap_or_else(|_| format!("{}", unix_timestamp_millis()));
    format!("{prefix}-{suffix}")
}

fn str_list(input: &JsonValue, key: &str) -> Vec<String> {
    input
        .get(key)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn join_lines(label: &str, items: &[String]) -> String {
    if items.is_empty() {
        String::new()
    } else {
        format!("\n{label}：\n{}", items.iter().map(|item| format!("- {item}")).collect::<Vec<_>>().join("\n"))
    }
}

/// 角色轮次内调用；scope 来自 task-local（缺 scope 说明不是 Fusion 角色轮次，直接拒绝）。
pub(super) async fn execute(normalized_name: &str, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    let Some(scope) = current_scope() else {
        return text_response("failed", "team_* 工具仅在 Fusion Team 角色轮次内可用".to_string());
    };
    match normalized_name {
        "team_plan" => team_plan(&scope, input),
        "team_dispatch" => team_dispatch(&scope, input),
        "team_query" => team_query(&scope, input),
        "team_wait" => team_wait(&scope, input),
        "team_recall" => team_recall(&scope, input),
        "team_report" => team_report(&scope, input),
        "team_escalate" => team_escalate(&scope, input),
        "team_ask_user" => team_ask_user(&scope, input),
        "team_accept" => team_accept(&scope, input).await,
        other => text_response("failed", format!("未知团队工具 `{other}`")),
    }
}

fn require_role(scope: &FusionToolScope, allowed: &[FusionRole], name: &str) -> Option<ApiResult<ToolDispatchResponse>> {
    if allowed.contains(&scope.role) {
        None
    } else {
        Some(text_response(
            "failed",
            format!("{} 无权调用 {name}（派发矩阵）", scope.role.display_name()),
        ))
    }
}

fn team_plan(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Host], "team_plan") {
        return denied;
    }
    let work_items = input
        .get("work_items")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    if work_items.is_empty() {
        return text_response("failed", "work_items 不能为空".to_string());
    }
    let task_id = match resolve_task_id(scope) {
        Some(task_id) => task_id,
        None => {
            let task_id = new_id("ftask");
            let title = input
                .get("title")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().take(160).collect::<String>())
                .or_else(|| scope.task_title_hint.clone());
            let mandate = json!({
                "workspace_id": scope.workspace_id,
                "room_id": scope.room_id,
                "team_id": scope.team_id,
            })
            .to_string();
            store::fusion_task_insert(
                &scope.db_path,
                &task_id,
                &scope.team_id,
                &scope.room_id,
                &scope.workspace_id,
                title.as_deref(),
                None,
                &mandate,
            )?;
            task_id
        }
    };
    let ids = store::fusion_work_items_replace(&scope.db_path, &task_id, &work_items)?;
    store::fusion_task_update(
        &scope.db_path,
        &task_id,
        "executing",
        Some(&input.to_string()),
        Some("task"),
    )?;
    // 计划卡片进聊天室（§4.2.4）：标题 + 各工作项验收概要。
    let title = store::fusion_task_get(&scope.db_path, &task_id)
        .ok()
        .flatten()
        .and_then(|task| task.title)
        .unwrap_or_else(|| "任务计划".to_string());
    let mut card = format!("主持 · 任务计划\n{title}");
    for item in work_items.iter() {
        let item_title = item
            .get("title")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim();
        if item_title.is_empty() {
            continue;
        }
        card.push_str(&format!("\n- {item_title}"));
    }
    runtime::emit_room_message(&scope.room_id, "主持", "fusion-plan", &card);
    text_response(
        "ok",
        format!("已登记计划 task_id={task_id}，{} 个工作项：{}", ids.len(), ids.join(", ")),
    )
}

fn team_dispatch(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Host], "team_dispatch") {
        return denied;
    }
    let to = input
        .get("to")
        .and_then(JsonValue::as_str)
        .and_then(FusionRole::parse);
    let Some(to_role) = to else {
        return text_response("failed", "to 必须是 executor 或 querier".to_string());
    };
    if to_role == FusionRole::Host {
        return text_response("failed", "不能派发给主持自身".to_string());
    }
    let objective = input
        .get("objective")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if objective.is_empty() {
        return text_response("failed", "objective 必填".to_string());
    }
    let kind = input
        .get("kind")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if to_role == FusionRole::Querier {
                "query".to_string()
            } else {
                "exec_task".to_string()
            }
        });
    if to_role == FusionRole::Querier && kind != "query" {
        return text_response("failed", "查询者只接受 kind=query 的 brief".to_string());
    }
    let brief_id = new_id("brief");
    let payload = json!({
        "objective": objective,
        "constraints": str_list(input, "constraints"),
        "acceptance_criteria": str_list(input, "acceptance_criteria"),
        "context_refs": str_list(input, "context_refs"),
    });
    let work_item_id = input
        .get("work_item_id")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    // depends_on 由宿主强制：依赖未全部 done 时拒绝派发，而不是排队等。
    if let Some(work_item_id) = work_item_id.as_deref() {
        match store::fusion_work_item_get(&scope.db_path, work_item_id)? {
            Some(item) => {
                if item.status == "blocked" {
                    return text_response(
                        "failed",
                        format!("工作项 {work_item_id} 已 blocked，请先在计划中调整或换派"),
                    );
                }
                let deps: Vec<String> = serde_json::from_str::<JsonValue>(&item.depends_on_json)
                    .ok()
                    .and_then(|value| {
                        value.as_array().map(|items| {
                            items
                                .iter()
                                .filter_map(|entry| entry.as_str().map(str::to_string))
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                if !deps.is_empty() {
                    let items = store::fusion_work_items_for_task(&scope.db_path, &item.task_id)
                        .unwrap_or_default();
                    let mut unmet = Vec::new();
                    let mut broken = Vec::new();
                    for dep in deps {
                        match items.iter().find(|entry| entry.id == dep) {
                            Some(dep_item) if dep_item.status == "done" => {}
                            Some(dep_item) if dep_item.status == "blocked" || dep_item.status == "failed" => {
                                broken.push(format!("{dep}（{}）", dep_item.status));
                            }
                            Some(dep_item) => unmet.push(format!("{dep}（{}）", dep_item.status)),
                            None => unmet.push(format!("{dep}（不存在）")),
                        }
                    }
                    if !broken.is_empty() {
                        return text_response(
                            "failed",
                            format!(
                                "工作项 {work_item_id} 的依赖已失败/阻塞：{}。请先处理依赖或调整计划。",
                                broken.join("、")
                            ),
                        );
                    }
                    if !unmet.is_empty() {
                        return text_response(
                            "failed",
                            format!(
                                "工作项 {work_item_id} 的依赖未完成：{}。请先派发/等待依赖完成。",
                                unmet.join("、")
                            ),
                        );
                    }
                }
            }
            None => {
                return text_response("failed", format!("工作项 {work_item_id} 不存在"));
            }
        }
    }
    let task_id = resolve_task_id(scope);
    store::fusion_brief_insert(
        &scope.db_path,
        &store::FusionBriefRow {
            id: brief_id.clone(),
            task_id,
            work_item_id: work_item_id.clone(),
            room_id: scope.room_id.clone(),
            team_id: scope.team_id.clone(),
            from_role: scope.role.as_str().to_string(),
            to_role: to_role.as_str().to_string(),
            kind,
            payload_json: payload.to_string(),
            status: "queued".to_string(),
            result_json: None,
        },
    )?;
    if let Some(work_item_id) = work_item_id.as_deref() {
        let _ = store::fusion_work_item_set_status(&scope.db_path, work_item_id, "running");
    }
    text_response("ok", format!("brief_id={brief_id}（已入队，结果稍后送达）"))
}

fn team_query(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    match scope.role {
        FusionRole::Host => {}
        FusionRole::Executor if scope.querier_is_separate => {}
        _ => {
            return text_response(
                "failed",
                format!("{} 不能发起 team_query", scope.role.display_name()),
            );
        }
    }
    let question = input
        .get("question")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if question.is_empty() {
        return text_response("failed", "question 必填".to_string());
    }
    let brief_id = new_id("brief");
    let payload = json!({
        "objective": question,
        "scope": input.get("scope").and_then(JsonValue::as_str).unwrap_or("mixed"),
        "context_refs": str_list(input, "refs"),
    });
    store::fusion_brief_insert(
        &scope.db_path,
        &store::FusionBriefRow {
            id: brief_id.clone(),
            task_id: resolve_task_id(scope),
            work_item_id: None,
            room_id: scope.room_id.clone(),
            team_id: scope.team_id.clone(),
            from_role: scope.role.as_str().to_string(),
            to_role: "querier".to_string(),
            kind: "query".to_string(),
            payload_json: payload.to_string(),
            status: "queued".to_string(),
            result_json: None,
        },
    )?;
    text_response("ok", format!("brief_id={brief_id}（查询已入队）"))
}

fn team_wait(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    let ids = str_list(input, "brief_ids");
    if ids.is_empty() {
        return text_response("failed", "brief_ids 必填".to_string());
    }
    let mut lines = Vec::new();
    for id in ids {
        match store::fusion_brief_get(&scope.db_path, &id) {
            Ok(Some(brief)) => {
                let summary = brief
                    .result_json
                    .as_deref()
                    .and_then(|encoded| serde_json::from_str::<JsonValue>(encoded).ok())
                    .and_then(|value| value.get("summary").and_then(JsonValue::as_str).map(str::to_string))
                    .unwrap_or_default();
                lines.push(format!("{id}: {}{}", brief.status, if summary.is_empty() { String::new() } else { format!(" — {summary}") }));
            }
            Ok(None) => lines.push(format!("{id}: 不存在")),
            Err(error) => lines.push(format!("{id}: 读取失败 {error:?}")),
        }
    }
    text_response("ok", lines.join("\n"))
}

fn team_recall(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    let id = input
        .get("id")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if id.is_empty() {
        return text_response("failed", "id 必填".to_string());
    }
    if let Ok(Some((kind, content))) = store::fusion_artifact_get(&scope.db_path, &id) {
        return text_response("ok", format!("artifact {id}（{kind}）：\n{content}"));
    }
    if let Ok(Some(brief)) = store::fusion_brief_get(&scope.db_path, &id) {
        let mut text = format!("brief {id}（{} → {}，{}）：\n{}", brief.from_role, brief.to_role, brief.status, brief.payload_json);
        if let Some(result) = brief.result_json {
            text.push_str(&format!("\nresult：{result}"));
        }
        return text_response("ok", text);
    }
    text_response("failed", format!("未找到 {id}"))
}

fn brief_id_of(scope: &FusionToolScope, input: &JsonValue) -> Option<String> {
    input
        .get("brief_id")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| scope.brief_id.clone())
}

fn team_report(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Executor, FusionRole::Querier], "team_report") {
        return denied;
    }
    let Some(brief_id) = brief_id_of(scope, input) else {
        return text_response("failed", "缺少 brief_id".to_string());
    };
    let status = input
        .get("status")
        .and_then(JsonValue::as_str)
        .unwrap_or("partial");
    if !matches!(status, "done" | "partial" | "blocked" | "failed") {
        return text_response("failed", "status 必须是 done|partial|blocked|failed".to_string());
    }
    let summary = input
        .get("summary")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if summary.is_empty() {
        return text_response("failed", "summary 必填".to_string());
    }
    // 长详情溢出到 artifact，结果内联只留引用（Inline & Spill）。
    let details_ref = input
        .get("details")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|details| {
            let artifact_id = new_id("spill");
            let task_id = resolve_task_id(scope);
            let _ = store::fusion_artifact_insert(&scope.db_path, &artifact_id, task_id.as_deref(), "details", details);
            artifact_id
        });
    let result = json!({
        "status": status,
        "summary": summary.chars().take(1200).collect::<String>(),
        "evidence": input.get("evidence").cloned().unwrap_or(JsonValue::Null),
        "open_issues": str_list(input, "open_issues"),
        "details_ref": details_ref,
    });
    store::fusion_brief_finish(&scope.db_path, &brief_id, status, &result.to_string())?;
    text_response("ok", format!("已汇报 {brief_id}：{status}"))
}

fn team_escalate(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Executor], "team_escalate") {
        return denied;
    }
    let Some(brief_id) = brief_id_of(scope, input) else {
        return text_response("failed", "缺少 brief_id".to_string());
    };
    let problem = input
        .get("problem")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if problem.is_empty() {
        return text_response("failed", "problem 必填".to_string());
    }
    let result = json!({
        "status": "blocked",
        "summary": problem,
        "escalated": true,
        "attempts": str_list(input, "attempts"),
        "suggested_options": str_list(input, "suggested_options"),
    });
    store::fusion_brief_finish(&scope.db_path, &brief_id, "blocked", &result.to_string())?;
    text_response("ok", format!("已上报主持：{brief_id}"))
}

fn team_ask_user(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Host], "team_ask_user") {
        return denied;
    }
    let Some(task_id) = resolve_task_id(scope) else {
        return text_response(
            "failed",
            "当前无任务记录；问答路径请直接以普通回复向用户提问".to_string(),
        );
    };
    let question = input
        .get("question")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if question.is_empty() {
        return text_response("failed", "question 必填".to_string());
    }
    let options = input.get("options").cloned().unwrap_or_else(|| json!([]));
    let allow_free_text = input
        .get("allow_free_text")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true);
    let decision_id = new_id("decision");
    // options_json 存成 {options, allow_free_text}——回答 API 与前端卡片共用一个真源。
    let stored = json!({"options": options, "allow_free_text": allow_free_text});
    store::fusion_decision_insert(&scope.db_path, &decision_id, &task_id, &question, &stored.to_string())?;
    store::fusion_task_update(&scope.db_path, &task_id, "awaiting_user", None, None)?;
    // 决策卡片进聊天室：[fusion-decision] 行携带机读 JSON，前端渲染可点选项。
    let mut card = format!("主持 · 需要你的决定\n{question}");
    if let Some(items) = options.as_array() {
        for item in items {
            let label = item
                .get("label")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .trim();
            if label.is_empty() {
                continue;
            }
            let detail = item
                .get("detail")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(|text| format!(" — {text}"))
                .unwrap_or_default();
            card.push_str(&format!("\n- {label}{detail}"));
        }
    }
    card.push_str(&format!(
        "\n[fusion-decision]{}",
        json!({"id": decision_id, "question": question, "options": options, "allow_free_text": allow_free_text})
    ));
    runtime::emit_room_message(&scope.room_id, "主持", "fusion-decision", &card);
    text_response("ok", format!("已发起决策 {decision_id}；任务挂起等待用户回复"))
}

async fn team_accept(scope: &FusionToolScope, input: &JsonValue) -> ApiResult<ToolDispatchResponse> {
    if let Some(denied) = require_role(scope, &[FusionRole::Host], "team_accept") {
        return denied;
    }
    let work_item_id = input
        .get("work_item_id")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let verdict = input
        .get("verdict")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_string();
    if work_item_id.is_empty() || !matches!(verdict.as_str(), "accept" | "rework") {
        return text_response("failed", "work_item_id 与 verdict(accept|rework) 必填".to_string());
    }
    if verdict == "rework"
        && input
            .get("reason")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return text_response("failed", "rework 必须给出 reason".to_string());
    }
    let mut reason = input
        .get("reason")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    let mut evidence_json: Option<String> = None;
    // 确定性验收门（§6.3）：accept 前先跑宿主校验，失败自动转 rework 且证据入库——
    // 主持不能用模型正文"口头通过" FilesExist/Command 合同。
    let item = match store::fusion_work_item_get(&scope.db_path, &work_item_id)? {
        Some(item) => item,
        None => return text_response("failed", format!("工作项 {work_item_id} 不存在")),
    };
    if verdict == "accept" && item.verification_json.is_some() {
        match verification::evaluate(item.verification_json.as_deref(), &active_workspace_path()).await {
            verification::GateOutcome::Passed => {
                evidence_json = Some(json!({"gate": "passed"}).to_string());
            }
            verification::GateOutcome::Failed { reason: gate_reason, evidence } => {
                evidence_json = Some(
                    json!({"gate": "failed", "gate_reason": gate_reason, "gate_evidence": evidence})
                        .to_string(),
                );
                reason = Some(format!("验收门未过：{gate_reason}"));
                // 越过 accept 直接落 rework，retry 计数由 store 层统一递增。
                return apply_verdict(scope, &work_item_id, "rework", reason.as_deref(), evidence_json.as_deref(), true);
            }
            verification::GateOutcome::Invalid { reason: invalid } => {
                return text_response(
                    "failed",
                    format!("工作项 {work_item_id} 的验收合同非法（{invalid}），请先用 team_plan 修正"),
                );
            }
            verification::GateOutcome::NotRequired => {}
        }
    }
    apply_verdict(scope, &work_item_id, &verdict, reason.as_deref(), evidence_json.as_deref(), false)
}

/// 裁决落库 + 任务收敛；gate_rejected 表示确定性门触发的自动 rework（提示语区分）。
fn apply_verdict(
    scope: &FusionToolScope,
    work_item_id: &str,
    verdict: &str,
    reason: Option<&str>,
    evidence_json: Option<&str>,
    gate_rejected: bool,
) -> ApiResult<ToolDispatchResponse> {
    match store::fusion_work_item_verdict(&scope.db_path, work_item_id, verdict, reason, evidence_json)? {
        None => text_response("failed", format!("工作项 {work_item_id} 不存在")),
        Some((retries, exceeded)) => {
            let mut text = if verdict == "accept" {
                format!("工作项 {work_item_id} 已通过验收（含确定性门校验）")
            } else if gate_rejected {
                format!("工作项 {work_item_id} 确定性验收门未过，已按返工处理（第 {retries} 次）：{}", reason.unwrap_or_default())
            } else {
                format!("工作项 {work_item_id} 退回返工（第 {retries} 次）")
            };
            if exceeded {
                text.push_str("；已超过重试上限，工作项转 blocked——请调整方案、改派更强模型或 team_ask_user 问用户");
                runtime::emit_room_message(
                    &scope.room_id,
                    "Fusion Team",
                    "fusion-status",
                    &format!("工作项 {work_item_id} 重试达上限，已 blocked；等待主持决策。"),
                );
            }
            // 全部工作项 done → 任务自动标记 done，供编排器收尾。
            if let Some(task_id) = resolve_task_id(scope) {
                if let Ok(items) = store::fusion_work_items_for_task(&scope.db_path, &task_id) {
                    if !items.is_empty() && items.iter().all(|item| item.status == "done") {
                        let _ = store::fusion_task_update(&scope.db_path, &task_id, "done", None, None);
                    }
                }
            }
            text_response("ok", text)
        }
    }
}

/// brief 投递给角色时渲染的输入文本（角色线程的 user 消息）。
pub(super) fn render_brief_prompt(brief: &store::FusionBriefRow) -> String {
    let payload = serde_json::from_str::<JsonValue>(&brief.payload_json).unwrap_or_else(|_| json!({}));
    let mut text = format!(
        "【{} brief · {}】\nbrief_id={}{}",
        brief.kind,
        brief.id,
        brief.id,
        brief
            .work_item_id
            .as_deref()
            .map(|id| format!(" work_item_id={id}"))
            .unwrap_or_default()
    );
    if let Some(objective) = payload.get("objective").and_then(JsonValue::as_str) {
        text.push_str(&format!("\n目标：{objective}"));
    }
    for (key, label) in [
        ("constraints", "约束"),
        ("acceptance_criteria", "验收标准"),
        ("context_refs", "引用"),
    ] {
        if let Some(items) = payload.get(key).and_then(JsonValue::as_array) {
            let items = items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>();
            if !items.is_empty() {
                text.push_str(&format!("\n{label}：\n{}", items.iter().map(|item| format!("- {item}")).collect::<Vec<_>>().join("\n")));
            }
        }
    }
    text.push_str("\n\n完成后必须调用 team_report 汇报；受阻（非查询类）用 team_escalate 上报主持。");
    text
}

/// brief 结果送达派发方线程的事件文本。
pub(super) fn render_result_event(brief: &store::FusionBriefRow) -> String {
    let result = brief
        .result_json
        .as_deref()
        .and_then(|encoded| serde_json::from_str::<JsonValue>(encoded).ok());
    let summary = result
        .as_ref()
        .and_then(|value| value.get("summary").and_then(JsonValue::as_str))
        .unwrap_or("");
    let mut text = format!(
        "【brief 结果送达】{} → {} · {} · {}",
        brief.from_role, brief.to_role, brief.id, brief.status
    );
    if !summary.is_empty() {
        text.push_str(&format!("\n摘要：{summary}"));
    }
    if result
        .as_ref()
        .and_then(|value| value.get("escalated").and_then(JsonValue::as_bool))
        == Some(true)
    {
        for (key, label) in [("attempts", "已尝试"), ("suggested_options", "建议")] {
            if let Some(items) = result.as_ref().and_then(|value| value.get(key)).and_then(JsonValue::as_array) {
                let items = items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>();
                text.push_str(&join_lines(label, &items));
            }
        }
    }
    if let Some(evidence) = result.as_ref().and_then(|value| value.get("evidence")) {
        if !evidence.is_null() {
            text.push_str(&format!("\n证据：{}", evidence));
        }
    }
    text
}

/// 房间 fusion-brief 展示文本。
pub(super) fn render_brief_card(brief: &store::FusionBriefRow) -> String {
    let payload = serde_json::from_str::<JsonValue>(&brief.payload_json).unwrap_or_else(|_| json!({}));
    let from = FusionRole::parse(&brief.from_role).map(|role| role.display_name()).unwrap_or(&brief.from_role);
    let to = FusionRole::parse(&brief.to_role).map(|role| role.display_name()).unwrap_or(&brief.to_role);
    let mut text = format!("{from} → {to}");
    if let Some(objective) = payload.get("objective").and_then(JsonValue::as_str) {
        text.push_str(&format!("\n目标：{objective}"));
    }
    let acceptance = payload
        .get("acceptance_criteria")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    text.push_str(&join_lines("验收", &acceptance));
    text
}

/// 房间 fusion-result 展示文本。
pub(super) fn render_result_card(brief: &store::FusionBriefRow) -> String {
    let actor = FusionRole::parse(&brief.to_role).map(|role| role.display_name()).unwrap_or(&brief.to_role);
    let result = brief
        .result_json
        .as_deref()
        .and_then(|encoded| serde_json::from_str::<JsonValue>(encoded).ok())
        .unwrap_or_else(|| json!({}));
    let summary = result
        .get("summary")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    let mut text = format!("{actor} · {}", brief.status);
    if !summary.is_empty() {
        text.push_str(&format!("\n{summary}"));
    }
    if let Some(files) = result
        .get("evidence")
        .and_then(|value| value.get("files_changed"))
        .and_then(JsonValue::as_array)
    {
        let files = files
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        text.push_str(&join_lines("改动文件", &files));
    }
    let open = result
        .get("open_issues")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    text.push_str(&join_lines("遗留", &open));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_scope(db_path: &std::path::Path) -> FusionToolScope {
        FusionToolScope {
            team_id: "t1".to_string(),
            room_id: "room-1".to_string(),
            workspace_id: "ws".to_string(),
            db_path: db_path.to_path_buf(),
            role: FusionRole::Host,
            brief_id: None,
            task_id: Some("task-1".to_string()),
            host_web_access: false,
            querier_is_separate: false,
            task_title_hint: None,
        }
    }

    fn temp_db(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "coolzhu-fusion-tools-{tag}-{}-{}.sqlite3",
            std::process::id(),
            unix_timestamp_millis()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[tokio::test]
    async fn team_dispatch_blocks_until_dependencies_done() {
        let path = temp_db("deps");
        store::fusion_task_insert(&path, "task-1", "t1", "room-1", "ws", None, None, "{}")
            .expect("insert task");
        store::fusion_work_items_replace(
            &path,
            "task-1",
            &[
                json!({"id":"wi-a","title":"A","depends_on":["wi-b"]}),
                json!({"id":"wi-b","title":"B"}),
            ],
        )
        .expect("replace items");
        // 依赖未完成 → 拒绝派发，且不产生 brief。
        let input = json!({"to":"executor","work_item_id":"wi-a","objective":"做 A"});
        let resp = scope(host_scope(&path), execute("team_dispatch", &input))
            .await
            .expect("dispatch");
        assert_eq!(resp.status, "failed");
        assert!(resp
            .tool_result_text
            .as_deref()
            .unwrap_or_default()
            .contains("依赖未完成"));
        assert!(store::fusion_briefs_queued(&path, "room-1", "t1")
            .expect("queued")
            .is_empty());
        // 依赖完成 → 允许派发且工作项转 running。
        store::fusion_work_item_set_status(&path, "wi-b", "done").expect("done dep");
        let resp = scope(host_scope(&path), execute("team_dispatch", &input))
            .await
            .expect("dispatch2");
        assert_eq!(resp.status, "ok");
        assert_eq!(store::fusion_briefs_queued(&path, "room-1", "t1")
            .expect("queued2")
            .len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn team_ask_user_writes_pending_decision_and_holds_task() {
        let path = temp_db("ask");
        store::fusion_task_insert(&path, "task-1", "t1", "room-1", "ws", None, None, "{}")
            .expect("insert task");
        let input = json!({
            "question": "选哪个？",
            "options": [{"id":"a","label":"方案 A"}],
            "allow_free_text": false,
        });
        let resp = scope(host_scope(&path), execute("team_ask_user", &input))
            .await
            .expect("ask");
        assert_eq!(resp.status, "ok");
        let pending = store::fusion_decisions_pending(&path, "task-1").expect("pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, "选哪个？");
        let task = store::fusion_task_get(&path, "task-1").expect("get").expect("task");
        assert_eq!(task.status, "awaiting_user");
        let _ = std::fs::remove_file(&path);
    }
}
