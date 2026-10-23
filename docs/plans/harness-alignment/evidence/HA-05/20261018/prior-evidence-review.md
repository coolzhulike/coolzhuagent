# 原证据适用性复核

本次差异只含main.rs接入FailureGuard与终态失败标记，以及新模块tool_loop_progress.rs、ha05_tests.rs。implementation.patch相对于HA04已验收输入1f87e1…，不是相对于Git HEAD。未变更请求绑定、权限、hook、取消、基线快照或CLI/adapter实现。

HA00冻结manifest/source.patch哈希重新核对未变；HA04的102项基线恢复、原基线测试和调用链审查仍适用，不将候选结果写成基线。新循环仍走同一工具入口、身份和live gate；当前HA05缺口由acceptance补充，HA06～10仍未完成。

HA01～04的Web测试70项在当前输入真实复跑，新增回执代替当前引用；旧对象及旧哈希保持原样。CLI与adapter相关输入未改，保留其原实际回执；不重复无关构建或账户测试。真实模型与HA06故障语义未因此获得通过。
