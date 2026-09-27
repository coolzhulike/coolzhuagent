//! **CU03 历史语料库守卫**：把"不许把合成说成历史回放、缺失就写缺失、没标签就不算语义指标"
//! 这些裁决口径变成可执行的断言，而不是只写在 README 里。
//!
//! 语料库本体在 `docs/testing/cu03-eval/corpus-2026-09-19-paint-window-drag/`，
//! 其 `manifest.json` 由 `tools/build_corpus.py` 从历史原始留存机器生成。

#[cfg(test)]
mod tests {
    use serde_json::Value;

    fn manifest_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../../docs/testing/cu03-eval/corpus-2026-09-19-paint-window-drag/manifest.json",
        )
    }

    fn corpus_root() -> std::path::PathBuf {
        manifest_path().parent().map(std::path::Path::to_path_buf).expect("语料库根目录")
    }

    fn load_manifest() -> Value {
        let path = manifest_path();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("语料库清单必须随仓库分发：{}（{error}）", path.display())
        });
        serde_json::from_str(&text).expect("语料库清单必须是合法 JSON")
    }

    /// 四类分类合法，且**合成样本不得被归入历史类**。
    #[test]
    fn corpus_categories_are_the_four_ruled_kinds_and_synthetic_is_never_history() {
        let manifest = load_manifest();
        let samples = manifest["samples"].as_array().expect("samples 必须是数组");
        assert!(!samples.is_empty(), "语料库不得为空");
        let allowed = ["history_original", "history_derived", "new_capture", "synthetic"];
        for sample in samples {
            let id = sample["sample_id"].as_str().expect("样本必须有 id");
            let category = sample["category"].as_str().expect("样本必须有类别");
            assert!(allowed.contains(&category), "{id} 的类别 {category} 不在裁决四类内");
            if category == "synthetic" {
                assert_eq!(
                    sample["not_history_replay"], Value::Bool(true),
                    "{id} 是合成样本，必须显式声明不是历史回放"
                );
            }
        }
        // 合成样本不得声称自己有历史运行时证据来源。
        for sample in samples.iter().filter(|item| item["category"] == "synthetic") {
            let source = sample["evidence_source"].to_string();
            assert!(
                !source.contains("web-sessions.sqlite3"),
                "合成样本 {} 不得引用历史运行时数据库作为来源",
                sample["sample_id"]
            );
        }
    }

    /// 历史样本必须逐项写明缺失；**没标签就必须写没标签**，不得装作有语义标签。
    #[test]
    fn history_samples_declare_missing_evidence_and_absence_of_labels() {
        let manifest = load_manifest();
        let samples = manifest["samples"].as_array().expect("samples 必须是数组");
        let mut history_seen = 0;
        for sample in samples.iter().filter(|item| item["category"] == "history_derived") {
            history_seen += 1;
            let id = sample["sample_id"].as_str().unwrap_or("?");
            let missing = sample["missing_items"].as_array().expect("必须有缺失项清单");
            assert!(!missing.is_empty(), "{id} 的缺失项不得为空（缺失要明确写出来）");
            assert_eq!(
                sample["allowed_behavior_labels"]["present"], Value::Bool(false),
                "{id} 没有语义标签，必须如实写 present=false"
            );
            // 历史运行早于反馈块：不得声称当时有反馈。
            assert_eq!(
                sample["available_feedback"]["feedback_block_present"], Value::Bool(false),
                "{id} 早于 CU-03 反馈块，不得声称存在反馈块"
            );
            // 事后验证摘要不得被当作反馈内容。
            let feedback = sample["available_feedback"]["note"].as_str().unwrap_or_default();
            assert!(
                feedback.contains("不等于当时交给规划器的反馈"),
                "{id} 必须写明事后验证摘要不等于当时交给规划器的反馈"
            );
        }
        assert_eq!(history_seen, 6, "历史样本应为 paint-r1..r6 共 6 条");
    }

    /// 历史运行全部未达成目标：不得把失败语料库描述成成功演示。
    #[test]
    fn corpus_does_not_claim_goal_achievement() {
        let manifest = load_manifest();
        assert_eq!(
            manifest["summary"]["all_history_runs_achieved_goal"], Value::Bool(false),
            "历史运行全部未达成目标，汇总口径不得写成已达成"
        );
        for sample in manifest["samples"].as_array().expect("samples 必须是数组")
            .iter().filter(|item| item["category"] == "history_derived")
        {
            let id = sample["sample_id"].as_str().unwrap_or("?");
            assert_ne!(
                sample["real_execution_state"]["goal_achieved"], Value::Bool(true),
                "{id} 未达成目标，不得记成达成"
            );
            assert!(
                sample["real_execution_state"]["terminal_error_code"].is_string(),
                "{id} 必须记录终态错误码"
            );
        }
    }

    /// 语义层指标在历史样本上必须写"不可评分"，且不得出现 0% 式的补零口径。
    #[test]
    fn history_samples_mark_semantic_metrics_unscorable_without_zero_filling() {
        let manifest = load_manifest();
        let semantic_markers = ["目标/操作选择错误", "无效重复副作用", "未核销的危险重放"];
        for sample in manifest["samples"].as_array().expect("samples 必须是数组")
            .iter().filter(|item| item["category"] == "history_derived")
        {
            let id = sample["sample_id"].as_str().unwrap_or("?");
            let facts = &sample["real_execution_state"]["structural_facts"];
            let unscorable = facts["unscorable_metrics"].as_array().expect("必须列出不可评分项");
            let joined = unscorable.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("|");
            for marker in semantic_markers {
                assert!(
                    joined.contains(marker),
                    "{id} 的不可评分项必须包含「{marker}」，实际 {joined}"
                );
            }
            assert!(
                facts["scorable_metrics"].as_array().is_some_and(|items| !items.is_empty()),
                "{id} 必须给出结构层可评分项"
            );
            // 关键：不可评分**不得**以 0 或 0% 表示。
            let text = facts.to_string();
            assert!(
                !text.contains("0%") && !text.contains("\"rate\""),
                "{id} 不得为不可评分指标补出比率：{text}"
            );
        }
    }

    /// 迁移过来的截图必须与文件名内嵌摘要一致（自校验），有图样本的摘要必须全部对得上。
    #[test]
    fn migrated_images_are_self_verifying() {
        let manifest = load_manifest();
        let images = manifest["images"].as_object().expect("images 必须是对象");
        assert!(!images.is_empty(), "语料库必须迁移至少一张真实截图");
        for (filename, meta) in images {
            let digest = meta["sha256"].as_str().expect("必须记录摘要");
            assert!(
                filename.contains(digest),
                "文件名 {filename} 必须内嵌其 sha256（自校验），实际 {digest}"
            );
            let path = corpus_root().join("images").join(filename);
            assert!(path.exists(), "迁移的截图必须随仓库分发：{}", path.display());
        }
        for sample in manifest["samples"].as_array().expect("samples 必须是数组")
            .iter().filter(|item| item["category"] == "history_derived")
        {
            let imaging = &sample["image_and_transform"];
            let count = imaging["image_count"].as_u64().unwrap_or_default();
            if count > 0 {
                assert_eq!(
                    imaging["all_digests_match"], Value::Bool(true),
                    "{} 的截图摘要必须全部校验通过",
                    sample["sample_id"]
                );
            }
            // 几何与变换是缺失的：不得声称能重建屏幕绝对坐标。
            assert_eq!(
                imaging["window_rect"].as_str(), Some("缺失"),
                "{} 未留存窗口 rect，不得声称有",
                sample["sample_id"]
            );
            assert_eq!(
                imaging["crop_or_scale_mapping"].as_str().is_some_and(|text| text.starts_with("缺失")),
                true,
                "{} 未留存裁剪/缩放映射，不得声称有",
                sample["sample_id"]
            );
        }
    }

    /// 迁移素材必须**逐字节**等于清单记录的摘要与体积。
    ///
    /// 这条守卫的存在理由：本机 `core.autocrlf=true` 且仓库根 `.gitattributes` 有
    /// `*.json text eol=lf`，检出/提交时的行尾归一化会静默改写字节、让已记录的溯源摘要失效。
    /// 语料库用 `samples/** -text` 把字节钉住，本测试就是那个钉子的检查者。
    #[test]
    fn migrated_sample_files_match_recorded_digests_byte_for_byte() {
        use sha2::{Digest, Sha256};
        let manifest = load_manifest();
        let root = corpus_root();
        let mut checked = 0;
        for sample in manifest["samples"].as_array().expect("samples 必须是数组") {
            for entry in sample["files"].as_array().into_iter().flatten() {
                let relative = entry["file"].as_str().expect("文件条目必须有相对路径");
                let path = root.join(relative);
                let bytes = std::fs::read(&path).unwrap_or_else(|error| {
                    panic!("迁移素材必须随仓库分发：{}（{error}）", path.display())
                });
                let recorded_size = entry["bytes"].as_u64().expect("必须记录字节数");
                assert_eq!(
                    bytes.len() as u64,
                    recorded_size,
                    "{relative} 的字节数被改动：记录 {recorded_size}，实际 {}",
                    bytes.len()
                );
                let digest = format!("{:x}", Sha256::digest(&bytes));
                assert_eq!(
                    Some(digest.as_str()),
                    entry["sha256"].as_str(),
                    "{relative} 的字节已与记录摘要不符（是否被行尾归一化改写了？）"
                );
                checked += 1;
            }
        }
        assert!(checked >= 60, "迁移素材条目过少（{checked}），语料库可能未完整分发");
    }

    /// 指纹含观察代次这一事实必须留在语料库里：否则"无效重复"会被静默算成 0 次。
    #[test]
    fn fingerprint_identity_caveat_is_recorded_with_a_real_counter_example() {
        let manifest = load_manifest();
        let samples = manifest["samples"].as_array().expect("samples 必须是数组");
        let sample = samples
            .iter()
            .find(|item| item["sample_id"] == "paint-r3")
            .expect("paint-r3 必须在语料库内");
        let facts = &sample["real_execution_state"]["structural_facts"];
        assert!(
            facts["fingerprint_identity_caveat"].as_str().is_some_and(|text| text.contains("observation_generation")),
            "必须记录指纹含观察代次这一口径"
        );
        // 真实反例：按指纹相等看不到重复，忽略代次才看得到。
        assert!(
            facts["consecutive_repeated_fingerprints"].as_array().is_some_and(Vec::is_empty),
            "paint-r3 两次点击因代次不同而指纹不同——按指纹判重复会漏掉"
        );
        let repeats = facts["consecutive_repeated_operations_ignoring_generation"]
            .as_array()
            .expect("必须给出忽略代次的重复视图");
        assert_eq!(repeats.len(), 1, "忽略代次后 paint-r3 应有且仅有 1 次真实重复");
        assert_eq!(repeats[0]["fingerprints_differ"], Value::Bool(true), "该重复的指纹必须确实不同");
        assert_eq!(
            repeats[0]["second_step_visible_progress"], Value::Bool(false),
            "该重复第二步无可见进展——正是「无效重复副作用」的真实样本"
        );
    }
}
