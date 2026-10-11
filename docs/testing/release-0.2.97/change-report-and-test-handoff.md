# 0.2.97 改动与针对性验收交接

## 本版改变

修复上传文本只有文件链接、模型读不到正文的问题。独立 `attachment_text` 模块在发送时从受控上传目录读取文本，保存原字节摘要、编码与正文快照；历史会话重用已保存快照，不重新读取已变化的文件。前端提交的同名快照不被信任。聊天正文与附件资料分开保存，附件内容不扩大 Computer Use 的原始授权范围。

支持 UTF-8（含 BOM）及带 BOM 的 UTF-16LE/BE。单文件上限256 KiB、最多8个文本附件、解码正文总量512 KiB；无效编码、二进制控制字节、非受控路径在模型派发前明确拒绝。当前轮上下文预算不足以容纳完整附件时拒绝截断发送。图片沿用原入口；Devin 的 PDF/音视频尚未接入，明确拒绝，不冒称多模态全覆盖。

修复右栏文件预览固定按 UTF-8 解码导致 UTF-16 乱码的问题，按 BOM 识别编码，保留原2 MiB读取限额和下载原文件行为。同步模型配置中已过时的 Devin 附件、计费及工具能力提示。

## 构建与安装

- 冻结提交 `0b4fddc4a717e699fe5d493b7d55542beb7b5ae3`，dirty=false；源码快照 `46b3cae738d321d88df0c2814eb375571edf7fe0c8cdb3aebd3710e29bfce1d1`。正常完整 release 构建，六项发布门禁全部 pass。
- MSI 285777415字节，SHA256 `1f1dda0c06a7c5f0d6015375dd36a56e35d4a004dbc0602b3e6a019c4f9eff27`。payload1159文件、432523192字节。未签名。
- 正常管理员安装返回0；Program Files全部1159文件逐长度/SHA一致，CLI0.2.97、Git SHA与冻结提交一致。正式安装二进制配套启动，没有静态资源覆盖或测试版浏览器参数。
- [构建摘要](installed-validation/package-097-verification.json)、[实际安装核验](installed-validation/installed-097-verification.json)、[安装收据](installed-validation/install-097-result.json)、[进程身份](installed-validation/installed-097-standard-processes.json)。构建摘要的“尚未安装”是出包阶段时间点，后续安装收据独立证明安装完成。
- [生产者构建报告](evidence/build-identity/pkg-report-release-20261008-112345312-a366adae/package-report.json)、[payload清单](evidence/build-identity/pkg-report-release-20261008-112345312-a366adae/payload-inventory.json)。

## 正式真实模型综合验收

继续原 SWE-2-medium、原聊天室、唯一 `island-kayak` 绑定，不新建 Devin 云会话。marker `TEXT-INSTALLED097-SKILL-PLUGIN-BROWSER-20261008`，run `run-chat-79f889a4979ab152c7c38fcaa17001480d10ab68052cb891`。

1. 通过正常上传接口提交新 UTF-8 BOM CSV 与 UTF-16BE BOM 备注，不复用候选轮答案。CSV竹林订单47×13和22×8；山外99×100明确排除。备注代号 `BAMBOO-INSTALLED097-86EB009E`。请求故意携带伪造快照9999/FORGED，持久化内容已核实为服务器读取的真实正文及SHA，伪造值未进入快照。
2. 选中“插件回执核验”SKILL；SWE实际读取附件并仅调用一次真实DSH calculator，算式 `47*13+22*8`，工具台账completed、宿主审计ok，结果787。
3. 单次 `computer_use_perform`、七个真实原生浏览器动作：填代号与金额→第一次提交被业务拒绝→根据新反馈勾复核→重新提交。可信网页事件仅两次，false→true，金额均787；最终实际页面“结算完成 BAMBOO-INSTALLED097-86EB009E / 787 / 已复核”。五次点击均released、两次文本输入not_needed，全部effect_observed，CU succeeded/goal_achieved=true。
4. 父run completed，单attempt `end_turn`、process_drained=1，原唯一远端解锁，internal远端仍空。正式聊天页正常刷新后显示附件、回复与6分4秒耗时。模型未要求的脚本、文件、其它工具及外部浏览器均未调用。

[结构化事实](installed-validation/result.json)、[真实页面实拍](installed-validation/01-browser-completed.jpg)、[持久化聊天回复](installed-validation/02-chat-persisted-reply.jpg)、[正式UTF-16BE侧栏预览](installed-validation/03-utf16be-preview.jpg)。页面是受控业务验证页面，模型与插件均真实调用，不使用模型响应夹具。工具瞬时完整响应未另行持久化，结合工具台账、宿主审计、模型转述与可信页面事件交叉验证，不伪造原始回执。

## 负例与回归

外部文本URL、含NUL二进制文本、256 KiB+1文件、Devin PDF四项均HTTP400；前后attempt927→927、消息1136→1136，模型零派发。[负例记录](installed-validation/negative-result.json)。API上传成功不替代GUI文件选择器通过：候选轮选择器被Computer Use目标窗口保护拒绝，按唯一重试规则停止，仍保留该缺口。

本地完整控制台1407通过/0失败/6既有忽略，lib8、native host1、workspace linkage8及tool-registry检查通过，最终offline build与前端语法检查通过。三项新增边界测试覆盖真实快照、编码/限额/路径及历史与预算，未以其替代真实模型验收。构建提交的[push检查](installed-validation/ci-push-0b4fddc.json)与[PR检查](installed-validation/ci-pr-0b4fddc.json)均success。

已公开[0.2.97 GitHub预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.97)，四份分发资产的服务器端长度/SHA256均与本地一致，标签指向实际构建提交。见[发布metadata](installed-validation/github-published-metadata.json)、[标签](installed-validation/github-tag.json)和[本地摘要](installed-validation/release-assets-local.json)。预发布不代表整体验收完成。

候选轮乱码失败截图与修正后实拍独立保留在[候选报告](../2026-10-08-text-attachments/report.md)，不追认旧安装包包含修复。

## 后续针对性测试

优先验证按下期间新nativeTarget替换与跨来源commit窄时序，不能将正常关闭/右栏设置切换当作新Target证明。会话附件仍缺账号过期/rebind、默认视觉Agnes路由、图片预算及群发、PDF/音视频和Goal/Relay入口。UI高DPI与动态审批、启动资源失败/减弱动作、调度恢复和其余架构工作包按[总体快照](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)继续，整体Goal未完成。Paint免测、微信不动、Opus暂停保持。
