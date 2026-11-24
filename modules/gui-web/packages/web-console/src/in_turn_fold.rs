//! 轮内工具结果折叠（M6）。
//!
//! 工具反馈循环每轮把 `assistant ToolUse` + `user ToolResult` 追加进 history 并整体重发；
//! 已被模型消费过的旧结果在后续请求中替换为"事实行 + 原文指针"。只替换正文取值，
//! `tool_use_id` / `is_error` / 消息顺序 / 块结构全部保留，tool_use/tool_result 配对不变。
//! 只作用轮内追加段（`base_len` 之后），跨轮历史由装配期预算与生命周期负责。
//! 高低水位批量滞回：越线一次把可折段折到低水位以下，两次折叠之间轮内前缀保持稳定。

use std::collections::HashMap;
use std::path::Path;

use api::{InputContentBlock, InputMessage, ToolResultContentBlock};
use serde_json::json;

/// 已折叠块的标记前缀；命中即不再二次折叠（幂等）。
const FOLDED_MARKER: &str = "[工具结果已折叠";
/// 小于此长度的结果没有折叠收益，保持原文。
const MIN_FOLDABLE_RESULT_CHARS: usize = 2_000;
/// 指针中 read_file 的单次续读上限，与 tool_result_spill 的节选约定一致。
const POINTER_READ_MAX_CHARS: u32 = 12_000;

/// 追加 tool_result 时登记的折叠事实；正文折叠后模型仍能看到"是什么工具、结果如何"。
#[derive(Debug, Clone)]
pub(crate) struct FoldMeta {
    pub(crate) tool_name: String,
    /// `dispatch.summary_text`（含 tool_name/route/status 行）或参数修复原因。
    pub(crate) summary: String,
}

impl FoldMeta {
    pub(crate) fn new(tool_name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self { tool_name: tool_name.into(), summary: summary.into() }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InTurnFoldOptions {
    pub(crate) enabled: bool,
    /// 轮内追加段估算 token 超过此值才触发一次折叠。
    pub(crate) high_tokens: u32,
    /// 触发后按最旧优先折叠，直到轮内段低于此值。
    pub(crate) low_tokens: u32,
    /// 无读取能力时折叠行保留的头部节选字符数。
    pub(crate) excerpt_chars: usize,
    /// `ToolUse.input` 序列化超过此字符数才替换为 stub。
    pub(crate) tool_use_input_chars: usize,
    pub(crate) image_token_estimate: u32,
}

impl InTurnFoldOptions {
    pub(crate) fn from_policy(policy: &crate::ContextLifecyclePolicy) -> Self {
        Self {
            enabled: policy.in_turn_fold_enabled,
            high_tokens: policy.in_turn_fold_high_tokens,
            low_tokens: policy.in_turn_fold_low_tokens,
            excerpt_chars: policy.in_turn_fold_excerpt_chars,
            tool_use_input_chars: policy.in_turn_fold_tool_use_input_chars,
            image_token_estimate: policy.image_token_estimate,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct FoldReport {
    pub(crate) folded_tool_results: usize,
    pub(crate) folded_tool_use_inputs: usize,
    pub(crate) tokens_before: u32,
    pub(crate) tokens_after: u32,
}

/// 与 `small_context_mcp_prompt_budget` 同口径的逐块估算；两处共用同一函数。
pub(crate) fn estimate_input_message_tokens(message: &InputMessage, image_token_estimate: u32) -> u32 {
    message.content.iter().fold(8_u32, |subtotal, block| {
        let tokens = match block {
            InputContentBlock::Text { text } | InputContentBlock::Thinking { thinking: text } => {
                crate::estimate_bead_tokens(text)
            }
            InputContentBlock::ImageUrl { .. } => image_token_estimate,
            InputContentBlock::ToolUse { name, input, .. } => crate::estimate_bead_tokens(name)
                .saturating_add(crate::estimate_bead_tokens(&input.to_string())),
            InputContentBlock::ToolResult { content, .. } => serde_json::to_string(content)
                .map(|value| crate::estimate_bead_tokens(&value))
                .unwrap_or(u32::MAX),
        };
        subtotal.saturating_add(tokens).saturating_add(8)
    })
}

fn estimate_messages_tokens(messages: &[InputMessage], image_token_estimate: u32) -> u32 {
    messages
        .iter()
        .fold(0_u32, |total, message| {
            total.saturating_add(estimate_input_message_tokens(message, image_token_estimate))
        })
}

/// 越线时把轮内段中"最新一轮问答对"之前的旧块按最旧优先折叠，直到降到低水位。
/// `metas` 在追加结果时登记；缺失时降级为无名摘要 + 节选，仍然折叠。
pub(crate) fn fold_stale(
    history: &mut [InputMessage],
    base_len: usize,
    metas: &HashMap<String, FoldMeta>,
    options: InTurnFoldOptions,
    blob_root: Option<&Path>,
    can_read_original: bool,
) -> FoldReport {
    let mut report = FoldReport::default();
    let tail_start = base_len.min(history.len());
    // 最新一轮问答对（assistant ToolUse + user ToolResult）必须原文送达一次。
    if !options.enabled || history.len() <= tail_start + 2 {
        return report;
    }
    report.tokens_before = estimate_messages_tokens(&history[tail_start..], options.image_token_estimate);
    if report.tokens_before <= options.high_tokens {
        return report;
    }
    let eligible_end = history.len() - 2;
    let mut tail_tokens = report.tokens_before;
    for index in tail_start..eligible_end {
        if tail_tokens <= options.low_tokens {
            break;
        }
        let before = estimate_input_message_tokens(&history[index], options.image_token_estimate);
        fold_message_blocks(&mut history[index], metas, options, blob_root, can_read_original, &mut report);
        let after = estimate_input_message_tokens(&history[index], options.image_token_estimate);
        tail_tokens = tail_tokens.saturating_sub(before).saturating_add(after);
    }
    report.tokens_after = estimate_messages_tokens(&history[tail_start..], options.image_token_estimate);
    report
}

fn fold_message_blocks(
    message: &mut InputMessage,
    metas: &HashMap<String, FoldMeta>,
    options: InTurnFoldOptions,
    blob_root: Option<&Path>,
    can_read_original: bool,
    report: &mut FoldReport,
) {
    for block in &mut message.content {
        match block {
            InputContentBlock::ToolResult { tool_use_id, content, is_error } => {
                let meta = metas.get(tool_use_id);
                for item in content.iter_mut() {
                    fold_tool_result_item(item, meta, *is_error, options, blob_root, can_read_original, report);
                }
            }
            InputContentBlock::ToolUse { input, .. } => {
                fold_tool_use_input(input, options, blob_root, report);
            }
            _ => {}
        }
    }
}

fn fold_tool_result_item(
    item: &mut ToolResultContentBlock,
    meta: Option<&FoldMeta>,
    is_error: bool,
    options: InTurnFoldOptions,
    blob_root: Option<&Path>,
    can_read_original: bool,
    report: &mut FoldReport,
) {
    let original = match item {
        ToolResultContentBlock::Text { text } => text.clone(),
        ToolResultContentBlock::Json { value } => value.to_string(),
    };
    if original.contains(FOLDED_MARKER) || original.chars().count() <= MIN_FOLDABLE_RESULT_CHARS {
        return;
    }
    let tool_name = meta.map(|meta| meta.tool_name.as_str()).unwrap_or("unknown");
    let summary = meta
        .map(|meta| meta.summary.trim())
        .filter(|summary| !summary.is_empty())
        .unwrap_or("（无派发摘要记录）");
    let status = if is_error { "failed" } else { "completed" };
    // 有读取能力：指针替代节选；无读取能力：无论 blob 是否落盘都必须保留有界节选，
    // 否则模型在折叠后看不到该结果的任何内容。
    let blob = save_blob(blob_root, original.as_bytes());
    let provenance = if can_read_original {
        match blob {
            Some(blob) => format!(
                "SHA256={} 字节数={} 已存原文。续读：read_file({})",
                blob.sha256,
                original.len(),
                json!({"path": blob.path, "character_offset": 0, "max_chars": POINTER_READ_MAX_CHARS})
            ),
            None => head_excerpt(&original, options.excerpt_chars),
        }
    } else {
        match blob {
            Some(blob) => format!(
                "SHA256={} 字节数={} 已存原文。{}",
                blob.sha256,
                original.len(),
                head_excerpt(&original, options.excerpt_chars)
            ),
            None => head_excerpt(&original, options.excerpt_chars),
        }
    };
    *item = ToolResultContentBlock::Text {
        text: format!(
            "{FOLDED_MARKER}；该结果已在此前请求中全文送达。tool={tool_name} status={status}\n{summary}\n原文共 {} 字符：{provenance}]",
            original.chars().count()
        ),
    };
    report.folded_tool_results += 1;
}

fn fold_tool_use_input(
    input: &mut serde_json::Value,
    options: InTurnFoldOptions,
    blob_root: Option<&Path>,
    report: &mut FoldReport,
) {
    if input.get("_folded").and_then(serde_json::Value::as_bool) == Some(true) {
        return;
    }
    let serialized = input.to_string();
    if serialized.len() <= options.tool_use_input_chars {
        return;
    }
    let sha256 = save_blob(blob_root, serialized.as_bytes())
        .map(|blob| blob.sha256)
        .unwrap_or_else(|| "（原文未落盘）".to_string());
    *input = json!({
        "_folded": true,
        "original_bytes": serialized.len(),
        "sha256": sha256,
        "note": "工具入参已折叠；执行事实见对应 tool_result",
    });
    report.folded_tool_use_inputs += 1;
}

fn save_blob(blob_root: Option<&Path>, bytes: &[u8]) -> Option<crate::content_blob_store::StoredBlob> {
    blob_root.and_then(|root| crate::content_blob_store::put(root, bytes, "txt").ok())
}

fn head_excerpt(original: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return "原文未保留节选".to_string();
    }
    format!("头部节选：\n{}", original.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn options() -> InTurnFoldOptions {
        InTurnFoldOptions {
            enabled: true,
            high_tokens: 200,
            low_tokens: 60,
            excerpt_chars: 120,
            tool_use_input_chars: 200,
            image_token_estimate: 512,
        }
    }

    fn metas(entries: &[(&str, &str, &str)]) -> HashMap<String, FoldMeta> {
        entries
            .iter()
            .map(|(id, name, summary)| (id.to_string(), FoldMeta::new(*name, *summary)))
            .collect()
    }

    fn tool_use(id: &str, name: &str, input: serde_json::Value) -> InputMessage {
        InputMessage {
            role: "assistant".into(),
            content: vec![InputContentBlock::ToolUse { id: id.into(), name: name.into(), input }],
        }
    }

    fn tool_result(id: &str, text: String, is_error: bool) -> InputMessage {
        InputMessage {
            role: "user".into(),
            content: vec![InputContentBlock::ToolResult {
                tool_use_id: id.into(),
                is_error,
                content: vec![ToolResultContentBlock::Text { text }],
            }],
        }
    }

    fn result_text(message: &InputMessage) -> &str {
        match &message.content[0] {
            InputContentBlock::ToolResult { content, .. } => match &content[0] {
                ToolResultContentBlock::Text { text } => text,
                ToolResultContentBlock::Json { .. } => panic!("expected text result"),
            },
            _ => panic!("expected tool_result"),
        }
    }

    /// 折叠只替换正文取值：tool_use_id/is_error/消息数/块结构不变；最新一轮与 base 段逐字节不动。
    #[test]
    fn folds_stale_result_preserving_pairing_and_newest_round() {
        let temp = tempfile::tempdir().unwrap();
        let base = vec![InputMessage::user_text("原始用户请求")];
        let old_result = "x".repeat(6000);
        let new_result = "本轮新结果，必须原文送达".to_string();
        let mut history = base.clone();
        history.push(tool_use("a", "read_file", json!({"path": "a.txt"})));
        history.push(tool_result("a", old_result.clone(), false));
        history.push(tool_use("b", "glob_search", json!({"pattern": "*.rs"})));
        history.push(tool_result("b", new_result.clone(), false));
        let metas = metas(&[
            ("a", "read_file", "tool_name: read_file\nstatus: ok\n已读取文件"),
            ("b", "glob_search", "tool_name: glob_search\nstatus: ok"),
        ]);

        let report = fold_stale(
            &mut history, 1, &metas, options(),
            Some(temp.path()), true,
        );

        assert_eq!(report.folded_tool_results, 1);
        assert!(report.tokens_after < report.tokens_before);
        assert_eq!(history.len(), 5);
        assert_eq!(history[0], base[0]);
        // 旧结果：id/标志保留，正文换为事实行+指针。
        match &history[2].content[0] {
            InputContentBlock::ToolResult { tool_use_id, is_error, .. } => {
                assert_eq!(tool_use_id, "a");
                assert!(!is_error);
            }
            _ => panic!("pairing broken"),
        }
        let folded = result_text(&history[2]);
        assert!(folded.contains("[工具结果已折叠"));
        assert!(folded.contains("tool=read_file"));
        assert!(folded.contains("已读取文件"));
        assert!(folded.contains("read_file("));
        // 最新一轮逐字节不变。
        assert_eq!(result_text(&history[4]), new_result);
        assert_eq!(history[3], tool_use("b", "glob_search", json!({"pattern": "*.rs"})));
    }

    /// 低于高水位完全不动手（轮内前缀保持稳定）。
    #[test]
    fn below_high_watermark_is_noop() {
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "read_file", json!({})));
        history.push(tool_result("a", "short".into(), false));
        history.push(tool_use("b", "read_file", json!({})));
        history.push(tool_result("b", "short".into(), false));
        let snapshot = history.clone();
        let report = fold_stale(&mut history, 1, &metas(&[]), options(), None, true);
        assert_eq!(report.folded_tool_results, 0);
        assert_eq!(history, snapshot);
    }

    /// 无读取能力：不出指针，保留有界头部节选；原文 blob 仍落盘（供后续恢复）。
    #[test]
    fn without_read_capability_no_pointer_but_bounded_excerpt() {
        let temp = tempfile::tempdir().unwrap();
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", "重要输出头".to_string() + &"y".repeat(5000), true));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));

        let report = fold_stale(&mut history, 1, &metas(&[]), options(), Some(temp.path()), false);

        assert_eq!(report.folded_tool_results, 1);
        let folded = result_text(&history[2]);
        assert!(!folded.contains("read_file("));
        assert!(folded.contains("头部节选"));
        assert!(folded.contains("status=failed"));
        assert!(folded.len() < 400);
    }

    /// 有读取能力：原文落盘且指针可续读回原文。
    #[test]
    fn read_pointer_round_trips_original() {
        let temp = tempfile::tempdir().unwrap();
        let original = "可续读原文".to_string() + &"z".repeat(4000);
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", original.clone(), false));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));

        fold_stale(&mut history, 1, &metas(&[]), options(), Some(temp.path()), true);

        let folded = result_text(&history[2]);
        assert!(folded.contains("SHA256="));
        assert!(folded.contains("read_file("));
        let stored = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.file_name().unwrap().to_string_lossy().starts_with("sha256-"))
            .expect("blob persisted");
        assert_eq!(std::fs::read_to_string(stored).unwrap(), original);
    }

    /// 幂等：已折叠块不再处理，二次调用零改动——两次折叠之间前缀字节稳定。
    #[test]
    fn second_pass_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", "r".repeat(5000), false));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));

        fold_stale(&mut history, 1, &metas(&[]), options(), Some(temp.path()), true);
        let after_first = history.clone();
        let second = fold_stale(&mut history, 1, &metas(&[]), options(), Some(temp.path()), true);

        assert_eq!(second.folded_tool_results, 0);
        assert_eq!(second.folded_tool_use_inputs, 0);
        assert_eq!(history, after_first);
    }

    /// 无 meta（缺失登记）仍按 unknown + 节选折叠，不阻塞收益。
    #[test]
    fn missing_meta_still_folds() {
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", "m".repeat(4000), false));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));

        let report = fold_stale(&mut history, 1, &metas(&[]), options(), None, true);
        assert_eq!(report.folded_tool_results, 1);
        assert!(result_text(&history[2]).contains("tool=unknown"));
    }

    /// blob 不可写/无根目录时退化为摘要+节选，不 panic、不丢折叠收益。
    #[test]
    fn blob_failure_falls_back_to_excerpt() {
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", "f".repeat(4000), false));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));

        let report = fold_stale(&mut history, 1, &metas(&[]), options(), None, true);
        let folded = result_text(&history[2]);
        assert_eq!(report.folded_tool_results, 1);
        assert!(folded.contains("头部节选"));
        assert!(!folded.contains("SHA256="));
    }

    /// 超大 ToolUse.input 替换为 stub（配对靠 id 不读 input）；小入参不动。
    #[test]
    fn oversized_tool_use_input_is_stubbed() {
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "write_file", json!({"path": "f.txt", "content": "w".repeat(2000)})));
        history.push(tool_result("a", "done".into(), false));
        history.push(tool_use("b", "bash", json!({"command": "ls"})));
        history.push(tool_result("b", "new".into(), false));
        // 拉高水位让 input 折叠成为剩余收益的主要来源。
        let mut opts = options();
        opts.high_tokens = 100;
        opts.tool_use_input_chars = 100;

        let report = fold_stale(&mut history, 1, &metas(&[]), opts, None, true);

        assert_eq!(report.folded_tool_use_inputs, 1);
        match &history[1].content[0] {
            InputContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "a");
                assert_eq!(name, "write_file");
                assert_eq!(input.get("_folded").and_then(serde_json::Value::as_bool), Some(true));
                assert!(input.get("original_bytes").is_some());
            }
            _ => panic!("tool_use replaced"),
        }
        // 最新一轮的 input 不动。
        assert_eq!(history[3], tool_use("b", "bash", json!({"command": "ls"})));
    }

    /// 关闭开关零改动。
    #[test]
    fn disabled_is_noop() {
        let mut history = vec![InputMessage::user_text("base")];
        history.push(tool_use("a", "bash", json!({})));
        history.push(tool_result("a", "r".repeat(5000), false));
        history.push(tool_use("b", "bash", json!({})));
        history.push(tool_result("b", "new".into(), false));
        let snapshot = history.clone();
        let mut opts = options();
        opts.enabled = false;
        let report = fold_stale(&mut history, 1, &metas(&[]), opts, None, true);
        assert_eq!(report.folded_tool_results, 0);
        assert_eq!(history, snapshot);
    }
}
