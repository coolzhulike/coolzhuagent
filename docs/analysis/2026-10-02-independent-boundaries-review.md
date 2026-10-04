# 无需人工恢复的独立工程边界复核

本轮修复启动演出在绘制/清理异常时无法及时完成交接的问题，并补上DSH真实进程执行期间的父预算截止测试。没有解除输入隔离、安装候选、调用模型或重启正式实例。

分支：`codex/cu-preinput-followup-20260930`；参考HEAD：`8181f08ae9c50d3e41aabd32a6af85343dcd92e8`。结果对应现有未提交工作树，五个本轮文件的SHA256见[证据索引](../testing/independent-boundaries-2026-10-02/evidence-index.json)。原有修改和历史记录保留。

## 本轮实现与验证

启动页原先的动画帧异常会逃出requestAnimationFrame回调，直到11秒看门狗才尝试退出；退出时Canvas清理再次抛错，又会打断隐藏页面和完成上报。新增回归先在原代码复现这两个故障，退出码1，日志保留。

现在普通帧失败立即走`resource-error`，减少动态模式的同步绘制异常也停止播放器。Canvas清理错误记录诊断后继续隐藏演出并完成通知。没有改变正常首次7.8秒、日常6.5秒、减少动态120毫秒或原生15秒兜底，也没有把演出结束当作后端ready。

Node生命周期脚本通过：原有完整/日常/Esc/减少动态/资源超时/恢复/创建失败/宿主上报失败场景，以及新增的普通帧绘制异常、退出清理异常、减少动态绘制异常。新场景检查播放器停止、定时器和帧回调清空、页面隐藏、错误可诊断、完成通知只有一次。该脚本使用受控Canvas，不证明画面真实显示。

DSH新增独立Windows集成测试`parent_deadline_stops_dispatched_node_and_rejects_delayed_result`：先通过生产校验器核验完整固定运行资源；真实Node夹具收到生产桥的execute消息后故意忽略取消，准备4秒后写入结果。生产桥沿用2秒父预算，不能用30秒局部时限续期；本次约3034毫秒返回`host_interrupted/timed_out`，`cleanup_confirmed=true`，观察至5秒未出现迟到写入。1通过、0失败、0忽略，执行6.50秒。

该测试平时显式忽略，必须提供`DSH_DEADLINE_RUNTIME`并用`--ignored`运行；本轮实际运行，没有把忽略算通过。范围是固定Node、生产Rust桥、真实进程回收与受控协议夹具；不冒充官方SDK执行中截止、真实模型或GUI验收。DSH生产代码本轮未修改。

离线编译：`coolzhu-tauri-shell`通过，32.86秒；`coolzhu-core-runtime`通过，0.35秒。已有未使用代码警告保留。`git diff --check`退出0。没有再次执行此前已经通过、且本轮未修改的Web全套或Browser聚合检查。

可复跑命令（PowerShell，仓库根目录）：

```powershell
& .\tmp\2026-10-02-browser-msi\fixed-runtime-source\node\node.exe modules/gui-desktop/packages/tauri-shell/ui/tests/coolzhu-seven-letter-bridge.cjs
$env:DSH_DEADLINE_RUNTIME=(Resolve-Path tmp/2026-10-02-browser-msi/fixed-runtime-source).Path
cargo test -p coolzhu-core-runtime --offline --test dsh_deadline -- --ignored --nocapture
cargo build --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml -p coolzhu-tauri-shell --offline
cargo build -p coolzhu-core-runtime --offline
```

## 有限核对后的剩余边界

历史材料仍无法唯一还原“原五项”的编号；以下按已知工作面列事实，不计算五项完成率。

| 工作面 | 已有及本轮工程证据 | 尚未验收/未跑 |
| --- | --- | --- |
| Browser use | 0.2.65包含请求单调截止修复；此前四条回归及SQLite取消/过期/绑定/claim后停止/释放终态检查通过，本轮未重跑 | 真正WebView输入及释放期间Close/Cancel/navigation竞态；8秒释放后才关闭的旧记录不能算覆盖 |
| Paint computer use | 原隔离和人工复核记录保留 | 完整绘画、真实工具执行及结果验收；需要恢复本地CU和满足原安全前置条件 |
| DSH | 来源解析、默认停用安装、按钮/API、启用快照和聊天派发已有工程报告；本轮新增运行中父预算进程回收测试 | 安装后真实UI、Qwen调用；DSH专属提交后故障/重启恢复实操未新增，不能用通用事务测试冒充；Goal入口仍维持缺少完整接纳身份时拒绝 |
| 启动演出 | 本轮修复并验证绘制和清理异常交接 | 连续原生画面、动作效果、实际退出后主控制台可用性 |
| 泛光及总体审核 | 有限检查现有物理屏幕布局、主屏标记、租约撤销、静态边光CSS；未发现本轮可证实的新纯逻辑缺陷，未改安全边界 | 真正多屏/DPI、点击穿透、生命周期显隐；总体验收仍未闭环 |

当前工具目录没有可控制本机Windows的CU/node_repl入口。此前CU启动诊断见[报告](2026-10-02-browser-use-cu-availability-review.md)；本轮没有重复重连排查、使用私有协议或绕过隔离。以上有限复核不代表所有潜在工程故障都已穷尽。

## 正式实例与候选边界

只读API再次确认：`resource_state=isolated`、`accepts_new_input=false`、未确认阻断1项，仍为`native-panel-fd88ef6ac7802d80f61b9c375dbad320`；待恢复0，历史人工复核操作5。没有签署或修改复核。正式Web PID2572和Shell PID3864及创建时间均未变。

正式两份exe、原事故三份证据、0.2.63/0.2.64交付文件和Codex配置逐项SHA256复核未变。0.2.65 MSI仍为275647003字节、SHA256 `fa80c6faf13383f926f03a4617b5a406918b9b602e2fbf08dbd2f7e89c35eb19`，未安装。

**本轮动画修复在0.2.65冻结之后，不在该候选内。** 没有修改其冻结清单或把新测试追记成旧包已通过；本轮不生成0.2.66。下一次需要交付这些动画修复时，应重新冻结、构建并校验新候选。真实GUI验收前仍须先恢复正确的本地CU，并按原有安全流程满足输入许可条件；安装和安全恢复均未在本轮执行。
