# 已安装版导航与 Paint 最小实操验收（2026-09-27）

验收对象是本机已安装的 0.2.17：`C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe` 启动的 `COOLZHU AGENT 控制台` 原生窗口，内嵌服务 `http://127.0.0.1:8765/`，实际工作区 `C:\Users\zhupu\coolzhuagent`。本次未重启或替换安装版，也未将源码工作树的后续改动算入安装版结果。

## 五个侧栏入口与三个顶栏下拉

在同一个原生窗口中，用 Computer Use 按可见控件逐项点击并立即观察；下列均是已安装版无遮挡原始截图：

| 入口 | 实测画面 |
| --- | --- |
| 首页 | A-01（私有现场原图 A-01-home.jpg 留在本机，未纳入 PR） |
| 定时任务 | A-02（私有现场原图 A-02-schedules.jpg 留在本机，未纳入 PR） |
| 设置 | A-03（私有现场原图 A-03-settings.jpg 留在本机，未纳入 PR） |
| 统计信息 | A-04（私有现场原图 A-04-statistics.jpg 留在本机，未纳入 PR） |
| 微信连接 | A-05（私有现场原图 A-05-wechat.jpg 留在本机，未纳入 PR） |
| 当前 Agent 下拉 | A-06（私有现场原图 A-06-agent-dropdown.jpg 留在本机，未纳入 PR） |
| 工程目录下拉 | A-07（私有现场原图 A-07-project-dropdown.jpg 留在本机，未纳入 PR） |
| 当前聊天室下拉 | A-08（私有现场原图 A-08-room-dropdown.jpg 留在本机，未纳入 PR） |

八个入口都能打开相应页面或菜单。只做导航观察，没有在定时任务、设置、微信连接页面提交配置或触发外部操作。设置页初始“编辑会话”显示 `bonsai-bench`，而同一画面顶栏当前 Agent 为 `GLM5.2`；两处选择在视觉上不一致，应作为体验问题继续核对，不能据此认定实际发送对象。顶栏下拉另外显示可用会话 `qwen(qwen3.8-flash)`。

## Qwen 到 Paint 的一次真实工具链请求

实际工作区 SQLite 中存在 `qwen` 会话，provider 为 `custom`，model 为 `qwen3.8-flash`，`api_key_ref` 非空；检查只输出是否有引用，没有读取或输出密钥。为避免影响现有聊天，先通过 UI 新建并命名聊天室 `验收-20260927`（room_id `room-1790469689319`），再通过 Agent 菜单选择 Qwen。发送前顶栏和 UIA 当前发送对象均显示 `qwen(qwen3.8-flash)`，菜单中仅 Qwen 复选框勾选，见 [B-01](evidence-installed/B-01-qwen-target.jpg)。

通过 Computer Use 打开系统已安装的 Paint，确认唯一窗口标题为“无标题 - 画图”、画布全白，见 [B-02 操作前](evidence-installed/B-02-paint-before.jpg)。随后只通过 Coolzhu 聊天界面发送一次最小任务：要求模型调用它自身的 `computer_use` 工具，在新画布中央画一条约 100 像素的黑色水平短线；若受阻直接报告，不使用终端或代码，也不保存文件。发送前的 Qwen、聊天室和任务文本见 [B-03](evidence-installed/B-03-before-send.jpg)。Computer Use 在本次仅负责打开 Paint、向 Coolzhu 输入任务、观察与截图，**没有代替本项目 Agent 画线**。

这一次请求的运行记录：

| 项目 | 回执 |
| --- | --- |
| chat run ID | `run-chat-ba5bba6ade581e6a649901a4d57f21ae6a17c4db163c2441` |
| session ID | `session-1779459149988` |
| tool call ID | `tool-77a1848d6a59f42d213bbec67cf2021baff00dac3fee6896be0c069e3b373d0c` |
| provider tool call ID | `call_908d9a2a18fa4663ad8ed23b` |
| 实际工具 | `computer_use_perform`，`tool_calls.status=failed` |
| 运行状态 | `runtime_runs.state=completed`，没有新建 `computer_use_runs` |

Qwen 确实给出工具调用并收到失败回执。聊天中的具体结果为 `blocked`，阶段 `intent_guard`，错误码 `input_safety_resource_not_accepting_new_input`；原因是物理输入资源当前不接受新输入（`state=unknown revision=1`）。已执行步骤数为 0，见 [B-04 工具阻断回执](evidence-installed/B-04-qwen-tool-blocked.jpg)。再次观察 Paint，仍是同一未命名空白画布，见 [B-05 操作后](evidence-installed/B-05-paint-after-block.jpg)。因此本次 **Paint 画线未通过**，阻断在本项目的 computer-use 输入资源门禁，不能归因为 Qwen 未调用工具，也不能归因为此前 `exec_command` 的外部启动策略。未重发、未换模型或执行通道，未清理既有 legacy runs，未改配置或安全策略，未保存/覆盖任何画作。

## 低高度窗口观察边界

在不修改系统 DPI、不重启已安装版的前提下，尝试通过原生窗口边缘和系统菜单的“大小”调整控制台高度；本次 Computer Use 操作没有得到尺寸变化，窗口仍为原始约 1443×897 画面，之后已清除误选文本并保持原窗口。故**低高度下的快捷栏、顶栏下拉和右栏可用性尚未验证**，没有低高度截图可作为通过证据。这不代表产品窗口不可缩放；本轮没有获得可供判断的低高度画面。

输入门禁的只读定位见 [安装版输入安全恢复诊断](installed-input-safety-recovery-diagnosis.md)。
