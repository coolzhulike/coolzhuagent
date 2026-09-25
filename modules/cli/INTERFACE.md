# CLI 对外接口说明

## 模块职责

`cli` 提供命令行入口，用于文本交互、调试、兼容命令和非 GUI 环境运行。

## 对外 package 与命令

- `coolzhu-command-line`
- bin：`coolzhu-cli`
- 用户文档：`docs/command-line.md`

## 配置域契约

- 启动模型优先级：`--model` > 用户/项目 `.claw` 配置 > 内置默认值。
- GUI 模型会话与 CLI 配置当前相互独立；CLI 不得静默声称已导入 GUI 会话或凭据。
- `agents` 表示 agent definition 文件，`skills` 表示 CLI roots 可发现的 skills；二者都不是 GUI 模型会话列表。
- Windows 用户 root 必须支持 `HOME`，并在其缺失时回退 `USERPROFILE`；`CODEX_HOME` 是额外 root。

## 接口变更审查点

- CLI 参数变化必须同步 `docs/command-line.md`。
- slash command 行为应从 `tooling/command-router` 获取，CLI 不单独维护一份协议。
- CLI 只作为入口，不直接实现 provider、tools、computer-use 业务。

## 独立验证

```powershell
cargo check -p coolzhu-command-line --offline
cargo run -p coolzhu-command-line -- --help
cargo test -p coolzhu-command-line --offline
```
