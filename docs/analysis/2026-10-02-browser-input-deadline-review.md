# Browser输入传输期限竞态修复

## 实际缺口与结果

CU恢复尚未完成时继续独立工程审查，发现Web输入传输仅在等待线程检查超时。等待线程到期却尚未获调度执行Guard清理时，Pending仍在：宿主领取入口可能交付过期请求，回执入口也可能接受已过本请求期限的Released回执并返回204。这个问题不是063 DH事故的根因结论，也不是旧Codex会话独占CU的证据。

在`Pending`加入期限字段、提取现有回执结算代码以便测试，但尚未加入期限拦截时，两项回归测试确定性失败：过期请求仍被领取；过期释放回执得到Ok(204)，预期Err(409)。测试直接构造“已到期、等待线程尚未清理”的状态，不依赖sleep或操作真实鼠标。

修复在`modules/gui-web/packages/web-console/src/native_browser_input.rs`：

- 发布请求时记录与等待线程一致的单调时钟deadline，限制仍是remaining与8秒的较小值；计时包含取得Pending互斥锁的等待。
- 宿主领取、回执结算均在原Pending互斥锁内检查该期限。到期不能领取；迟到回执返回409，留待原Guard按request_id清理，不追认成功或替新请求结账。
- 认证、回执格式、host/request/resource、ticket/attempt/executor绑定检查仍走原生产路径；提取的settle_reply为私有函数。
- 已领取动作在期限内仍可按原资源提交真实释放回执，关闭或navigation_revision变化不能强行改成NotDispatched。宿主3秒释放确认、ReleaseUnknown隔离、禁止重放及数据库保持原样。

## 实际验证

| 命令/阶段 | 结果 |
| --- | --- |
| 加入回归测试、入口期限拦截前：`cargo test -p coolzhu-web-console --offline native_browser_input::tests::expired_ -- --test-threads=1` | exit 101，2失败，复现上述领取/回执缺口 |
| 修复后：`cargo build -p coolzhu-web-console --offline` | exit 0，29.63秒，编译存在既有warning |
| 修复后：`cargo test -p coolzhu-web-console --offline native_browser_input::tests -- --test-threads=1` | exit 0，6通过/0失败/1303过滤；lib 8及宿主1项被过滤，未计作通过 |
| `git diff --check` | exit 0；既有inventory文档CRLF提示 |

4项新增行为用例：过期请求不交付；过期释放回执不追认；已领取后关闭/导航仍按原资源结账且重复回执拒绝；旧request_id、旧navigation_revision、错误attempt或executor均不能吞掉替代请求，当前正确回执仍可结算。既有2项覆盖资源撤销与回执类型约束。

验证只涉及进程内传输状态。未覆盖真实WebView按下至释放期间关闭、GUI取消竞态、真实导航或Qwen连续输入；未跑全套测试，没有重新计入上一阶段Shell5项作为本轮新通过项。没有安装、启动新版本或更新MSI；正式安装仍保留原版本和隔离。新增deadline仅收紧传输边界，不是恢复安全隔离的入口。

## 版本、交付与下一验收点

基线分支`codex/cu-preinput-followup-20260930`，HEAD `8181f08ae9c50d3e41aabd32a6af85343dcd92e8`；本轮源码修改未提交，workspace版本0.2.0。正式0.2.63与未安装0.2.64候选不包含此次改动。只有Web输入传输生产文件变更，没有修改桌面代码、安全配置或原事故记录。

[工程证据索引](../testing/browser-input-deadline-2026-10-02/evidence-index.json)包含失败/通过日志、构建日志、变更补丁与源文件哈希。[CU端点及旧会话占用诊断](2026-10-02-browser-use-cu-availability-review.md)独立保留：尚未证实旧会话独占，当前无本地CU工具入口。

下一实际GUI验收点仍是本地CU只读枚举Windows窗口、核对原事故；随后按既定恢复流程和逐项授权处理原执行者及隔离，再在包含新代码的候选上验证真实关闭/取消与释放时序。不能用本轮进程内测试替代这些观察。
