# 2026-10-10 构建采集 PS5.1 参数兼容

为整包接入准备，将真实进程检查驱动兼容下限改为 PS5.1。PS7 使用 ProcessStartInfo.ArgumentList；PS5.1 按 Windows CRT 规则处理内嵌引号前与尾部反斜杠，不以简单空格包引号代替参数编码。该函数当前仅在测试驱动内，尚未接入生产 package-all。

真实子进程接收含中文、内嵌引号与末尾反斜杠的参数，以及显式空字符串。空参数声明 Mandatory + AllowEmptyString，避免参数丢失后默认空值造成假通过。PS5.1 与 PS7 驱动均实际退出0，各通过增量屏障、既有stdout、既有stderr、未结束后代拒绝四项，日志 tmp/analysis-build-capture-ps5-argument-final.txt、tmp/analysis-build-capture-ps7-argument-final.txt。使用已验证的默认 build-capture.exe，本轮仅驱动/文档修改。

整包接入仍有必要实现：目前9个构建入口均声明cargo-json-messages，辅助程序尚未在manifest中声明。不能直接借用tmp里无身份的EXE；需自举构建、冻结绑定、辅助二进制身份和自身构建回执，并处理辅助程序自采集的构建顺序。尚未改生产采集路径或发布载荷。真实磁盘故障、宿主强退及32WBS正式验收继续开放。
