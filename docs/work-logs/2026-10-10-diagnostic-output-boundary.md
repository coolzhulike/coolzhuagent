# 诊断输出边界脱敏补齐

常规 emit/emit_span_closed 已进行脱敏；审查发现内部 Output 的直接调用依赖上层先净化：文件 span 格式化、控制台 event/span、GUI event/span 未全部自行兜底。输出模块是私有模块，不能据此宣称存在对外公开绕过或已发生泄漏。

本轮将 event 脱敏补到 ConsoleOutput/GuiOutput，将 span 脱敏统一为 SpanClosed::redacted，并在文件格式化、控制台及 emit 路径调用。统一 span 净化同时覆盖事件名称与嵌套属性，不修改调用者原始对象、追踪身份、时间和业务值。GUI span 映射成事件后再次遵守事件边界规则。

实际文件回归直接传入带合成凭据的原始 span，验证文件和 GUI 无原秘密、中文及 duration/trace 保留；嵌套事件属性无原秘密、属性二次脱敏幂等、原对象不被修改。cargo build -p coolzhu-diagnostics --offline -j1 实际 exit 0；完整诊断测试 28 passed/0 failed，实际 exit 0。日志见 tmp/analysis-diagnostic-output-boundary-build.txt 和 tmp/analysis-diagnostic-output-boundary-tests.txt。控制台边界经过源码接线检查，本轮未捕获真实 stderr，不能扩大文件/GUI 回归为控制台实机证据。

候选尚未进入已安装 0.2.128。历史日志清理/完整导出脱敏、子进程输出采集、全局配额及正式 32WBS 验收仍未闭环。
