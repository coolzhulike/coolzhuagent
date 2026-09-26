//! 测试专用：**进程环境锁 + 恢复式作用域**（RPR-01b 的"同进程读者"那一半）。
//!
//! ## 为什么光有恢复还不够
//!
//! 之前各处用的守卫只解决了**泄漏**（用例结束/panic 后把变量恢复），没有解决**并发**：
//! `std::env::set_var` 改的是**整个进程**的环境，而 Rust 的测试默认在同一个进程里
//! **多线程并行**跑。于是两个用例各自把"输入安全库根"指向自己的临时目录时，
//! 任一方在作用域内读到的都可能是**对方的**值——读到的库根与它自己刚设定的不同，
//! 表现为随机失败或"根未注入"，且只在并发时才出现。
//!
//! 因此本模块把两件事绑在一起：
//!
//! 1. **一把全 crate 共用的锁**（`process_env_lock`）——只在**同一测试二进制**内有效，
//!    而 cargo 给每个 crate 的单元测试各自一个进程，所以跨 crate 不会互相干扰。
//! 2. **作用域守卫同时持锁**——进入时记原值，Drop 时**在锁内**恢复。
//!
//! ## 约定（与 `llm-adapter::process_env_lock`、`tool-registry::process_state_lock` 同一口径）
//!
//! - 规则：**任何**改写进程环境的测试代码都必须经由本模块的 `set`/`remove` 拿守卫，
//!   不要在别处写裸 `std::env::set_var`。源码级守卫 `test_environment_writes_are_paired_with_a_restoring_guard`
//!   会检查 web-console 的测试区里没有裸写入（`test_env.rs` 之外）。
//! - 析构顺序：守卫的 `Drop` 先跑（恢复环境），随后才释放锁字段——所以**恢复动作始终在锁内**。
//!   这也是"必须把锁放在同一个结构体里"而不是让调用方自己先取锁的原因：调用方一旦写错顺序，
//!   就会在无锁状态下恢复。
//! - 取锁容忍毒化：某用例持锁 panic 后，不应把后续用例级联变成 `PoisonError` 失败。
//! - **能不用进程环境就不用**：解析逻辑应提供**显式取值**的纯函数（例如
//!   `input_safety_store::input_safety_state_root_from`），测试优先走那条路；
//!   本模块只服务于"确实要验证环境变量被读取"的那几个用例。

use std::ffi::{OsStr, OsString};

/// 全 crate 共用的进程环境锁。
///
/// 取锁容忍毒化（`PoisonError::into_inner`）：持锁用例 panic 后仍应允许后续用例取锁。
fn lock_process_env() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

thread_local! {
    /// 本线程**存活中**的环境守卫个数。用于让守卫可重入：
    /// 嵌套 `set`（测试辅助函数先设一次、用例再设一次）是常见写法，
    /// 若每次都去抢同一把非重入锁就会**自锁**。
    static LIVE_GUARDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 需要时才取锁：本线程已经有存活守卫时不再取（重入）。
///
/// 为什么重入是安全的：内层守卫一定在外层之前析构，所以内层的**恢复动作**
/// 发生在外层仍持锁的时候；外层析构时先恢复、后释放锁。整个过程恢复都在锁内。
fn lock_unless_reentrant() -> Option<std::sync::MutexGuard<'static, ()>> {
    if LIVE_GUARDS.with(std::cell::Cell::get) > 0 {
        None
    } else {
        Some(lock_process_env())
    }
}

/// 一个环境变量的恢复守卫；**持有进程环境锁直到它被析构**。
///
/// 字段顺序即析构顺序：`Drop::drop`（恢复环境）先执行，之后 `_lock` 才释放。
#[must_use = "守卫必须在作用域内存活到测试结束，否则环境变量会被提前恢复"]
pub(crate) struct ScopedEnvVar {
    // 刻意放在守卫结构体内（而不是让调用方先取锁）：杜绝"在无锁状态下恢复"这种顺序错误。
    // `None` 表示本线程已重入持有该锁（见 `lock_unless_reentrant`）。
    _lock: Option<std::sync::MutexGuard<'static, ()>>,
    name: String,
    previous: Option<OsString>,
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        LIVE_GUARDS.with(|depth| depth.set(depth.get().saturating_sub(1)));
        match self.previous.take() {
            Some(value) => std::env::set_var(&self.name, value),
            None => std::env::remove_var(&self.name),
        }
    }
}

/// 设置一个环境变量，并在作用域结束时（锁内）恢复原值。
///
/// 值类型放宽到 `AsRef<OsStr>`：调用点既有字符串也有 `PathBuf`（临时目录）。
pub(crate) fn set<V: AsRef<OsStr>>(name: &str, value: Option<V>) -> ScopedEnvVar {
    let lock = lock_unless_reentrant();
    LIVE_GUARDS.with(|depth| depth.set(depth.get() + 1));
    let previous = std::env::var_os(name);
    match &value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    ScopedEnvVar {
        _lock: lock,
        name: name.to_string(),
        previous,
    }
}

/// 删除一个环境变量，并在作用域结束时（锁内）恢复原值（原本不存在则仍是删除）。
pub(crate) fn remove(name: &str) -> ScopedEnvVar {
    // 显式给出类型参数：`None` 无法单独推断 `V`。
    set::<&str>(name, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &str = "COOLZHU_TEST_ENV_PROBE_RPR01B";

    /// 基本恢复：进入前的值在作用域结束后回来（原本存在）。
    #[test]
    fn value_is_restored_after_the_scope() {
        let _outer = set(PROBE, Some("outer"));
        {
            let _inner = set(PROBE, Some("inner"));
            assert_eq!(std::env::var(PROBE).as_deref(), Ok("inner"));
        }
        assert_eq!(std::env::var(PROBE).as_deref(), Ok("outer"));
    }

    /// 原本**不存在**的变量，作用域结束后必须仍然不存在（不是留空串）。
    #[test]
    fn absent_variable_stays_absent() {
        let _cleanup = remove(PROBE);
        assert!(std::env::var_os(PROBE).is_none());
        {
            let _scoped = set(PROBE, Some("temporary"));
            assert_eq!(std::env::var(PROBE).as_deref(), Ok("temporary"));
        }
        assert!(
            std::env::var_os(PROBE).is_none(),
            "原本不存在就必须恢复成不存在"
        );
    }

    /// **锁的意义**：并发改写同一变量时，持锁者在自己作用域内读到的必须是**自己的**值。
    ///
    /// 判别性：去掉 `set` 里的取锁，这个用例就会不稳定地红——因为另一个线程会在它的作用域
    /// 中间把值改掉。这正是"只有恢复、没有锁"时会发生的事。
    #[test]
    fn concurrent_writers_cannot_observe_each_others_value() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // 先清干净，避免上一个用例的残留影响判定。
        // **放在块里**：守卫持有进程环境锁，若让它在整个测试体内存活，
        // 下面派生的线程会永远等锁（自锁）。
        {
            let _cleanup = remove(PROBE);
        }

        let mismatches = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for index in 0..4 {
            let mismatches = Arc::clone(&mismatches);
            handles.push(std::thread::spawn(move || {
                // 每个线程在**自己的锁作用域**里反复读自己那一份值。
                let _scoped = set(PROBE, Some(format!("writer-{index}")));
                let expected = format!("writer-{index}");
                for _ in 0..200 {
                    if std::env::var(PROBE).ok().as_deref() != Some(expected.as_str()) {
                        mismatches.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }));
        }
        for handle in handles {
            handle.join().expect("线程不得 panic");
        }
        assert_eq!(
            mismatches.load(Ordering::SeqCst),
            0,
            "持锁期间读到了别人的值 ⇒ 进程环境锁没有生效"
        );
    }

    /// 可重入：嵌套 `set` 不得自锁，且内层结束后外层仍有效。
    ///
    /// 判别性：把守卫改成"每次 set 都直接取非重入锁"，这个用例会**永久挂起**。
    #[test]
    fn nested_scopes_do_not_self_lock_and_inner_restore_leaves_outer_intact() {
        let _outer = set(PROBE, Some("outer"));
        {
            let _inner = set(PROBE, Some("inner"));
            assert_eq!(std::env::var(PROBE).as_deref(), Ok("inner"));
        }
        assert_eq!(
            std::env::var(PROBE).as_deref(),
            Ok("outer"),
            "内层析构不得把外层设的值一起带走"
        );
    }
}
