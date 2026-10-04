# DSH正式启用与父轮工具派发证据

[审查报告](../../analysis/2026-10-02-dsh-web-dispatch-review.md) / [工作记录](../../work-logs/2026-10-02-local-session-dsh-web-dispatch.md) / [摘要索引](evidence-index.json)

最终成功/失败日志、当前25个源文件和2个本轮二进制摘要独立归档；35个复制产物逐一核对SHA256。没有复制用户会话库或密钥。原文档SHA256未变，165条历史未跟踪入口均存在。

- [完整真实工程回执](final-engineering-result.json)：官方源码、完整固定资源、原Web派发/审批、实际96、SDK错误、并发审批与一次性领取、父轮过期/结束和停用/重装拒绝；真实Node握手后的取消及停用均确认收尾。provider元数据为受控工程输入，不冒充模型回包。
- [默认栈完整工程日志](probe-final-07.txt)：1通过、0失败，76.46秒；生产资源选择器使用测试二进制旁的真实固定资源，无SDK/Node替代实现。
- [最终构建](final-build-after-poll.txt)：三crate通过，48.25秒。
- [最终完整回归](aggregate-test-final-after-poll.txt)：插件37、Web主目标1302/0/3忽略，另8+1；3忽略中的真实资源场景由上述显式工程运行单独验证，其他2仍忽略。
- [Core定向传播](core-control-test.txt)：1通过、356过滤，未验Core全套；[模块链接](module-linkage.txt)8通过。
- [最终静态检查](final-checks.json)：tool-registry、两份JS语法及diff检查全部退出0。
- [正式只读状态](installed-readonly.json)、[事故执行者](incident-executor-readonly.json)、[正式文件摘要](installed-hashes-readonly.json)：正式仍063/09dc04d，isolated、不接受输入、1未放行block、epoch48；Shell3864原创建身份仍存活。

`history-*`保留中间源码时点全部构建/测试成功和失败：宏/测试辅助签名、启用超时、栈溢出、取消观察失败、并发审批超时等。`history-expanded-stack-result.json`仅为扩大测试栈后的诊断；最终采用默认栈，不把历史部分通过拼成完整成功。

本目录不能证明真实Qwen、正式MSI/按钮UI、Browser竞争边界、完整Paint、动画/泛光或总体验收。没有发项目模型网络请求、创建凭证、上传GitHub、安装MSI、放行隔离或代签人工复核。后续需明确免费模型预算/验收环境并取得原生computer-use能力；人工处理入口及退出条件见审查报告。
