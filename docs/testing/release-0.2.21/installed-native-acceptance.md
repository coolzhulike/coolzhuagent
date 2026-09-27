# 0.2.21 安装版原生功能与运行后数据验收

日期：2026-09-27。对象为已正常升级并由系统登记为 `0.2.21` 的安装版；安装身份、静止数据见[安装与数据报告](installed-upgrade-and-static-data.md)。本轮操作均在该安装版桌面窗口完成，取图区域为 1443×897。测试房名称中的 `019` 是早期建立的受控房名，**不是当前程序版本**。未改产品源码、全局 FullAccess、用户安全策略或数据库。

## Rust 语言服务

在应用的工程下拉中切换到已有受控 Rust 工程 A，使用原有合法授权的 `019 原生验收` 房，打开已保存的 `src/main.rs` 并点击“启动 Rust 服务”。021 不再出现 020 正常入口的 `expected_workspace` 409；界面显示服务已启动，真实 rust-analyzer 返回 `6:9 Error · expected pattern, found '='`。点击诊断后光标到 `6:9`。[启动和诊断](installed-native/01-lsp-started-diagnostic.png)、[诊断定位](installed-native/02-lsp-diagnostic-click-6-9.png)。

在 `answer()` 调用处请求定义，结果为 `src/main.rs:1:4`，点击结果后光标到 `1:4`；请求引用得到 `src/main.rs:4:17` 和 `src/main.rs:1:4`，点击调用引用后光标到 `4:17`。[定义结果](installed-native/03-lsp-definition-result.png)、[定义定位](installed-native/04-lsp-definition-click-1-4.png)、[引用结果](installed-native/05-lsp-references-result.png)、[引用定位](installed-native/06-lsp-reference-click-4-17.png)。

点击语言服务自己的“关闭”后，界面返回“Rust 语言服务已配置，需手动启动”且结果清空。[停止界面](installed-native/07-lsp-stopped.png)。另一次在 A 启动后通过正常工程下拉切至受控工程 B，B 显示自己的目录与文件；返回 A 后需要手动启动，旧 A 面板没有被复用。[切到 B](installed-native/08-lsp-workspace-b-after-a-running.png)、[返回 A](installed-native/09-lsp-return-a-requires-manual-start.png)。

进程证据与界面证据分开核对：A→B→A 后 `rust-analyzer.exe` 数量为 0；在 A 重新启动后数量为 1，点击语言服务“关闭”后又为 0。三个私有时间点收据位于 `tmp/candidate-0.2.21/lsp-after-workspace-switch-process.json`、`lsp-running-process.json`、`lsp-stopped-process.json`。这证明本轮停止和切工程后的进程退出；未通过 UI 构造旧句柄、撤权或“旧关闭误停 B 新实例”的负例，不能把未测子情形写成已通过。

## 已有视频的预览

切回原用户工程，在既有受控测试房 `验收-20260927` 中点击早期版本留下的 `预览验收.mp4` 消息附件。右栏打开内容预览，点击播放器后画面正常渲染，播放进度走到 `0:06/0:06`；关闭右栏后回到测试房，输入框无草稿。[预览打开](installed-native/10-existing-video-preview-open.jpg)、[播放画面](installed-native/11-existing-video-playing.jpg)、[播完](installed-native/12-existing-video-ended.jpg)、[关闭且无草稿](installed-native/13-video-closed-no-draft.jpg)。这是 **021 播放已有附件**；本轮没有重新上传视频，不能据此宣称原生文件选择或新上传通过。此前 020 的文件选择框被操作工具的窗口归属检查拒绝，未作为产品播放失败。

## 首次启动后及实操完成后的数据

安装前和安装后首次启动前，两份静止快照已核对为 66 文件、41 附件（49,153,852 字节）、6 个 SQLite 库，文件与白名单数据库语义摘要相同。完成上述原生操作后，用相同的 SQLite 在线备份方式另取最终快照；仍是 66 文件、41 附件（字节数相同）、6 库，**无文件增加或删除，41 个附件逐项哈希未变，配置摘要未变**。对比收据留在忽略目录 `tmp/candidate-0.2.21/upgrade-20260927-candidate-021/post-runtime-final/` 与 `tmp/candidate-0.2.21/runtime-diff-summary.json`，原始私聊、附件、密钥不入公开证据。

运行后有三份文件内容变化，均已按字段核对：`web-sessions.json` 及对应 SQLite `metadata` 仅改变 `active_chat_room_id`，与最终停留在测试房相符；`memory_access` 表仍为 85 行，只有 2 行的 `last_accessed_at`、`access_count` 变化；输入安全库新增 1 条事件（88→89），归属 epoch 的一行发生进程接管/释放字段变化。其余 workspace SQLite 表的带密钥逻辑摘要相同，未见消息、附件引用或其他行数改写。输入安全状态仍显示 2 条历史归属缺口、2 条待本人复核、2 条待收敛遗留，当前输入隔离且资源状态未知；本轮没有人工放行，也未清除事故记录。

本轮确认了 021 的正常 Rust 服务 UI 链、已有视频播放与静止/运行后数据保留。021 画面左上仍为旧字标、外围并非用户最终指定的竹林原稿，故这里的受控窗口截图只证明上述功能，不作为新视觉最终通过。显式 text Custom 模型误入图片生成及失败后 run 被标完成的另一个已复现缺陷，按[专项补审](../../analysis/2026-09-27-explicit-model-routing-review.md)继续处理；本次原生 LSP 与视频成功不覆盖该缺陷。原用户工程启动图已转入忽略的 `tmp/candidate-0.2.21/`，不作为公开证据。
