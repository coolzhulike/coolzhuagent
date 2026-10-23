# HA08 读并行/写独占：部分实施

状态：**仅实现读写锁分类与构建/回归；HA08 四条未逐条验收，HA08 仍 in_progress。**

## 实现

- `model_tool_is_safe_parallel_read`：仅 `read_file`/`glob_search`/`grep_search`/`WebFetch`/`WebSearch` 被视为安全读。
- `dispatch_model_tool_calls_parallel` 增加 `tokio::sync::RwLock<()>`：
  - 安全读持有读锁，可并行；
  - 写、未知工具、已配置 hook 默认持有写锁，互斥；
  - 安全读与写互斥，避免同批次读写交错。
- 有 hook 配置时所有工具按独占处理（未知 hook 默认有序）。

## 已验证

- `cargo build -p coolzhu-web-console --offline` exit 0（`runs/build-01/output.log`）。
- 当前源码既有 124 项定向回归通过、0 失败、0 忽略，180.29 秒，运行期间无源码变化（`runs/current-regression-01/`）。
- 既有 `dispatch_model_tool_calls_parallel_preserves_order` 不在该 124 项过滤内，未在本次证据中单独执行。

## 未完成

- HA08-01：两个安全读执行区间重叠的屏障/时间证据未补。
- HA08-02：同资源读写、未知工具/hook 的记录顺序和最终内容断言未补。
- HA08-03：插件宿主/浏览器表面/受限进程的资源资格、HOST_BUSY 计数未补。
- HA08-04：排队取消/撤销、单调用失败、CLI 同步合同未补。
- 当前实现是全局读写锁，不区分不同独立资源；可能过度串行，后续需按真实资源资格收窄。
