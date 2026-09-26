//! **宿主侧的 READY 等待与校验**（8.3c-A-runtime · Phase 1 的宿主半边）。
//!
//! 对应裁决 §三 Phase 1 的出口条件，以及 **T12（伪 READY）**：
//! READY **不是"文件存在"**——必须校验 **valid nonce ＋ valid state ＋ valid helper identity**。
//!
//! ## 边界（为什么这一块可以先行而不算半升级）
//!
//! - **本模块不启用任何东西**：它只是一个"等待并校验 READY"的宿主侧原语，
//!   生产 `controlled_*` 路径**不调用它**（Phase 3 才做一次性切换）。
//! - **不改内嵌 PowerShell／C# 脚本**：脚本侧写入 READY 属下一步；本模块先在宿主侧把
//!   "怎么算合法 READY""等多久""怎么失败"定死并测住，使脚本侧落地时不需要同时改两端语义。
//! - **有界**：等待必须有上限，且**不能**是忙等（固定间隔轮询 + 预算）。
//!
//! ## 与既有等待原语的关系
//!
//! 形状与 `input.rs` 里 `cancel_file` 的轮询一致（固定间隔、有界、可取消），
//! **不引入新的等待机制**；cancel 优先级在 helper 侧循环里实现，本模块只负责"等到 READY"。

//! ## 为什么暂时允许 dead_code（显式，不是掩盖）
//!
//! 本模块是 Phase 1 的**宿主半边**，当前只有它自己的用例在调它；把它接进生产调用链
//! 属 Phase 2（`ExecutorStore` ＋ `PermitGate` 编排）。因此本模块当前**没有生产调用者**，
//! 但**不是**被丢弃的代码：它已由本模块用例与后续 T6 系列真实驱动。
//! **接线落地后应移除此豁免**——留着它才是问题。
#![allow(dead_code)]

use std::path::Path;
use std::time::{Duration, Instant};

use runtime::HelperReadySignal;

/// 轮询间隔：与既有 cancel/progress 轮询同量级，避免高频空转。
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// 等待 READY 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyOutcome {
    /// 拿到**合法** READY（已过 nonce／类型／协议版本／pid 校验）。
    Ready {
        waited: Duration,
        signal: HelperReadySignal,
    },
    /// 等到文件、但它不是合法 READY（伪造／缺字段／nonce 不符）：**必须拒绝**，不得继续。
    Rejected { reason: String },
    /// 有界等待超时：**没有** READY。
    TimedOut { waited: Duration },
}

impl ReadyOutcome {
    /// 是否允许继续进入下一阶段（只有合法 READY 可以）。
    #[must_use]
    pub const fn may_proceed(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// **有界等待 helper 的 READY 信号**（宿主侧）。
///
/// `expected_nonce` 是本次会话 nonce：READY 里的 nonce 必须与它一致——
/// 这是防止"读到别的 helper 遗留的 READY"的唯一手段（与 permit 的 nonce 校验同口径）。
///
/// 实现要点：
/// - **不忙等**：固定 `POLL_INTERVAL` 轮询；
/// - **有界**：超过 `budget` 即返回 `TimedOut`，绝不无限等待；
/// - **读到但非法 ⇒ 立即 `Rejected`**（不重试、不当作"还没到"——伪造信号重试只会拖长窗口）。
pub fn await_helper_ready(
    ready_file: &Path,
    expected_nonce: &str,
    budget: Duration,
) -> ReadyOutcome {
    let started = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(ready_file) {
            match serde_json::from_str::<HelperReadySignal>(&text) {
                Ok(signal) => {
                    // T12：**不是文件存在就算 READY**。三重校验之外还要核对 nonce 归属。
                    if let Err(error) = signal.validate() {
                        return ReadyOutcome::Rejected {
                            reason: format!("READY 信号非法（{}）：{}", error.field, error.reason),
                        };
                    }
                    if signal.nonce != expected_nonce {
                        return ReadyOutcome::Rejected {
                            reason: format!(
                                "READY 的 nonce（{}）不属于本次会话（{}）：疑似其它 helper 遗留",
                                signal.nonce, expected_nonce
                            ),
                        };
                    }
                    return ReadyOutcome::Ready {
                        waited: started.elapsed(),
                        signal,
                    };
                }
                Err(error) => {
                    // 解析失败同样是**拒绝**（可能是半截写入或伪造），不是"再等一会"。
                    return ReadyOutcome::Rejected {
                        reason: format!("READY 文件不是合法信号：{error}"),
                    };
                }
            }
        }
        if started.elapsed() >= budget {
            return ReadyOutcome::TimedOut {
                waited: started.elapsed(),
            };
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_json(nonce: &str) -> String {
        format!(
            r#"{{"type":"ready","helper_protocol_version":1,"nonce":"{nonce}","pid":1234,"timestamp_unix_ms":9}}"#
        )
    }

    /// 合法 READY ⇒ 允许继续；且**等待是有界的**（这里应当很快返回）。
    #[test]
    fn valid_ready_for_this_session_proceeds() {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let path = directory.path().join("ready.json");
        std::fs::write(&path, ready_json("n-1")).expect("write");
        let outcome = await_helper_ready(&path, "n-1", Duration::from_secs(2));
        match &outcome {
            ReadyOutcome::Ready { waited, signal } => {
                assert_eq!(signal.nonce, "n-1");
                assert!(*waited < Duration::from_secs(2), "应在预算内返回");
            }
            other => panic!("合法 READY 必须放行：{other:?}"),
        }
        assert!(outcome.may_proceed());
    }

    /// **T12 核心**：文件存在 ≠ READY。
    /// ① 别的 helper 的 nonce；② 缺必填字段；③ 多带授权声明（伪造执行身份）——都必须**拒绝**。
    #[test]
    fn forged_or_foreign_ready_is_rejected_not_treated_as_absent() {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let path = directory.path().join("ready.json");

        // ① nonce 属于别的会话（旧 helper 遗留）。
        std::fs::write(&path, ready_json("n-other")).expect("write");
        let foreign = await_helper_ready(&path, "n-1", Duration::from_secs(2));
        assert!(
            matches!(foreign, ReadyOutcome::Rejected { .. }),
            "别的会话的 READY 必须被拒：{foreign:?}"
        );
        assert!(!foreign.may_proceed());
        if let ReadyOutcome::Rejected { reason } = &foreign {
            assert!(reason.contains("非本次会话") || reason.contains("不属于本次会话"), "{reason}");
        }

        // ② 缺必填字段（例如没有 pid）。
        std::fs::write(
            &path,
            r#"{"type":"ready","helper_protocol_version":1,"nonce":"n-1","timestamp_unix_ms":9}"#,
        )
        .expect("write");
        assert!(
            matches!(
                await_helper_ready(&path, "n-1", Duration::from_secs(2)),
                ReadyOutcome::Rejected { .. }
            ),
            "缺字段必须被拒"
        );

        // ③ 多带授权声明 ⇒ 解析即失败（helper 不能自证执行身份）。
        std::fs::write(
            &path,
            r#"{"type":"ready","helper_protocol_version":1,"nonce":"n-1","pid":1,"timestamp_unix_ms":9,"authorization":"granted"}"#,
        )
        .expect("write");
        let forged = await_helper_ready(&path, "n-1", Duration::from_secs(2));
        assert!(
            matches!(forged, ReadyOutcome::Rejected { .. }),
            "带授权声明的 READY 必须被拒：{forged:?}"
        );
    }

    /// 超时是**有界**的：没有文件 ⇒ `TimedOut`，且耗时贴近预算而不是无限等。
    #[test]
    fn missing_ready_times_out_within_budget() {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let path = directory.path().join("absent.json");
        let budget = Duration::from_millis(150);
        let started = Instant::now();
        let outcome = await_helper_ready(&path, "n-1", budget);
        let elapsed = started.elapsed();
        assert!(matches!(outcome, ReadyOutcome::TimedOut { .. }), "{outcome:?}");
        assert!(!outcome.may_proceed());
        assert!(
            elapsed < budget + Duration::from_secs(1),
            "必须有界：实际 {elapsed:?}"
        );
    }
}
