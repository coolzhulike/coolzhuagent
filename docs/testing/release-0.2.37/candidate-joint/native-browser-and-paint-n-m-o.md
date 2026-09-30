# 037 候选真实 Browser Use 与 Paint 回归

2026-09-30；主会话独立执行，不使用子代理。原 qwen3.8-flash / Base URL / API_KEY / medium 不变，无模型夹具、外部抓取或人工代画。正式安装仍034；用户手动启动037候选后，运行时 `d428c2307556 · 2026-09-30`，Web PID18960、Tauri PID26096，原工作区/数据库/schema27、原验收聊天室与会话均不变；关键包产物10/10匹配。原启动自动审批拒绝已由用户正常启动处理，未绕过拒绝。

## 真实结果与因果身份

| 轮次 | 根运行 | 工具结果 | 验收结论 |
| --- | --- | --- | --- |
| N 原生浏览器只读 | `chat-turn-1790766837517-0` / `run-chat-5a30bd73501a1d234f8ff276ff5dd9ee05d955e78f8fd22b` | `native_browser_unavailable`，observation阶段，0 attempts、0输入、无页面证据 | 未通过；本轮选路保持browser，没有续接旧Paint或回退外部浏览器 |
| M Paint折线 | `chat-turn-1790767046613-1` / `run-chat-df3abb085ee6a3fadd55434b04bef646e3b52120561c2c65` | 首个click的UIA引用在执行快照不存在，`target_not_found`；未进入drag | 未通过；原轨迹输入状态unknown，实际输入安全库safe、待恢复0、未确认阻断0 |
| O 准备铅笔后新折线 | `chat-turn-1790774357433-2` / `run-chat-920b3131ac7799910d45a03c7e461000ee1dddafdc108a4b` | 1 drag、三点、1500ms、path_completed、released；后续视觉验收 `invalid_verification` | 原图明确新增L形直角，连续移动修复获得现场证据；机器整轮仍blocked/goal_achieved=false，不能改写为整轮成功 |
| P 新闭合矩形请求 | `chat-turn-1790774582465-3` / `run-chat-fcd8059d5f539e198a9873a5a91a23eee58fbd9369357301` | 0结构化派发，最新CU仍O；模型误读任务为确认历史结果 | 未执行闭合路径，不能拿O的动作记作P；保留模型失败，无隐式补发 |

完整工具ID及原始持久化事实见各轮 `*-db-facts.json` / `*-run-report.json`。N没有标题/正文/链接，不能以用户可见网页截图代替Browser Use。后续核对时，运行中的壳没有可操作控制台窗口，已请用户显示窗口并保持原生右栏打开；这只是后续现场状态，不能据此断言N当时失败的唯一原因。浏览器输入能力仍未开放。

## O 原图与提示联合观察

O前，主会话仅恢复Paint窗口并选择铅笔，未触碰画布、未画任何线、未删除或保存内容。第一次按控件索引准备失败（coordinate input geometry is unavailable），重新截图后仅用明确工具栏坐标选择铅笔；本轮模型依据新原图自主规划路径 `[0.311,0.36] → [0.35,0.36] → [0.35,0.451]`。新的L形与已有斜线、短线同时保留。

- [执行前Paint原图](paint-o-before.png)、[执行后Paint原图](paint-o-after.png)：2560×1152，未编辑。
- [活动期间全桌面原图](desktop-o-active.png)：2560×1440，四边静态泛光、顶部准确文字 `Coolzhu Agent is using your computer` 可见。
- [结束后全桌面原图](desktop-after-o.png)：提示撤除；activity=false、lease_ms=0，输入safe/accepts_new_input=true、待恢复0/未确认0。

主屏本轮正常结束提示通过现场复核，不外推多屏、负原点或取消。O视觉验收返回657字节，Syntax/第1行第511列；按既有安全投影未保存原文，不能猜测是代码围栏、引号还是其他语法原因，更不能宽松补JSON后算成功。没有新增输入隔离待办。

## 本轮源码修复（不在037包内）

M暴露的Agent问题已最小修复：原生执行器只在控件解析、可用性、边界和窗口身份这些确定的输入准备阶段附 `NotSent/NotNeeded` 回执，helper开始后的失败保持真实事实；桌面适配器对非拖动UIA引用核对新旧唯一性、定位身份和边界，变化则输入前stale拒绝，复用既有一次重新观察，不猜坐标、不重放未知输入。焦点/选择/value不当作定位身份。未修改UIA引用协议或权限链。

两项边界回归先失败后修复，无真实模型替身，也不替代实操。离线build退出0（22.07秒）；完整Web 1276通过/0失败/2忽略（35.71秒），另lib8和native-host静态1通过。原有编译warning保留，未称零warning。新修复需后续正常发布包、真实点击/输入前拒绝和重新规划实操验证。

## 未完成验收

Browser Use原生读取与后续click/scroll/type；闭合路径、简易海绵宝宝与模型视觉验收有效回复；多屏/取消提示；最新安装版与审批徽记UI复核；DSH远程插件实际安装运行；四项整体任务与最终报告。微信保持不改不测，Devin暂缓；本轮没有新GPT6 Pro共同审核回复。
