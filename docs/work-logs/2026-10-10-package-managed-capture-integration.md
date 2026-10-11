# 2026-10-10 整包增量受管采集接入

package-all.ps1 的 cargo-json-messages 路径正式改为 Invoke-ManagedBuildCapture 固定缓冲双管道增量落盘，移除 ReadToEnd/ReadToEndAsync 全量驻留及文本重编码。stderr仅逐行回显。每轮首次采集执行两步helper引导，独立helper副本SHA受核验，原始helper Cargo JSON按package.build-capture纳入构建证据归档；报告登记候选引导信息。

完整Loader契约第二次真实进程句柄26620终态exit0，日志 tmp/analysis-capture-integrated-loader-second.txt；此前125的两项已覆盖预期PRODUCER-NOT-FOUND、ARCH-MISMATCH。manifest检查已通过。首次失败日志保留，不能用第二次成功抹去失败记录。

四项跳过保持未闭环：全新target离线构建、清洁Windows原生窗口、Runtime分发变化、DLL删除审计。本轮没有正式MSI安装验收。source_snapshot_verified在引导回执仍false；仅现有整包前后采样不能证明不可变输入，用户/祖先Cargo配置尚未全部核验，已有build-identity明确用户配置不纳入。helper暂不属于分发payload，记录构建工具身份不等于分发组件完成。宿主退出、诊断写入失败下生命周期仍需验证。32WBS与正式交付未完成。
