# 2026-09-28 Qwen Paint 原生验收记录

## 运行身份与前置状态

- 测试入口：已安装版 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，实际控制台进程为 `C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe`。
- 原生接口：`@oai/sky` Windows API；未使用浏览器自动化或自制输入通道。
- 安全详情截图 B-06 已显示“当前输入已开放”，遗留数量为 0。
- 新建聊天室真实返回 HTTP 400（最多 8 个聊天室），未删除或重命名既有聊天室；随后复用既有 `验收-20260927`。房间顶部发送对象核对为 `qwen(qwen3.8-flash)`。百炼配置仍保留，报告不记录 base URL 或 key。

## 本轮请求与真实回执

请求唯一前缀：`NATIVE-PAINT-20260928-A`。请求限定模型只使用 `computer_use`，在已经打开的“无标题 - 画图”白色空白画布中央手动画约 100 像素黑色水平短线；禁止终端、代码、脚本、其他工具、保存文件和操作其他窗口。

真实模型工具轨迹：

```text
computer_use_perform
  status: blocked
  stage: intent_guard
  error_code: computer_use_room_full_access_required
  retryable: false
  executed_steps: 0
```

只读存储核对（本地时间 2026-09-28 22:55:08 左右）：

- 聊天运行 `run_id`：`run-chat-581a73a35bea51e75e21cf472e97a632c05f99d40d52144c`
- computer-use `call_id`：`cu-session-1779459149988-000000000000000218d983da89b651dc-tool-6febb9a20540c97d8b28a4c02bbf4a04a272231cb4015e6dc19f4abff6269eb0`
- tool call：`tool-6febb9a20540c97d8b28a4c02bbf4a04a272231cb4015e6dc19f4abff6269eb0`；provider 记录为 `call_92bd13cd39e64623ae8bfea9`
- 聊天室：`room-1790469689319`（`验收-20260927`）；会话 `session-1779459149988`；turn `000000000000000218d983da89b651dc`
- 数据库终态 JSON 与聊天回执一致：`status=blocked`、`stage=intent_guard`、`retryable=false`、`attempts=0`、`steps_completed=0`。

模型明确报告需要为发起聊天室开启 computer-use 全访问授权，画布未被改动。随后按用户授权通过安装版权限 UI 完成“完全访问（双重确认）”两级确认；只读核库确认 `chat_room_permissions` 新增/更新为 `full-access`（本地时间约 23:02:26）。

授权后的唯一一次重试使用前缀 `NATIVE-PAINT-20260928-R1`，发送对象仍是 `qwen(qwen3.8-flash)`。这次已通过 intent guard，但在观察阶段失败：

```text
computer_use_perform
  status: failed
  stage: observation
  error_code: input_failed
  message: 窗口截图结果无效——管道监督事实显示输出被截断
           dropped_bytes=134266, truncated=true, eof_seen=true
           JSON 解析在 line 1 column 262144 处 EOF 中断
  retryable: true (retry_owner=system)
  executed_steps: 0
```

本次重试的只读追踪信息（本地时间约 23:03:30）：

- 聊天运行 `run_id`：`run-chat-3e997a587fc456e59cbd2effe20674cd6194fbd44efe3d3a`
- computer-use `call_id`：`cu-session-1779459149988-000000000000000518d9844f9908fe6c-tool-505a50de0f39d96308c815d8021bc37ad044e976ef5301dc99102b87fe44155c`
- tool call：`tool-505a50de0f39d96308c815d8021bc37ad044e976ef5301dc99102b87fe44155c`；provider 记录为 `call_1b78bac254484e2d8a01cd3b`
- 聊天室仍为 `room-1790469689319`（`验收-20260927`）。

模型回执确认未产生任何鼠标输入，画布未被改动；该系统可重试标记未被本轮手动使用。没有发起海绵宝宝测试。

## 截图证据

- B-04：Paint 空白基线。
- B-08：控制台真实 qwen3.8-flash 回执，显示上述错误码及步骤 0。
- B-09：控制台覆盖 Paint 的截图，仅作过程记录，不能作为画布证据。
- B-10：模型结束后将 Paint 置前获取的新截图，无遮挡，画布仍为空白。
- B-11：安装版 UI 完成完全访问双重确认后的新截图；顶部显示完全访问，数据库同时确认 `full-access`。
- B-12：授权后唯一重试的 qwen 回执，显示 `input_failed`、观察阶段和步骤 0。
- B-13：B-12 后将 Paint 置前获取的新截图，无遮挡，画布仍为空白。

结论：首次请求因聊天室缺少真实全访问授权而被门禁阻断；完成正常 UI 授权后，唯一一次重试在窗口截图观察管道因输出截断失败，仍未执行任何画布动作。按约定不再重试，也不进入海绵宝宝测试。

## 2026-09-29 修复候选等待前状态

未发送新模型请求，未重启应用。安装版进程仍为 `coolzhu-tauri-shell.exe` PID 9212 与 `coolzhu-web-console.exe` PID 32852，路径均在 `C:\Program Files\CoolzhuAgent\bin\`；Paint 为 `mspaint.exe` PID 18272。Sky 新鲜窗口状态显示控制台和 Paint 均存在，聊天室仍显示完全访问，数据库仍为 `room-1790469689319 / full-access`。

- B-14：当前安装版控制台新鲜截图。
- B-15：当前 Paint 新鲜无遮挡截图，画布仍为空白。
