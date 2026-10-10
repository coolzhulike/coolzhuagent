# CLI 进程恢复验证

直接运行独立集成工作区 debug/coolzhu-cli.exe --resume，使用 tmp 中隔离构造的会话与事件文件，没有模型请求、后台服务或正式会话写入。本次是实际编译二进制的离线入口验证，不代表已安装版或外部模型正式恢复验收。

首轮 9 场景中，未来 schema 与空身份错误地退出 0；其余旧会话、completed、failed、interrupted、缺终态、重复身份、错误序号行为符合预期。原始失败记录 before-fix.json 保留。修复为仅接纳 schema_version=1，且 thread/turn 非空白。

修复后正常离线编译退出 0，完整 CLI 回归 90 通过。真实进程再运行 9 场景全部符合预期：四种有效/旧会话退出 0，五种无效记录退出 1，所有会话文件前后 SHA 不变。最终工具会话 4746 退出 0，after-fix.json 包含实际二进制 SHA、退出码与输出。

脚本和日志位于 tmp/analysis-cli-history-process.ps1、tmp/analysis-cli-history-schema-build.txt、tmp/analysis-cli-history-schema-tests.txt；证据内 fixture_root 为实际临时测试目录。输入为合成轨迹，不将其计为真实模型回合、断电恢复或 UI 验收。正式 0.2.130 不含该修复；32WBS 继续未闭环。
