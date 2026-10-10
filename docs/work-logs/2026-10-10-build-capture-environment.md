# 2026-10-10 采集引导环境约束

引导构建在证据目录创建前拒绝未核验的 rustflags、编译器/包装器、构建 target 环境覆盖，以及故障注入环境变量；不打印变量值。target-dir 由显式参数指定，Cargo并行数由-j1固定。

PS5.1/PS7 各六个进程内环境污染场景通过（实际退出0），证据 tmp/analysis-capture-environment-ps5.txt、tmp/analysis-capture-environment-ps7.txt。污染环境下证据目录未创建。PS5.1 干净环境实际两次 locked/offline 构建及 Cargo 产物绑定也通过，日志 tmp/analysis-bootstrap-environment-ps5.txt，结果 tmp/analysis-bootstrap-environment-ps5-result.json。

这是环境覆盖门禁，不是全部 Cargo 配置控制：仓库/祖先/用户 Cargo 配置、全量输入冻结、工具链身份与路径重解析仍需纳入整包来源核验。正式整包尚未接入，正式验收与32WBS未闭环。
