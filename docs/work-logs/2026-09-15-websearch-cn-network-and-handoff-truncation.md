# 2026-09-15 WebSearch 国内网络可用性 & 交接消息截断修复

## 本轮范围

外部附件《网络搜索超时与消息截断-问题分析报告》登记的两个问题：

1. `WebSearch` 工具 30s 硬超时、无 stdout/无 result，只返回 `runtime-timeout`。
2. 回复消息疑似被截断（长回复在句子中间戛然而止，发生在跨 agent handoff 交接消息里）。

用户追加：桌面版**在标题栏点击关闭无法退出**（见第五节）。

按 `docs/plans/2026-09-14-chat-tool-observability-backlog.md` 的要求，先重新取证再改代码：
附件里 `search.coolzhu.dev`、网络保护层、截断根因都只是推测，不作为实施依据。本轮**未**按附件
建议硬编码"Bing 抓取降级特例"，而是把后端做成可配置项 + 有限预算。

## 一、取证：附件的第一推断不成立

本机（国内网络）实测：

| 目标 | DNS | TCP 443 |
| --- | --- | --- |
| `search.coolzhu.dev` | 失败（不知道这样的主机） | 未测（无 IP） |
| `html.duckduckgo.com` | 成功（108.160.169.37） | **超时**（443 与 80 均 4s 无响应） |
| `cn.bing.com` | 成功（202.89.233.101） | 27–64ms 可达 |
| `www.bing.com` | 成功（202.89.233.100） | 27ms 可达 |

- `search.coolzhu.dev` 确实解析失败，但**没有任何代码引用该域名**（全仓 grep 仅命中文档）。
  它不是 WebSearch 的后端，附件的主因推断属于误判，不能据此改配置。
- 真实默认后端是 `https://html.duckduckgo.com/html/`（改动前 `lib.rs` 的
  `WebSearchConfig::default`）。该域名 DNS 能解析但 TCP 全端口不通——正是"挂住直到被强杀"的形态。

超时归属（结构性缺陷，与具体后端无关）：

- `WebSearch` 的 input schema 没有 `timeout`/`timeout_ms` 字段，`tool_timeout_ms_for`
  对它恒取策略默认值 **30000ms**（`main.rs` 的 `ConfigToolExecution` 默认值）。
- 工具内部最坏路径 = reqwest 20s（`build_http_client`，且**无 connect_timeout**）+
  PowerShell 降级 30s（`-TimeoutSec 30`）≈ 50s+，**远大于**外层 tokio 工具 deadline。
- 于是工具必然在返回任何可读原因之前被强杀 → `runtime_tool_timeout_outcome` 只给出
  `timeout_ms` 元信息，模型和用户都看不到"哪个后端不可达"。

## 二、修复

### 2.1 WebSearch 后端可配置 + 有限预算（`modules/tooling/packages/tool-registry/src/lib.rs`）

`WebSearchConfig`（lib.rs:796）新增字段，均可在 `coolzhu.toml` 的 `[web_search]` 覆盖：

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `base_url` | `https://cn.bing.com/search?mkt=zh-CN&setlang=zh-CN` | 主后端，改成国内可达站点 |
| `fallback_urls` | `["https://html.duckduckgo.com/html/"]` | 有序降级后端，去重 |
| `attempt_timeout_ms` | 10000 | 单次尝试总超时（含连接与读体） |
| `connect_timeout_ms` | 5000 | 连接超时，避免被墙主机占满预算 |
| `total_budget_ms` | 20000 | 整次调用的内部预算，超出即停止尝试 |

- `execute_web_search`（lib.rs:1234）改为**按后端循环**：逐个尝试，前一个后端抓不到结果才走下一个；
  预算不足时停止并保留逐后端原因。全部后端都抓不到 → 返回带具体 URL、失败原因和配置指引的终态失败，
  而不是空结果或超时。
- `web_search_total_budget_ms()`（lib.rs:891，pub）对外声明工具需要多少时间。
- 新增 `backend` / `notes` 输出字段，记录实际出结果的后端和逐后端尝试记录（便于审计与回放）。
- 解析不再只认 DuckDuckGo：`extract_search_hits_for_page`（lib.rs:1633）按
  DDG `result__a` → Bing `b_algo` → 通用链接 的顺序解析，并过滤搜索引擎自身的导航链接；
  `decode_bing_redirect`（lib.rs:1729）还原 Bing `ck/a?u=a1<base64url>` 跳转包裹。
- `WebFetch` 的 client 也补上了 5s connect_timeout（原来只有 20s 总超时）。

### 2.2 让运行时按工具声明的预算发时间（`modules/gui-web/packages/web-console/src/main.rs`）

`tool_timeout_ms_for`（main.rs:6392）对 WebSearch 取
`max(声明预算 + 5s 收尾余量, 用户配置的 default_timeout_ms)`：
预算调大时不会被 30s 默认值强杀，用户调大默认值时也不会被改小。其它工具行为不变。

补充（提交前自查发现并修正的两处）：

1. **默认不带降级后端**。最初把 DuckDuckGo 设成默认 `fallback_urls`，结果"只配了 `base_url`"
   的场景会隐式多打一次境外站点，而且让已有测试重新依赖真实网络（本机装了 Clash，
   DuckDuckGo 实测真的连通并返回了结果，使一个"必须失败"的测试变成成功）。
   现在默认只有一个后端，需要多后端降级时显式配置 `fallback_urls`。
2. **`tool_timeout_ms_for` 不再改全局 config root**。最初为了读预算在超时计算里调用了
   `tools::set_project_config_root`，这会覆盖调用方（含测试）已指定的 workspace，
   让 WebSearch 测试变得依赖执行顺序。改为新增只读口径 `web_search_total_budget_ms_at(root)`，
   由调用方把自己的 workspace 传进来。

### 2.3 配置往返不再抹掉 `[web_search]`

`save_workspace_config_at` 是整文件重写，而 `WorkspaceConfig` 不认识 `[web_search]`
（它是 tool-registry 的段）。用户手写搜索后端后，只要动一次 dashboard（加允许目录、改任务计划等）
配置就被静默删除——等于 2.1 的可配置性形同虚设。

新增 `render_config_with_preserved_sections`（main.rs:6554）：写盘前把"主控台不认识的顶层段"
从原文件原样带回。`WorkspaceConfig` 字段全部参与序列化，所以"新版里没有的顶层段"就是本进程不认识的段，
判定无需额外白名单。

### 2.4 交接附件截断（问题二）

定位到 `render_handoff_message_excerpt` 走的是通用 `compact_message_snippet(&content, 1200)`：

- `split_whitespace().collect().join(" ")` 把 Markdown 表格和分段**全部压成一行**；
- `chars().take(1200)` 按字符硬切，正文从句子中间断开；
- 截断时执行 `snippet.push('"')`——明显是把省略号写成了英文双引号，接收方会在残句尾部多看到一个引号。

三件事叠加正好复现"含表格的长报告被截断在句子中间"的现象。

改为交接专用 `render_handoff_excerpt`（main.rs:27873，上限 `HANDOFF_ATTACH_EXCERPT_LIMIT = 6000`）：
保留换行与表格结构，超长时显式标注"已截断 / 原文多少字符 / 如何取全文"。
`compact_message_snippet` 保持不变——工具卡和日志摘要的单行压平是刻意的。

### 2.5 输出上限改为按模型能力参数下发（后续追加）

问题二还有第二层：即使不受交接附件影响，长回复也会被上游按 `finish_reason=length` 截断，
因为请求体里的 `max_tokens` 被一个固定天花板压着。

`request_max_tokens_for_limit`（main.rs:31711 附近）原来写死：

```rust
const MIN_TOOL_SAFE_OUTPUT_TOKENS: u32 = 4_096;
const MAX_TOOL_SAFE_OUTPUT_TOKENS: u32 = 16_384;   // ← 无论模型支持多少，最多只发 16384
```

对 GLM-4.6（能力表 200K 上下文 / 128K 输出）、Claude、Grok、DeepSeek 这类模型，
请求只声明 16384 输出 token，长表格/多段报告写到一半就被上游截断。

改为只依据模型能力参数取值，不再有固定天花板：

- 新增 `output_reserve_tokens(context_window, model_max_output)`，把原先内联在
  `context_build_options_for_agent_with_floor_and_room` 里的"输出预留"公式抽成一份，
  上下文预算与请求下发值**共用**，避免两者漂移。
- `request_max_tokens_for_limit` = `min(模型 max_output_tokens, 输出预留)`。
  模型能力是主来源；预留（上限为窗口一半）兜底，保证
  `prompt + max_tokens ≤ 窗口 − 安全余量` 恒成立，不会因为下发大值把窗口撑爆而被 provider 400。
- 小输出模型不被抬高（8192/2048 的本地端点仍是 2048），
  session 级 `[model-limit]` 覆盖继续生效（custom provider 落到默认表时按窗口一半兜底）。

## 三、验收证据

编译与测试：

- `cargo build -p coolzhu-tool-registry --offline`、`cargo build -p coolzhu-web-console --offline` 通过。
- `cargo test -p coolzhu-tool-registry --offline`：43 passed / 0 failed（新增 4 条）。
- `cargo test -p coolzhu-web-console --offline`：929 passed / 1 failed（新增 4 条）。
  唯一失败 `registry_executor_marks_failed_shell_exit_as_failed` 与本轮改动无关且为**预存在**：
  它断言 summary 含 `exit_code` / `not recognized` / `not found`，而中文 Windows 下 cmd.exe 输出
  "不是内部或外部命令"、JSON 字段是 `exitCode`（驼峰），该断言只在英文 locale 下成立。
  本轮 diff 未触及该测试与 bash 工具链路（`git diff` 中该测试名出现 0 次）。
- `cargo clippy -p coolzhu-tool-registry -p coolzhu-web-console --offline`：新增代码无告警（pedantic 开启）。

真实网络端到端（`COOLZHU_RUNTIME_DIR` 指向临时 workspace，`POST /api/tools/runtime-execute`）：

1. **默认后端可用**：`{"query":"rust lang"}` → `status=ok`、`elapsed_ms=265`、
   `backend=https://cn.bing.com/search?mkt=zh-CN&setlang=zh-CN&q=rust+lang`，返回 8 条真实结果
   （含 rust-lang.org、菜鸟教程、知乎、rustwiki 等）。修复前该调用会挂满 30s 后 `runtime-timeout`。
2. **不可达后端给出可读终态**：把 `base_url` 配成被墙的 DuckDuckGo、`transport=reqwest` →
   `status=failed`、`elapsed_ms=5004`，summary 明确指出
   `could not reach any configured search backend within 20000ms. Attempts: <URL>: <原因>`
   并提示改 `[web_search]` 或改用 WebFetch。不再是无原因硬超时。
3. **配置往返**：在同一个 workspace 里调用 `POST /api/tools/allowed-roots` 触发 dashboard 写盘，
   写盘后 `coolzhu.toml` 的 `[web_search]` 段完整保留（修复前会被删除）。

4. **输出上限按模型能力下发**（`tmp/websearch-verify/err.log` 里的 `[LLM-TOOLS] build_request` 诊断行）：

   - 默认（glm-4.6，能力表 200K/128K）：`model=glm-4.6 stream=false max_tokens=100000`。
     修复前这里是 `16384`。100000 = min(128000, 预留 100000)，即窗口一半的预留上限，
     因为 128000 输出 + 任何非空 prompt 已经超过 200K 窗口。
   - session 级覆盖生效：`POST /api/sessions/mario-demo/model-limit {"max_output_tokens":24000}`
     之后同一条诊断变为 `max_tokens=24000`；清空覆盖后回到 128000 的默认能力。
   - `GET /api/sessions/{id}/model-limit` 显示的 `default_max_output_tokens=128000`
     即模型能力表原值，可见"天花板"已完全由模型能力参数决定。

复现命令（PowerShell/Git Bash，注意临时 workspace 隔离）：

```
COOLZHU_RUNTIME_DIR=<tmp-workspace> ./target/debug/coolzhu-web-console.exe
curl -X POST http://127.0.0.1:8765/api/tools/runtime-execute \
  -H "Content-Type: application/json" \
  -d '{"call_id":"verify","tool_name":"WebSearch","input":{"query":"rust lang"},
       "user_authorized":true,"user_confirmed_twice":true}'
```

## 四、Release 打包与安装（0.2.12）

按仓库既有流程 `scripts/build-msi.ps1 -Version <X.Y.Z> -Configuration release`（内部调用
`package-all.ps1` → 逐 artifact 构建 + staging + 安全校验 → WiX 出包 → installer report）。
上一版是 0.2.11，故本轮为 **0.2.12**。

产物：

| 项 | 值 |
| --- | --- |
| MSI | `dist/CoolzhuAgent-0.2.12.msi`，220839681 bytes |
| SHA-256 | `DDB1E4FB03AB732379E2A023C7990F55CB3E2E25B0E749A12AB0BC77EDE3B961` |
| 签名 | unsigned（沿用现状，未签名） |
| 安全校验 | `dist/CoolzhuAgent-0.2.12-package-safety.json`：815 files、safe=true、findings=0 |
| 构建日志 | `tmp/release-0.2.12/build-msi.log`（exit 0） |
| package report | `tmp/package-reports/package-report-release-20260915-010456364.json` |
| WiX | 5.0.2（`tmp/tools/wix`） |
| MSI 属性 | ProductVersion=0.2.12、ProductCode=`{A194FDE0-7208-402C-AE80-23AAA25DE3CC}`、UpgradeCode=`{7873714F-87EE-4DFA-8AAB-2A2402B4ABEA}`、Scope=perMachine、`MajorUpgrade` |

实际重新编译的范围（只编译改动部分）：

- 重新编译：`coolzhu-tool-registry` → `coolzhu-web-console`（含 browser-native-host）、`coolzhu-cli`（build.rs 读
  `COOLZHU_RELEASE_VERSION`，版本号变化必须重建）。
- 未重新编译：vision（`2026-09-03`）、computer-use（`2026-08-23`）、clawbot-sidecar（`2026-08-23`）——
  时间戳与上一版一致。
- `coolzhu-tauri-shell` 仍被重新编译（1m45s，63.0MB → 74.7MB）。原因不是本轮改动，而是
  `tauri.conf.json` 的 `frontendDist: "../ui"`：Tauri 把整个 `ui/` **编译进二进制**，
  而 `ui/assets/launch-b/`（09-06/09-09 新增）有 12MB，正好对应这 11.6MB 增量。
  也就是说桌面壳的"前端 UI"在本项目里是编译期产物，改 UI 必然触发壳重建；
  仓库里并不存在独立的 npm/vite 前端构建步骤，`ui/` 同时以资源形式随包发布。

安装：

- 提权方式 `Start-Process msiexec -Verb RunAs`（perMachine，必须管理员）。
  前两次 UAC 被取消（`The operation was canceled by the user.`），第三次通过。
- 证据：`tmp/release-0.2.12/msiexec-0.2.12-result.json`（pid 18780、`exit_code=0`、
  `uac_cancelled_or_denied=false`）、`msiexec-install-0.2.12.log`
  （日志内同时出现 `ProductVersion = 0.2.10` 与 `= 0.2.12`，即 MajorUpgrade 先卸旧后装新）。
- 安装后核验：注册表 `COOLZHU CODE Agent | 0.2.12`，旧的 0.2.10 条目已由 MajorUpgrade 移除；
   `C:\Program Files\CoolzhuAgent` 下 10 个 artifact 的 SHA-256 与 package report **逐项一致**
   （mismatches=0）。
- 已安装产物的实机验收（隔离 `127.0.0.1:8791` + 临时 workspace，未触碰用户配置）：
  `build_version=457d980f5e73 · 2026-09-14`，`WebSearch` 查询 `rust lang` → `status=ok`、
  `elapsed_ms=272`、`backend=https://cn.bing.com/search?mkt=zh-CN&setlang=zh-CN&q=rust+lang`、8 条真实结果。

⚠️ 可追溯性缺口：本轮构建时工作区有未提交改动（本文件所述修改 + 更早的前端改动），
但 `build-msi.ps1` 记录的 `COOLZHU_GIT_SHA` 取的是 `git rev-parse HEAD`（`457d980`），
所以 `build_version` 里的 SHA 并不对应实际编译的内容。要发布可追溯的版本，需先提交再重新打包。

## 五、标题栏点击关闭无法退出（用户追加）

### 5.1 复现与定位

装好的 0.2.12 上点控制台窗口标题栏的关闭按钮：**窗口消失，但 `coolzhu-tauri-shell`、
`coolzhu-web-console` 和 `coolzhu-browser-native-host` 三个进程全部存活**，8765 端口的 agent
继续驻留——用户看到的就是"点了退出却没退出"。

原因在 `build_console_window`（`modules/gui-desktop/.../src-tauri/src/main.rs`）：
窗口的 `CloseRequested` 被改写成了

```rust
api.prevent_close();
console_for_close.hide().ok();
```

即"拦截 + 隐藏到托盘"，窗口关闭语义被彻底占用，X 永远不可能退出应用。仓库里没有任何测试
把这一行为钉成预期（既有的 `hide_pet` 测试只约束托盘菜单的「隐藏桌宠」）。

### 5.2 修复

- 关闭改为真正退出：`prevent_close()` 后调用与桌宠关闭、托盘退出同一个 `quit_application`。
  隐藏到托盘的能力保留（托盘菜单「隐藏控制台」+ 托盘左键切换），不再借用窗口关闭语义。
- `quit_application` 增加进程退出兜底：`app.exit(0)` 只是向事件循环投递 `RequestExit`，
  实测在该调用路径下事件循环并未因此退出——taskkill 已成功回收 web-console（日志可见
  `成功: 已终止 PID 8904`），但 Tauri 进程与窗口仍在。补 `std::process::exit(0)` 之后，
  标题栏关闭 / 托盘退出 / 桌宠关闭三条路径都拿到确定的退出结果。
- 新增回归测试 `console_window_close_quits_instead_of_only_hiding`：断言关闭路径调用
  `quit_application`、**不出现** `.hide()`，且 `quit_application` 的兜底退出在 `app.exit` 之后。
  该测试在修复前必然失败。

### 5.3 GUI 验收

在隔离 workspace（`[pet] enabled=false`，避免 web-console 自己拉起桌宠 shell 造成单实例干扰）
下启动 web-console + 打完补丁的 tauri-shell，用桌面自动化点击窗口关闭按钮：

| | 修复前（0.2.12 已安装产物） | 修复后 |
| --- | --- | --- |
| 窗口 | 消失（hide） | 关闭 |
| `coolzhu-tauri-shell` | **存活** | 退出 |
| `coolzhu-web-console` | **存活** | 被回收 |
| 8765 agent | **继续驻留** | 结束 |

排查插曲：最初带 `--web-console-pid=` 启动时壳总是秒退（exit 0），一度以为是新 bug；
实际是 web-console 自己按 `[pet]` 配置拉起了桌宠 shell 成为"第一实例"，
我手工启动的进程被 `tauri-plugin-single-instance` 转发参数后正常退出。关掉 `[pet]` 后复现稳定。

## 六、未闭环 / 后续建议

1. **`finish_reason == "length"` 仍未被消费**（问题二的最后一条候选链）：
   固定 16_384 天花板已在 2.5 移除，撞长度的概率大幅下降，但"撞上了也不告诉用户"这一点还在——
   流式循环里 `StreamEvent::MessageDelta` 只取 `usage`、丢弃 `delta.stop_reason`（main.rs:16988），
   全仓没有 `"length"` 分支。模型因长度被截断时，用户看到的是"正常结束但少了一半"，没有任何提示。
   建议：读 `stop_reason`，命中 `length` 时在消息上标注或续写，并纳入 TODO-CHAT-OBS-003 的 turn 元信息。
2. **流中断会被当正常完成落盘**：LLM 请求 600s 超时或流错误时只记 `diagnostic_note` 并保留残文，
   聊天区不提示"被截断"。建议与第 1 条一起处理。
3. **`[web_search]` 未写进自动生成的默认 `coolzhu.toml`**：目前只在首次手写时出现，
   可发现性依赖失败信息里的指引。若要暴露给前端设置面板，需把该段纳入 `WorkspaceConfig`
   （或在设置页增加只读展示），本项目前按"tool-registry 自有段 + 原样保留"处理。
4. `registry_executor_marks_failed_shell_exit_as_failed` 的断言依赖英文 locale，
   建议改为断言 stderr 非空或 `exitCode != 0`，否则 CI 在中文 Windows 上必然红。
