# 启动路径解析契约（PATH-01）与既有工作区保留/恢复（PATH-02）

状态：**已实现**（第四轮裁决第四节 + 第五节，2026-09-25）。
范围：`packages/app-launcher/**`、`config/package-launcher.json`、本文件。
**未改动**：任何 `modules/**` 下的 Rust 文件、`scripts/package-all.ps1`、`config/package-manifest.json`。

---

## 1. 目录分类（先固化术语）

| 类别 | 定义 | 实际位置（打包默认） | 是否存用户数据 |
| --- | --- | --- | --- |
| 安装根 | 程序、随包资源与产品默认配置 | `%ProgramFiles%\CoolzhuAgent`（由 MSI 决定） | **不存**用户会话和秘密 |
| 工作区根 | 用户当前**明确选择**的工程/配置根 | `%USERPROFILE%\coolzhuagent`（**仅是"没有既有有效选择的新用户"首次建立工作区时的默认建议**） | 存（工作区自己的 `coolzhu.toml`、代码、数据） |
| 业务数据根/数据库 | 会话库、附件、兼容 JSON、审计日志 | 按**工作区配置**解析：`<工作区>/coolzhu.toml` 的 `paths.data_dir`（默认 `.coolzhu`）+ `session.db_path` / `session.json_path` / `attachment.store_dir` 覆盖 | 存 |
| 普通诊断日志根 | launcher 与各子进程的 stdout/stderr | `%LOCALAPPDATA%\CoolzhuAgent\logs`（由 `log_dir` 配置） | 只存日志 |
| 用户级启动选择 | 用户选择了哪个工作区 | `%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json` | 只存选择（**不复制**模型参数/Base URL/密钥/整份 `coolzhu.toml`） |
| 跨工作区输入安全状态 | 桌面输入隔离/释放义务的共享状态 | `%LOCALAPPDATA%\CoolzhuAgent\input-safety`（用户级共享） | 存（RPR-05 实装后由该组件落盘） |

**裁决要求的不变量**

1. **不要求同根**：工作区可在用户选择的任意位置，诊断日志默认在 LocalAppData —— 允许，但**来源、用途与实际路径必须公开呈现**（见 §5 的 `ResolvedLaunchPaths` 与启动日志）。
2. **不通过删除 `runtime_dir` 来"统一目录"**：删除只改变选源，不迁移已有数据；因此"删掉键就会回到 LocalAppData"**不是**正式修复建议。
3. **不再让日志目录变化隐式改变新版本的工作区**：`log_dir` 只影响日志路径与"旧版缺键时的迁移候选"，不参与工作区选择（`ResolvedLaunchPaths.per_process_log_paths` 与工作区字段完全解耦）。
4. **本轮不改名 `runtime_dir`**：配置键名保持不变，其**现行含义**在本文件与 `LauncherConfig` 文档注释中固化；后续改名另做兼容迁移。
5. **用户级选择不随 MSI 覆盖**，**不从 `log_dir` 反推**（否则改日志设置会移动启动选择的权威位置）。

### 1.1 `runtime_dir` 的现行含义

```jsonc
{
  "launcher_config_version": 2,
  "log_dir":    "%LOCALAPPDATA%\\CoolzhuAgent\\logs\\package-launcher",
  "runtime_dir":"%USERPROFILE%\\coolzhuagent"   // = 工作区默认建议值（仅首次初始化）
}
```

- `launcher_config_version >= 2`：`runtime_dir` 是**工作区默认建议值**，只用于"尚未建立有效选择"的首次初始化。**缺失 / 空值 / 未解析变量 / 非法路径 = 配置错误**，不会从 `log_dir` 推导。
- `launcher_config_version == 1`（或没有该键的旧包配置）：保留可识别的兼容解析。缺键时从 `log_dir` 推导的结果只作为**迁移候选/迁移信息**（见 §4 的 ⑤），**不再是新版本常规的隐式数据源**。

---

## 2. 冻结的解析优先级（①–⑤）

实现在 `packages/app-launcher/src/launch_paths.rs` 的 `resolve_launch_paths`。

| 优先级 | 来源 | 语义 | 实现位置 |
| --- | --- | --- | --- |
| ① | `CandidateSource::UserSpecified` | 本次明确的用户/控制面选择（`--workspace`）：**只影响声明的本次启动**，不隐式扩展 | `launch_paths.rs` `resolve_launch_paths` 第 ① 段 |
| ② | `CandidateSource::SavedSelection` | 已持久保存的用户级选择（正常启动默认使用；MSI 升级不得覆盖） | 同上第 ② 段 |
| ③ | `CandidateSource::ImportedLegacySelection` / `ConfigSnapshot` / `KnownHistoricalDefault` | 可确认的旧版用户覆盖或旧有效选择（按升级规则**一次性导入**，保留来源与旧值） | `adopt_single_legacy_candidate` 分支 |
| ④ | `CandidateSource::PackagedDefault` | 随包 `runtime_dir` 默认值，**仅用于尚未建立选择的首次初始化** | `first_init_packaged_default` 分支 |
| ⑤ | `CandidateSource::LegacyLogDirDerivation` | 兼容支路：**仅旧版配置缺键**时从 `log_dir` 推导，只作迁移候选 | 最后的 `WorkspaceSelectionRequired` 分支 |

**核心约束（拒绝路径）**：**高优先级来源无效时，不自动落到低优先级**——否则"用户磁盘暂时断开"会再次变成"系统创建了一个新空工作区"。

`ResolutionSource`（自检/日志里的实际来源）：`explicit_this_launch` / `persisted_user_selection` / `imported_legacy_selection` /
`packaged_default_first_init` / `packaged_default_existing_data` / `legacy_config_declared_default`，并带 `revision` / `origin` / `config_key` 细节。

### 2.1 采用规则（③/④ 的判定表）

| 已保存选择 ② | 含数据的可信候选（③） | 随包默认值位置 ④ | 行为 |
| --- | --- | --- | --- |
| 有效 | — | — | 用 ②（**不写盘**，不动配置） |
| 无效（缺失/不可写/不是目录/无法判定） | — | — | **阻断**：`WorkspaceUnavailable{saved_user_selection}` + 修正入口 |
| 无 | 恰好 1 个 | 无数据 | **一次性采用**该候选（保留 origin）并落库 |
| 无 | ≥ 2 个 | — | **阻断**：`WorkspaceSelectionAmbiguous`（展示全部候选，不按时间/容量/数量挑、不合并、不删除） |
| 无 | 无 | 已有数据 | 沿用该位置（`packaged_default_existing_data`）并落库（**不另建空库**） |
| 无 | 无 | 不存在 | **明确首次初始化**（显式创建 + 落库 + 打印；P01） |
| 无 | 无 | 配置缺键（v2） | **配置错误**（P07） |
| 无 | 无 | 配置缺键（v1） | ⑤：输出推导结果作迁移信息 + 要求 `--select-workspace` 确认一次 |

**候选发现只用可信来源**：已保存选择、已知历史默认目录（`<log_dir 推导出的 LocalAppData 根>\CoolzhuAgent`）、可信旧启动记录（配置快照）。**不为找数据库扫描整个用户磁盘**。
候选检查**不创建目录、不打开数据库、不升级 schema、不写默认配置**（`resolve_launch_paths` 是纯函数；写盘集中在 `apply_resolution_actions`）。

### 2.2 用户级状态条目不算"工作区数据"

`%LOCALAPPDATA%\CoolzhuAgent` 同时是"已知历史默认工作区"与用户级状态位置，所以
`probe_workspace_data` **排除** `logs` / `runtime` / `config-snapshots` / `input-safety` / `launcher-user.json*`
（`USER_LEVEL_STATE_ENTRY_NAMES`）。否则全新用户会被误判成"已有工作区"，从而不再在 `%USERPROFILE%\coolzhuagent` 首次初始化。
这条是端到端实验（P01）里实际发现并修掉的缺陷，见 `tmp/path-01-02-experiment.txt`。

---

## 3. 用户级选择的落点、revision 与原子发布

- 文件：`%LOCALAPPDATA%\CoolzhuAgent\launcher-user.json`（`launcher_config_version` 无关；**不从 `log_dir` 反推**）。
- schema：`{schema_version, revision, selection{workspace_root, workspace_id, source, recorded_at_ms}, legacy_selections[], observations{}}`；
  `observations` 是**非权威观测值**（上次后台构建版本、上次 launch id、上次工作区、上次观测到的会话库），只服务复用判定，不参与选择优先级。
- **只存选择**：不写入模型参数 / Base URL / 密钥 / `coolzhu.toml` 内容（有单测断言）。
- 更新：`store_user_state(user_state_root, state, expected_revision)`
  1. 取跨进程锁（`launcher-user.json.lock`，`create_new` 原子占位，2s 超时即报 `SelectionLocked`）；
  2. 重读盘上 revision，**不匹配即 `SelectionConflict{expected, actual}`，不写盘**；
  3. 写 `launcher-user.json.tmp` → `sync_all` → `rename` 覆盖（原子发布）；
  4. 返回新 revision，**只有返回 Ok 之后调用方才打印"已保存"**。
- 显式更换工作区时，旧值以 `legacy_selections[]`（带 `origin=replaced_by_explicit_select@rN`）保留，不静默丢弃。
- 冲突/失败时**保持旧选择**（不在内存里先宣布切换成功）；损坏/未知 schema 的选择文件 → 明确报错（`SelectionStore`），**不静默忽略**。
- `--user-state-dir <绝对路径>` 只用于隔离测试/受控运维（不是第二条选源）。

## 3.1 配置基线（升级前保留旧启动配置）

每次启动把当前 `package-launcher.json` 的原文按内容哈希留一份到
`%LOCALAPPDATA%\CoolzhuAgent\config-snapshots\launcher-config-<hash>.json`（含 `recorded_at_ms`、`config_path`、`declared_runtime_dir` 原值），最多保留 16 份。
作用：① 提供"上一版随包配置声明的 `runtime_dir`"作为 ③ 的可信旧记录；② 当声明值发生变化时打印
`packaged_config_workspace_changed`（**只报告，不动作**）：升级不得因此迁移或新建工作区。
**本次刚写入的快照会被排除在候选之外**（它不是"旧启动记录"）。

---

## 4. 在真实用户启动时解析路径

- 随包配置保留 `%USERPROFILE%` / `%LOCALAPPDATA%` **模板**，在**实际运行用户**的目录上下文解析；
  **不在构建机上替换成开发者绝对路径**（`expand_os_env_placeholders` 只做运行时展开）。
- 命令行工作区路径同样按运行用户上下文展开（`expand_user_path_text`）；变量未解析 ⇒ **明确报错**，不落回当前目录/临时目录。
- 模板未解析、用户目录不可取得、目标无权限 ⇒ **可理解的启动错误**（`WorkspaceUnavailable` / `SelectionStore`），**不回退到当前目录或系统临时目录创建业务数据库**。
- 含空格/中文的路径**按路径参数传递**（`Command::arg`），不拼进 shell 命令字符串。
- 由提权 MSI 安装账号**不预先创建**该账号下的用户工作区：选择由各用户在**各自用户上下文**首次建立（`user_state_root_for` 只看 `%LOCALAPPDATA%`/`%USERPROFILE%`）。

## 5. `ResolvedLaunchPaths`：启动只解析一次，子进程共享

`packages/app-launcher/src/launch_paths.rs`（结构定义）——这是"本次启动已解析结果的快照"，**不是新增一套业务配置权威**。

| 字段 | 含义 |
| --- | --- |
| `launch_id` / `package_identity` / `config_schema_version` | 本次启动与包身份 |
| `selection_revision` / `workspace_id` | 用户级选择 revision / 稳定 workspace 身份（`ws-{:016x}`，与 web-console 的 `workspace_identity` 同构；目录不存在时为 `None`） |
| `workspace_root` | 工作区根（**请求的路径**） |
| `data_dir_binding` | 业务数据根绑定方式：`workspace_config_authority`（launcher 不注入覆盖） |
| `requested_session_db` | 请求固定的库路径；**恒为 `None`**（由工作区配置决定） |
| `observed_workspace` / `observed_session_db` / `observed_build_version` | **后台实际使用的**路径与构建（健康就绪后回读） |
| `user_state_root` | 用户级选择根 |
| `input_safety_state_root` | 跨工作区输入安全状态根（**不随工作区变化**） |
| `per_process_log_paths` | 各子进程实际日志位置 |
| `resolution_source` | 实际生效来源（①–⑤） |

**共享方式**：`launch_environment(resolved)` 注入 `COOLZHU_RUNTIME_DIR` / `COOLZHU_LAUNCH_ID` / `COOLZHU_USER_STATE_ROOT` /
`COOLZHU_LAUNCH_RESOLUTION_SOURCE`；launcher、web-console、Tauri 与必要子进程用**同一份**结果，
不能各自根据 cwd / 日志目录 / 全局当前选择重新猜。

### 5.1 业务数据根不再被 launcher 覆盖（P08 的修复）

旧实现由 launcher 注入 `COOLZHU_WEB_SESSION_DB` / `_STORE` / `_ATTACHMENT_STORE` = `<runtime_dir>\.coolzhu\...`，
在 web-console 里这些环境变量**优先于**工作区配置 ⇒ `paths.data_dir` 之类的覆盖被**静默忽略**。
现在：

- 只注入工作区根；业务数据路径由工作区配置解析（`workspace_config_authority`）。
- 父环境里继承来的三个覆盖变量会被**显式清除**并在启动日志/自检里记录（`inherited_data_path_override`），不静默忽略。
- 自检的 `observed.*` 段如实记录后台实际路径，并给出 `data_dir_override_observed`：
  **只打印配置值不足以证明生效**——端到端实验已实证 `paths.data_dir = "agent-data"` 真正生效（见 `tmp/path-e2e-real.txt`）。

---

## 6. 复用判定的身份核对（裁决第六节）

`packages/app-launcher/src/service_identity.rs`。复用既有后台服务**不能只核对端口健康**，至少核对：

| 核对项 | 期望 | 来源 | 不匹配 |
| --- | --- | --- | --- |
| `health` | ready | `/api/diagnostics/health` | 未健康 ⇒ 正常启动 |
| `listener_process` | `coolzhu-web-console.exe` | 监听者归属（`Get-NetTCPConnection` + `Win32_Process`） | **冲突**（不杀进程） |
| `instance_owner_user` | 本进程用户 | `Win32_Process.GetOwner()` | **冲突**（他人实例） |
| `protocol_shape` | `/api/system/info` + `/api/diagnostics/health` 形状完整 | 两个只读接口 | **冲突**（构建/协议不兼容） |
| `build_identity` | 上次本机记录的后台构建版本 | `observations.last_background_build_version` | 不同 ⇒ **冲突**；无记录 ⇒ `unverified`（记录并放行） |
| `workspace_canonical_path` | 本次解析的工作区 | `/api/system/info.workspace` | 不同/取不到 ⇒ **冲突** |
| `session_db_binding` | 上次同工作区观测到的库 | `/api/diagnostics/health.paths.session_db` | 不同 ⇒ **冲突** |
| `launch_state` | 仅记录 | `summary.status` / `active_sessions` | —— |

- 不匹配 ⇒ `LaunchError::ServiceConflict`：**报告冲突、不静默连接、不按端口杀进程**（实测既有实例在冲突后仍存活）。
- 判定结果写入自检 `reuse_verification`（含每项的 expected/observed/verdict）。
- 端口回收只对**既有的**孤儿/已知持有者（`llama-server.exe` / `conhost.exe`）生效，语义未变。

## 6.1 "工作区不见了"的恢复流程（PATH-02）

**优先恢复入口，不优先搬数据。**

| 场景 | 行为 |
| --- | --- |
| 已保存选择有效、另有别的默认目录 | 用已保存选择；默认值变化只打印 `packaged_config_workspace_changed` |
| 已保存选择暂时不可访问 | **停在明确错误 + 恢复入口**，不创建替代工作区（实测） |
| 无保存选择但有可信旧配置归属 | 唯一候选 ⇒ 一次性采用（保留 origin）并落库；≥2 个 ⇒ 分别展示、要求用户显式选择 |
| 两个历史默认目录都有数据 | 阻断 + 分别展示，**禁止自动合并或取"最新"**（实测两份数据都保留） |
| 只有一个已知合法候选但无可靠历史选择 | 一次性采用，且**不把"唯一发现"解释成"全磁盘唯一"**（候选集限定在可信来源内） |
| 无候选 | 提供默认路径与明确的首次初始化（P01） |
| 曾误建空目录但后来已写入新消息 | 它**也是用户数据**：不自动删除或覆盖（P13；恢复旧目录后两份都保留） |
| `runtime_dir` 正确但 `paths.data_dir` 指向别处 | 按实际配置解释并在自检里如实呈现覆盖，**不静默忽略覆盖来制造"恢复成功"** |

命令面（`COOLZHU-AGENT.exe`）：

```
--workspace <绝对路径>         仅本次启动使用（①）
--select-workspace <绝对路径>  校验并持久化为用户级选择（revision + 原子发布）后退出
--list-candidates              只读列出候选与来源（不启动、不写盘）
--print-resolved-paths         打印本次启动路径快照（JSON）后退出——**解析动作仍会执行**
--user-state-dir <绝对路径>    隔离测试/受控运维（默认 %LOCALAPPDATA%\CoolzhuAgent）
-h, --help
```

---

## 7. P01–P15 实现状态（裁决第八节）

| 编号 | 状态 | 证据 |
| --- | --- | --- |
| P01 全新用户首次启动 | **已实现并跑过** | 单测 `p01_fresh_user_initializes_packaged_default_without_inheriting_installer_context`、`fresh_user_first_init_ignores_state_only_dirs...`；隔离用户实验（首次初始化 + `revision=1`） |
| P02 既有自定义工作区升级 | **已实现并跑过** | 单测 `p02_persisted_custom_workspace_survives_upgrade_and_keeps_config_untouched`；实验（`coolzhu.toml` 字节不变、选择 revision 1→2） |
| P03 旧版 LocalAppData 工作区 | **已实现并跑过** | 单测 `p03_legacy_local_app_data_workspace_is_not_abandoned_for_new_default`；实验（采用 LocalAppData、未新建 `%USERPROFILE%\coolzhuagent`） |
| P04 已选磁盘断开/不存在/权限拒绝 | **已实现并跑过** | 单测 `p04_*`（缺失 + 注入只读）；实验（`Z:/disconnected/workspace` 阻断、退出码 1、无替代目录、自检落盘） |
| P05 两个历史目录都有数据 | **已实现并跑过** | 单测 `p05_...`；实验（阻断 + `--list-candidates` 展示 + 两份数据均保留） |
| P06 修改 `log_dir` | **已实现并跑过** | 单测 `p06_...`；实验（工作区/数据绑定/输入安全根不变，仅日志路径变） |
| P07 缺键语义（新/旧配置） | **已实现并跑过** | 单测 `v2_config_requires_explicit_runtime_dir_and_rejects_newer_schema`、`v1_config_with_runtime_dir_keeps_declared_workspace_as_legacy_origin`、`p07_...`；实验（v2 缺键 ⇒ 配置错误） |
| P08 `paths.data_dir` 覆盖 | **已实现并跑过（真实端到端）** | 单测 `p08_...`；**真实拉起 web-console**：`data_dir=agent-data` 真正生效、`data_dir_override_observed=true` |
| P09 后台健康但绑定另一工作区 | **已实现并跑过（真实端到端）** | 单测 `p09_...`；真实实例上换工作区 ⇒ 冲突、退出码 1、**既有实例仍存活** |
| P10 中文/空格/链接/未解析变量 | **已实现并跑过**（别名/链接未单独验证） | 单测 `p10_...`；实验（中文+空格可用；相对路径与 `%NOPE%` 明确报错，不落回 cwd） |
| P11 提权安装 + 多用户 | **已实现并跑过（注入式）** | 单测 `p11_...`（各自用户上下文、互不可见）。**真实多用户装机未测**（无第二用户环境） |
| P12 保存中断 / 并发改选择 | **已实现并跑过（真并发）** | 单测 `p12_...`（冲突/revision/损坏文件）；实验：3 个启动器并发 ⇒ revision 1、2 连续成功，第三个**报 revision 冲突**、旧选择未变、无残留锁/临时文件、无矛盾选择 |
| P13 误建目录 + 恢复旧目录 | **已实现并跑过** | 单测 `p13_...`（阻断、两份数据保留、恢复入口不删误建目录） |
| P14 输入隔离不随工作区重置 | **已实现并跑过（弱保证）** | 单测 `p14_...`（`input_safety_state_root` 跨工作区恒定、不落在工作区内）。存储组件未实装 ⇒ 只有路径契约保证，见 §8.2 |
| P15 MSI repair/升级/卸载再装 | **已实现并跑过（部分）** | 单测 `p15_...`（选择与配置基线在安装根之外、快照去重）；**MSI 行为未实测**（不改 installer/脚本范围） |

## 8. 未闭环 / 受限

1. **物理迁移不在本轮**：没有实现 M1–M7 搬迁；`select_workspace` 只改选择，不动数据。
2. `input_safety_state_root` 是**声明的契约路径**（`<user_state_root>\input-safety`）：RPR-05 的输入安全存储尚未实装，
   实装时必须使用该位置（用户级共享、不随工作区移动），否则 P14 只有"路径不变"的弱保证。
3. 复用核对依赖 web-console 的两个只读接口（`/api/system/info`、`/api/diagnostics/health`）：两者不暴露构建/工作区时，
   复用会被判为冲突（**fail-closed**），用户需重启实例。**未修改 `modules/**`**，所以没有给后台加新的身份接口。
4. `--print-resolved-paths` 会执行解析动作（首次初始化 / 选择落库），不是纯 dry-run。
5. 未签名的 MSI 与"卸载再装"路径未实测（P15 只验证了"选择与配置基线在安装根之外"这一必要性质）。
6. **配置基线快照有"从本版本起才有效"的边界**：旧版启动器不写快照，所以**本版本之前的最后一次升级**没有
   "上一版随包配置"的机器可读记录；那一次过渡仍依赖磁盘上的已知历史默认目录（`%LOCALAPPDATA%\CoolzhuAgent`）
   与用户选择文件来避免误判。从本版本起，每次启动都会先留快照，后续升级的旧基线可查（P02/P03 的长期保障）。
7. `MSI` 侧仍按 `config/package-launcher.json` → `config/package-launcher.json` 原样复制（未改 `package-manifest.json`），
   所以"安装期不覆盖用户选择"靠的是选择文件位于 `%LOCALAPPDATA%`（安装根之外），而不是靠安装脚本动作。
