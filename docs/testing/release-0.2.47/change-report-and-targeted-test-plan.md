# 0.2.47 安装、真实 Browser Use 回归与后续测试依据

2026-10-01，主会话独立实施与测试。保持真实 qwen3.8-flash、原百炼地址、既有密钥及 medium，不使用模型响应夹具。测试页为普通静态 HTML；测试者只发送任务，不代模型填字段或导航。

## 安装身份和工程结果

源码 f033a409f0a8560ea130ee0a12e07c429aaa9991。正常发布六门通过，859 个载荷文件及正式安装目录 10 项关键产物摘要匹配，系统唯一登记 0.2.47；正式入口启动，8765 由已核验 Web 进程提供。MSI SHA256 为 20522fcb336ee9b6060963a64c3b16acf1ed0425cb11c14359d5c5d9ca608509。源快照 be577df8a54a9cfeb3fe39507cecdeace163f4512d50e0f18fb272abfaf1a780，报告 pkg-report-release-20261001-115707046-4dd4827e。原件见 evidence/build-identity，安装核验见 installed-native/installed-artifacts.json。

该源码离线 Web build、1291 项 Web 检查（0 失败、2 既有忽略）、126 项模型适配器检查和 8 项模块连接检查通过。远端 f033a40 的 push 36812597375、pull_request 36812601113 均 success。工程结果不等于实操通过。

## 实操事实

| 轮次 | 父运行 / CU / 页面结果 | 结论 |
| --- | --- | --- |
| TEXT-AS | completed；初始验收 JSON 正常，met=false；规划返回 done=true，零动作、verification_failed；字段空 | 格式故障此轮未复现，文字输入未通过 |
| NAV-AT | completed；模型把 next.html 目的地址填进 target.url，实际源页面为 text.html；观察匹配检查拒绝，零动作 | 工具参数说明存在源与目的地址歧义；不是页面断开证据 |
| NAV-AU | 父 failed；零 CU 登记、零内部请求；模型正文输出调用 JSON 并声称本轮无工具；地址仍为 text.html | 没有真实派发，不据此推断执行接口或权限失败 |
| TEXT-AV | completed；真实工具派发、验收与规划 JSON 可解析；规划 done=true，零动作、verification_failed；字段空 | 简化自然任务后仍停步；输入未通过 |
| NAV-AW | completed；明确 target.url 为源页面后，观察/验收正常；规划 done=true，零动作、verification_failed；没有实际导航 | 源地址歧义不能解释全部停步；导航未通过 |

父运行、provider 响应标识、账本终态和步骤见 installed-native/BU047-*-facts.json，终态原图见 04、06、08、09、10。AS/AV/AW 没有保存原始规划自由文本；不能事后猜测模型停步理由。没有新增安全隔离审批要求，也没有借历史导航、点击、滚动图计算本版通过。

## 下一轮最小修正及审查

1. 工具定义、target.url 参数说明及模型操作约束统一说明它是当前源页面。目的地址由 objective 表达，规划动作再放入 arguments.url；错误源页面仍输入前拒绝，不静默替换模型参数。
2. 047 曾对纯文本规划的根 oneOf 动作联合也启用 JSON Schema；已有审查文档把它笼统描述为“无 schema 规划使用 JSON Object”，不准确。下一轮明确固定验收对象使用 Schema，动作规划使用 JSON Object，并保留提示词中的动作 schema、严格解析、最新节点接地、许可和独立验收。官方资料未明确确认根 oneOf 的兼容性，连续停步也不足以证明供应商不支持该关键字，因此只作协议对照，不把推断写成已确认根因。
3. 内部脱敏诊断增加节点/可操作引用/文本框/根文档数量及布尔能力，保留已知 target_not_found 停步码；不保存正文、控件名、字段值、网址、节点令牌或原始思考，不加入正式前端调试栏。

## 针对性验收

- 原空字段：由真实模型先 click 再 TextInput，截图显示本轮独有回显，账本两动作及派发/释放、独立目标验证齐全。
- 源 A 到目的 B：实际一次 Navigate；动作回执绑定 A，目的地回执及新观察证明 B 的 URL/正文/环境。不能用文字调用 JSON 或旧图替代。
- 诊断与隐私：节点计数应和本次观察一致；不含控件名、原值、URL、凭据。规划停止仍 goal=false，不自动重试或重放。
- 浏览器基础与边界：实际点击、滚动、只读零动作、取消和资源变化拒绝；不同版本的通过保持各自证据范围。
- Browser Use 闭环后：真实 Paint 闭合轮廓、简笔海绵宝宝和全桌面使用电脑提示联合验收，原图、动作投递与释放、独立视觉判定齐备。

文字输入、当前版本导航、Paint、DSH 远程插件实际安装运行及四项总体审核仍开放，PR74 保持 Draft。微信不改不测，Devin 暂缓，Pro 总体复审按用户要求暂停。047 不包含上述下一轮修正。
