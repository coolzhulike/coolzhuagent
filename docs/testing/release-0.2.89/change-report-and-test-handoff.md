# 0.2.89 改动报告与针对性测试交接

本版修复内置原生浏览器不填写URL时无法操作当前已加载页面的问题，并补齐“右栏已加载原生网页”等名称的路由识别。主会话独立实施和测试，使用原SWE-2-medium与唯一island-kayak云端会话；Paint不再测试，微信不改不测，Opus暂停。

## 实现及安装身份

冻结产品源码`5e5454f869a39905f3ab2ee47ab06e0e9135ed09`，源码快照`a38e27eb9514088f5216068633038f767306553fff6bb994034bba9c96beccc0`。正常完整构建六门通过、Windows安装返回0、1150个安装文件长度与SHA逐一匹配。MSI 276347522字节，SHA256 `998710a59a3bad0d19f2dfd9982bd98bf8ce232f747411c85f1110204e0dd833`；桌面安装包`C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.89.msi`。完整身份见[安装核验](installed-current-page/installed-089-verification.json)与[evidence中的构建收据](evidence/build-identity/)。

- `native_browser_adapter.rs`在每个执行实例第一次取得已加载认证宿主事实时，保存URL、宿主、工程/聊天室/面板资源、代次、导航版本和文档token；后续观察不覆盖初始来源。
- `native_browser_verification.rs`仅在请求URL缺省时使用该来源，明确URL仍严格匹配；已结算输入或导航沿既有来源链验收。无来源标记的空URL不能匹配任意页面。
- `computer_use_turn_scope.rs`补齐明确指向右栏原生网页的表述，设置、统计、Paint和Chrome任务仍不误分类。

没有增加权限、绕过节点/文档检查、清除安全记录或增加正式前端调试信息。设计及风险见[当前页方案](../../analysis/2026-09-21-integration-review/native-browser-current-page-plan.md)。实际offline build通过，完整主控制台1397通过、0失败、6忽略；必要页面验收8项及路由8项通过。自动化回归不替代真实截图。

## 正式安装版四项真实验收

正常Program Files安装文件启动，无调试浏览器参数；通过聊天室普通输入发送独立请求，room-1791131523339、Agent session-1791131217833保持。每轮一个ACP attempt、end_turn、process_drained=1，绑定锁释放，内部lane远端为空。

| 用例 | 实际结果 | 窗口原图 |
| --- | --- | --- |
| 当前页点击 | 省略整个target；真实按下/释放/点击各一次，计数0→1，succeeded、1/1 | [点击结果](installed-current-page/installed089-click-after.jpg) |
| 当前页独立子滚动 | 省略整个target；跨来源LTR子RootWebArea right一次，位置0→195.3333282470703，子可信wheel1、父位置/滚轮0/0，succeeded、3/3 | [滚动结果](installed-current-page/installed089-scroll-after.jpg) |
| 明确错误URL | different.html与当前click.html不匹配；blocked、完成动作0、网页事件0，计数保持1；外层run如实failed | [错误地址](installed-current-page/installed089-wrong-url-after.jpg) |
| 规划期间刷新 | 省略整个target；在真实规划窗口通过普通刷新按钮替换文档，document_changed、not_sent/not_needed、完成动作0、网页事件0，计数0；外层run如实failed | [旧引用拒绝](installed-current-page/installed089-stale-after.jpg) |

两项正向的requested_url均null、observation_origin为current_page，bound_initial_url来自本轮宿主事实，不是模型补URL。负例按预期拒绝通过，不写成工具succeeded。

严格时序为规划请求`1791364075176` < 刷新开始`1791364076120` ≤ 刷新完成`1791364076215` < 规划回复`1791364078998`，见[原始时间核验](installed-current-page/stale-timing-analysis.json)。只读观察器等待真实请求出现后执行一次普通UI刷新，没有延迟或伪造模型响应。刷新返回不等于文档变化本身，节点预检的真实document_changed与当前新页面计数0共同提供证据。

[完整清单](installed-current-page/manifest.json)含请求、实际参数、终态回执、可信网页事件、云端绑定和安装身份。候选六轮含原路由失败及两轮未命中规划窗口的记录继续保留于[候选报告](../2026-10-07-native-browser-current-page/change-report.md)，不能用本次结果追改旧失败。

## 恢复、远端检查和剩余边界

正常桌面入口已恢复原工程`C:/Users/zhupu/coolzhuagent`与原输入安全库，启动自检和实际Program Files进程一致，见[恢复核验](startup-recovery/restored-daily-089-verification.json)与[恢复窗口](startup-recovery/restored-daily089.jpg)。日常窗口显示原Qwen仅为恢复原选择，本轮真实测试全部SWE，没有另发Qwen请求。

安全资源safe、accepts_new_input=1；历史2个outcome_unknown许可与9个closed记录保留，见[只读安全快照](installed-current-page/installed-safety-summary.json)。冻结产品5e545两条远端检查success，PR84仍OPEN/MERGEABLE，没有自动合入；后续文档提交的CI另核。

本版闭环无URL当前页基础入口与严格规划期间刷新旧引用拒绝，不代表全部HTML或四项总体完成。复杂旋转/变换、严格按住跨URL、按下中关闭/面板替换、插件配置/取消超时及许可窄竞争、Goal/Relay附件、启动演出其它模式和多屏等见[总队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)。自动升级完整链按既有决定延期。

## 后续模型设计测试用例

从正常聊天室发送新请求，缺省URL用例整个target省略、surface=browser、max_actions=1；先确认已加载页面和初始计数。正向须核验实际投递/释放、可信事件和新鲜页面原文；错误URL必须保留明确地址，不允许模型换目标或导航凑匹配。文档变化负例同时保存规划请求/回复与UI变化区间，零输入、计数不变且无补发；晚于回复的变化不能计为规划中通过。切换宿主/工程/聊天室/面板代次和无初始来源仍需各自证据，不能由本次刷新外推。


## 公开预发布核验

[0.2.89预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.89)已公开。四项分发资产的服务端长度及SHA256与本地完全一致，实际标签绑定冻结源码5e5454f869a39905f3ab2ee47ab06e0e9135ed09，见[服务端核验](evidence/github-release-verification.json)。此为预发布，不代表四项任务总体完成或PR已合入。
