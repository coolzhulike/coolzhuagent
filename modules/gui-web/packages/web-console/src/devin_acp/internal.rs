//! 规划和验收由当前聊天中的模型回答，共享一个远端上下文；宿主仍校验每个动作。
use super::{chat, planning_exchange::Exchange};
use crate::{AgentSessionDto, FrozenParentContext};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(crate) async fn complete(
    agent: AgentSessionDto, request: api::MessageRequest, parent: FrozenParentContext,
    call: String, kind: String, remaining: Duration,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>, exchange: Arc<Exchange>,
) -> Result<api::MessageResponse, api::ApiError> {
    let db = parent.runtime_db_path.as_deref().ok_or_else(|| chat::error("规划请求缺少父数据库。"))?;
    crate::validate_frozen_parent_relations(db,&parent)
        .map_err(|reason| chat::error(format!("规划父运行无效：{reason:?}")))?;
    if request.model != agent.model || request.tools.as_ref().is_some_and(|tools| !tools.is_empty())
        || request.tool_choice.is_some() || call.is_empty() {
        return Err(chat::error("规划仅接受当前模型的无工具回复请求。"));
    }
    let budget = parent.root_budget.as_ref().ok_or_else(|| chat::error("规划缺少父预算。"))?;
    let deadline = tokio::time::Instant::now() + remaining.min(budget.remaining());
    if cancelled() || tokio::time::Instant::now() >= deadline {
        return Err(chat::error("规划请求在提交前已停止。"));
    }
    let request_id = crate::random_hex_identifier(48,"CU 规划请求").map_err(chat::error)?;
    crate::append_runtime_run_event(db,parent.parent_run_id.as_deref().ok_or_else(||chat::error("规划缺少父运行。"))?,
        "devin.planning_requested",json!({"request_id":request_id,"call_id":call,"kind":kind,"model":agent.model,"transport":"current_chat_tool_bridge"}))
        .map_err(|_|chat::error("规划来源记录失败；未发出请求。"))?;
    let answer = tokio::select! { biased;
        _ = async { loop { if cancelled() { break; } tokio::time::sleep(Duration::from_millis(50)).await; } } =>
            return Err(chat::error("规划已停止；迟到回复不产生动作。")),
        _ = tokio::time::sleep_until(deadline) => return Err(chat::error("规划已超过父预算；未创建替代会话。")),
        result = exchange.request(request_id,request,&kind) => result.map_err(chat::error)?,
    };
    if cancelled() || tokio::time::Instant::now() >= deadline {
        return Err(chat::error("规划回复到达时本轮已停止。"));
    }
    Ok(api::MessageResponse { id:None,kind:"message".into(),role:"assistant".into(),
        content:vec![api::OutputContentBlock::Text { text:answer }],model:agent.model,
        stop_reason:Some("end_turn".into()),stop_sequence:None,request_id:None,
        // 用量属于实际外层 ACP 会话；不为工具参数中的回复重复计费或伪造独立供应商请求。
        usage:api::Usage { input_tokens:0,output_tokens:0,cache_creation_input_tokens:0,cache_read_input_tokens:0 } })
}
