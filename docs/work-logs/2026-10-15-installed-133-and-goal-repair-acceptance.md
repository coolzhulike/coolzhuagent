# 2026-10-15 安装版、Goal 回退链与 C 盘只读验收

本轮由 Devin Desktop 在 `D:\coolzhuagent` 直接执行。本地 HEAD 为 `8a13551b037f5f673188802f2c9128984245fab8`，接手时工作区已有大量未提交修改，本轮未回退它们，也未使用旧 321 驱动或 336 夹具。

原始证据根为 `tmp/goal-32wbs-acceptance-20261013/`。本轮构建与适用完整测试均已结束；结果以实际退出码、HTTP、数据库只读快照与文件哈希为准。关键收据和截图归档到 `docs/testing/2026-10-15-acceptance-evidence/`，其清单记录逐文件 SHA-256。

## 已核实的接手状态

- 0.2.133 已注册安装，安装根为 `C:\Program Files\CoolzhuAgent`。接手时 8765 的 PID 为 28944，仍运行临时 target 中的旧候选；安装版 Tauri 连接了这个后台。
- 正式工作区由 `/api/workspace` 核实为 `D:\coolzhuagent`；正式库为 `D:\tmp\.coolzhu\web-sessions.sqlite3`。`/api/diagnostics/health.paths.workspace` 使用 `codex_project_root()`，不能用它判断嵌套夹具的实际工作区。
- 第 335 轮完整测试确实为 **1517 passed / 0 failed / 6 ignored，Cargo exit 0**。接手时核对的 `main.rs` 与 `Cargo.lock` 哈希仍与该收据一致。不能继续把第 328/333 轮失败写成该源码的最新结果。
- 第 358 轮作业 `d937a25ae0624e94abf640e7e9bfdfd8` 实际为 **failed / exit 1**，失败原因是 launcher 产物路径缺失。随后成功出包属于独立报告 `pkg-report-release-20261015-085414907-b86a8314`。
- 第 359 轮的模型与只读工具烟测已有独立审计；计费归属仍未证明。第 360 轮在 UIA observation 阶段 `target_not_found`，0 输入、0 截图，不能计动画通过。

## 本轮新增安装证据

- 安装目录 **1222/1222 文件**与正式载荷清单逐项长度和 SHA-256 匹配；MSI SHA-256 为 `b9674a7e30936d04ca9da6158bb09057fb1a30cf80beefe4f71cc0466e183ee5`，仍未签名。
- 使用安装目录 EXE、独立 29421 端口/数据库，在没有源码静态树的工作目录、不设置 `COOLZHU_WEB_STATIC_ROOT` 的条件下，**513/513 个 Web 静态文件 HTTP 200 且哈希匹配**。重启后首页、JS、图标和头像再次匹配，测试会话的 ID、模型、显式头像字段保留。随后正式8765冷启动也完成了4类资源哈希复核。
- 两次早期夹具前置检查失败均保留：默认 pet 自动启动会触发 Tauri 单实例联动；健康接口的工程根不是实际工作区。修正夹具后才执行静态与持久化断言。
- 真实 Windows 桌面、144 DPI 下取得首次 11 帧、日常 6 帧。关键帧 `controller-installed-133-first-09.png` 与 `controller-installed-133-daily-04.png` 可见 COOLZHU 下方居中静坐人物，未见旧印章或左下舞剑。
- 原生日志确认首次 `completed`、日常 `completed`，各一次交接，`console_visible=true`；`--show-console` 恢复窗口未重播演出。
- 减少动态实机路径完成，页面报告 `reduced-motion=true`、`reason=reduced-motion`、130 ms；系统动画首选项已恢复为原值。早期 Esc 未确认前台、UIA 按钮未找到及交接超时记录保留。正式 D 冷启动补足 Esc：核定安装进程与前台 HWND 后投递，原生日志唯一 `finished=skipped`，随后 `console_visible=true`，exit 0。代表帧 `controller-installed-133-cold-d-03.png` 已目视核；资源故障原生交接仍缺，EX-03不关闭。

首次经正常安装入口启动时，发现用户级保存选择仍为 `C:\Users\zhupu\coolzhuagent`，因此打开了另一份现有库；这次验收 **exit 1**，没有把错误工作区判成通过。通过启动器官方 `--select-workspace D:\coolzhuagent` 持久选择 D 后，再经默认入口冷启动，后台 PID **33632**、Tauri PID **34004** 均来自安装目录，工作区回到 D、数据库仍为原 `D:\tmp\.coolzhu\web-sessions.sqlite3`。这不是直接改 JSON/SQL 状态，也没有删除 C 用户目录的数据。

正式库一致性备份已通过完整性检查。`controller-installed-133-history-review.json` 与测试后的 `controller-installed-133-history-after-tests.json` 对11张表逐行比较，均 **0 增删改**：消息2336、工具6060、运行828、运行事件9130、ACP attempts1352、bindings9、goals4、phases34、goal events41、CU runs352、memory beads158。配置字节也不变；历史 `fair-amaryllis` unknown锁保留，未伪造解锁。`controller-installed-133-d-hydrated-console.png` 可见原第360轮回复、gpt-6-luna-max标签、头像与权限文案，不能外推全部历史身份或混合DPI。

最后补齐鼠标退出路径：`controller-installed-133-click-button.json` 与 `controller-installed-133-click-area.json` 分别经正常安装入口，在无在途任务、核定窗口/前台/实际点击点归属后，真实点击跳过按钮和演出区域。两项均exit **0**，各有唯一 `finished=skipped` 和唯一可见控制台交接。最终正式后台 PID **35284**、Tauri PID **7140**；`controller-installed-133-history-after-clicks.json` 再次核11表和配置无变化。Esc、按钮、区域三条退出路径均已实际执行，未用JS伪造完成事件。

## Goal 新反例与修复

新夹具 `desktop-goal-repair-chain` 使用独立 29422/29423 端口、数据库、输入安全根与 LocalAppData。模型服务只返回确定性的工具请求，不直接写验收产物；真实宿主 `write_file` 在 implementer 的 run 内完成写入，并留下 `tool_calls.status=completed`。

安装版反例为：FilesExist 拒绝缺失产物 → retry_count=1、转 implementer → 真实工具写出准确内容 → 转回 verifier。第三次派发被 `cycle detected in handoff chain` 拒绝，Goal没有完成。这份 exit 1 反例摆脱了旧30秒工具树截断，保留在 `controller-goal-repair-chain-20261015.json`。

本轮对 `main.rs` 做定点修复：普通手工/模型交接仍走原防环门；只有持有当前 durable claim、工作区/房间/目标/commander 和冻结阶段匹配、失败预算与 Goal 迭代预算有效的调度器回退交接，才允许再次到访角色。自交接、深度、去重窗口和总交接配额继续先行拒绝。公开 payload 中伪造 retry 契约不能取得这一资格。

扩展已有回归，要求通过正式 dispatch 返回 verifier，并检查伪造契约、错误房间、错误目标和陈旧 claim 被拒绝。`controller-goal-fix-build-20261015.json` 已完成：离线 build exit **0**，回归 **1过/0败/0忽略**。

修复候选另用新库、新目录完成连续 HTTP 链，见 `controller-goal-repair-chain-fixed-20261015.json`：**拒绝 → implementer真实write_file → 原verifier复验pass → Goal completed**，retry_count仍为1；三个run依次failed/completed/completed，4次模型HTTP请求，driver未代写artifact、未SQL改状态。Goal为 `goal-1792089379340-4932963d`，artifact SHA-256 `b17d197b0d78ef1ba543f3e1638f26fe9555da86e3c3bbcbaae12c042e0fea51`。

模型决定来自受控本地夹具，这项证明宿主回退/真实工具/确定性验收机制，不证明生产模型自主修复。候选二进制 SHA-256 为 `4d51b1f61993bee3381dd0687d4a8e044493647b7e2ad83cbbb0a64ebce37e2a`，当前 `main.rs` SHA-256 为 `aeb17997621668388b99591a3b2cb0211529ade78a7b2fcfa34f50132eefdb09`。**修复尚未进入已安装0.2.133**，不能把候选结论套到正式安装版。

## 完整测试与只读轨迹

唯一完整作业 `controller-console-full-20261015.json` 已完成，Cargo exit **0**，源码前后哈希相同，耗时约2164秒。命令为：

```powershell
cargo test -p coolzhu-web-console --offline -j 1 --target-dir D:\coolzhuagent\tmp\goal-32wbs-acceptance-20261013\cargo-target -- --test-threads=1 --skip wechat
```

这是 **test/debug** 适用完整套件：主套件 **1488过/0败/6忽略，微信26过滤**；另有 **3个子进程探针通过**，沿335轮的所有调用口径共 **1491过**。`controller-console-full-review-20261015.json` 独立区分主套件与子探针；原始汇总中的4556 filtered包含子进程自己的4530精确筛选，不能误报成微信排除数。原始收据与完整日志保留，EX-01按用户排除范围验收；不宣称当前候选release或MSI已经验证。

`controller-real-trace-readonly-20261015.json` 为正式服务的真实第360轮run做4项只读检查：指定run唯一返回、2工具与SQLite一致；跨房间、跨工程和缺失run均404。源文件与安装文件哈希一致，但消息按钮只调用无参数 `openTraceArchive()`，打开通用归档，未把消息/run定位参数传入。没有把接口通过冒充正式UI直达，也未宣称完整后代树或迟到/并发矩阵完成。

## C 盘只读结果

`controller-c-readonly-20261015.json` 已完成，exit 0。本轮未删除任何路径，实际释放 **0 字节**。

16 个具体目录已与 `D:\old-repos\coolzhuagent-c-desktop-20261012` 逐文件哈希比对，文件系统报告的分配量合计 **290,115,208字节**；详见 `docs/testing/2026-10-15-c-drive-manual-cleanup.md`。此数不是删除后实测释放量，且不含目录/MFT 开销。

补充 `controller-c-reference-readonly-20261015.json`：未发现指向旧Desktop仓库的进程命令行引用，D当前Git worktree均登记D路径，两处正式快捷方式均指向安装目录。旧C与D归档仍保留C侧工作树登记，因此不建议删`.git`、junction入口或整仓。核验没有冒充全部内核句柄审计，旧项目未退出使用前仍不应手工处理候选。

旧仓库的 `tmp`、`dist`、用户数据和未完整匹配的模块仍保护；构建缓存尚未证明所有独有产物可丢弃。两份 gotdot 工作树仍存在 D 盘缺失内容，不列入可删除清单。Codex 会话/缓存仍不能按历史大小全删；指向 D 盘的 junction 不计 C 盘回收。没有修改 Codex 数据库或侧栏注册信息。

## 仍缺证据

4.5的生产模型/正式UI、并行DAG、durable job和unknown恢复仍开放；4.2的逐run UI定位有明确接线缺口。EX-03尚缺原生资源故障交接。真实多屏/混合DPI、凭据全来源/跨设备、完整升级故障矩阵、签名/许可证映射仍未闭环，保持原门禁。附件与记忆GC引用图未补齐，删除未启用。

早期因构建内存压力暂停只读切换准备、在线quick_check达到60秒、原生跳过探测失败及首次打开C工作区等记录均保留。后续一致性备份、官方工作区选择、正式冷启动、真实Esc交接和前后对账分别补证，没有清空失败或unknown历史来制造通过。

归档共69份关键证据，清单SHA-256为`dc1b6eb0d0bb013e7977d56366fa1fff2f673d90baab3a51c801b93989779cbc`。JSON与Markdown的35项门禁同步；生产安装版、修复候选和C盘只读建议各有独立范围，未把隔离结果、文件级清单或编译成功冒充整项生产验收。
