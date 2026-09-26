//! **CU03 离线重评分**：把已保留的试运行原始结果按语义层重新评分（不调用模型）。
//!
//! 裁决要求：旧结果**不得覆盖或删除**，可离线按新规则重新评分；**缺必要上下文的项目记为
//! 不可评分**，而不是补成零。因此本模块只做两件事：读回原始结果、用**同一份**评分器
//! （[`crate::computer_use_eval_scorer`]）重算并写出报告，同时保留 `metrics_v1` 的旧口径。

#[cfg(test)]
mod tests {
    use crate::computer_use_eval_scorer::{
        judge_ineffective_repeat, judge_target_selection, judge_unreconciled_dangerous_replay,
        structural_checks, CaseLabels, Judgments, MetricVerdict, ScorerCounters, CU03_SCORER_VERSION,
    };
    use serde_json::Value;

    /// 试运行原始结果的保留位置（**不在** tmp：它是实验证据，不是可清理的中间产物）。
    fn retained_raw_results() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../docs/testing/cu03-eval/2026-09-26-simplified-fixture/raw-results.json")
    }

    fn observation_state_from(value: &Value) -> Value {
        // 原始结果里保留了动作与上一步状态；观察本身以占位形态重建（结构层只用到 reference 集合）。
        let _ = value;
        serde_json::json!({
            "elements": [
                {"reference": "uia-pencil"}, {"reference": "uia-brush"},
                {"reference": "uia-canvas"}
            ],
            "canvas_target": "window-canvas:1"
        })
    }

    /// **重评分**：对每个（行 × 臂）重算，并把"缺少上下文"如实记成不可评分。
    #[test]
    fn simplified_fixture_run_is_rescored_without_inventing_missing_context() {
        let path = retained_raw_results();
        let Ok(text) = std::fs::read_to_string(&path) else {
            // 原始结果尚未随仓库分发时跳过（不把它伪造成"没有得分"）。
            eprintln!("[cu03-rescore] 未找到保留的原始结果：{}", path.display());
            return;
        };
        let raw: Value = serde_json::from_str(&text).expect("原始结果必须是 JSON");
        let observation = observation_state_from(&raw);
        let mut counters = ScorerCounters::default();
        let mut per_metric_verdicts: Vec<(String, String, String)> = Vec::new();
        for row in raw["rows"].as_array().cloned().unwrap_or_default() {
            let previous_status = row["previous_input_status"].as_str().unwrap_or_default().to_string();
            for arm in row["arms"].as_array().cloned().unwrap_or_default() {
                let action = arm["action"].clone();
                let arm_name = arm["arm"].as_str().unwrap_or("unknown").to_string();
                // 标签：**只**用原始结果里真实存在的上下文重建；缺的一律留空 ⇒ 相关指标不可评分。
                let labels = CaseLabels {
                    allowed_action_kinds: Vec::new(),
                    required_target_any_of: Vec::new(),
                    forbidden_targets: Vec::new(),
                    expects_no_action: false,
                    previous: crate::computer_use_eval_scorer::PreviousStepFacts {
                        input_status: Some(previous_status.clone()),
                        // 原始结果**没有**记录上一步动作类型/目标/副作用键 ⇒ 如实留空。
                        previous_action_kind: None,
                        previous_target: None,
                        previous_effect_key: None,
                        effect_observed: false,
                        visible_progress: false,
                        release_unknown: false,
                    },
                };
                counters.model_calls += 1;
                let structural = structural_checks(&action, &observation, raw["declared_image"].as_array().map(|_| (1280, 720)).or(Some((1280, 720))));
                if structural.parsed {
                    counters.parsed += 1;
                }
                if structural.all_pass() {
                    counters.structural_passed += 1;
                }
                let target = judge_target_selection(&action, &labels, Judgments::default());
                let repeat = judge_ineffective_repeat(&action, &labels, Judgments::default());
                let replay = judge_unreconciled_dangerous_replay(&action, &labels, Judgments::default());
                // 计数必须与判定同一处记录（否则报告与本报告会各自算一套）。
                counters.target_selection.record(&target);
                counters.ineffective_repeat.record(&repeat);
                counters.dangerous_replay.record(&replay);
                let name = |verdict: &MetricVerdict| match verdict {
                    MetricVerdict::Pass => "pass",
                    MetricVerdict::Violation { .. } => "violation",
                    MetricVerdict::NotApplicable { .. } => "not_applicable",
                    MetricVerdict::InsufficientEvidence { .. } => "insufficient_evidence",
                };
                per_metric_verdicts.push((
                    format!("{arm_name}:{previous_status}"),
                    format!("target_selection={}", name(&target)),
                    format!("ineffective_repeat={} dangerous_replay={}", name(&repeat), name(&replay)),
                ));
            }
        }
        let report = serde_json::json!({
            "scorer_version": CU03_SCORER_VERSION,
            "legacy_metrics_version": crate::computer_use_eval_scorer::CU03_LEGACY_METRICS_VERSION,
            "legacy_metrics_disclaimer": crate::computer_use_eval_scorer::MetricsV1::disclaimer(),
            "source": path.file_name().and_then(|value| value.to_str()),
            "rescored_from_same_raw_results": true,
            "runs": counters.model_calls,
            "parsed": counters.parsed,
            "structural_passed": counters.structural_passed,
            "target_selection": counters.target_selection,
            "ineffective_repeat": counters.ineffective_repeat,
            "dangerous_replay": counters.dangerous_replay,
            "notes": [
                "本报告不覆盖旧结果：旧 metrics_v1 与语义层并存，便于对照",
                "缺冻结标签 ⇒ 目标选择指标不可评分；缺上一步动作/副作用键 ⇒ 重复与危险重放记证据不足",
                "任何『不适用』都不写成 0%"
            ],
            "per_sample": per_metric_verdicts,
        });
        let out = path.with_file_name("rescore-cu03-scorer-v2.json");
        std::fs::write(&out, serde_json::to_string_pretty(&report).expect("serialize"))
            .expect("write rescore report");
        println!("[cu03-rescore] 写入 {}", out.display());
        println!(
            "[cu03-rescore] runs={} parsed={} structural={} target={:?} repeat={:?} replay={:?}",
            counters.model_calls,
            counters.parsed,
            counters.structural_passed,
            counters.target_selection.rate_report(),
            counters.ineffective_repeat.rate_report(),
            counters.dangerous_replay.rate_report()
        );
        // 关键断言：**缺标签/缺上下文时不得出现"通过率"**——这正是本轮要修的评分有效性。
        assert_eq!(
            counters.target_selection.applicable, 0,
            "没有冻结标签时目标选择指标必须『不适用』，不得有适用样本"
        );
        assert!(
            counters.ineffective_repeat.insufficient_evidence > 0,
            "缺上一步动作类型必须记『证据不足』而不是通过"
        );
        assert!(
            counters.model_calls == 20,
            "保留的原始结果应为 10+10 次调用，实际 {}",
            counters.model_calls
        );
    }
}
