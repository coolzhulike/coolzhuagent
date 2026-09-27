# 2026-09-25 work-log：PATH-01 + PATH-02 落地（统一启动路径解析 + 既有工作区保留与恢复）

裁决输入：第四轮裁决第四节（目录分类 / 解析优先级 / 用户选择落点 / 缺键语义）与第五节（"工作区不见了"的正式修复流程），
及第六节中"复用判定不能只核对端口健康""启动自检要同时记录请求路径与后台实际路径"。
允许修改范围：`packages/app-launcher/**`、`config/package-launcher.json`、`docs/**`。
**未改动**：任何 `modules/**` Rust 文件、`scripts/package-all.ps1`、`config/package-manifest.json`。

契约定稿：`docs/launch-path-resolution-contract.md`（术语、①–⑤ 优先级、缺键语义、恢复流程、未闭环项）。

## 一、先核实裁决前提（与源码对照）

| 裁决/附件的主张 | 源码事实 | 结论 |
| --- | --- | --- |
| 打包默认工作区在 `%USERPROFILE%\coolzhuagent`，日志在 `%LOCALAPPDATA%\CoolzhuAgent\logs`，二者不同根 | `config/package-launcher.json` 的 `runtime_dir` / `log_dir` 原样如此 | **成立**（非"巧合"，是随包声明） |
| 缺 `runtime_dir` 时从 `log_dir` 推导（`default_runtime_dir_from_log_dir`） | `lib.rs` 该函数只从 `<...>\CoolzhuAgent\logs\package-launcher` 反推 | **成立**；且在新版配置里只在缺键时触发 |
| "补上正确 runtime_dir 后历史工作区恢复可见" | 工作区由 launcher 通过 `COOLZHU_RUNTIME_DIR` + cwd 传给 web-console，web-console 用它解析 workspace/DB | **成立**：现象与"选错数据源"一致，**不支持**认定数据丢失 |
| launcher 只注入工作区根、数据路径由工作区配置解析 | **不成立**：旧实现还注入了 `COOLZHU_WEB_SESSION_DB/_STORE/_ATTACHMENT_STORE`，在 web-console 中这些**优先于** `coolzhu.toml` 的 `paths.data_dir`/`session.*` | **裁决前提需修正**：旧实现存在**静默忽略工作区配置覆盖**的缺陷，本轮一并修复（P08） |
| 已有用户级"启动设置"可供复用 | core-runtime 的 User 层是 `.claw.json` / `<config_home>/settings.json`（运行特性/MCP/hook 配置），desktop-console 是 `gui-settings.json`（GUI 设置）——**都不是"打开哪个工作区"的启动选择** | 裁决授权的新增最小文件必要且正确 |
| `%LOCALAPPDATA%\CoolzhuAgent` 只放产品状态 | 它同时被 browser bridge 用作 `runtime/browser-bridge-nonce` | 需在"是否有工作区数据"的判定里排除产品状态条目（已修） |

## 二、交付

| 文件 | 内容 |
| --- | --- |
| `packages/app-launcher/src/launch_paths.rs`（新增） | `ResolvedLaunchPaths`、①–⑤ 优先级解析、候选发现（只读）、用户级选择文件的 load/store（revision + 锁 + 原子发布）、配置基线快照、workspace 身份、动作执行 |
| `packages/app-launcher/src/service_identity.rs`（新增） | `/api/system/info` + `/api/diagnostics/health` 回读、8 项复用核对与 `ReuseDecision`、极简 HTTP（支持 chunked） |
| `packages/app-launcher/src/lib.rs`（改） | 配置 schema 版本化与缺键语义分流；`launch_environment` 替代旧的三路径注入；`LaunchSpawner`/`launch` 接收同一份已解析快照；自检 payload 增加 requested/observed 与 notes；7 个新 `LaunchError` 变体 |
| `packages/app-launcher/src/main.rs`（改） | CLI（`--workspace` / `--select-workspace` / `--list-candidates` / `--print-resolved-paths` / `--user-state-dir`）、拒绝路径呈现与自检、复用判定接线、观测值落库、`GetOwner` 归属核对 |
| `config/package-launcher.json`（改） | 增加 `launcher_config_version: 2` 与 `runtime_dir_note`（键名不变） |
| `docs/launch-path-resolution-contract.md`（新增） | 契约与决策记录 |

## 三、端到端实验（隔离在 `tmp/`，未触碰真实用户目录）

1. `tmp/path-01-02-experiment.sh` → `tmp/path-01-02-experiment.txt`：**PATH 解析层**，用 `USERPROFILE`/`LOCALAPPDATA` 指向 `tmp/` 造隔离用户。
   覆盖 P01/P02/P03/P04/P05/P06/P07/P10/P12。
2. `tmp/path-p12-concurrent.sh` → `tmp/path-p12-concurrent.txt`：**真并发**——3 个启动器同时建立选择 ⇒ revision 1、2 连续成功，第三个报 `SelectionConflict`（期望 0/实际 2），旧选择未变、无残留锁与临时文件、**不出现两个矛盾的"已确认选择"**。
3. `tmp/path-e2e-real.sh` → `tmp/path-e2e-real.txt`：**真实拉起 web-console**（debug 二进制，端口 8799，`[pet] enabled=false`），验证
   ① 请求路径 vs 后台实际路径回读；② `paths.data_dir = "agent-data"` 真的生效；③ 复用核对 8 项全 match；④ 换工作区时报告冲突且**既有实例仍存活**；末尾清理进程。

**实验发现并修掉的两个真实缺陷**

1. `probe_workspace_data` 把产品自己的用户级状态（`logs` / `runtime` / `config-snapshots` / `input-safety` / `launcher-user.json*`）
   当成"工作区有数据" ⇒ 全新用户会被误判为"已有工作区"，**不再**在 `%USERPROFILE%\coolzhuagent` 首次初始化。
   修法：`USER_LEVEL_STATE_ENTRY_NAMES` 排除表 + 回归测试 `user_level_state_entries_are_not_workspace_data` / `fresh_user_first_init_ignores_state_only_dirs...`。
2. 本次刚写入的配置快照被当成"旧启动记录"参与候选（自我循环）。修法：`run()` 里把本次快照从候选列表排除。

## 四、门禁

| 项 | 基线 | 本轮 |
| --- | --- | --- |
| `cargo test -p coolzhu-app-launcher --offline` | 26 + 2 = **28** | **63 + 5 = 68**（新增 40：P01–P15 逐条 + 复用 8 项核对 + 选择文件/自检/CLI） |
| `cargo build -p coolzhu-app-launcher --offline` | 0 error 0 warning | 0 error 0 warning |
| `cargo build -p coolzhu-web-console --offline` | exit 0 | 见下（消费者未被破坏：未改任何 `modules/**`，仅环境变量注入方式变化） |
| `cargo build -p coolzhu-web-console --offline`（消费者未被破坏） | exit 0 | **exit 0**（重新编译 18s，非缓存命中） |
| `cargo test --test module_linkage_smoke --offline` | 4/4 | **4/4** |
| `cargo test -p coolzhu-web-console --offline` | —— | **测试目标当前编译失败，但不是本工单造成**：错误在 `computer_use_desktop_bridge.rs`（`StrokeFailure.facts`/`HelperInputFacts` 字段），该文件与 `computer-use-core/src/input_stroke.rs` 在本轮进行中被**另一并发工单**改动（mtime 06:16 / 06:19，本工单只改 `packages/app-launcher/**`）；非测试目标的 `cargo build` 通过 |

（命令输出见 `tmp/path-01-02-tests.txt`；三个真实实验见上表文件。）

## 五、需知悉的行为变更

1. **打包版业务数据路径改由工作区配置决定**：launcher 不再注入 `COOLZHU_WEB_SESSION_DB/_STORE/_ATTACHMENT_STORE`。
   无 `coolzhu.toml` 覆盖时结果与旧实现**完全一致**（`<工作区>\.coolzhu\...`）；存在 `paths.data_dir`/`session.*` 覆盖时**新实现遵循配置**（P08 要求）。
2. **首次启动新用户会建立选择**：`%USERPROFILE%\coolzhuagent` 会被显式创建并落库 `launcher-user.json`（revision=1）。
3. **已在使用 LocalAppData 工作区的用户不会被迁走**：唯一可信候选一次性采用并落库（P03）。
4. 复用既有实例的门槛变严：工作区/用户/协议/构建/数据绑定任一不符即**报告冲突**（此前只要端口健康就转发）。
