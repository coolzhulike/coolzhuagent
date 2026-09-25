# Tooling 对外接口说明

## 模块职责

`tooling` 负责工具注册、插件系统、slash command、兼容性抽取与 CLI Anything 等工具扩展入口。

## 对外 crate

- `coolzhu-tool-registry`，兼容 crate alias：`tools`
- `coolzhu-plugin-system`，兼容 crate alias：`plugins`
- `coolzhu-command-router`，兼容 crate alias：`commands`
- `coolzhu-compatibility-harness`，兼容 crate alias：`compat_harness`

## 稳定接口

- `tools::GlobalToolRegistry`
- `plugins::PluginManager`
- `plugins::PluginHooks`
- `commands::SlashCommand`
- `commands::slash_command_specs`

## 接口变更审查点

- 工具 schema、工具名称、超时语义变化必须同步 GUI 工具调用区。
- 插件 hook 新增或删除必须更新插件接口文档。
- 所有真实执行工具必须提供 dry-run 或风险说明。

## 独立验证

```powershell
cargo check -p coolzhu-tool-registry --offline
cargo test -p coolzhu-plugin-system --offline
cargo test -p coolzhu-command-router --offline
```
