# 0.2.57 正式安装回归与未完成项

2026-10-01，主会话独立实施及测试。真实 qwen3.8-flash、medium、既有百炼 Base URL 与 API_KEY 保持不变；没有模型夹具，没有人工代模型绘画或点击认领成功。微信不改不测，Devin 搁置，Pro 补审按用户决定暂停。

## 发布和安装身份

正式 MSI 正常安装退出码 0，唯一注册版本 0.2.57，10 个关键安装产物匹配发布摘要。运行 Shell PID27672、Web PID21608，8765 由正式 Web 占用。MSI 247041405 字节，SHA256 `684a02f6c4fe90d5f773db944ffeac395753927932a5787d77beb4a365f8fc81`；报告 `pkg-report-release-20261001-170836193-47d2d42d`。源快照 `db3d7e506164641c1590f8d8f2f3608da64948fd88a90cbef87971040655519e`，载荷 `5ffb705f1e34db3331b4c9ea88ebbc953a2ccd4052bfcd656c7827f30a54ffd6`，859 文件、324538854 字节，6 项发布门通过、安全扫描 0 项发现。原始身份记录在 evidence/build-identity，不改写为后续源代码的身份。

前端语法检查及离线 Web build 通过。056 的完整工程检查 1296/0/2 既有忽略、另 lib8/宿主1 是上一版结果，不标成057新跑全量。安静安装尝试的1603/1730失败收据与后续正常交互安装成功收据同时保留。

## 此包修改及职责

三个前端模块清理浏览器上下文展示：workspace_panels 识别工程或聊天室变更；native_browser_panel 的 close/forgetTarget 清理旧重开目标并关闭原生资源；app 的 resetBrowser 清空地址、框架地址、状态及导航选择。发送模型变化不等于工程或聊天室变化。没有添加第二执行入口或持久化状态表。057不包含后续输入到期分类修复。

设计及风险见 ../../analysis/2026-10-01-browser-context-display-review.md。

## 正式软件与真实模型结果

| 测试 | 结果及限制 | installed-native 证据 |
|---|---|---|
| 同聊天室关闭后重开 | 自动显示原网页，不需要再次打开或刷新 | 02-browser-same-room-auto-reopen.jpg，具体文件名以原图索引为准 |
| 跨聊天室 | 地址、状态和页面清空，不加载前一聊天室网页 | 03 开头原图 |
| 跨工程 | 切到既有 workspace-a 后地址、状态和页面清空；仅检查界面，未用该工程其它模型发请求 | 04/05 开头原图 |
| BU057-SINGLE-BZ | 当前参数的一次真实单击，计数0→1；CU succeeded，sent/released，31.333秒 | BZ-facts、06-browser-qwen-single-click-completed.jpg |
| BU057-CLOSE-CA | 模型错误选到不存在的扩展宿主，extension_unavailable，0动作；不算关闭验收 | CA-facts、07 开头图 |
| BU057-CLOSE-CB/CC | 模型提交历史参数，与本轮完整契约不符，输入前拒绝，0动作；不能据模型的参数自述认领符合 | CB/CC-facts、08/09 开头图 |
| BU057-CLOSE-CD | 在已有独立聊天室测试，模型漏掉必填 success_criteria，invalid_tool_input，0动作；不算关闭验收 | CD-facts、10/11 开头图 |
| BU057-CLOSE-CE | 2次单击已发出并释放，用户关闭右栏后第3步 not_sent/not_needed，native_browser_panel_unavailable；无重开、续发或重试，63.739秒 | CE-facts、close-ui.json、12/13 开头图 |
| CU057-PAINT-CF | 1次五点闭合路径仅注入3点，partial=true/path_completed=false/released，许可到期；两个眼睛未达成，134.296秒 | CF-facts、16/17/18/19/20 原图、活动收据 |

CE 的关闭开始1790847865507、结束1790847865595；第1步完成1790847829680，第2步完成1790847855525，第3步1790847873360开始后被拒绝。因此**关闭发生在步骤之间**，不能将较早观察到 active/pending 的票据当作关闭时仍在交付。按下到释放之间关闭的竞争仍未覆盖。终态活动 active=false、lease_ms=0。

CF 初始视觉验证8096ms；第1次规划102991ms，19795输入/9860输出tokens，消耗了大部分共享120秒预算。drag计划duration_ms1400，实际步骤耗时4587ms，在第3点后 helper 报 `stroke_cancelled: permit expired`。上层057把确定的期限到期归为 cancelled，属于 Agent 分类缺陷；回执正确保留 sent/partial/3点/released，没有把释放追认为路径完成。原生截图与独立全桌面图显示半菱形位于已有大矩形左侧，模型选点也未满足内部眼睛要求；现有证据不支持断言坐标映射故障。两项原因分别记录，不能仅增加预算或降低验证要求认领绘画完成。

CF 原生失败图20与来源字节一致，摘要 `d7c0642fd8f4d7f3b17a3f3f378d7e038b4131831dc67ca1c7f825510751da09`，2560×1152。Sky窗口图与原生/全桌面图存在旧帧差异，均保留，不以Sky单帧认领绘画结果。运行中全Windows四边泛光和准确英文提示见18，结束 active=false、lease_ms=0。056已通过的一次闭合矩形与日常舞剑动画详见056报告，不重复外推到CF或动画全部边界。

## 下一模型的定向用例设计要求

1. 正向单击及同房间重开、跨房间/工程展示均已实操通过；在下一正式包只做受改动影响的回归。必须保留真实父run/CU/step关联、前后原图和时间，不能以父 completed 等同目标完成。
2. 关闭竞争单独设计：证明关闭时已经按下或输入已派发且尚未释放，记录投递、释放及资源世代；若只能关闭在步骤之间，就标“未覆盖”。不能为制造成功使用隐蔽输入、绕过宿主或手写票据。
3. 到期分类：确定 permit expired 应为 deadline_exceeded/timed_out，不自动重试；部分路径及释放事实保持独立；释放未知仍优先安全阻断。057旧记录保留原 cancelled，不回填改写历史。
4. Paint以真实原图为标准。先验证当前矩形内部的一次闭合眼睛，再做两眼/嘴/腿的简化海绵宝宝；记录每次模型规划时长、剩余预算及坐标。不能手画补齐、换模型、删验证或以 released 宣称画成。
5. 动画首次/减少动态效果/资源失败、提示多屏与取消分支、DSH远程插件安装及真实Qwen调用仍开放。DSH实施分阶段方案见 ../../analysis/2026-10-01-dsh-remote-plugin-runtime-plan.md，其中P1–P5尚未实施验收。四项总体未通过，PR74保持Draft。
