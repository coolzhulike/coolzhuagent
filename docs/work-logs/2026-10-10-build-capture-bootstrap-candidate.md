# 2026-10-10 构建采集引导候选入口

新增 New-BuildCaptureBootstrap：串行 locked/offline -j1 构建 build-capture，直接继承终端避免整体驻留输出；复制到本轮新建证据目录，持摘要运行第二次机器可读构建，核对第二次目标字节与副本一致。真实调用退出0，辅助程序 SHA256 ec6ddbbc1c5c0c5407f089214efa84c1f3b971fdcd6ea74d6bece306ac7d1847；结果 tmp/analysis-build-capture-bootstrap-result.json，实际原始双日志路径见结果。

候选尚未验证 Cargo 回执中 compiler-artifact executable 与目标绑定，也未由调用方冻结/核对源码快照，返回 cargo_record_validated=false、source_snapshot_verified=false。不能据此宣布正式引导来源有效。尚需排除继承环境/feature污染，覆盖 PS5.1、异常引导及证据替换，再接入整包报告。package-all 原整份输出采集仍未替换；正式交付未闭环。
