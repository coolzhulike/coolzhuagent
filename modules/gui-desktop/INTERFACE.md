# GUI Desktop Console 对外接口说明

## 模块职责

`gui-desktop` 是 Windows 桌面 GUI。当前仍包含部分桌面截图、输入注入和 Agent 编排代码，后续应拆出到 `computer-use` 和 `core-runtime`。

## 对外 package 与命令

- `coolzhu-desktop-console`

## 当前主要内部模块

- `app.rs`
- `service.rs`
- `desktop_agent.rs`
- `desktop_capture.rs`
- `desktop_anchor.rs`
- `input_backend.rs`
- `sessions.rs`

## 接口变更审查点

- UI 面板变更不应改变 core/computer-use DTO。
- 输入注入和截图逻辑迁移时必须保留安全测试。
- 配置项必须进入统一配置加载，不允许散落在 UI 绘制代码。

## 独立验证

```powershell
cargo check -p coolzhu-desktop-console --offline
```
