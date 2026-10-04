# 2026-10-02 Browser Use续验与本会话Computer Use启动故障

用户先要求gpt-6-astra/High/标准速度继续Browser遗留，随后要求先解决Codex Computer Use可用性，再自主测试。此轮完成新的工具、插件、依赖、客户端日志和正式事故只读核查；没有以历史“缺工具”结论代替本次诊断。生产源码、配置、安全设置、旧包和事故未修改，未安装或重启软件，未发项目模型请求。

## 当前可核验的故障

环境变量CODEX_THREAD_ID与CODEX_SESSION_ID均为`01a0fadf-42a7-7117-821e-ccf121358007`。本轮工具目录再次枚举，`node_repl`、`cua_repl`、Computer Use、sky和工具搜索入口均无匹配；本机配置的Obsidian/Unreal MCP也未出现在可调用目录。技能存在不代表工具已经注入。

客户端日志直接记录当前会话：

- `2026-10-02T08:23:37.724Z`，`cua_repl`、`codex_app` starting。
- `2026-10-02T08:23:40.080Z`，两者failed，error=`MCP server failed to start.`，failureReason=null。
- 同一会话`codex_apps`随后ready。
- 父会话`01a0f094-d711-76e5-b92a-a4938a6f100a`的`cua_repl`在`2026-10-02T10:38:04.489Z`为ready、error=null。父会话随后明确说明该工具连接dot云端Linux桌面，不能操作用户本地Windows；本报告原先建议父直接操作本地的路径已撤回。

因此已证实**当前会话报告CU服务启动失败且无可调用本地CU入口**；这不足以证明失败的服务进程位于本地Windows。日志没有给出底层失败原因，不能进一步断言是依赖损坏、Windows拒权或服务版本不兼容。父会话与本会话状态不同，不能把某个会话失败扩大为整台机器没有Computer Use。

### 后续有限只读诊断与端点纠正

两者启动状态均为`hostId=durable`，不能把日志所在电脑当成被控端点。父线程的VM生命周期记录关联`ccarenv_6abc9097c9108191a5c41a4265ed1b3c`（日志中为base64表示，已解码）；当前线程恢复记录关联不同环境`ccarenv_6abbfff9c70c8191805c03adad4fa87a`。当前执行器确实可在本地Windows运行PowerShell，但不证明CU服务也路由至此。父端点的dot/Linux性质来自父会话明确说明；本机日志没有CU端点URI或OS字段，不将其冒充独立验证。

核查2026-10-02三个客户端日志，当前失败在08:23:40.080Z，仍只有`MCP server failed to start.`、failureReason=null，没有对应底层错误码或握手stderr。08:23:41.244Z的`-32601`对应另一个线程`01a0f4ed…`的`thread/unsubscribe`，不归为CU故障。Windows Application事件日志08:23:30–08:24:00 UTC未检出Level=2事件；这个有限时间窗的无结果不证明服务正常。

已有Node实际执行`--version`返回`v24.21.0`、exit 0、无stderr；node_repl.exe及Node文件可读并已记录哈希。结合已有模块/可信路径存在证据，只能确认基础依赖可读取、Node可运行，不能证明MCP握手、sky或窗口控制健康。没有从普通shell导入sky、启动CU辅助程序或建立私有协议。

`codex-cli 0.159.2`的`mcp --help`只列list/get/add/remove/login/logout/help，无restart/reconnect；`debug --help`也未提供现有会话CU重试。当前工具目录无本地node_repl/CU、工具搜索或服务重启工具。**本轮未找到可在会话内安全执行的一次官方重试路径，因此未重试、未枚举窗口，未声称修复。**配置哈希仍与原记录相同。新增[端点与依赖证据](../testing/browser-use-resume-2026-10-02/endpoint-health.json)。

本地核查：

| 项目 | 实际事实 |
| --- | --- |
| 客户端安装 | `OpenAI.Codex_26.928.3736.0_x64`；ChatGPT.exe文件版本154.0.8037.57（文件版本不冒充产品发布号） |
| 实际CLI | `codex-cli 0.159.2`；旧version.json缓存的0.154.0不用于判断当前版本 |
| 功能 | CLI报告computer_use、browser_use、plugins为stable/true；js_repl为removed/false，不应盲目启用已移除开关 |
| 插件 | `computer-use@openai-bundled`配置enabled=true，已安装技能版本26.928.31416 |
| 依赖 | 配置中的node_repl.exe、Node、模块目录及分号拆分后的两个trusted路径均存在，@oai/sky 0.7.5存在 |
| 进程 | 配置路径对应node_repl PID4328、31308存活；进程存在本身不等于当前会话工具健康 |
| 权限 | 本地已有computer_use.windows配置；未修改授权列表。云端Plugin Management不识别本地内置包，不据其not_installed结果删装本地插件 |
| 配置完整性 | 本次读取的config.toml SHA256 `3a946a45e2223faebb565c2323d358fa272fd51fabb17068e5085082804c6a5e`，没有写入；不归档完整配置或原生管道/可信服务值 |

CLI只读诊断出现创建arg0临时别名的访问拒绝，但命令仍正常返回版本与功能列表；这属于当前受限执行环境下该命令的警告，不能当成此前cua_repl启动失败的根因。

## 最小恢复路径

### 旧会话独占假设的有限核查

结论：**尚未证实，也不能排除未记录的占用；不据此结束任何进程。**只读筛查10月1日、2日客户端日志元数据，未检出CU相关exclusive/session lock/already running/port or address in use/EADDRINUSE/pipe busy/another session/connection leak等冲突记录。旧线程`01a0b94c-b964-7390-a446-776f760d90d4`在10月1日03:15:26 UTC及03:24:51 UTC的node_repl、cua_repl、codex_app为`hostId=local`、ready；这只是历史启动状态。当前失败为`hostId=durable`，不能默认在争同一本地服务。

本次再次观察到node_repl PID4328、31308仍存活，创建于10月1日11:15:26 +08:00，与旧线程启动时间相近。只能证明时间相关，不能证明它们持有旧线程的排他锁；没有可用的CU owner查询工具。CIM父子关系查询被系统拒绝访问，未提权或采用绕过方式。`coolzhuagent`旧Shell3864的ReleaseUnknown是被测产品自身安全状态，与Codex CU独占是不同问题；没有证据将两者相连。

最小验证：先在本地客户端当前任务的受支持详细诊断中核对CU实际执行位置与owner/session信息（若提供），比较旧线程local与当前durable；如出现明确占用错误，记录持有者、目标资源及时间再决定处理；若无此信息，向客户端支持提供两个线程ID及启动时间，请其核对握手失败与owner关联。不要以杀掉旧进程或删锁作为诊断试验。本轮未向旧会话发执行指令，未重启、删除锁或解隔离。[占用核查元数据](../testing/browser-use-resume-2026-10-02/cu-ownership-probe.json)。

1. 在用户本地Windows客户端选中当前本地任务/执行器，使用受支持的`/mcp verbose`查看该会话详细诊断，明确服务执行位置、错误码及实际工具目录。父云端Linux的ready不参与本地窗口验收。该查看动作不操作被测应用或改变隔离。[诊断命令](https://learn.chatgpt.com/docs/developer-commands)。
2. 如果本地客户端的MCP设置确实提供失败服务的独立Restart入口，可在没有进行CU输入时对该服务重试一次，再继续本地任务核对工具目录；这会重新建立该服务连接，可能清空其REPL状态/中断该服务在途调用，不要求退出被测应用或重启整个客户端。不要为制造入口添加重复MCP、重写配置或改安全权限。官方说明通用MCP配置保存后可Restart，但不能据此保证内置cua_repl存在独立重启按钮。[MCP配置与执行环境](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)。
3. 当前会话没有“重启已有客户端MCP/重新注入工具”动作。不能通过复制私有管道参数、另造助手协议客户端、编辑会话数据库或直接驱动辅助程序来绕过这一缺失。官方支持在设置中保存MCP配置后选择Restart；实际内置服务是否显示该入口应由可用UI观察确认，不能假造按钮。
4. 若无独立入口或重试仍失败，保留上述线程、环境、时间和诊断交由客户端支持排查，不循环尝试。完整退出/重启Codex会中断活动任务，本轮明确不执行；没有证据要求重装、新软件或扩大持久授权。[Windows CU与权限说明](https://learn.chatgpt.com/docs/computer-use)。
5. 仅在本地执行器真实获得node_repl工具后，按[Computer Use技能](C:/Users/zhupu/.codex/plugins/cache/openai-bundled/computer-use/26.928.31416/skills/computer-use/SKILL.md)在该会话中导入`@oai/sky`，先只读枚举Windows应用/窗口，确认目标进程和桌面，再观察事故详情。不得用云端Linux窗口枚举结果或进程列表替代这个验收点。

本机默认config读取为gpt-6.1-sol/low/priority，**不是活动回合的模型证据**。本次要求仍为gpt-6-astra/High/标准速度；未提供可查询当前回合模型的工具，未改全局默认或打开Fast。应由父会话的活动模型选择/状态核验实际回合设置。21%是用户报告的Codex账户额度，不推导为项目Qwen API预算。

## 原事故和建议审核操作

正式只读API仍为0.2.63构建`09dc04d0f812 · 2026-10-01`，isolated、accepts_new_input=false、未放行block=1、待恢复操作=0。Shell3864创建身份`134353335372030593`、Web2572创建身份`134353335363833026`仍存活。

阻断`native-panel-fd88ef6ac7802d80f61b9c375dbad320`对应063 DH：pointerdown到pointerup为8002ms，超过宿主3秒释放确认时限；关闭请求在pointerup之后16988ms。原call/run及原事实继续以063归档为准。随后计数1不是宿主期限内释放确认，也不证明按下至释放之间的关闭竞争被命中。

恢复入口语义已从生产源码核查：可信原生窗口“运行轨迹 / 安全详情 → 连接与输入安全详情 → 放行隔离…”。确认内容列出scope、阻断、历史run、permit和executor事实；确认表示接受列出的历史未知结果，旧未知不会改成成功，后端还要独立检查。`native_recovery::commit_confirmed_release`明确拒绝原PID与创建身份对应实例仍存活的申请。

具备CU后的第一步应只读检查上述事故和实际界面，不直接点最终确认。可供用户逐项批准的具体后续是：保存当前工作并正常退出原Shell3864/关联后台2572（停止现有会话运行，保留数据库和事故）；重新通过正式桌面入口取得恢复界面；在确认无在途执行者、物理鼠标键盘释放及机器恢复前置条件后，明确接受**本次列出的063 DH未知结果**并申请恢复。只有机器返回opened且accepts_new_input=true才开始新输入。用户泛称自主验收不用于代签未核实的物理状态；本轮未退出、申请challenge、确认恢复或清除事件。

## Browser工程检查与尚未覆盖的真实验收

生产审查确认：按下/释放在同一原controller中顺序入队，不等down回调才安排up；Close先撤销资源并隐藏，等待已有execution gate清理再销毁；提交输入后后台等待释放回执不被普通取消立即丢弃。真正超时保持ReleaseUnknown、不自动重放。这些代码事实不替代真实竞争场景。

此轮实际运行两组定向测试，均退出0：

- Shell `native_browser`：5通过、0失败、59过滤。范围为有界CDP回包、AX投影、原文档/房间/有效期与撤销引用、当前AX与交互状态、几何/命中前置条件。
- Web `native_browser_input::tests`：2通过、0失败、1303过滤。范围为同一认证宿主未领取时撤资源可确认零输入、领取后不能改写，以及点击/滚动/导航ACK、尝试/执行实例和目标资源不能串用。

未跑Shell/Web全套，本轮没有新增生产修改。没有真实页面输入、释放回执、新观察截图或项目Qwen调用，因此Close/Cancel/导航在按下至释放间的竞争、8秒慢事件后的真实恢复、旧文档代次端到端拒绝、连续真实模型闭环均保持未验收。0.2.64候选只保留既有静态包核验，未安装。Paint、动画、DSH扩展和GitHub上传未推进。

本轮证据：[索引](../testing/browser-use-resume-2026-10-02/evidence-index.json)。日志只保存诊断事件片段和必要元数据，不复制原始客户端日志、配置秘密或会话数据库。

后续独立工程：用户要求CU诊断不阻塞实现，已修复Web输入传输期限竞态，详见[单独修复报告](2026-10-02-browser-input-deadline-review.md)。本报告上述“无生产修改”和原7项结果描述的是先前诊断阶段，不能替代后续源码版本与验证记录。
