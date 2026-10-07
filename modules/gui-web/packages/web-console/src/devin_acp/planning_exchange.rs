//! 将宿主规划请求交回当前聊天模型；只交换回复，不创建远端会话或执行输入。
use api::{InputContentBlock, MessageRequest};
use futures_util::task::AtomicWaker;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::oneshot;

struct Pending {
    id: String,
    content: Vec<Value>,
    image_presented: bool,
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
        let prompt = format!("这是宿主的本次规划/验收请求。只依据本次快照回答，历史观察和引用不可复用。严格按照下方输出格式生成完整回复，通过 computer_use_respond 的 response 参数交回；不要直接执行动作、创建任务或调用其它工具。\n阶段：{kind}\n\n{}\n\n宿主消息快照：\n{}",
            request.system.as_deref().unwrap_or(""),serde_json::to_string(&messages).map_err(|_|"规划快照编码失败。")?);
        let mut content = vec![json!({"type":"text","text":prompt})];
        content.extend(images);
        let (sender,receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_|"规划等待状态不可用。")?;
            if pending.is_some() { return Err("已有规划请求未回答，未覆盖旧请求。".into()); }
            let image_presented = images_are_absent(&content);
            *pending = Some(Pending { id:id.clone(),content,image_presented,sender });
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
            let mut content = vec![json!({"type":"text","text":json!({"status":"running","goal_achieved":false,
                "job_id":job,"request_id":item.id,"next_tool":"computer_use_respond",
                "message":"当前任务等待你回答下方规划/验收请求。用 computer_use_respond 交回完整回复；这不是完成回执，不要重提 perform。"}).to_string()})];
            content.extend(item.content.iter().filter(|block|block["type"] == "text").cloned());
            json!({"content":content,"isError":false})
        }))
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
