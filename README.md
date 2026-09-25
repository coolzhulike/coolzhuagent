# coolzhu agent

coolzhu agent 是一个面向 Windows 桌面环境的 Rust Agent 工作区，提供模型会话、聊天室、多阶段任务、实时语音、Computer Use、浏览器桥接、微信私聊连接和桌面控制台等能力。

## 主要模块

- `modules/core-runtime`：会话运行时、权限、记忆与任务编排。
- `modules/llm-adapter`：模型 Provider、流式响应和嵌入适配。
- `modules/tooling`：工具注册、插件、命令路由和路径影响分析。
- `modules/vision`：截图、UIA、本地与远端视觉定位。
- `modules/computer-use`：鼠标、键盘、坐标映射和输入安全控制。
- `modules/gui-web`：Web 控制台、实时语音、浏览器桥接与微信连接。
- `modules/gui-desktop`：桌面控制台、桌宠和 Tauri shell。
- `modules/cli`、`modules/diagnostics`：命令行与诊断工具。

完整结构见 [仓库结构](docs/repository-structure.md)，安装与交互方法见
[0.2.0 离线使用说明和验收表](docs/user-guide/index.html)，公开设计文档入口见
[GitHub 文档索引](docs/github-public-index.md)。

## 开发与运行

环境要求：Windows、Rust stable、PowerShell 5.1 或更高版本。首次构建需要准备 Cargo 依赖；已有本地缓存时可以离线构建。

```powershell
cargo build -p coolzhu-web-console --offline
cargo run -p coolzhu-web-console -- --open
```

主控制台默认监听 `http://127.0.0.1:8765/`。前端资源由 Rust 在编译期内联，修改 `index.html`、`src/app.js` 或 `src/styles.css` 后必须重新构建 `coolzhu-web-console`。

常用验证命令：

```powershell
cargo test -p coolzhu-web-console --offline
cargo test -p coolzhu-computer-use-core --offline
cargo test --test module_linkage_smoke --offline
```

## 配置与隐私边界

仓库只保留可公开的启动/打包配置：

- `config/package-launcher.json`
- `config/package-manifest.json`

模型密钥、登录凭据、微信身份、会话数据库、`.env`、`coolzhu.toml`、`.coolzhu` 运行时状态、模型权重、日志、备份、安装包和编译产物不属于源码仓库。`.coolzhu/plugins` 是例外：它只承载 Cargo workspace 中的插件源码。

## 打包

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-msi.ps1 -Version 0.2.0 -Configuration release
```

GitHub-ready 源码和精选 Obsidian 文档可用以下命令生成：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/project-delivery.ps1
```

脚本会生成文件清单、SHA-256、展开目录、ZIP 往返校验和隐私扫描报告。

## 许可证

本项目使用 [MIT License](LICENSE)。
