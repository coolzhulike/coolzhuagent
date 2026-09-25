# COOLZHU CODE Agent 命令行

`coolzhu-cli` 是 COOLZHU CODE Agent 的独立命令行入口，可在当前工作目录执行交互式 Code Agent、one-shot prompt、工具调用和会话导出。

## 安装态启动

MSI 默认安装到：

```text
C:\Program Files\CoolzhuAgent\bin\coolzhu-cli.exe
```

如果该目录未加入 `PATH`，请在 PowerShell 中使用完整路径：

```powershell
& 'C:\Program Files\CoolzhuAgent\bin\coolzhu-cli.exe' --version
& 'C:\Program Files\CoolzhuAgent\bin\coolzhu-cli.exe' --help
```

源码工作区可使用：

```powershell
cargo run -p coolzhu-command-line -- --help
```

## 常用命令

```powershell
coolzhu-cli                                      # 进入交互式 REPL
coolzhu-cli "概括这个仓库"                       # one-shot prompt
coolzhu-cli prompt "解释 src/main.rs"            # 显式 one-shot prompt
coolzhu-cli --output-format json prompt "任务"   # JSON 输出
coolzhu-cli --model glm-5.2 "检查当前工程"        # 临时选择模型
coolzhu-cli agents                               # 列出 CLI agent 定义
coolzhu-cli skills                               # 列出 CLI 可发现的 skills
coolzhu-cli /version                             # 直接打印版本
```

`/status`、`/model`、`/permissions` 等依赖当前会话状态的 slash command 应在无参数启动后的 REPL 中执行。支持恢复模式的命令也可以写成：

```powershell
coolzhu-cli --resume session.json /status /diff /export notes.txt
```

## 模型配置优先级

CLI 按以下顺序选择启动模型：

1. 命令行显式 `--model MODEL`；
2. CLI 用户或项目 `.claw` 配置中的 `model`；
3. 内置默认模型。

用户配置示例（Windows：`%USERPROFILE%\.claw\settings.json`）：

```json
{
  "model": "glm-5.2"
}
```

项目可以在仓库根目录使用 `.claw.json`、`.claw/settings.json` 或仅本机生效的 `.claw/settings.local.json`。更具体的配置会覆盖前面的共享配置。

### GUI 与 CLI 配置边界

Web Console 中的 GLM-5.2、agnes 等“模型会话”保存在 GUI 的 LocalAppData 状态库。`coolzhu-cli` 当前不会自动导入这些会话或其中的 API key；CLI 使用上述 `.claw` 配置和 CLI 进程环境。这是明确的配置域边界，不是 `agents` 命令的数据来源。

当初始化模型失败时，CLI 会打印实际选中的 model、provider、model 来源和对应凭据变量，避免把内置 Claude 默认值误解成 GUI 会话配置。

## Provider 凭据

凭据只应放在安全的用户环境或受保护的凭据文件中，不要提交到仓库。

| Provider | 常用模型示例 | 凭据变量 |
|---|---|---|
| 阿里百炼 / DashScope | `glm-5.2`、`qwen-plus` | `DASHSCOPE_API_KEY` |
| 智谱 AI | `glm-4.7` | `ZAI_API_KEY` 或 `BIGMODEL_API_KEY` |
| Anthropic / CLI OAuth | `claude-opus-4-6` | `ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_API_KEY`，或 `coolzhu-cli login` |
| OpenAI | `gpt-4.1` | `OPENAI_API_KEY` |
| xAI | `grok-3` | `XAI_API_KEY` |
| DeepSeek | `deepseek-v4-flash` | `DEEPSEEK_API_KEY` |
| 百度千帆 | `ernie-4.5-turbo-128k` | `QIANFAN_API_KEY` |
| 火山方舟 | `doubao-1-5-pro-32k-250115` | `ARK_API_KEY` |
| 自定义 OpenAI 兼容端点 | 自定义 model id | `CUSTOM_API_KEY`；本地无鉴权端点可为空 |

当前 PowerShell 会话中临时设置阿里百炼凭据的示例：

```powershell
$env:DASHSCOPE_API_KEY = '<your-key>'
coolzhu-cli --model glm-5.2 "检查 Cargo workspace"
```

## `agents` 与 `skills` 的含义

`coolzhu-cli agents` 列出的是 agent definition TOML 文件，不是 GUI 模型会话。发现顺序包括：

- 当前目录及其祖先的 `.codex/agents`、`.claw/agents`；
- `$CODEX_HOME/agents`；
- 用户目录的 `.codex/agents`、`.claw/agents`。

`coolzhu-cli skills` 发现：

- 当前目录及其祖先的 `.codex/skills`、`.claw/skills`；
- `$CODEX_HOME/skills`；
- 用户目录的 `.codex/skills`、`.claw/skills`；
- 对应 `commands` 目录中的兼容 markdown command。

Windows 用户目录从 `HOME` 读取；未设置时回退 `USERPROFILE`。`CODEX_HOME` 如果存在，会作为额外来源参与发现。

## 权限与工具

```powershell
coolzhu-cli --permission-mode read-only "检查代码"
coolzhu-cli --permission-mode workspace-write "修复并测试"
coolzhu-cli --allowedTools read,glob "概括模块结构"
```

`workspace-write` 只允许在工作区内写入；需要更高权限的工具会请求确认。`--dangerously-skip-permissions` 会跳过权限检查，只应在明确隔离且可信的环境使用。

## 版本与发行元数据

`coolzhu-cli --version` 输出产品版本、Git SHA、target 和 UTC build date。发行构建可通过以下编译期变量注入与发行包一致的元数据：

- `COOLZHU_RELEASE_VERSION`
- `COOLZHU_BUILD_DATE`（`YYYY-MM-DD`）
- `COOLZHU_GIT_SHA`
- `COOLZHU_BUILD_TARGET`

未显式注入时，开发构建使用 CLI Cargo package 版本、当前 UTC 日期和当前 Git commit。
