# Core Runtime 对外接口说明

## 模块职责

`core-runtime` 负责会话、上下文、权限、配置、Agent 执行状态和会话 HTTP 服务。它是其它模块的业务核心依赖，但不依赖 GUI、桌面输入或具体视觉实现。

## 对外 crate

- `coolzhu-core-runtime`，兼容 crate alias：`runtime`
- `coolzhu-agent-server`，兼容 crate alias：`server`
- `coolzhu-language-service`，兼容 crate alias：`lsp`

## 稳定接口

- `runtime::Session`
- `runtime::ConversationMessage`
- `runtime::ToolExecutor`
- `runtime::ConfigLoader`
- `server::app`
- `server::AppState`

## 接口变更审查点

- 修改 `Session`、`ConversationMessage`、工具调用结构时，需要同步更新 `docs/interface-contracts.md`。
- 修改 HTTP 路由、SSE 事件或状态字段时，需要同步更新 GUI Web 和集成测试。
- 不允许引入 GUI 或平台输入依赖。

## 独立验证

```powershell
cargo check -p coolzhu-core-runtime --offline
cargo test -p coolzhu-agent-server --offline
```
