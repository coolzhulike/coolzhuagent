use crate::session::Session;
use serde::{Deserialize, Serialize};

const DEFAULT_INPUT_COST_PER_MILLION: f64 = 15.0;
const DEFAULT_OUTPUT_COST_PER_MILLION: f64 = 75.0;
const DEFAULT_CACHE_CREATION_COST_PER_MILLION: f64 = 18.75;
const DEFAULT_CACHE_READ_COST_PER_MILLION: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cache_creation_cost_per_million: f64,
    pub cache_read_cost_per_million: f64,
}

impl ModelPricing {
    #[must_use]
    pub const fn default_sonnet_tier() -> Self {
        Self {
            input_cost_per_million: DEFAULT_INPUT_COST_PER_MILLION,
            output_cost_per_million: DEFAULT_OUTPUT_COST_PER_MILLION,
            cache_creation_cost_per_million: DEFAULT_CACHE_CREATION_COST_PER_MILLION,
            cache_read_cost_per_million: DEFAULT_CACHE_READ_COST_PER_MILLION,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

/// 一次由供应商上报的用量事实。
///
/// 每个维度都是 `Option`：`None` 表示**该维度未知**（供应商没有返回），**不是 0**。
/// §2.4 要求"缺 usage 为 unknown，不是 0"——用 0 表示缺失会把"没拿到"与"确实是 0"
/// 混为一谈，并使汇总看起来完整。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReportedUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

impl ReportedUsage {
    /// 完全没有拿到用量事实。
    #[must_use]
    pub fn unknown() -> Self {
        Self::default()
    }

    /// 是否有任一维度未知。
    #[must_use]
    pub fn has_unknown(&self) -> bool {
        self.input_tokens.is_none()
            || self.output_tokens.is_none()
            || self.cache_creation_input_tokens.is_none()
            || self.cache_read_input_tokens.is_none()
    }

    /// 已知维度的求和；未知维度按 0 参与，**调用方必须同时用 `has_unknown()` 标注
    /// 该轮不完整**，不得只报汇总。
    #[must_use]
    pub fn known_totals(self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens.unwrap_or(0),
            output_tokens: self.output_tokens.unwrap_or(0),
            cache_creation_input_tokens: self.cache_creation_input_tokens.unwrap_or(0),
            cache_read_input_tokens: self.cache_read_input_tokens.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageCostEstimate {
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub cache_creation_cost_usd: f64,
    pub cache_read_cost_usd: f64,
}

impl UsageCostEstimate {
    #[must_use]
    pub fn total_cost_usd(self) -> f64 {
        self.input_cost_usd
            + self.output_cost_usd
            + self.cache_creation_cost_usd
            + self.cache_read_cost_usd
    }
}

#[must_use]
pub fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("haiku") {
        return Some(ModelPricing {
            input_cost_per_million: 1.0,
            output_cost_per_million: 5.0,
            cache_creation_cost_per_million: 1.25,
            cache_read_cost_per_million: 0.1,
        });
    }
    if normalized.contains("opus") {
        return Some(ModelPricing {
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_creation_cost_per_million: 18.75,
            cache_read_cost_per_million: 1.5,
        });
    }
    if normalized.contains("sonnet") {
        return Some(ModelPricing::default_sonnet_tier());
    }
    None
}

impl TokenUsage {
    #[must_use]
    pub fn total_tokens(self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    #[must_use]
    pub fn estimate_cost_usd(self) -> UsageCostEstimate {
        self.estimate_cost_usd_with_pricing(ModelPricing::default_sonnet_tier())
    }

    #[must_use]
    pub fn estimate_cost_usd_with_pricing(self, pricing: ModelPricing) -> UsageCostEstimate {
        UsageCostEstimate {
            input_cost_usd: cost_for_tokens(self.input_tokens, pricing.input_cost_per_million),
            output_cost_usd: cost_for_tokens(self.output_tokens, pricing.output_cost_per_million),
            cache_creation_cost_usd: cost_for_tokens(
                self.cache_creation_input_tokens,
                pricing.cache_creation_cost_per_million,
            ),
            cache_read_cost_usd: cost_for_tokens(
                self.cache_read_input_tokens,
                pricing.cache_read_cost_per_million,
            ),
        }
    }

    #[must_use]
    pub fn summary_lines(self, label: &str) -> Vec<String> {
        self.summary_lines_for_model(label, None)
    }

    #[must_use]
    pub fn summary_lines_for_model(self, label: &str, model: Option<&str>) -> Vec<String> {
        let pricing = model.and_then(pricing_for_model);
        let cost = pricing.map_or_else(
            || self.estimate_cost_usd(),
            |pricing| self.estimate_cost_usd_with_pricing(pricing),
        );
        let model_suffix =
            model.map_or_else(String::new, |model_name| format!(" model={model_name}"));
        let pricing_suffix = if pricing.is_some() {
            ""
        } else if model.is_some() {
            " pricing=estimated-default"
        } else {
            ""
        };
        vec![
            format!(
                "{label}: total_tokens={} input={} output={} cache_write={} cache_read={} estimated_cost={}{}{}",
                self.total_tokens(),
                self.input_tokens,
                self.output_tokens,
                self.cache_creation_input_tokens,
                self.cache_read_input_tokens,
                format_usd(cost.total_cost_usd()),
                model_suffix,
                pricing_suffix,
            ),
            format!(
                "  cost breakdown: input={} output={} cache_write={} cache_read={}",
                format_usd(cost.input_cost_usd),
                format_usd(cost.output_cost_usd),
                format_usd(cost.cache_creation_cost_usd),
                format_usd(cost.cache_read_cost_usd),
            ),
        ]
    }
}

fn cost_for_tokens(tokens: u32, usd_per_million_tokens: f64) -> f64 {
    f64::from(tokens) / 1_000_000.0 * usd_per_million_tokens
}

#[must_use]
pub fn format_usd(amount: f64) -> String {
    format!("${amount:.4}")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageTracker {
    latest_turn: TokenUsage,
    cumulative: TokenUsage,
    turns: u32,
    /// 用量事实不完整的轮数（任一维度未知）。非 0 时累计值不得被当作完整用量。
    unknown_usage_turns: u32,
}

impl UsageTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        let mut tracker = Self::new();
        for message in &session.messages {
            if let Some(usage) = message.usage {
                tracker.record(usage);
            }
        }
        tracker
    }

    pub fn record(&mut self, usage: TokenUsage) {
        self.latest_turn = usage;
        self.cumulative.input_tokens += usage.input_tokens;
        self.cumulative.output_tokens += usage.output_tokens;
        self.cumulative.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        self.cumulative.cache_read_input_tokens += usage.cache_read_input_tokens;
        self.turns += 1;
    }

    /// 记录一轮**上报**的用量。任一维度未知即计入 `unknown_usage_turns`，
    /// 使消费方能把汇总判定为"不完整"，而不是把缺失静默当成 0。
    pub fn record_reported(&mut self, reported: ReportedUsage) {
        let incomplete = reported.has_unknown();
        self.record(reported.known_totals());
        if incomplete {
            self.unknown_usage_turns = self.unknown_usage_turns.saturating_add(1);
        }
    }

    /// 用量事实不完整的轮数。
    #[must_use]
    pub fn unknown_usage_turns(&self) -> u32 {
        self.unknown_usage_turns
    }

    /// 累计用量是否完整（没有任何未知轮）。为 false 时不得把汇总呈现为完整账单。
    #[must_use]
    pub fn usage_is_complete(&self) -> bool {
        self.unknown_usage_turns == 0
    }

    #[must_use]
    pub fn current_turn_usage(&self) -> TokenUsage {
        self.latest_turn
    }

    #[must_use]
    pub fn cumulative_usage(&self) -> TokenUsage {
        self.cumulative
    }

    #[must_use]
    pub fn turns(&self) -> u32 {
        self.turns
    }
}

/// 一次网络尝试的终局。**失败与超时也要保留登记**（§2.4：每次真实请求开始即
/// 登记 attempt 与用途，失败/超时也保留）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAttemptOutcome {
    Completed,
    Failed,
    TimedOut,
}

/// 一次**网络尝试**的用量事实。
///
/// `logical_request_id` 标识逻辑请求（同一逻辑请求重试多次共享它），`attempt_id`
/// 标识网络尝试（每次重试不同）。两者分开才能"逻辑请求与网络重试分账"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageAttempt {
    pub run_id: String,
    pub logical_request_id: String,
    pub attempt_id: String,
    pub outcome: UsageAttemptOutcome,
    /// 供应商上报的用量；各维度可未知（见 `ReportedUsage`）。
    pub provider_usage: ReportedUsage,
    /// 本地估算用量；与供应商值**分别记录**，不得互相冒充。
    pub estimated_usage: Option<TokenUsage>,
    /// 计价所用价格版本；缺失表示**没有价格**，此时不得产生账单数字。
    pub price_version: Option<String>,
}

/// 用量账本：按 attempt 追加事实，不重写历史。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageLedger {
    attempts: Vec<UsageAttempt>,
}

impl UsageLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一次尝试。迟到的事实同样走这里追加，不修改既有条目。
    pub fn record(&mut self, attempt: UsageAttempt) {
        self.attempts.push(attempt);
    }

    /// 网络尝试总数，**含失败与超时**。
    #[must_use]
    pub fn network_attempts(&self) -> usize {
        self.attempts.len()
    }

    /// 逻辑请求数（按 `logical_request_id` 去重）。重试不增加这个数。
    #[must_use]
    pub fn logical_requests(&self) -> usize {
        let mut ids: Vec<&str> = self
            .attempts
            .iter()
            .map(|attempt| attempt.logical_request_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// 用量事实不完整的尝试数（任一维度未知）。
    #[must_use]
    pub fn unknown_usage_attempts(&self) -> usize {
        self.attempts
            .iter()
            .filter(|attempt| attempt.provider_usage.has_unknown())
            .count()
    }

    /// 用量事实是否完整（没有任何未知尝试）。false 时不得把汇总当完整账单。
    #[must_use]
    pub fn usage_is_complete(&self) -> bool {
        self.unknown_usage_attempts() == 0
    }

    /// 供应商已知 token 的求和；未知维度不参与，也不会被当作 0 计入"完整"。
    #[must_use]
    pub fn provider_tokens(&self) -> TokenUsage {
        self.attempts
            .iter()
            .fold(TokenUsage::default(), |total, attempt| {
                let known = attempt.provider_usage.known_totals();
                TokenUsage {
                    input_tokens: total.input_tokens.saturating_add(known.input_tokens),
                    output_tokens: total.output_tokens.saturating_add(known.output_tokens),
                    cache_creation_input_tokens: total
                        .cache_creation_input_tokens
                        .saturating_add(known.cache_creation_input_tokens),
                    cache_read_input_tokens: total
                        .cache_read_input_tokens
                        .saturating_add(known.cache_read_input_tokens),
                }
            })
    }

    /// 本地估算 token 的求和，与供应商值分列。
    #[must_use]
    pub fn estimated_tokens(&self) -> TokenUsage {
        self.attempts
            .iter()
            .filter_map(|attempt| attempt.estimated_usage)
            .fold(TokenUsage::default(), |total, usage| TokenUsage {
                input_tokens: total.input_tokens.saturating_add(usage.input_tokens),
                output_tokens: total.output_tokens.saturating_add(usage.output_tokens),
                cache_creation_input_tokens: total
                    .cache_creation_input_tokens
                    .saturating_add(usage.cache_creation_input_tokens),
                cache_read_input_tokens: total
                    .cache_read_input_tokens
                    .saturating_add(usage.cache_read_input_tokens),
            })
    }

    /// 本账本出现过的价格版本（去重排序）。
    #[must_use]
    pub fn price_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self
            .attempts
            .iter()
            .filter_map(|attempt| attempt.price_version.clone())
            .collect();
        versions.sort();
        versions.dedup();
        versions
    }

    /// 按**统一价格版本**计价。
    ///
    /// 只有当账本里每一次尝试都带同一个 `version` 时才返回估算；否则返回 `None`——
    /// §2.4 明确"没有价格不能把输入占比当账单占比"，所以缺价格或版本混杂都必须
    /// 拒绝出数，而不是拿 token 占比冒充账单占比。
    #[must_use]
    pub fn cost_for_version(
        &self,
        pricing: ModelPricing,
        version: &str,
    ) -> Option<UsageCostEstimate> {
        if self.attempts.is_empty() {
            return None;
        }
        let all_tagged = self
            .attempts
            .iter()
            .all(|attempt| attempt.price_version.as_deref() == Some(version));
        if !all_tagged {
            return None;
        }
        Some(self.provider_tokens().estimate_cost_usd_with_pricing(pricing))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_usd, pricing_for_model, ModelPricing, ReportedUsage, TokenUsage,
        UsageAttempt, UsageAttemptOutcome, UsageLedger, UsageTracker,
    };
    use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};

    #[test]
    fn tracks_true_cumulative_usage() {
        let mut tracker = UsageTracker::new();
        tracker.record(TokenUsage {
            input_tokens: 10,
            output_tokens: 4,
            cache_creation_input_tokens: 2,
            cache_read_input_tokens: 1,
        });
        tracker.record(TokenUsage {
            input_tokens: 20,
            output_tokens: 6,
            cache_creation_input_tokens: 3,
            cache_read_input_tokens: 2,
        });

        assert_eq!(tracker.turns(), 2);
        assert_eq!(tracker.current_turn_usage().input_tokens, 20);
        assert_eq!(tracker.current_turn_usage().output_tokens, 6);
        assert_eq!(tracker.cumulative_usage().output_tokens, 10);
        assert_eq!(tracker.cumulative_usage().input_tokens, 30);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 48);
    }

    #[test]
    fn computes_cost_summary_lines() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 100_000,
            cache_read_input_tokens: 200_000,
        };

        let cost = usage.estimate_cost_usd();
        assert_eq!(format_usd(cost.input_cost_usd), "$15.0000");
        assert_eq!(format_usd(cost.output_cost_usd), "$37.5000");
        let lines = usage.summary_lines_for_model("usage", Some("claude-sonnet-4-6"));
        assert!(lines[0].contains("estimated_cost=$54.6750"));
        assert!(lines[0].contains("model=claude-sonnet-4-6"));
        assert!(lines[1].contains("cache_read=$0.3000"));
    }

    #[test]
    fn supports_model_specific_pricing() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        let haiku = pricing_for_model("claude-haiku-4-5-20251213").expect("haiku pricing");
        let opus = pricing_for_model("claude-opus-4-6").expect("opus pricing");
        let haiku_cost = usage.estimate_cost_usd_with_pricing(haiku);
        let opus_cost = usage.estimate_cost_usd_with_pricing(opus);
        assert_eq!(format_usd(haiku_cost.total_cost_usd()), "$3.5000");
        assert_eq!(format_usd(opus_cost.total_cost_usd()), "$52.5000");
    }

    #[test]
    fn marks_unknown_model_pricing_as_fallback() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 100,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let lines = usage.summary_lines_for_model("usage", Some("custom-model"));
        assert!(lines[0].contains("pricing=estimated-default"));
    }

    #[test]
    fn reconstructs_usage_from_session_messages() {
        let session = Session {
            version: 1,
            messages: vec![ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: "done".to_string(),
                }],
                usage: Some(TokenUsage {
                    input_tokens: 5,
                    output_tokens: 2,
                    cache_creation_input_tokens: 1,
                    cache_read_input_tokens: 0,
                }),
            }],
        };

        let tracker = UsageTracker::from_session(&session);
        assert_eq!(tracker.turns(), 1);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 8);
    }

    /// S2.2：缺 usage 必须记成**未知**，而不是 0——否则消费方无法区分
    /// "没拿到用量"与"用量确实是 0"，会把不完整的汇总当成完整账单。
    #[test]
    fn missing_usage_is_unknown_not_zero() {
        let mut tracker = UsageTracker::new();
        tracker.record_reported(ReportedUsage::unknown());

        assert_eq!(tracker.unknown_usage_turns(), 1);
        assert!(
            !tracker.usage_is_complete(),
            "缺 usage 的轮次必须让汇总被判定为不完整"
        );
        // 汇总数值本身仍是已知量的求和（这里没有已知量），但**必须**伴随不完整标记。
        assert_eq!(tracker.cumulative_usage().total_tokens(), 0);
    }

    /// 完整上报的用量不标记不完整，且逐维求和。
    #[test]
    fn fully_known_usage_keeps_the_total_complete() {
        let mut tracker = UsageTracker::new();
        tracker.record_reported(ReportedUsage {
            input_tokens: Some(10),
            output_tokens: Some(4),
            cache_creation_input_tokens: Some(2),
            cache_read_input_tokens: Some(1),
        });

        assert_eq!(tracker.unknown_usage_turns(), 0);
        assert!(tracker.usage_is_complete());
        assert_eq!(tracker.cumulative_usage().total_tokens(), 17);
    }

    /// 部分维度未知同样算不完整：已知维度照常求和，但不得把汇总当完整用量。
    #[test]
    fn partially_known_usage_is_flagged_incomplete() {
        let mut tracker = UsageTracker::new();
        tracker.record_reported(ReportedUsage {
            input_tokens: Some(10),
            output_tokens: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        });

        assert_eq!(tracker.cumulative_usage().input_tokens, 10);
        assert_eq!(tracker.unknown_usage_turns(), 1);
        assert!(!tracker.usage_is_complete());
    }

    fn attempt(
        logical: &str,
        attempt_id: &str,
        outcome: UsageAttemptOutcome,
        provider_usage: ReportedUsage,
        price_version: Option<&str>,
    ) -> UsageAttempt {
        UsageAttempt {
            run_id: "run-1".into(),
            logical_request_id: logical.into(),
            attempt_id: attempt_id.into(),
            outcome,
            provider_usage,
            estimated_usage: None,
            price_version: price_version.map(str::to_string),
        }
    }

    fn test_pricing() -> ModelPricing {
        ModelPricing {
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_creation_cost_per_million: 18.75,
            cache_read_cost_per_million: 1.5,
        }
    }

    fn known(input: u32, output: u32) -> ReportedUsage {
        ReportedUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(0),
        }
    }

    /// S2.2：逻辑请求与网络重试必须**分账**——同一次逻辑请求重试 3 次，
    /// 逻辑请求数是 1，网络尝试数是 3。
    #[test]
    fn retries_share_one_logical_request_but_count_as_separate_attempts() {
        let mut ledger = UsageLedger::new();
        for index in 0..3 {
            ledger.record(attempt(
                "logical-1",
                &format!("attempt-{index}"),
                UsageAttemptOutcome::Completed,
                known(10, 2),
                Some("price-v1"),
            ));
        }
        assert_eq!(ledger.logical_requests(), 1);
        assert_eq!(ledger.network_attempts(), 3);
    }

    /// 失败与超时的尝试同样保留，不能被丢弃（否则账单与重试事实都会少算）。
    #[test]
    fn failed_and_timed_out_attempts_are_retained() {
        let mut ledger = UsageLedger::new();
        ledger.record(attempt(
            "logical-1",
            "attempt-0",
            UsageAttemptOutcome::TimedOut,
            ReportedUsage::unknown(),
            None,
        ));
        ledger.record(attempt(
            "logical-1",
            "attempt-1",
            UsageAttemptOutcome::Failed,
            ReportedUsage::unknown(),
            None,
        ));

        assert_eq!(ledger.network_attempts(), 2);
        assert_eq!(ledger.logical_requests(), 1);
        assert_eq!(ledger.unknown_usage_attempts(), 2);
        assert!(!ledger.usage_is_complete());
    }

    /// 未知用量不得以 0 的形式混进汇总：已知量照常求和，但账本必须被判为不完整。
    #[test]
    fn unknown_provider_usage_never_contaminates_totals_as_zero() {
        let mut ledger = UsageLedger::new();
        ledger.record(attempt(
            "logical-1",
            "attempt-0",
            UsageAttemptOutcome::Completed,
            known(100, 20),
            Some("price-v1"),
        ));
        ledger.record(attempt(
            "logical-2",
            "attempt-0",
            UsageAttemptOutcome::Completed,
            ReportedUsage::unknown(),
            Some("price-v1"),
        ));

        assert_eq!(ledger.provider_tokens().input_tokens, 100);
        assert_eq!(ledger.unknown_usage_attempts(), 1);
        assert!(!ledger.usage_is_complete());
    }

    /// 没有价格版本时，**不得**用 token 产出账单估算（§2.4）。
    #[test]
    fn cost_is_refused_when_no_price_version_is_recorded() {
        let mut ledger = UsageLedger::new();
        ledger.record(attempt(
            "logical-1",
            "attempt-0",
            UsageAttemptOutcome::Completed,
            known(1000, 500),
            None,
        ));

        assert!(ledger.price_versions().is_empty());
        assert!(ledger.cost_for_version(test_pricing(), "price-v1").is_none());
    }

    /// 只有每一次尝试都带同一个价格版本才允许计价；版本混杂时拒绝出数。
    #[test]
    fn cost_requires_every_attempt_to_share_one_price_version() {
        let mut ledger = UsageLedger::new();
        let pricing = test_pricing();
        ledger.record(attempt(
            "logical-1",
            "attempt-0",
            UsageAttemptOutcome::Completed,
            known(1000, 500),
            Some("price-v1"),
        ));
        assert!(ledger.cost_for_version(pricing, "price-v1").is_some());
        assert!(ledger.cost_for_version(pricing, "price-v2").is_none());

        // 再来一次带不同版本的尝试 → 版本混杂，任何版本都不再出数。
        ledger.record(attempt(
            "logical-2",
            "attempt-0",
            UsageAttemptOutcome::Completed,
            known(10, 5),
            Some("price-v2"),
        ));
        assert_eq!(ledger.price_versions(), vec!["price-v1", "price-v2"]);
        assert!(ledger.cost_for_version(pricing, "price-v1").is_none());
        assert!(ledger.cost_for_version(pricing, "price-v2").is_none());
    }

    /// 迟到/后来的未知轮不会抹掉已记录的已知量，但会持续标记不完整。
    #[test]
    fn later_unknown_turn_keeps_earlier_known_totals_and_flags_incompleteness() {
        let mut tracker = UsageTracker::new();
        tracker.record_reported(ReportedUsage {
            input_tokens: Some(7),
            output_tokens: Some(3),
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(0),
        });
        tracker.record_reported(ReportedUsage::unknown());

        assert_eq!(tracker.cumulative_usage().total_tokens(), 10);
        assert_eq!(tracker.turns(), 2);
        assert_eq!(tracker.unknown_usage_turns(), 1);
        assert!(!tracker.usage_is_complete());
    }
}
