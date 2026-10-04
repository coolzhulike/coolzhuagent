# 2026-10-02 本地接续与DSH固定来源检查

基线分支 `codex/cu-preinput-followup-20260930`、HEAD `8181f08`；正式安装仍0.2.63，本轮未提交、打包、安装或改变安全状态。原文档修改和历史未跟踪图片保留，源码改前备份在 `tmp/2026-10-02-dsh-source/backups/`。

已核查AGENTS、受支持记忆、指定停点报告及实际computer-use工具。独立完成目录条目→固定Git来源→完整源码核验的只读API和前端入口；npm为null时从固定提交根清单取得身份，有npm声明时严格比对。检查不启用/安装插件，不运行脚本，不发模型请求。

真实官方22文件、固定commit/tree、HEAD、npm冲突、预取消、浮动参数拒绝与暂存0项工程通过，14.525秒。首轮网络失败和全部回归失败保留。最终Web离线build28.49秒；隔离根Web1300/0/2既有忽略（39.47秒），另8+1通过；tool-registry check通过，链接8/0（2.44秒）。

正式只读安全状态仍isolated、accepts_new_input=false：1未放行阻断、5历史人工复核操作。未代签复核、删事故、重置库或绕过release_unknown。原生GUI调用入口缺失；完整Paint、Browser释放期间关闭、DSH正式安装/快照/Qwen/MSI和动画边界继续开放。五项原始编号清单已异步请求补充。

详细版本、职责、原失败与下一门槛：[本轮审查](../analysis/2026-10-02-dsh-source-resolution-review.md)。原始回执/日志：[证据目录](../testing/dsh-source-resolution-2026-10-02/)。
