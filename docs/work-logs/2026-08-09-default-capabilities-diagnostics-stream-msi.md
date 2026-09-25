# 默认能力、聊天室诊断与流式看护交付记录（2026-08-09）

## 本轮交付

- 首次启动配置继续保持“已实现能力默认可用、授权与凭据不默认放宽”：模型能力、computer-use 桥接能力、实时音频自动收发默认开启；full-access、工具审批和外部目录授权仍由聊天室/会话显式授予。
- 新增 `/api/diagnostics/functional` 只读常用功能自检，覆盖配置、工作区、会话存储、聊天室、工具目录、流式看护与浏览器桥接登记；明确不调用真实模型、STT/TTS 或桌面输入。
- 自检状态按真实运行态返回：缺 provider 凭据为 `warn`，浏览器桥未连接为 `error` 并给出安装/连接提示，流式中断契约未做网络注入时为 `warn` 而非伪造成功。
- 新增聊天室诊断偏好 GET/PATCH API 与持久化表 `chat_room_diagnostics`（基础诊断、自动刷新、显示流式中断、显示详情），不改变权限配置；前端增加“诊断设置”弹窗和“常用功能自检”按钮。
- 同一弹窗可按聊天室收紧真实模型、模型工具、computer-use，并通过既有 `/permissions` 二次确认设置 workspace-write/full-access；运行链按聊天室配置进一步收紧，全局关闭优先。
- 流式中断显示用户停止/通道提前关闭原因，异常中断不自动递归重试；SSE 缺失 `done` 事件会给出可见诊断。

## 验证

- `cargo build -p coolzhu-web-console --offline` 通过。
- `cargo test -p coolzhu-web-console --offline`：803 个主测试、8 个库测试、1 个 native-host 测试全部通过。
- 隔离 `COOLZHU_RUNTIME_DIR` 首启生成 `coolzhu.toml`，能力默认值与 API 持久化读回已验证。
- `dist/CoolzhuAgent-0.2.0-20260809-213039.msi`：WiX 5.0.2，SHA256 `8981CBF8A773484119EF6EC10ED8E049298047237F15A6775F441BD03209DD92`，未签名（需发行证书后另行签名）。
- 最新重建包：`dist/CoolzhuAgent-0.2.0-20260809-214749.msi`，SHA256 `37CBFA60C896D8EE6503CA097E2027FBE265C9423E538601CAB9A3D3490BA868`，仍为未签名（`NotSigned`）。
- 本轮隔离 API 自检（`127.0.0.1:8878`）：`GET /api/diagnostics/functional` 返回 `summary.status=error`，其中缺 provider 凭据为 `warn`、浏览器桥未连接为 `error`、流式看护为 `warn`（明确“未注入真实网络中断”）；失败项均带 `fix_hint`，未伪造成功。
- `GET /api/diagnostics/health` 返回 `summary.status=error`，原因是测试隔离环境无 LLM provider 凭据；Web 绑定、配置解析、SQLite、工具目录检查均为 `ok`，缺失项包含可执行修复提示。
- `GET /api/chat/rooms/main-room/diagnostics` 读回默认 `real_llm_enabled=true / llm_tools_enabled=true / computer_use_enabled=true`；PATCH 收紧为 `false/false/false` 后再次 GET 读回一致，证明聊天室配置持久化。
- `node --check modules/gui-web/packages/web-console/src/app.js` 通过；输出原文保存在 `tmp/api-selfcheck-output.json`（临时文件不入库）。

## 未闭环

- 人工麦克风/STT/TTS 与真实浏览器登录态仍需设备/用户参与的端到端验收。
- health 汇总在无桌宠/WebView2/本地模型等发行环境依赖时可能为 error；functional 自检本身保持可用并明确依赖边界。
- MSI 行政解包在当前环境的 `msiexec /a` 流程挂起，未据此宣称解包成功；包静态安全扫描仍为 `safe=true`，安装包未提交仓库。
