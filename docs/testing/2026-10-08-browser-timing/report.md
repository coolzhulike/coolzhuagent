# Browser 输入时序观测候选

本轮只补观测能力，不改变输入/释放/导航授权协议。正式101仍运行；本候选尚未完成实机验收，不在101安装包内。设计及实际模型审查取舍见[方案](../../analysis/2026-09-21-integration-review/browser-input-timing-observation-design-2026-10-08.md)。

改动限桌面壳三文件。复用原 `COOLZHU_BROWSER_NAV_DIAGNOSTICS` 默认关闭开关、日志通道；共享进程内单调时钟及采样序号。generation 变更在状态锁内采前后区间、锁外写日志；create_state 只代表状态世代分配，另一个 view_created_observed 才代表 add_child 已返回成功。SourceChanged 只记录实际 IsNewDocument 布尔，不作为精确 commit。

click 开关开启时仅采有界内存记录，至多12个采样，结算后一次写日志；记录原 attempt/generation、down/up 入队前后与结果、ACK及结算。默认关闭不创建记录；无坐标、URL、query、scope、正文或凭据。上限/锁异常可能使记录不全，不能把日志缺失当作事件未发生。结果返回仍由既有双 ACK/3秒等待决定；晚到或重复 ACK 不修改结算结果，不补发。向原 controller 尝试 up 的顺序不变。

已执行桌面壳 offline build 及现有76项检查，全部通过。初次从根 workspace 直接 `-p coolzhu-tauri-shell` 失败，因为桌面壳有独立 manifest；修正为显式 `--manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml` 后通过。原失败[日志](initial-build-command-failure.log)、最终[编译](build.log)、[检查](tests.log)均保留。不将这些检查当作真实时序验收。

真实单次跨来源 Browser 任务、独立网页记录器及原唯一 SWE/island 会话的提交脚本已在 tmp 准备，未执行模型请求。只读检查确认原验收与日常库均无活动运行，原远端未锁，internal 无绑定。

随后自动审批拒绝“停止正式桌面壳并启动诊断候选壳”的组合操作，返回 `blocked by policy`，未给出具体原因。操作未执行；没有重试等价命令或更换接口绕过拒绝，正式101及原安全历史保持。之后通过正常 Computer Use 重新枚举并捕获 Program Files 正式壳窗口，确认原轨迹与聊天仍可见：[当前正式版实拍](formal-101-preserved.jpg)。此阻塞只影响候选原生实操，归档、编译、代码审查和其它可执行验收继续。

严格新 Target 替换、跨来源 commit 恰在 down/up 之间仍开放。后续实操需分别核宿主采样与网页真实事件；只有有充分时序证据才关闭对应子项。采样序号不是跨线程因果，ACK不是 DOM 事件；不同进程/页面时钟不得未经校准混比较，导航回调不冒充 commit。诊断的内存/日志开销也应计入观测条件，不能宣称风险为零。
