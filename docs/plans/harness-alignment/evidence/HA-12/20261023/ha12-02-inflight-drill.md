# HA12-02 在途命令/未知效果演练

## 受控演练（check_plan 对真实 state 副本的拒绝）

脚本 `tmp/ha11-run/ha12-drill.py` 对真实 `state.json` 的内存副本注入异常，调用 `validate()`（complete 模式）：

| 注入 | 校验器输出 |
|------|-----------|
| handoff.running_commands 放入一条带副作用命令 | `仍有 running_commands，不能宣布全部收口` |
| handoff.unresolved_effects 放入未知效果 | `仍有 unresolved_effects，不能宣布全部收口` |
| active_task 指向已验收 HA-05 | `active_task 必须指向未完成必做项或为 null` + `全部收口时 active_task 应为 null` |
| 删除整个 HA-11 工作包 | `工作包编号缺失…` + `HA-12: 无效依赖 HA-11` |

原始输出：`tmp/ha11-run/ha12-drill.log`。4/4 均被拒——未完成/未知状态不会被静默收口。

## 真实在途事件处置记录（本轮实际发生，非模拟）

1. **服务僵死**：基线/候选隔离服务两次在执行 Goal 阶段 HTTP 请求中途失去响应（进程存活、HTTP 无应答、sqlite 只读探测也阻塞）。处置顺序：先用只读 sqlite 探针确认非平台性损坏 → 定位具体 PID → 只杀确认无响应的服务进程 → 重启后静置再驱动。**未全局杀进程、未重跑未确认副作用**。
2. **中断证据保全**：`run-b08b.ps1` 曾覆盖半成品 flow；加固为 `RunTag` 分目录 + 每步增量落盘 + 启动活性预检。此后中断的运行保留各自目录，不覆盖、不合并。
3. **构建环境污染**：`cargo test` 期间发现 `target\debug\deps` 内 56 个 0 字节 `.rmeta/.o` 残件（早前 rustc 停滞被杀留下），报错形态为"only metadata stub"/"invalid metadata"。处置：清残件与对应指纹后重编，不整体 clean 未证伪的缓存。
4. **效果计数交叉验证**：任何"门执行/回执复用"结论均以文件行数计数器（b08-gate-*-count.txt）+ sqlite `tool_calls` 审计行数为准，不以进程退出码推断。

结论：检查点对在途/未知状态拒绝收口；真实中断按"先核对进程与记录，再决定重跑范围"执行，效果计数经审计证实。
