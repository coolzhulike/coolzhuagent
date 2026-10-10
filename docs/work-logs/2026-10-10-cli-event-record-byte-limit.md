# 2026-10-10 CLI 单条事件读写字节上限一致

恢复端原有 8MiB 单条限制，保存端却 to_vec_pretty 无界序列化并发布。当前改为有界 Write 适配器驱动 serde_json::to_writer_pretty，超过共享 MAX_EVENT_RECORD_BYTES 时返回错误，发生在预留 pending 之前。原始事件对象本身仍已驻留内存，Vec 容量分配不是峰值内存实测。不发布截断 JSON，不把保存失败冒充执行成功。

新增测试按实际序列化字节验证中文和转义、边界恰好成功与少一字节拒绝、实际 8MiB 原始字符串因 JSON 引号额外字节而拒绝。首次构建退出 101，原因 CLI 缺直接 serde 依赖，日志 tmp/analysis-cli-event-size-build.txt。已补 serde 1 直接依赖，Cargo.lock 仅反映依赖关联，使用已缓存 crate 离线构建。

修复后 cargo build -p coolzhu-command-line --offline -j1 实际退出 0；完整 cargo test 同包实际退出 0，93 通过、0 失败、0 忽略。串行工具会话 36494 终态 0，日志 tmp/analysis-cli-event-size-build-repair.txt、tmp/analysis-cli-event-size-tests.txt。

尚未闭环：恢复累计 64MiB / 10000 条限制尚无写入端跨进程原子预算；超限保存失败发生时会话消息可能已经保存，与事件并非统一事务；外部执行事实不会因轨迹拒绝而自动撤销。正式安装仍为旧 0.2.130，32WBS 正式恢复、完整输出引用及总目标未完成。
