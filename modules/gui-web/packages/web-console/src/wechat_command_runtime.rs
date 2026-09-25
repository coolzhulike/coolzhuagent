use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WechatCatalogKind {
    Rooms,
    Sessions,
    Models,
    Targets,
    Tasks,
    Workspaces,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WechatCatalogEntry {
    pub id: String,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WechatCatalogSnapshot {
    pub generation: u64,
    pub account_id: String,
    pub peer_id: String,
    pub kind: WechatCatalogKind,
    pub entries: Vec<WechatCatalogEntry>,
}

impl WechatCatalogSnapshot {
    pub fn render(&self, title: &str) -> String {
        let mut lines = vec![format!("{title}（目录版本：{}）", self.generation)];
        if self.entries.is_empty() {
            lines.push("（当前没有可用项目）".to_string());
        } else {
            for (index, entry) in self.entries.iter().enumerate() {
                let detail = if entry.detail.trim().is_empty() {
                    String::new()
                } else {
                    format!(" · {}", entry.detail.trim())
                };
                lines.push(format!(
                    "{}. {}{detail}\n   ID: {}",
                    index + 1,
                    entry.label,
                    entry.id
                ));
            }
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WechatCommandRuntimeError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct WechatCommandRuntime {
    next_generation: u64,
    snapshots: HashMap<(String, String, WechatCatalogKind), WechatCatalogSnapshot>,
}

impl WechatCommandRuntime {
    pub fn publish_snapshot(
        &mut self,
        account_id: &str,
        peer_id: &str,
        kind: WechatCatalogKind,
        entries: Vec<WechatCatalogEntry>,
    ) -> WechatCatalogSnapshot {
        self.next_generation = self.next_generation.saturating_add(1).max(1);
        let snapshot = WechatCatalogSnapshot {
            generation: self.next_generation,
            account_id: account_id.to_string(),
            peer_id: peer_id.to_string(),
            kind,
            entries,
        };
        self.snapshots.insert(
            (account_id.to_string(), peer_id.to_string(), kind),
            snapshot.clone(),
        );
        snapshot
    }

    pub fn resolve_selector(
        &self,
        account_id: &str,
        peer_id: &str,
        kind: WechatCatalogKind,
        selector: &str,
    ) -> Result<WechatCatalogEntry, WechatCommandRuntimeError> {
        let snapshot = self
            .snapshots
            .get(&(account_id.to_string(), peer_id.to_string(), kind))
            .ok_or_else(|| WechatCommandRuntimeError {
                code: "catalog_snapshot_missing",
                message: "目录快照不存在或不属于当前微信会话，请先重新发送对应列表命令。"
                    .to_string(),
            })?;
        let selector = selector.trim();
        if selector.is_empty() {
            return Err(WechatCommandRuntimeError {
                code: "selector_required",
                message: "必须提供目录编号或稳定 ID。".to_string(),
            });
        }
        if let Some(selected) = selector
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| snapshot.entries.get(index))
        {
            return Ok(selected.clone());
        }
        if let Some(selected) = snapshot
            .entries
            .iter()
            .find(|entry| entry.id.eq_ignore_ascii_case(selector))
        {
            return Ok(selected.clone());
        }
        let label_matches = snapshot
            .entries
            .iter()
            .filter(|entry| entry.label.eq_ignore_ascii_case(selector))
            .collect::<Vec<_>>();
        if label_matches.len() == 1 {
            return Ok(label_matches[0].clone());
        }
        if label_matches.len() > 1 {
            let candidate_ids = label_matches
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>()
                .join("、");
            return Err(WechatCommandRuntimeError {
                code: "selector_ambiguous",
                message: format!(
                    "选择项名称“{selector}”存在多个匹配：{candidate_ids}。请使用目录编号或稳定 ID。"
                ),
            });
        }
        Err(WechatCommandRuntimeError {
            code: "selector_not_found",
            message: format!(
                "目录版本 {} 中未找到选择项：{selector}。请重新发送对应列表命令。",
                snapshot.generation
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, label: &str) -> WechatCatalogEntry {
        WechatCatalogEntry {
            id: id.to_string(),
            label: label.to_string(),
            detail: String::new(),
        }
    }

    #[test]
    fn numbered_and_id_selection_resolve_the_same_snapshot_entry() {
        let mut runtime = WechatCommandRuntime::default();
        runtime.publish_snapshot(
            "wx-main",
            "peer-a",
            WechatCatalogKind::Rooms,
            vec![entry("room-a", "A"), entry("room-b", "B")],
        );

        let by_number = runtime
            .resolve_selector("wx-main", "peer-a", WechatCatalogKind::Rooms, "2")
            .expect("number selector");
        let by_id = runtime
            .resolve_selector("wx-main", "peer-a", WechatCatalogKind::Rooms, "room-b")
            .expect("id selector");

        assert_eq!(by_number, by_id);
        assert_eq!(by_number.id, "room-b");
    }

    #[test]
    fn duplicate_labels_require_number_or_stable_id() {
        let mut runtime = WechatCommandRuntime::default();
        runtime.publish_snapshot(
            "wx-main",
            "peer-a",
            WechatCatalogKind::Sessions,
            vec![
                entry("session-bailian", "GLM5.2"),
                entry("session-openai", "GLM5.2"),
            ],
        );

        let error = runtime
            .resolve_selector("wx-main", "peer-a", WechatCatalogKind::Sessions, "GLM5.2")
            .expect_err("重名会话不能静默选择第一个");

        assert_eq!(error.code, "selector_ambiguous");
        assert!(error.message.contains("session-bailian"));
        assert!(error.message.contains("session-openai"));
    }

    #[test]
    fn numbered_selection_never_reuses_another_peers_snapshot() {
        let mut runtime = WechatCommandRuntime::default();
        runtime.publish_snapshot(
            "wx-main",
            "peer-a",
            WechatCatalogKind::Sessions,
            vec![entry("session-a", "A")],
        );

        let error = runtime
            .resolve_selector("wx-main", "peer-b", WechatCatalogKind::Sessions, "1")
            .expect_err("peer-b must not reuse peer-a snapshot");
        assert_eq!(error.code, "catalog_snapshot_missing");
    }

    #[test]
    fn published_catalog_is_numbered_and_contains_stable_ids() {
        let mut runtime = WechatCommandRuntime::default();
        let snapshot = runtime.publish_snapshot(
            "wx-main",
            "peer-a",
            WechatCatalogKind::Models,
            vec![entry("bailian::glm-5.2", "GLM5.2")],
        );

        let text = snapshot.render("可用模型");
        assert!(text.contains("1. GLM5.2"));
        assert!(text.contains("ID: bailian::glm-5.2"));
        assert!(text.contains("目录版本：1"));
    }
}
