//! 网络重复提交的去重契约（§2.1）。
//!
//! 规范原文：
//!
//! > 网络重复提交 | `client_message_id + scope + content_digest`：同 ID 同内容返回同收据，
//! > 不再次执行；同 ID 不同内容拒绝
//!
//! 现有的聊天去重是**纯内容指纹 + 时间窗**（`main.rs` 的 `check_chat_request_duplicate`）：
//! 它能拦住"同样内容重复提交"，但拦不住"同一个客户端消息 ID 下内容被改过"的第二次提交——
//! 那时两次内容不同、指纹不同，于是**两次都会执行**。本模块把缺的那一维（客户端消息 ID
//! 与作用域）补成可回归的纯逻辑；**尚未接线**到请求入口，接线属 S2.1 之后的迁移。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一次消息提交的去重键：客户端消息 ID + 作用域 + 内容摘要。
///
/// `scope` 用于把不同会话/房间/目标的同名 ID 隔离开（同 ID 在不同作用域互不影响）。
///
/// RPR-03 追加：`Serialize`/`Deserialize` 派生只为让**同一类型**可被事实日志持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageSubmissionKey {
    pub client_message_id: String,
    pub scope: String,
    pub content_digest: String,
}

impl MessageSubmissionKey {
    #[must_use]
    pub fn new(
        client_message_id: impl Into<String>,
        scope: impl Into<String>,
        content_digest: impl Into<String>,
    ) -> Self {
        Self {
            client_message_id: client_message_id.into(),
            scope: scope.into(),
            content_digest: content_digest.into(),
        }
    }

    fn slot(&self) -> (String, String) {
        (self.client_message_id.clone(), self.scope.clone())
    }
}

/// 提交判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionDecision {
    /// 首次见到该 (ID, 作用域)：正常执行，收据按传入值登记。
    New,
    /// 同 ID 同内容：返回既有收据，**不再执行**。
    ReturnExistingReceipt { receipt_id: String },
    /// 同 ID 不同内容：**拒绝**，并带出已在处理的摘要供诊断。
    RejectConflict { existing_digest: String },
}

/// 已登记的提交记录。
///
/// RPR-03：由私有结构改为公开（**不新增同义类型**），以便事实存储把"这个槽位曾以
/// 哪个摘要、哪张收据提交过"原样读回；比较摘要的职责仍在调用方，存储不替它下结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionRecord {
    pub content_digest: String,
    pub receipt_id: String,
}

/// 进程内的提交去重表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageSubmissionRegistry {
    seen: BTreeMap<(String, String), SubmissionRecord>,
}

impl MessageSubmissionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一次提交并给出判决。只有 `New` 才允许继续执行。
    pub fn submit(
        &mut self,
        key: &MessageSubmissionKey,
        receipt_id: impl Into<String>,
    ) -> SubmissionDecision {
        match self.lookup(key) {
            SubmissionDecision::New => {
                self.seen.insert(
                    key.slot(),
                    SubmissionRecord {
                        content_digest: key.content_digest.clone(),
                        receipt_id: receipt_id.into(),
                    },
                );
                SubmissionDecision::New
            }
            decision => decision,
        }
    }

    /// 只读判决：口径与 `submit` 完全一致，但**不登记、不改变任何状态**。
    ///
    /// 存储层需要"先判决、后落盘"（落盘失败时内存不得领先于磁盘），因此把它单独
    /// 暴露出来；`submit` 也已改为调用本方法，两条路径不可能分叉。
    #[must_use]
    pub fn lookup(&self, key: &MessageSubmissionKey) -> SubmissionDecision {
        match self.seen.get(&key.slot()) {
            Some(existing) if existing.content_digest == key.content_digest => {
                SubmissionDecision::ReturnExistingReceipt {
                    receipt_id: existing.receipt_id.clone(),
                }
            }
            Some(existing) => SubmissionDecision::RejectConflict {
                existing_digest: existing.content_digest.clone(),
            },
            None => SubmissionDecision::New,
        }
    }

    /// 只读查回该槽位已登记的记录。`None` 表示**从未提交过**（unknown），
    /// 不是"某条默认记录"，也不是"内容摘要为空"。
    #[must_use]
    pub fn record(&self, key: &MessageSubmissionKey) -> Option<&SubmissionRecord> {
        self.seen.get(&key.slot())
    }

    /// 已登记的 (ID, 作用域) 数量，便于测试与诊断。
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageSubmissionKey, MessageSubmissionRegistry, SubmissionDecision};

    fn key(id: &str, scope: &str, digest: &str) -> MessageSubmissionKey {
        MessageSubmissionKey::new(id, scope, digest)
    }

    /// 首次提交放行。
    #[test]
    fn first_submission_is_new() {
        let mut registry = MessageSubmissionRegistry::new();
        assert_eq!(
            registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-1"),
            SubmissionDecision::New
        );
        assert_eq!(registry.tracked(), 1);
    }

    /// 同 ID 同内容：返回**既有收据**，不再次执行。
    #[test]
    fn same_id_same_content_returns_the_existing_receipt() {
        let mut registry = MessageSubmissionRegistry::new();
        registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-1");

        assert_eq!(
            registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-2"),
            SubmissionDecision::ReturnExistingReceipt {
                receipt_id: "receipt-1".to_string()
            }
        );
        assert_eq!(registry.tracked(), 1, "重复提交不得新增登记");
    }

    /// 同 ID 不同内容：**拒绝**（这是内容指纹方案拦不住的那一类）。
    #[test]
    fn same_id_different_content_is_rejected() {
        let mut registry = MessageSubmissionRegistry::new();
        registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-1");

        assert_eq!(
            registry.submit(&key("m-1", "room-1", "digest-b"), "receipt-2"),
            SubmissionDecision::RejectConflict {
                existing_digest: "digest-a".to_string()
            }
        );
        assert_eq!(registry.tracked(), 1);
    }

    /// 不同 ID 即使内容相同也各自放行（是两次不同的消息）。
    #[test]
    fn different_ids_with_identical_content_are_independent() {
        let mut registry = MessageSubmissionRegistry::new();
        assert_eq!(
            registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-1"),
            SubmissionDecision::New
        );
        assert_eq!(
            registry.submit(&key("m-2", "room-1", "digest-a"), "receipt-2"),
            SubmissionDecision::New
        );
        assert_eq!(registry.tracked(), 2);
    }

    /// 作用域隔离：同 ID 在不同作用域互不影响。
    #[test]
    fn scope_isolates_identical_client_ids() {
        let mut registry = MessageSubmissionRegistry::new();
        assert_eq!(
            registry.submit(&key("m-1", "room-1", "digest-a"), "receipt-1"),
            SubmissionDecision::New
        );
        assert_eq!(
            registry.submit(&key("m-1", "room-2", "digest-b"), "receipt-2"),
            SubmissionDecision::New
        );
        assert_eq!(registry.tracked(), 2);
    }
}
