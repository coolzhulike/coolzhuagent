//! 将宿主规划请求交回当前聊天模型；只交换回复，不创建远端会话或执行输入。
use api::{InputContentBlock, MessageRequest};
use futures_util::task::AtomicWaker;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::oneshot;

// 留出 MCP 包装余量，避免第三方客户端将大快照转成不可读取的本地溢出文件。
const REQUEST_PAGE_BYTES: usize = 6000;

enum ResponseContract {
    Planning(computer_use::ComputerUseSurface),
    BrowserVerification { criteria_count: usize, node_count: usize },
}

struct Pending {
    id: String,
    content: Vec<Value>,
    image_presented: bool,
    text: String,
    presented_bytes: usize,
    response_contract: Option<ResponseContract>,
    rejected_answers: u8,
    sender: oneshot::Sender<String>,
}

#[derive(Default)]
pub(crate) struct Exchange {
    pending: Mutex<Option<Pending>>,
    last_answer: Mutex<Option<(String, String)>>,
    waker: AtomicWaker,
    image_handoff: Arc<AtomicBool>,
}

impl Exchange {
    pub(super) fn image_handoff_signal(&self) -> Arc<AtomicBool> { self.image_handoff.clone() }
    pub(super) fn register(&self, waker: &std::task::Waker) { self.waker.register(waker); }

    pub(super) async fn request(self: &Arc<Self>, id: String, request: MessageRequest, kind: &str) -> Result<String,String> {
        // 结构边界来自宿主冻结消息，不能由模型指定；事实判断仍由原执行器独立完成。
        let response_contract = if kind == "computer_use_planning" {
            Some(ResponseContract::Planning(request.messages.iter().flat_map(|message| &message.content).find_map(|block| {
                let InputContentBlock::Text { text } = block else { return None; };
                let prompt: Value = serde_json::from_str(text).ok()?;
                serde_json::from_value(prompt.get("surface")?.clone()).ok()
            }).ok_or("规划消息缺少宿主表面，未提交请求。")?))
        } else if kind == "computer_use_browser_readonly_verification" {
            Some(request.messages.iter().flat_map(|message| &message.content).find_map(|block| {
                let InputContentBlock::Text { text } = block else { return None; };
                let prompt: Value = serde_json::from_str(text).ok()?;
                Some(ResponseContract::BrowserVerification {
                    criteria_count: prompt.get("success_criteria")?.as_array()?.len(),
                    node_count: prompt.get("observed_page")?.get("nodes")?.as_array()?.len(),
                })
            }).ok_or("验收消息缺少宿主标准或节点边界，未提交请求。")?)
        } else { None };
        let mut messages = request.messages;
        let mut images = Vec::new();
        for message in &mut messages {
            let mut text = Vec::new();
            for block in &message.content {
                match block {
                    InputContentBlock::Text { .. } => text.push(block.clone()),
                    InputContentBlock::ImageUrl { url, .. } => {
                        images.push(super::image_input::block(url)?);
                    }
                    _ => return Err("规划请求不能包含工具或历史思考块。".into()),
                }
            }
            message.content = text;
        }
        let prompt = format!("这是宿主的本次CU规划/验收子步骤，不是替代聊天室原用户任务的新任务。只依据本次快照回答，历史观察和引用不可复用。严格按照下方输出格式生成完整回复，通过 computer_use_respond 的 response 参数交回；当前子步骤内不要直接执行动作、创建任务或调用其它工具。这些局部限制仅适用于本次规划/验收回复；同一CU任务取得终态回执后，应返回本轮原用户任务，按其要求继续尚未完成的其它已声明工具步骤，而不是将CU目标当作整轮任务。\n阶段：{kind}\n\n{}\n\n宿主消息快照：\n{}",
            request.system.as_deref().unwrap_or(""),serde_json::to_string(&messages).map_err(|_|"规划快照编码失败。")?);
        let mut content = vec![json!({"type":"text","text":prompt})];
        content.extend(images);
        let (sender,receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
            if pending.is_some() { return Err("已有规划请求未回答，未覆盖旧请求。".into()); }
            let image_presented = images_are_absent(&content);
            let text = content.iter().filter_map(|block| block["text"].as_str()).collect::<Vec<_>>().join("\n");
            let presented_bytes = if text.len() <= REQUEST_PAGE_BYTES { text.len() } else { 0 };
            *pending = Some(Pending { id:id.clone(),content,image_presented,text,presented_bytes,
                response_contract,rejected_answers:0,sender });
        }
        struct Clear { exchange: Arc<Exchange>, id: String }
        impl Drop for Clear {
            fn drop(&mut self) {
                let mut pending = self.exchange.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if pending.as_ref().is_some_and(|item| item.id == self.id) {
                    pending.take();
                    self.exchange.image_handoff.store(false, Ordering::Release);
                }
            }
        }
        let _clear = Clear { exchange:self.clone(),id };
        self.waker.wake();
        receiver.await.map_err(|_|"当前规划连接已结束，未生成替代回复。".into())
    }

    pub(super) fn waiting(&self, job: &str) -> Result<Option<Value>,String> {
        let pending = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
        Ok(pending.as_ref().map(|item| {
            if !item.image_presented {
                self.image_handoff.store(true, Ordering::Release);
                return json!({"content":[{"type":"text","text":json!({"status":"running","goal_achieved":false,
                    "job_id":job,"request_id":item.id,"next_tool":"host_image_handoff",
                    "message":"本次需要原图。宿主将在同一会话下一段提示发送原图。现在立即结束这一段生成，不输出最终答案，不回答规划，不调用其它工具；宿主任务仍在运行，不重提 perform。"}).to_string()}],"isError":false});
            }
            if item.text.len() > REQUEST_PAGE_BYTES {
                return json!({"content":[{"type":"text","text":json!({"status":"running","goal_achieved":false,
                    "job_id":job,"request_id":item.id,"next_tool":"computer_use_read_request","offset":0,
                    "total_bytes":item.text.len(),
                    "message":"规划快照采用只读分页。调用 computer_use_read_request，从 offset=0 开始依次读取 next_offset，直至 complete=true；合并完整快照后才用 computer_use_respond 回答。不得读取溢出文件或重提 perform。"}).to_string()}],"isError":false});
            }
            let mut content = vec![json!({"type":"text","text":json!({"status":"running","goal_achieved":false,
                "job_id":job,"request_id":item.id,"next_tool":"computer_use_respond",
                "message":"当前任务等待你回答下方规划/验收请求。用 computer_use_respond 交回完整回复；这不是完成回执，不要重提 perform。"}).to_string()})];
            content.extend(item.content.iter().filter(|block|block["type"] == "text").cloned());
            json!({"content":content,"isError":false})
        }))
    }

    /// 页偏移只定位当前请求的 UTF-8 文本，不接受文件路径，也不触发电脑输入。
    pub(super) fn read_request(&self, job: &str, id: &str, offset: usize) -> Result<Value,String> {
        let mut slot = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
        let item = slot.as_mut().filter(|item|item.id == id).ok_or("规划请求已结束或不属于当前任务。")?;
        if !item.image_presented { return Err("本次原图尚未发送，不能读取替代文字。".into()); }
        if offset > item.presented_bytes || offset >= item.text.len() || !item.text.is_char_boundary(offset) {
            return Err("分页偏移无效；须从0开始按 next_offset 读取。".into());
        }
        let mut end = offset.saturating_add(REQUEST_PAGE_BYTES).min(item.text.len());
        while !item.text.is_char_boundary(end) { end -= 1; }
        item.presented_bytes = item.presented_bytes.max(end);
        Ok(json!({"content":[{"type":"text","text":json!({"status":"running","goal_achieved":false,
            "job_id":job,"request_id":id,"offset":offset,"next_offset":end,"total_bytes":item.text.len(),
            "complete":end == item.text.len(),"snapshot_text":&item.text[offset..end],
            "next_tool":if end == item.text.len() {"computer_use_respond"} else {"computer_use_read_request"}}).to_string()}],"isError":false}))
    }

    /// 仅由已结束当前生成段的宿主调用；不会通过 MCP 接受图片已读标记。
    pub(super) fn take_image_continuation(&self, job: &str) -> Result<Option<Vec<Value>>,String> {
        let mut slot = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
        let Some(item) = slot.as_mut().filter(|item| !item.image_presented) else { return Ok(None); };
        if !self.image_handoff.load(Ordering::Acquire) { return Ok(None); }
        let mut content = vec![json!({"type":"text","text":format!(
            "宿主续接原任务，未新建会话。job_id={job}，request_id={}。本次提示包含原图，请按下方当前快照规划/验收，通过 computer_use_respond 交回完整回复。随后等待同一 job 的最终回执；不要重提 perform。",item.id)})];
        content.extend(item.content.clone());
        item.image_presented = true;
        item.presented_bytes = item.text.len();
        self.image_handoff.store(false, Ordering::Release);
        Ok(Some(content))
    }

    pub(super) fn answer(&self, id: &str, response: &str) -> Result<(),String> {
        if response.trim().is_empty() || response.len() > 1024*1024 { return Err("规划回复为空或超过大小限制。".into()); }
        let digest = super::chat::digest(response.as_bytes());
        let mut last = self.last_answer.lock().map_err(|_|"规划回执状态不可用。")?;
        if let Some((_,original)) = last.as_ref().filter(|(previous,_)| previous == id) {
            return if original == &digest { Ok(()) } else { Err("已回答的规划请求不能更改回复。".into()) };
        }
        let mut slot = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
        if !slot.as_ref().is_some_and(|item| item.id == id) { return Err("规划请求已结束或不属于当前任务。".into()); }
        if slot.as_ref().is_some_and(|item| !item.image_presented) { return Err("本次原图尚未发送，不能接受无图验收。".into()); }
        if slot.as_ref().is_some_and(|item| item.presented_bytes < item.text.len()) { return Err("规划快照尚未读完；请继续读取剩余分页。".into()); }
        let item = slot.as_mut().expect("已核对规划请求存在");
        if let Some(contract) = &item.response_contract {
            let (invalid, format_hint) = match contract {
                ResponseContract::Planning(surface) => (
                    crate::computer_use_planner::parse_planner_response(response, *surface).is_err(),
                    "summary只能放在顶层；action仅含kind、target、arguments，参数须符合当前表面。".to_string()),
                ResponseContract::BrowserVerification { criteria_count, node_count } => (
                    crate::native_browser_verification::validate_reply_shape(response, *criteria_count, *node_count).is_err(),
                    format!("criteria须恰好有{criteria_count}项；每个index从0到{}恰好出现一次，不增漏或重复。每项仅含index、met、evidence、node_indices；非空evidence最多512字符，节点引用最多8个且须来自本次原数组（节点总数{node_count}）。", criteria_count.saturating_sub(1))),
            };
            if invalid && item.rejected_answers < 2 {
                item.rejected_answers += 1;
                // 仅拒绝未接收的格式；不修写模型回复、不重新执行动作，也不延长原请求的预算。
                // 超过两次后交回原解析器，按既有失败终态保留脱敏诊断。
                return Err(format!("CU回复格式未接收，未执行动作。格式反馈{}/2：请在同一job_id/request_id内纠正回复；不要重提perform。两次反馈后的下一份不合规回复将交由原执行器终止。严格遵循当前response_schema：{format_hint}保持原快照与引用；原预算、取消与权限仍生效。", item.rejected_answers));
            }
        }
        let pending = slot.take().expect("已核对规划请求存在");
        pending.sender.send(response.into()).map_err(|_|"规划请求已取消；迟到回复未执行。")?;
        *last = Some((id.into(),digest));
        Ok(())
    }
}

fn images_are_absent(content: &[Value]) -> bool { !content.iter().any(|block| block["type"] == "image") }

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn 验收重复索引可在原请求内纠正但不生成成功判断() {
        let exchange = Arc::new(Exchange::default());
        let request = api::MessageRequest { model:"actual".into(),max_tokens:100,
            messages:vec![api::InputMessage::user_text(r#"{"success_criteria":["first","second"],"observed_page":{"nodes":[{"name":"fact"}]}}"#)],
            system:None,tools:None,tool_choice:None,reasoning_effort:None,stream:false };
        let bad = r#"{"criteria":[{"index":0,"met":true,"evidence":"fact","node_indices":[0]},{"index":0,"met":true,"evidence":"fact","node_indices":[0]}]}"#;
        let good = r#"{"criteria":[{"index":0,"met":true,"evidence":"fact","node_indices":[0]},{"index":1,"met":false,"evidence":"no evidence","node_indices":[]}]}"#;
        let mut pending = Box::pin(exchange.request("verdict".into(),request.clone(),"computer_use_browser_readonly_verification"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        assert!(exchange.answer("verdict",bad).unwrap_err().contains("恰好有2项"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        exchange.answer("verdict",good).unwrap();
        assert_eq!(pending.await.unwrap(),good);
        assert!(exchange.answer("verdict",bad).is_err());
        let mut cancelled = Box::pin(exchange.request("cancelled-verdict".into(),request,"computer_use_browser_readonly_verification"));
        assert!(futures_util::poll!(&mut cancelled).is_pending());
        assert!(exchange.answer("cancelled-verdict",bad).is_err());
        drop(cancelled);
        assert!(exchange.answer("cancelled-verdict",good).is_err());
    }
    #[tokio::test]
    async fn 格式纠正保留原请求且超限仍交由执行器拒绝() {
        let exchange = Arc::new(Exchange::default());
        let request = api::MessageRequest { model:"actual".into(),max_tokens:100,
            messages:vec![api::InputMessage::user_text(r#"{"surface":"browser"}"#)],
            system:None,tools:None,tool_choice:None,reasoning_effort:None,stream:false };
        let bad = r#"{"done":false,"action":{"kind":"key_combination","target":"dom-current","arguments":{"keys":["enter"]},"summary":"wrong location"}}"#;
        let good = r#"{"done":false,"summary":"submit","action":{"kind":"key_combination","target":"dom-current","arguments":{"keys":["enter"]}}}"#;
        let mut pending = Box::pin(exchange.request("format".into(),request.clone(),"computer_use_planning"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        assert!(exchange.answer("format",bad).unwrap_err().contains("未执行动作"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        exchange.answer("format",good).unwrap();
        assert_eq!(pending.await.unwrap(),good);
        exchange.answer("format",good).unwrap();
        assert!(exchange.answer("format",bad).is_err());
        let mut exhausted = Box::pin(exchange.request("exhausted".into(),request,"computer_use_planning"));
        assert!(futures_util::poll!(&mut exhausted).is_pending());
        assert!(exchange.answer("exhausted",bad).is_err());
        assert!(exchange.answer("exhausted",bad).is_err());
        exchange.answer("exhausted",bad).unwrap();
        let rejected = exhausted.await.unwrap();
        assert!(crate::computer_use_planner::parse_planner_response(&rejected,computer_use::ComputerUseSurface::Browser).is_err());
    }
    #[tokio::test]
    async fn large_snapshot_pages_preserve_utf8_and_reject_unread_or_stale_answers() {
        let exchange = Arc::new(Exchange::default());
        let request = api::MessageRequest { model:"actual".into(),max_tokens:100,
            messages:vec![api::InputMessage::user_text(&"竹林校验🟢".repeat(2500))],
            system:None,tools:None,tool_choice:None,reasoning_effort:None,stream:false };
        let mut pending = Box::pin(exchange.request("paged".into(),request,"planning"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        let waiting = exchange.waiting("one").unwrap().unwrap();
        let status: Value = serde_json::from_str(waiting["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(status["next_tool"],"computer_use_read_request");
        assert!(exchange.read_request("one","other",0).is_err());
        assert!(exchange.read_request("one","paged",REQUEST_PAGE_BYTES).is_err());
        assert!(exchange.answer("paged","未读快照").is_err());
        let original = exchange.pending.lock().unwrap().as_ref().unwrap().text.clone();
        let mut restored = String::new();
        let mut offset = 0;
        loop {
            let response = exchange.read_request("one","paged",offset).unwrap();
            let page: Value = serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
            let text = page["snapshot_text"].as_str().unwrap();
            assert!(text.len() <= REQUEST_PAGE_BYTES);
            restored.push_str(text);
            offset = page["next_offset"].as_u64().unwrap() as usize;
            if page["complete"] == true { break; }
        }
        assert_eq!(restored,original);
        exchange.answer("paged","本轮回复").unwrap();
        assert_eq!(pending.await.unwrap(),"本轮回复");
        assert!(exchange.read_request("one","paged",0).is_err());
    }

    #[tokio::test]
    async fn original_images_require_one_host_handoff_and_cancellation_clears_it() {
        let exchange = Arc::new(Exchange::default());
        let request: api::MessageRequest = serde_json::from_value(json!({"model":"actual","max_tokens":100,
            "messages":[{"role":"user","content":[{"type":"text","text":"本次原图"},
                {"type":"image_url","url":"data:image/png;base64,YQ=="}]}],"stream":false})).unwrap();
        let mut pending = Box::pin(exchange.request("image".into(),request.clone(),"verification"));
        assert!(futures_util::poll!(&mut pending).is_pending());
        assert!(exchange.take_image_continuation("one").unwrap().is_none());
        let waiting = exchange.waiting("one").unwrap().unwrap();
        assert!(!waiting["content"].as_array().unwrap().iter().any(|block|block["type"]=="image"));
        assert!(exchange.image_handoff_signal().load(Ordering::Acquire));
        assert!(exchange.answer("image","无图猜测").is_err());
        let images = exchange.take_image_continuation("one").unwrap().unwrap();
        assert_eq!(images.iter().filter(|block|block["type"]=="image").count(),1);
        assert!(exchange.take_image_continuation("one").unwrap().is_none());
        assert!(!exchange.image_handoff_signal().load(Ordering::Acquire));
        exchange.answer("image","本轮图像回复").unwrap();
        assert_eq!(pending.await.unwrap(),"本轮图像回复");
        let mut cancelled = Box::pin(exchange.request("cancelled-image".into(),request,"verification"));
        assert!(futures_util::poll!(&mut cancelled).is_pending());
        exchange.waiting("one").unwrap();
        drop(cancelled);
        assert!(!exchange.image_handoff_signal().load(Ordering::Acquire));
        assert!(exchange.take_image_continuation("one").unwrap().is_none());
        assert!(exchange.answer("cancelled-image","迟到回复").is_err());
    }
    #[tokio::test]
    async fn exchange_rejects_other_requests_and_never_replaces_an_accepted_answer() {
        let exchange = Arc::new(Exchange::default());
        let request = api::MessageRequest { model:"actual".into(),max_tokens:100,messages:vec![api::InputMessage::user_text("本次快照")],
            system:None,tools:None,tool_choice:None,reasoning_effort:None,stream:false };
        let pending = exchange.request("first".into(),request.clone(),"planning"); tokio::pin!(pending);
        assert!(futures_util::poll!(&mut pending).is_pending());
        let value = exchange.waiting("one").unwrap().unwrap();
        let status: Value = serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap();
        let id = status["request_id"].as_str().unwrap();
        assert!(exchange.answer("other-request","{}").is_err());
        assert!(exchange.request("second".into(),request.clone(),"other").await.is_err());
        exchange.answer(id,"{\"plan\":1}").unwrap();
        assert_eq!(pending.await.unwrap(),"{\"plan\":1}");
        exchange.answer(id,"{\"plan\":1}").unwrap();
        assert!(exchange.answer(id,"{\"plan\":2}").is_err());
        assert!(exchange.waiting("one").unwrap().is_none());
        // 调用者取消后必须清掉请求；迟到回复不能被下一轮接收。
        let mut cancelled = Box::pin(exchange.request("cancelled".into(),request.clone(),"planning"));
        assert!(futures_util::poll!(&mut cancelled).is_pending());
        drop(cancelled);
        assert!(exchange.waiting("one").unwrap().is_none());
        assert!(exchange.answer("cancelled","迟到回复").is_err());
        let mut next = Box::pin(exchange.request("next".into(),request,"verification"));
        assert!(futures_util::poll!(&mut next).is_pending());
        assert!(exchange.answer("cancelled","迟到回复").is_err());
        exchange.answer("next","本轮回复").unwrap();
        assert_eq!(next.await.unwrap(),"本轮回复");
    }
}
