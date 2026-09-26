//! CLI 协议流事实：累计快照合并与未确认终态的明确失败。
use runtime::{RuntimeError, TokenUsage};

#[derive(Default)]
pub(crate) struct StreamFacts {
    usage: Option<TokenUsage>,
    evidence: api::UsageEvidence,
}

impl StreamFacts {
    pub(crate) fn merge(&mut self, evidence: api::UsageEvidence) {
        if evidence.known_mask()==0 {return;}
        self.evidence.merge(evidence);
        let value=self.evidence.usage();
        self.usage=Some(TokenUsage {input_tokens:value.input_tokens,output_tokens:value.output_tokens,
            cache_creation_input_tokens:value.cache_creation_input_tokens,cache_read_input_tokens:value.cache_read_input_tokens});
    }

    pub(crate) fn usage(&self) -> Option<TokenUsage> { self.usage }

    pub(crate) fn incomplete(&self, reason: &str) -> RuntimeError {
        let number=|value:Option<u32>|value.map_or_else(||"未知".to_string(),|n|n.to_string());
        let usage=if self.evidence.known_mask()==0 {"未收到用量快照，用量未知".to_string()} else {format!(
            "已收到累计用量快照：输入 {}、输出 {}、缓存写入 {}、缓存读取 {}；此快照不是完整账单",
            number(self.evidence.input_tokens),number(self.evidence.output_tokens),
            number(self.evidence.cache_write_tokens),number(self.evidence.cache_read_tokens))};
        RuntimeError::new(format!(
            "模型流未确认完成：{reason}。已输出内容仅为不完整结果；本次流中的工具请求均未执行，也不会自动重放请求。{usage}。"
        ))
    }
}
