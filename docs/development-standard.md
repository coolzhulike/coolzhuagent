# 开发规范

## 配置管理原则（2026-05-08 新增）

- **禁止硬编码路径**：所有路径（目录名、文件名、默认路径）必须从配置常量或配置文件读取，不得在代码中直接写死字符串。
- **禁止环境变量依赖**：新增业务配置、功能开关、路径、模型参数、工具参数不得使用 `COOLZHU_*`、`CLAW_*` 等自定义环境变量，必须读写 `coolzhu.toml` 或模块内明确的配置文件。历史环境变量只能作为待迁移兼容债务保留，不得扩散到新代码路径。
- **不可避免环境变量例外**：仅 OS/工具链必需变量（如 `PATH`、`HOME`、`USERPROFILE`）、第三方 SDK 暂无配置文件入口的认证变量、或子进程启动协议强制要求的 one-shot 变量可临时使用。代码旁必须写明“为何无法配置文件化”，并在需求/工作日志记录迁移风险和后续替代方案。
- **测试配置隔离**：自动化测试不得通过修改全局环境变量切换业务行为；优先使用临时 `coolzhu.toml`、显式 config root、进程内互斥锁或 scoped config helper。确需覆盖系统环境时必须串行化并在测试结束恢复。
- **统一配置文件**：所有路径、容量、开关等配置项集中在 workspace 根目录下的 `coolzhu.toml` 中。首次运行时自动生成默认配置。
- **workspace 跟随**：当用户通过 UI 修改工程目录路径后，`coolzhu.toml` 会自动复制到新 workspace。程序始终从 `active_workspace_path()` 对应的配置文件读取。
- **配置项命名**：使用 TOML `[section]` 分组，key 使用 `snake_case`。示例结构见 `coolzhu.toml`。

## 代码组织

- 模块内部可以独立迭代，模块之间只通过已文档化接口通信。
- 不允许 GUI 直接实现 core-runtime、vision、computer-use 的业务细节。
- 不允许模型 provider 配置散落到 GUI 代码。
- 不允许真实键鼠输入默认开启，必须显式配置和显示状态。

## 命名规范

- 目录命名使用 kebab-case，例如 `computer-use-core`。
- Cargo package 使用 `coolzhu-*`，例如 `coolzhu-web-console`。
- 新增代码、文档、配置不得再引入 `rust-claw`、`claw`、`crate/rust` 这类项目无关命名。
- 既有 `CLAW_*` 环境变量、`.claw-plugin` 清单目录等属于历史兼容协议字段；修改前必须同步更新对应模块 `INTERFACE.md` 并评估外部兼容风险。不得新增自定义环境变量协议，新增配置必须进入 `coolzhu.toml` 或明确的模块配置文件。
- Rust 依赖别名暂时允许保留 `runtime`、`api`、`vision` 等短别名，用于降低首轮迁移风险；不得新增含旧项目名的 crate/package/bin。

## Gerrit 提交规范

每个 change 至少说明：

- 修改目的。
- 涉及模块。
- 是否影响对外接口。
- 测试命令。
- 回滚方式。

推荐 topic：

- `topic/contracts`
- `topic/computer-use`
- `topic/vision`
- `topic/gui-web`
- `topic/tooling`
- `topic/diagnostics`

## 编译规范

每个模块修改后至少运行：

```powershell
cargo fmt -p <package-name>
cargo check -p <package-name> --offline
cargo test -p <package-name> --offline
```

## 模块独立编译与 package 规范（2026-06-19 新增）

- 每个可执行模块必须使用本模块目录下的独立 Cargo `target` 目录，禁止多个并行开发模块共用根目录 `target` 作为交付来源。
- 所有可运行二进制及其资源由 `config/package-manifest.json` 声明；新增、重命名或移除二进制时必须同步修改清单，不得把模块名、输出路径和复制规则散落硬编码到多个脚本。
- 统一执行 `.\package.ps1 all -Configuration debug` 或 `npm run package:all` 完成独立编译和汇总。正式运行入口统一使用 `package/run.ps1`，不得直接依赖各模块临时构建目录。
- 汇总前必须比较源二进制的修改时间与 SHA-256。仅当内容变化时替换 `package/bin` 中的旧文件，并将旧文件备份到 `package/backup/<二进制名>/`。
- 同名二进制备份按时间戳命名，仅保留最近 10 次；清理范围必须限定在该二进制自己的备份目录。
- `tmp/package-reports/package-report-<config>-<stamp>.json`（**实际落点**，由 `scripts/package-all.ps1` 写出；同目录下还有 `latest-<config>.json` 指针与 `retention-index.json` 保留索引）必须记录构建配置、源路径、目标路径、复制状态和 SHA-256，供并行 agent 判断模块是否产生新构建。
- 该报告还必须携带 `report_identity`（唯一 `report_id` + 可重算的 `content_sha256`）与三个分开的身份（`source_snapshot_digest` / `build_input_digest` / `payload_digest`，见 `docs/analysis/2026-09-21-integration-review/build-identity-and-report-governance.md`），并被包内 `payload-inventory.json` 与 `dist/CoolzhuAgent-<version>-installer-report.json` 引用。
- 已被正式发布引用的报告受 `scripts/package-report-retention.ps1` 保护（`-Action Verify|Protect|Prune`）：普通临时清理只能删除未被索引的报告，索引缺失或损坏时**拒绝清理**；`build-msi.ps1` 出包后自动 `Protect -Archive` 到 `docs/testing/release-<version>/evidence/build-identity/`。
- package 脚本和全量验证脚本必须设置明确的命令超时；输出统一写入 `tmp/logs/`，失败时以日志定位，不得静默吞掉 stderr。

## 用户可见功能验证规范（2026-06-18 新增）

- GUI、设置、按钮、会话配置、模型切换、TTS/STT、工具详情等用户可见功能，最终验收必须启动真实前端。computer-use 稳定可用时，优先用它模拟用户鼠标点击、选择和键盘输入。
- 用户明确选择人工确认，或 computer-use 插件存在中断、失焦、连接不稳定时，可以改用编译、HTTP/API、日志和真实后端链路作为自动化功能证据，由用户完成人工前端视觉与交互确认。
- 改为人工确认时，work-log 必须明确记录“自动化功能验证已完成、视觉/交互待人工确认”，在用户确认前不得宣称视觉验收完成。
- 功能验收必须观察用户可见反馈，并同时核对配置文件、后端日志或服务状态，证明操作真实生效。
- 直接调用 HTTP API、静态字符串断言、DOM 注入调用函数和纯后端单元测试只能作为辅助证据，不得单独作为用户可见功能完成依据。
- 涉及真实键鼠的测试必须限制在明确测试窗口和安全区域，保留截图、输入坐标、目标控件和结果证据。

## 最小 TDD 规范（2026-06-18 新增）

- 继续遵循 Red → Green → Refactor，但每个根因优先只增加一个能够准确复现问题的基本回归测试。
- 不为相同实现细节重复堆叠测试，不做无收益的 provider、状态和参数全排列。
- 协议、安全、数据持久化和权限边界可以补必要的契约测试；用户交互正确性优先使用真实前端 computer-use 验收，插件不稳定或用户指定时改由人工前端确认。
- 测试必须先确认因目标缺陷而失败，修复后再确认通过。

## 禁止硬编码与配置闸口规范（2026-06-18 新增）

- GUI 和业务模块不得散落硬编码端口、路径、模型名、Provider URL、endpoint、启动器、启动参数、超时、轮询次数和功能开关。
- 上述业务参数必须进入 workspace 根 `coolzhu.toml` 的 typed 配置结构；内置默认值由配置模块集中生成，不允许多个模块重复维护。
- 前端可设置的业务参数必须与后端实际读取使用同一个配置源。禁止存在不受前端设置影响的隐藏全局变量、环境变量或 gate 覆盖用户配置。
- 新增代码不得依赖 `COOLZHU_*`、`CLAW_*` 等自定义环境变量控制业务行为。历史变量在本轮触及相关链路时应迁移到 config-first，并记录兼容与移除计划。
- 仅 OS/工具链变量、第三方认证协议强制变量和进程 one-shot 协议变量可作为例外；代码旁和 work-log 必须说明原因。

## 图像资源修复规范（2026-06-18 新增）

- 修复既有角色、桌宠或 sprite 帧时，禁止使用非等比拉伸、CSS 单轴缩放或改变人物比例的补偿。
- 优先通过帧选择、透明边距、画布定位和锚点校正解决。
- 现有源图无法无损修复时，可使用 Image Gen 参考原图重新生成；必须约束身份、服装、武器、画风、画布、可见尺寸和基线，只修改目标动作。
- AI 生成候选必须使用版本化文件名，经过自动指标和人工对比后才能替换正式资源。

## 高风险修改备份规范（2026-06-18 新增）

- 会话协议、配置存储、进程控制、数据库迁移和正式图像资源属于高风险修改。
- 修改前必须把完整受影响源码目录复制到 `tmp/backups/<timestamp>-<topic>-pre/`，并生成文件清单与 SHA-256 摘要日志。
- 备份与验证命令必须设置超时，输出写入 `tmp/logs/`。
- work-log 必须记录备份路径、恢复方法和验证结果。

## 模块日志与启动自检规范（2026-06-19 新增）

- package 启动入口必须创建 `tmp/logs/`，并将各长驻模块的 stdout/stderr 分别重定向到 `<module>.stdout.log` 与 `<module>.stderr.log`。
- 启动器必须生成最近一次自检报告 `tmp/logs/package-selfcheck-last.json`；报告至少包含 package report、关键二进制、关键资源、日志目录可写性，以及 web-console health 摘要。
- 新增模块 probe 时优先复用统一自检报告结构：`id/status/detail`，`status` 只允许 `ok/warn/error`；阻断启动的条件必须在方案或 work-log 里说明。
- 结构化 err 日志最低字段为 `event/module/level/err_kind/message/code_site/trace_id`；用户可见功能失败还应记录对应 API、配置项或前端动作来源。
- 启动脚本不得新增业务环境变量。当前 package 启动器设置 `COOLZHU_LOG_DIR` 仅作为历史 diagnostics 输出目录的 one-shot 兼容入口；新增业务配置必须进入 `coolzhu.toml`。

## 本地模型容量与 Provider 边界规范（2026-06-25 新增）

- 必须区分“模型架构最大上下文”和“当前硬件安全运行上下文”。不得把模型卡的最大值直接写入启动参数；运行值应结合显存、内存、量化、KV cache、并发槽位和目标输出长度确定，并由 `coolzhu.toml` 配置。
- 本地文本推理模型文件路径必须从 workspace `coolzhu.toml [model].local_chat_model_path` 读取，并提供前端文件选择/保存入口；不得在启动链路硬编码用户目录、模型文件名或 GGUF 路径。
- 用户可见的本地模型身份统一为 `coolzhu-model`。聊天室回复、推理卡片、Goal 执行消息和可见错误文案不得暴露原始模型名、GGUF 文件名或模型家族；内部日志可保留必要诊断信息，但不得输出密钥或附件正文。
- 本地模型必须为最终回复预留独立输出空间。支持 thinking 的模型还应限制思考预算，禁止让 `reasoning_content` 吃满 `max_output_tokens` 后返回空正文。
- 上下文装配必须把系统提示、当前输入、历史、记忆、附件视觉 token、输出预留和 tokenizer/协议安全余量纳入同一硬预算；达到阈值后复用统一上下文压缩与记忆回灌流程，不得为本地模型另建静默截断分支。
- Data URI、裸 Base64、远端 URL 等附件表示应在通用聊天层保留完整语义，只在具体 Provider adapter 边界转换成目标协议格式；转换前必须本地校验，日志不得输出附件正文或密钥。

## Goal 长任务 Skills 约束规范（2026-06-26 新增）

- Goal 长任务执行前必须加载项目内置 baseline Skills：`coolzhu-goal-session-chain`、`coolzhu-goal-model-reasoning`、`coolzhu-goal-tool-execution`；phase 自带 skills 只能追加，不得替代 baseline。
- 这三类 Skills 分别约束“会话链路/上下文边界”“模型推理/预算/证据”和“工具执行/路径/日志/超时”。后续复盘 GLM5.2 或其它 Goal 模型的失败案例时，优先沉淀到对应 Skill，而不是只写 work-log。
- Goal prompt 中必须包含已加载 Skill 的正文或明确的 missing 提示；不得只列 skill 名称后让执行模型自行猜测约束。
- 修改 Goal Skills 后至少运行 skill 校验脚本和一条 Goal prompt 契约测试，确认 baseline skill 名称与正文被注入执行提示。
