# Computer Use 无 ShowUI 降级路由实现（2026-08-23）

## 目标

在本地没有 ShowUI 模型时，Computer Use 仍能使用确定性 UIA、无模型模板定位和人工确认降级；ShowUI、Browser Use、远程视觉的不可用状态必须如实返回，不能伪报成功。

## 实现

- `vision::locate::BackendId` 新增 `OcrTemplate`，支持 `ocr-template`、`ocr_template`、`template` 等 JSON/API 别名，并声明模型依赖与推荐优先级。
- grounding 默认顺序调整为 `uia → ocr_template → local_vlm → remote_vlm`。
- `LocateTarget::Uia` 增加可选 `process_id`、`window_name`；Web Console 现在会读取前台窗口 UIA 快照并执行通用 AutomationId、Name、ClassName、ControlType 查询。
- `uia-resolver::resolve_query` 现在真正校验 `class_name`。
- 新增 Windows 无模型颜色模板定位后端，当前支持自然语言目标中的绿色/金色按钮提示；不满足模板条件时返回 `skipped`，不把 OCR 能力伪报为可用。
- `/api/computer-use/capabilities` 新增 `observations`：`browser_dom`、`uia`、`ocr_template`、`local_vlm`、`remote_vlm`、`manual_confirmation`，并返回优先级、是否需要模型和降级原因。
- 无自动定位结果时，`/api/vision/locate` 明确返回 dry-run/人工确认提示。
- UIA PowerShell 输出增加 UTF-8 → GB18030 回退，避免中文控件名乱码。

## 验证

| 检查 | 结果 |
|---|---|
| `cargo test -p coolzhu-vision-service --offline` | PASS，36/36 |
| `cargo test -p coolzhu-uia-resolver --offline` | PASS，2/2 |
| `cargo test -p coolzhu-web-console --offline` | PASS，816/816 |
| `cargo build -p coolzhu-web-console --offline` | PASS；保留仓库已有 warning |
| 隔离实例 `/api/computer-use/capabilities` | ShowUI=skipped、Browser DOM=skipped、UIA/template/manual=available |
| UIA StartButton 实测 | PASS，bbox `527,1008 68x72`，置信度 `0.99`，中文名称无乱码 |
| 模板定位实测 | PASS，绿色按钮 bbox `140,110 120x60`，中心 `(200,140)`，置信度 `0.72` |
| 无模板提示实测 | PASS，`unavailable/skipped`，返回人工确认提示，无坐标假成功 |
| 本地视觉端点预检 | PASS，端口不可达时立即 skipped；默认路由耗时约 `819ms`，不会等待模型超时 |

## 已知边界

- 当前 `ocr_template` 是无模型颜色/几何模板后端，不是通用 OCR；未配置 OCR provider 时，文本类目标会跳过并转人工确认或后备视觉模型。
- Browser DOM 仍需要浏览器扩展/native host；当前设备未连接，因此本轮只完成能力探测，没有执行浏览器真实输入。
- 本机未检测到 ShowUI 服务，按约定跳过本地 ShowUI 真实测试。
