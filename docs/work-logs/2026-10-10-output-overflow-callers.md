# 输出超限在 Hook 与 Goal 入口的负例

审查实际调用者：Web 插件 managed_process 错误经 map_err 返回 execution_failed，Goal 验证命令返回 Err，注册工具 REPL 与 bash 传播错误；未找到这些调用者将采集超限转换为退出码零的分支。源码检查不能替代完整宿主运行验收。

新增 Hook 真实 Windows 负例：stdout/stderr 分别产生超过单管道上限的输出，脚本计划正常退出零；调用者必须返回 Deny 并保留“输出超限”事实，不能 Warn/Allow。Hook 全部针对性回归实际 exit 0，12 通过、0 失败，359 过滤；日志 tmp/analysis-hook-overflow-final-tests.txt，不外推为完整运行时测试。

初次用 -Command 内嵌引号的测试失败，诊断确认实际只输出 37 字节脚本文本；是测试命令经 cmd 转义改变了含义，并未执行超限输出。改为关闭临时文件句柄后的 .ps1 -File 路径执行，原失败日志 tmp/analysis-hook-overflow-dispatch-tests.txt 与 tmp/analysis-hook-overflow-probe.txt 保留，不将其算作生产保护失效或通过。

Goal 验证命令也新增 stdout/stderr 超限、计划退出零的负例，要求返回含已启动及超限事实的 Err。针对性命令实际 exit 0，2 通过、0 失败，1462 过滤；日志 tmp/analysis-goal-overflow-tests.txt。本轮仅新增测试，未更改生产代码，不将针对性测试计为完整主控制台回归。无正式模型调用、真实共享后台变更或安装更新；完整输出留存/引用与诊断全局配额仍开放。
