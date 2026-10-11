# 0.2.100 改动与验收交接

本版修补跨客户端聊天记录自动同步，冻结源码 `2816e3e8f825c4a5ffc8f8dea3fd306761aa345b`。不宣称整体整合计划已完成；ACP统计遗漏修补另行实施，不包含在本安装包。

## 行为与职责

原页面在外部客户端提交并完成新轮后，仍停在旧历史，需要手动刷新。现在通过SQLite同事务房间版本和SSE失效通知读取已提交历史。分页、持久消息和运行状态沿用原有模块，通知不包含正文、不作为第二套消息账本。跨项目/房间切换及迟到响应由世代检查隔离，忙时延后，流式结束及重新可见时唤醒；回看历史保留位置，不强行跳到末尾。

`chat_history_sync.rs`负责版本投影/作用域与通知；独立JS协调请求、分页覆盖及重连；app合并实际DOM并保留锚点。删除后重新创建房间版本仍递增，no-op更新不发新版本，回滚不泄漏通知。初次分页查询参数解析400已修复并加入真实URI解析检查。

可据此设计测试：外部提交与回复、本页流式结束、忙时多通知合并、切房/切工程迟到响应、历史锚点删除、200条以上分页覆盖、后台断开期间新增/删除后重连。下述正式实拍仅证明已实际执行的部分。

## 构建安装与实操

- 正常完整release构建、六门均pass，源码快照构建期间未变化。源快照 `a0ebb5a9742e03bc9d1a48c6ba6b8ebfdab2404e69a53e0f3885ba0c262f36df`，payload `f9905c98e3945b5009e4a19a51b1093d8ab24bfa30a1be4a8eb2c65122d6f9ce`。
- MSI 285814280字节，SHA256 `84c8e87728462b26c2180f54997e18b7fee689a22c61e3eb1228159de5bd4641`，未签名。正常管理员安装返回0，1159安装文件逐长度/SHA一致，CLI版本100/源码2816e3e，Program Files正式后台和原生壳启动。见[安装核验](installed-validation/installed-100-verification.json)。
- SQLite版本/事务、JS协调器五项专项和数字查询三项专项通过，offline build通过；此前完整1413/0/6忽略在数字解析修正前执行，不将其冒称为最新源码全量回归。首次编译和临时资源失败保留在[候选报告](../2026-10-08-chat-history-sync/report.md)。
- 正式100原页面未经CtrlR自动出现外部提交的任务#679与完整SWE回复#680：[新消息实拍](installed-validation/external-message-auto.jpg)、[完整回复实拍](installed-validation/external-reply-auto.jpg)。真实文本附件审查和单次DSH计算器完成，模型回复表达式539+881-539的值881，与开始前只读SQL交叉一致；瞬时计算器响应体未独立持久化，不把模型所述executed=true冒充原始审计字段。原始审计status=ok/allow-auto，整轮completed/end_turn/drained，原唯一island-kayak解锁、internal NULL。见[实际台账和审计](installed-validation/integration-result.json)。

审查建议由主会话核源码后取舍：不按call/turn猜配不同attempt，不在读取层把prepared改unknown，不凭时间推断派发事实。真实旧统计539条全与ACP前缀记录重复，但ACP已有881条；该发现不改变100包含功能范围。

## 仍开放

正式100已关闭外部新消息和回复自动出现缺口。本页真实流式忙时延后、断线期间大批新增/删除、完整跨工程并发矩阵仍需独立验收。候选已拍的历史位置保持与空闲重启不能冒充正式完整矩阵。

ACP统计与轨迹目前遗漏独立台账中的部分新请求，正在修补查询归并与未知状态表达；未知token仍为未知。Browser新原生Target替换和跨来源commit严格down/up、其它工作包保持[总体队列](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)状态。Paint免测、微信不动、Opus暂停、不使用子代理，Goal继续。

[0.2.100公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.100)已交付四份资产，服务器长度/SHA与本地一致，标签指向实际构建提交2816e3e，两路CI均success。release未标latest、未签名；不包含后续统计候选修补。见[服务器原始元数据](installed-validation/github-published-metadata.json)。
