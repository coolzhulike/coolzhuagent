# 公共诊断入口脱敏候选

源码检查确认 `coolzhu-diagnostics` 原先将 `LogEntry.message/fields` 直接写入JSONL、控制台与GUI回调；初始化前的error fallback、span属性及人读面包屑也没有统一脱敏。本次在公共诊断入口及JSONL格式化边界复用一个独立无依赖规则模块，先脱敏后输出，不修改会话、usage、输入安全等权威业务数据。

规则覆盖明确的API密钥、凭据引用、密码、授权/cookie及token字段；文本中的JSON/TOML/CLI/查询参数形式、独立Bearer、常见平台令牌前缀、DPAPI引用、JWT形状与HTTP URL userinfo。字段名大小写及分隔符归一化，`input_tokens/output_tokens/token_count/run_id`等用量与公开关联身份保留。UTF-8切片只在合法ASCII边界进行，多次经过输出边界结果相同。

验证：

- `cargo build -p coolzhu-diagnostics --offline -j1` 实际退出0；`cargo test -p coolzhu-diagnostics --offline -j1` 实际退出0，27项通过，日志 `tmp/analysis-diagnostics-redaction-build.txt`、`tmp/analysis-diagnostics-redaction-regression.txt`。
- 首次独立规则测试揭示Bearer占位符被重复处理后多出右括号，已修正并在幂等测试覆盖；不隐去该失败。
- 独立可执行程序链接实际编译的诊断rlib，经公共init/info/error_event/start_span/append_diagnostic_line API检验JSONL、GUI回调、span和面包屑；控制台与初始化前stderr另行检查，虚构测试凭据均未泄露，中文、token计数和公开run身份保留，实际退出0。源与输出在 `tmp/analysis-diagnostic-sinks.rs`、`tmp/analysis-diagnostic-sinks-console.txt`。此处GUI为实际公共回调，不作为正式原生页面实拍。

仍开放：未经过公共诊断crate的tracing/eprintln/直接文件写入生产者、历史日志的受控脱敏导出、任意无字段语义的秘密文本、全局磁盘配额及错误故障矩阵。规则覆盖不能宣称所有内容绝不含秘密；旧日志不由此自动改写。WBS6.1整体未闭环，正式安装后的公共日志路径及其它生产者待统一验收。
