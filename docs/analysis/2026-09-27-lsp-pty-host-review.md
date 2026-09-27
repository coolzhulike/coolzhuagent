# S5.4 LSP 与受控终端接线补审

日期：2026-09-27。与 GPT-6 Pro 会话“整合审查执行计划”共同审查，轮次 `85be79a9-b4e9-4fe4-8e0e-1d65c278d026`。以下为实施设计，不是完成证明。

## 源码核查

`language-service` 已有 LspManager 的文档同步、诊断、定义、引用、关闭能力，但本轮全模块检索没有找到生产宿主实例化，只有库测试与 reexport。现右栏 `terminalWindowRunPowerShell` 向 runtime-execute 发起一次性 PowerShell 调用，不能等价 PTY；仓库未找到 PTY/ConPTY 的生产实现。

因此 S5.4 为真实未完成项，不能只标记“待截图”。

## 共同采纳的最小方案

1. 先接 LSP 宿主实例，再接真实 Windows ConPTY/成熟适配；沿用现有运行时和进程监督，不新建通用 IDE 框架。
2. 资源由后端创建不透明 ID，绑定宿主解析的工作区及适用的 session/room/run。前端字段仅用于查找已授权上下文，不能作为路径或 owner 权限证明。
3. LSP 只启动明确配置的 server，不接受模型临时传入可执行命令。文件规范化并受工作区范围约束；生命周期随工作区资源，不能随每轮聊天结束强制销毁。
4. PTY 支持持久 stdin/stdout、调整大小、中断、关闭、同一宿主内重连。输出采用有界序号缓冲，丢弃旧输出时显式报告截断；宿主重启后的旧资源明确失效，不偷偷启动新进程冒充恢复。
5. 复用已有权限、取消和进程监督。资源释放应有期限、清理进程树、记录终态；不只保存 PID。LSP 初始化失败与 server 崩溃同样必须回收，不无限重启。
6. 最初只承诺 PowerShell 文本交互与 LSP 文档/诊断/定义/引用，不宣称支持全部全屏 TUI。

## 主会话补充校正

- Pro 验收建议中“Ctrl+C 后进程退出”应细分：中断当前前台命令、shell 保持可交互；关闭终端才要求进程树退出。不能把每次 Ctrl+C 都关闭整个 shell 当成正确终端行为。
- 现 LspClient 直接 spawn，需检查初始化失败/超时后是否回收、消息大小界限和 stderr 消费，不能仅因为库 API 存在便认定生命周期已受监督。

后续源码复核已确认具体缺口：`client.rs` 的请求没有超时，正常 EOF 不清待回复请求，初始化失败缺少可靠子进程收尾，stderr 用 `read_to_end(Vec)` 持续累积，Content-Length 和头部未设上限。现有 `ChildProcessJob::spawn_managed_async` 提供先挂 Job 再运行的公共边界，可在核实依赖无环后复用。Sol Max 已接手库修复及宿主配置/接口方案；这些是待修事实，不是新的测试通过声明。

## LSP 生产接线补审

Pro 轮次 `d65601d3-05a9-47d8-9653-e095aac11b3a` 认可以下最小方案，主会话已分配 Sol Max 实施：

- server 选择来源于宿主用户配置，并受现有进程执行权限约束。工作区可被仓库或模型写入的 JSON 不等于用户启动授权；本轮不增加自动采纳工作区建议功能。
- 设置/代码栏提供明确配置与启动动作，GET 状态和打开文件不启动进程。状态不暴露配置中的秘密环境变量。
- 宿主资源使用 canonical workspace 与不透明 handle；后续诊断/同步/定义/引用以 handle 路由，文件仍须经过后端范围检查。切换工作区停止旧资源，旧请求不能写入新工作区。
- 将诊断与跳转接入已有右栏代码预览，不增加独立 IDE 页面。
- 实际 rust-analyzer 可用性是环境证据，不是生产宿主已经启动成功的证据。测试须包含真实 server 初始化、诊断、定义、停止与异常释放。

补充协议复核：服务端 request 同时具有 method/id，不得被误当成客户端 response 消费同编号 pending；不支持的方法应明确返回协议错误。shutdown 使用独立短期限，不能按普通请求超时逐个拖住整个工作区关闭。

## 验收出口

LSP：真实配置 server 启动、打开文档产生诊断、定义/引用、工作区关闭释放、异常与越界路径拒绝。PTY：真实 PowerShell 输入与输出、中断前台任务后继续输入、owner 不串会话、输出截断与游标、断线重连、宿主重启旧 ID 失效及进程退出。最终右栏操作须保存包含对应构建身份的实操截图。

## 当前核查进展

PATH 中最初的 rust-analyzer.exe 是 rustup shim，实际缺少组件；不能把 Get-Command 成功计为服务可用。随后安装了当前 stable 工具链的官方 rust-analyzer 组件（未升级或切换工具链），Sol Max 使用 1.94.1 完成真实初始化、诊断、定义/引用和停止验证。生产宿主及右栏仍在接线，不继承库层验收结果。

宿主初期明确只支持 Rust 预设，从用户安装的 rustup 解析并校验实际二进制路径，避免在工作区 cwd 下搜索到同名程序。主会话复核还要求修正并发关闭误取新实例、服务崩溃后的状态显示及查找子进程的超时释放。以上待整合编译和界面验收。

PTY 方案选型参考 [portable-pty 官方 API](https://docs.rs/portable-pty/latest/portable_pty/) 与 [Microsoft 创建伪控制台说明](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)。成熟适配可作为首选，但必须核对受监督启动的接点；不能用 spawn 后登记 PID 代替启动前纳入 Job。Microsoft 明确要求正确管理双向管道和句柄生命周期，关闭/阻塞读写的收尾是实际验收点。尚未确定实现方案，未宣称 PTY 已交付。

## PTY 取舍补审

Pro 轮次 `2f13807e-8d9c-4fa4-9981-527449592428` 建议优先在已有 `windows-process-guard` 安全边界增加小型 Windows ConPTY 封装，不引入 portable-pty 专用辅助进程与额外 IPC。主会话采纳这个优先方向，仍要求实施者核对成熟库实际 Windows 后端、windows-sys 接口和现有句柄/Job 封装后定案；不能因公共文档没列某个方法就断言库完全不支持。

最小边界为：可信系统 PowerShell、ConPTY 双向管道、挂起创建 shell、加入 Job 后恢复、独立读写、有界输出、resize、中断和关闭。权限、owner、工作区解析留在宿主，FFI 留在允许 unsafe 的既有 crate。Ctrl+C 必须停止前台命令后还能继续输入，关闭才回收进程树；不采纳把两者混为一谈的验收。新增通用终端框架、跨平台后端和完整 TUI 兼容均不在本轮。
# 2026-09-27 实施中增量核查

- Sol 已检查 portable-pty 0.9.0 的 Windows `spawn_command` 实现，创建 flags 没有挂起创建或启动前 Job 接入点。本轮采用既有 `windows-process-guard` 内的小型 ConPTY 封装，不新增辅助进程执行框架。
- 主会话对照[微软伪控制台示例](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)发现初版 `UpdateProcThreadAttribute` 传了 HPCON 变量地址；该属性应传句柄值，已交 Sol 修正。读写保持独立，关闭期间继续排空输出。
- 真实 PowerShell 测试目前未通过：进程与管道创建成功，但输入尚未产生文件副作用。主会话已要求不把命令回显中的标记当成功，改以实际副作用确认输入、前台中断和恢复；根因仍定位中，不能称 PTY 已交付。
- 后续诊断发现 cmd 同样把输出送到测试进程控制台，而非 PTY 输出环。主会话查到[微软 Terminal 维护者关于重定向父进程的说明](https://github.com/microsoft/terminal/discussions/15814)：即使不继承一般句柄，Windows 仍可能复制父进程的标准句柄；应设置 `STARTF_USESTDHANDLES` 并让三个标准句柄为空，以使用伪控制台连接。当前实现恰为默认 flags，已交 Sol 按此核验；在实测前仍不将推断记为已解决。
- LSP 源码版右栏已实际启动 rust-analyzer，定义返回一项、引用返回两项；目前结果区域在旧断点布局中被挤出可视范围，需修布局并重截图后才满足可见验收。诊断还发现 Windows 普通路径与 `\\?\` 规范路径精确比较导致结果被过滤的缺陷，Sol 正修复。临时夹具最初遗漏独立 `[workspace]` 造成 Cargo 报错，已单独纠正，不将那次空结果当成功。

## PTY 底层实测与手动权限裁决

`STARTF_USESTDHANDLES` 修正后，真实 cmd 和固定 PowerShell 都能通过 PTY 输入产生隔离目录中的文件副作用。PowerShell 的 15 秒前台命令在 Ctrl+C 后约 1 秒恢复提示符，原命令的后续文件未生成；继续输入的新命令成功，shell 保持存活。再启动后代 PowerShell，关闭 Job 后 shell 与后代均退出。临时原始日志为 `tmp/s54-conpty-powershell-job-tree.log`，仍需归档并完成宿主/右栏验收，不能称整个终端产品已通过。

Pro 权限补审轮次 `58638c80-d741-4c7a-9f12-40ff301d6552` 认可主会话裁决：

- 手动创建和每次写入均检查真实 session/room/workspace、现有完整访问 grant 与权限门禁；只读或授权不足明确拒绝，指向已有设置入口。不新增逐命令审批体系、不自动提权。
- `user_authorized` 仅表示本次 UI 动作，不等于 grant 或二次确认。模型工具、MCP 与后台任务不能借用手动终端路径。
- 权限降级后仍允许清理自己已拥有的资源：Ctrl+C 与关闭可用，新命令与重启拒绝。
- 同一宿主、相同 scope 可以刷新续读；跨 room/workspace 或宿主重启后的旧 ID 不可恢复成新进程。
- 同步创建/关闭移入 blocking 执行区，不持全局注册表锁等待；右栏实际输入、中断、关闭及身份变化仍须截图验证。

主会话校正：Pro 验收示例中“只读工作区创建成功”不能作为预期；本裁决要求只读状态创建即拒绝，已有终端的只读降级场景再验证收尾动作。

## 终端 HTTP 权限反例与通用路径语义审计

隔离 HTTP 测试发现默认 workspace-write、dev_open=false 仍可创建终端。根因是通用 `OpaqueCommandExtractor` 把 cwd 当执行路径边界，手动动作的 `user_authorized=true` 又足以满足工程内危险操作判定。终端入口已补显式检查现有完整访问 grant 的 authorized/confirmed_twice 后再走原 gate。

修正后的真实 HTTP 验证已通过：默认工程写入拒绝创建；完整访问可执行中文文件命令、同 scope 续读且游标不重；伪造 room 被拒绝；从完整访问降为工程写入后，输入和输出均返回 403，状态及中断响应不泄露输出，但自身 Ctrl+C 与关闭仍可用；恢复完整访问后可继续执行，切换 room 撤销旧句柄。证据为 `tmp/s54-terminal-e2e-23c1d8d167/result.json`，仍待归档及 UI 验证。公开 room API 只支持 workspace-write/full-access，本轮未通过该 API 实测 ReadOnly 分支，不能记为“只读降级通过”。

主会话查明 `modules/tooling/packages/tool-registry/src/path_effect.rs` 的该抽取器也服务 bash/PowerShell/REPL/Agent。Pro 轮次 `1002cdd4-74b3-40c4-9978-e62d154ffbf8` 建议独立于 PTY 处理，避免重写权限体系。主会话将其列为本轮单独修复：先核实执行链是否有真正的 OS 访问隔离；若无，cwd 只可作为启动目录或审计事实，不能证明命令访问局限在工程内。保持 FullAccess/既有二次授权的行为，补默认 WorkspaceAuto 与工程内 cwd 也不能自行取得不透明命令执行许可的反例。此项未完成前不宣布通用权限审计通过。

后续源码审计确认：Windows bash/PowerShell 和 REPL 只设置启动目录；Linux 可选 unshare 也没有因 cwd 自动形成文件访问边界；Agent 继承父权限但不提供这种 OS 隔离。已将不透明命令路径效果改为范围未知，保留原始 cwd 供执行和审计。路径门禁 8/8、Agent 继承回归 1/1、独立 tool-registry 构建及组合 Web/MCP 8K 实测均通过。此结论只针对已定位的 cwd 自动放行缺口，不泛化为所有沙箱逃逸场景均已验证。

终端源码 UI 使用 DFD39612 构建完成真实中文文件命令、保持子目录、刷新同句柄续接、60 秒前台命令中断后继续执行及关闭；主会话已查看三张页面截图。终端输入框在窄栏只显示原始长命令的可视片段，完整命令在快照中保留，文件副作用另核实。它是源码页面 E2，正式安装版仍须全窗口截图。归档见 [受控终端证据](../testing/navigation-update-followup/s54-controlled-terminal.md)。
