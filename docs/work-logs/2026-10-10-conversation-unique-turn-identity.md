# 跨运行时回合身份修复

此前运行时创建后计数归零，同一历史会话恢复后的首回合再次使用 turn-1。本次删除进程内回合计数，使用操作系统随机源生成 256 位不透明回合身份。分配在修改用户消息之前进行；随机源失败返回错误，不以重复编号降级。回合顺序应从事件及历史表达，不再从标识字符串推断。未更改 Session 旧格式及手写 JSON。

新增恢复测试通过 Session 的真实 to_json/from_json 路径恢复历史后重建运行时，验证错误回合身份不同，且轨迹内身份一致。既有成功、失败矩阵及取消测试也通过。未发现源码中解析 turn- 数字后缀的调用方；这项搜索不能替代外部调用方兼容性验证。

实际串行离线验证：core-runtime 和 command-line build 退出 0；core-runtime 完整回归 372 通过、0 失败、总计 3 忽略；CLI 完整回归 89 通过、0 失败。工具会话 81104 最终退出 0。日志分别为 tmp/analysis-conversation-unique-turn-build.txt、tmp/analysis-conversation-unique-turn-runtime-tests.txt、tmp/analysis-conversation-unique-turn-cli-tests.txt。

256 位随机标识避免确定性的重启重复，不宣称数学上绝无碰撞。默认 thread-default 仍需正式会话身份接入；成功事件实际持久消费、崩溃恢复、错误与取消竞争以及随机源故障注入仍未完成。该源码尚未进入已安装 0.2.130，32WBS 整体验收未闭环。
