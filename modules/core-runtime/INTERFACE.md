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
- `runtime::render_prompt_memory_context`：历史资料投影与固定信任边界；非空输出为说明及JSON数组，保留层级、种类、来源和摘要，序列化转义摘要换行、引号与控制字符。调用方继续负责召回、预算和执行权限，不将摘要内容当作授权。
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
