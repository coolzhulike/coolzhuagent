//! 测试专用：**恢复式**进程环境作用域（RPR-01b）。
//!
//! 为什么需要它：进程环境是**全局**的。测试里 `set_var`/`remove_var` 之后不恢复，会让结果
//! 依赖执行顺序——一个用例留下的 key 会被另一个用例当成"已配置"。既有的 `process_env_lock`
//! 只解决**并发**，不解决**泄漏**；本模块补上后者。
//!
//! 用法：把裸写替换成拿到一个守卫（守卫在**作用域结束时**把变量恢复为进入前的值，
//! 原本不存在则删除）：
//!
//! ```ignore
//! let _guard = crate::process_env_lock();          // 仍然必须先取锁
//! let _scoped = crate::test_env::set("XAI_API_KEY", Some("k"));
//! let _none = crate::test_env::remove("ZAI_API_KEY");
//! ```
//!
//! 两个必须知道的边界：
//!
//! 1. **锁要在守卫之前声明**：守卫按声明的逆序析构（Rust 的 drop 顺序），因此
//!    `_guard` 先声明 ⇒ 它在所有守卫恢复**之后**才释放，恢复动作始终在锁内完成；
//!    若把守卫声明在锁之前，恢复会在无锁状态下发生（并发用例可能观察到中间值）。
//! 2. **不要在会提前结束的花括号里设置**：守卫随其所在作用域析构。把 `set` 放进内层块、
//!    块结束后还要用这个变量，就会提前恢复（此时请把守卫提到与使用范围相同的作用域）。

use std::ffi::OsString;

/// 一个变量的恢复守卫（drop 时恢复进入前的值）。
#[cfg(test)]
#[must_use = "守卫必须在作用域内存活到测试结束，否则环境变量会被提前恢复"]
pub(crate) struct ScopedEnvVar {
    name: String,
    previous: Option<OsString>,
}

#[cfg(test)]
impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(&self.name, value),
            None => std::env::remove_var(&self.name),
        }
    }
}

/// 设置一个环境变量，并在作用域结束时恢复原值。
///
/// 值类型放宽到 `AsRef<OsStr>`：调用点既有字符串也有 `PathBuf`（例如 `CLAW_CONFIG_HOME`
/// 指向临时目录），不给调用方强加转换。
#[cfg(test)]
pub(crate) fn set<V: AsRef<std::ffi::OsStr>>(name: &str, value: Option<V>) -> ScopedEnvVar {
    let previous = std::env::var_os(name);
    match &value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    ScopedEnvVar {
        name: name.to_string(),
        previous,
    }
}

/// 删除一个环境变量，并在作用域结束时恢复原值（原本不存在则仍是删除）。
#[cfg(test)]
pub(crate) fn remove(name: &str) -> ScopedEnvVar {
    // 显式给出类型参数：`None` 无法单独推断 `V`。
    set::<&str>(name, None)
}
