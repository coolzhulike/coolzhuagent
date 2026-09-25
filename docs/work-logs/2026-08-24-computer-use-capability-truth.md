# 2026-08-24 Computer Use 无 ShowUI 能力真实性

## 发现

隔离实例启动桌宠壳后，`/api/showui/service` 的旧 `running=true` 只代表桌宠壳进程存活，并不代表 ShowUI grounding 端点在 `8000` 端口就绪。视觉实验室因此可能把“壳进程存活”显示成“ShowUI running”，与 `/api/computer-use/capabilities` 的 `showui=skipped` 产生误导。

## 修复

- `ShowUiServiceResponse` 增加 `available`，按 ShowUI 端口实际监听状态返回；`running` 继续表示管理进程/桌宠壳状态。
- 前端 `renderShowUiServiceStatus` 以 `running && available` 判断真实可用，明确显示 `ShowUI shell running, backend unavailable`，并把启动/停止按钮状态绑定到真实可用性。
- 保留已有无模型降级链：UIA → 无模型颜色/几何模板 → 本地/远程视觉 → 人工确认；无自动定位时不生成坐标假成功。

## 本地验证

- `cargo test -p coolzhu-web-console web_frontend_overview_card_can_toggle_showui_service --offline -- --test-threads=1`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过。
- `cargo test -p coolzhu-web-console --offline -- --test-threads=1`：835 passed，0 failed。
- `node --check modules/gui-web/packages/web-console/src/app.js` 与 `git diff --check`：通过。
- 隔离实例 `127.0.0.1:8801`：`showui_service.running=true`、`available=false`，能力探测 `showui.status=skipped`，同时 UIA、ocr_template、manual_confirmation 为 `available`。
- `POST /api/vision/locate` 对未知紫色控件返回 `status=unavailable`、`point=null`、`bbox=null`，并明确要求 dry-run/人工确认，没有硬编码锚点回退。
- 关闭隔离实例后确认 8801 端口及 `coolzhu-web-console` 进程均已退出。

## 证据

- 能力与服务状态：`tmp/compute-use-fallback-api-evidence-round10.json`
- 无模型定位降级：`tmp/compute-use-locate-evidence-round10.json`
- 视觉实验室界面：`tmp/compute-use-fallback-ui-evidence-round10.png`
- 全量测试日志：`tmp/round10-web-tests.log`

本机没有可用 ShowUI 本地模型/8000 服务，按用户约定跳过真实 ShowUI 输入测试；本轮仅验证 capability skip、无模型定位链和安全 dry-run。
