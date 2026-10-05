# Devin 插件与 SKILL 接线：改动报告和测试交接

日期：2026-10-06。0.2.76 已经正常构建、安装，1150 个安装文件逐项核验，正式安装版完成真实 Devin `swe-2-medium` 的 DSH、原生插件和 SKILL 回归。架构复审继续原 `claude-opus-5-5-high` 会话，受该模型请求的远端额度／速率限制影响仍未完成。候选与安装版证据分开保存，不代表原整合计划全部验收完成。

## 功能与使用方法

1. 在“更多 → 插件市场”搜索 DSH 社区插件，查看固定源码后安装。安装默认停用，点击启用完成当前工程的宿主兼容性检查。
2. 在“设置”选择 Devin 会话，在插件工具区域勾选需要的已启用工具并保存。安装或启用后可以点击刷新图标更新工具目录；刷新不覆盖未保存的模型参数和工具选择。
3. 在原聊天室选择该发送对象。插件执行沿用聊天室权限；完全访问适用于该聊天室的实际发送对象。工具白名单仍按各个模型会话配置分别选择，不能从主会话借用另一模型的授权。
4. 在“更多 → SKILL”扫描当前工程工作流并选用，后续会话自动注入它的说明。SKILL 不授予新权限；停用会发送替代的系统说明，同一远端上下文继续保留。
5. 缺少授权时明确返回未执行，不留下 Devin 回合结束后还可执行的延期审批任务。授权完成后需要发起新的一轮请求。
6. SWE 与 Opus 分别保留原远端上下文，回复仍显示在本地聊天室。本轮没有切换 Qwen，没有伪造模型回包。

## 模块职责与实现

文件主要位于 `modules/gui-web/packages/web-console/src/`。

| 模块 | 职责与本轮改动 | 测试重点 |
|---|---|---|
| `devin_acp/host_tools.rs` | 配置保存和执行共同使用的已接通宿主能力分类；只读、CU、原生插件与 DSH 分开选择 | CU 未启用时不能选择 CU；插件无需借助 CU 开关；未接通能力仍拒绝 |
| `devin_acp/chat.rs`、`agent_session_backend.rs` | 使用共用边界，保持原会话续接和只读审查模式 | 精确模型不被替换；只读审查仍只开放三种工具 |
| `devin_acp/session.rs`、`journal.rs` | 区分响应身份错误与固定分类的远端拒绝；补齐异常终止的停止意图；原运行停止入口可停止已失败、已排空且工具结账的旧回合 | 不改写 unknown、不伪造远端取消确认、不自动重发旧提示、不清原远端绑定；工具未收尾仍拒绝续接 |
| `devin_acp/bridge.rs` | 捕获本轮实际发送对象的工具定义；DSH 复用异步派发；原生插件使用现有监督器及执行器 | 同库接纳、冻结权限、取消、根预算、只登记一次、实际回执 |
| `dsh_web.rs`、`chat_run_admission.rs` | 按 `(实际 Agent, 工具)` 冻结 DSH 授权；审批归属实际目标；ACP 不进入延期审批队列 | 非主发送对象不能继承主会话授权；未授权未执行；终态后无悬空审批 |
| `main.rs`、`model_settings.js` | 模型配置读取已启用插件目录；复选框选择和独立刷新 | 保留未保存草稿；失效工具选择不被静默删除；不改变聊天发送对象 |
| `index.html`、`dsh_market.js` | 精确定位插件市场的根节点，修复被模型页同名区域抢先匹配而导致的按钮未接线 | 搜索、分页、查看固定源码、下载、安装与启用 |
| `dsh_market.rs` | 去除过时的“尚未支持安装／模型接线待验收”界面文字 | 正式页面表达可执行功能，不显示调试验收状态 |
| 随附 `cli-anything-bridge` | 升级 0.1.1，移除 Windows PowerShell 5.1 不支持的 `ConvertFrom-Json -Depth` | 安装后正常同步；真正执行插件状态工具；区分宿主执行结束与插件内部成功 |

MCP 已认证工具请求的逻辑拒绝现在返回 `isError=true` 的工具回执，避免 HTTP 冲突被客户端显示成连接故障。认证令牌、Origin、Host 和协议检查仍保留。错误回执不包含请求参数或连接凭据，未确认结果不声称成功或可安全重试。

本轮复用既有工具监督器、权限体系、DSH 固定源码和运行时、聊天运行台账，不新增第二套审批状态机，不开放原生 CLI 命令、任意写文件、外部 MCP 或 Agent 子任务。

## 候选真实功能检查

| 标记 | 操作与预期 | 已观察结果 | 阶段 |
|---|---|---|---|
| `DEVIN-DSH-20261006` | 官方 DSH calculator 搜索、安装、启用；真实 SWE 调用 `(17+7)*4` | 返回 96；一条实际 DSH 调用 completed，审计 ok、7479ms；ACP terminal/end_turn/drained=1 | 候选通过 |
| `DEVIN-SKILL-20261006` | UI 选择实际工程 SKILL；请求 `sqrt(81)+13*7`，不在请求里复述格式 | 返回“竹简回执／表达式／宿主结果：100”；一条 DSH 调用 completed，审计 ok、7114ms | 候选通过 |
| `DEVIN-NATIVE-PLUGIN-COMPAT-20261006` | 真实 SWE 一次调用 `plugin__cli_anything_status` 检查 python | `available=true`、`pythonAvailable=true`；调用 completed，审计 ok、660ms | 候选通过 |
| 工具目录刷新 | 保持未保存会话名称和三项工具复选框，点击刷新图标 | 草稿和勾选状态保留，目录更新 | 候选通过 |
| 原上下文 | 上述轮次以及正常后台重启 | 原 SWE 绑定 `veiled-anise` 不变，历史增量为 0；requested/effective 为 swe-2-medium | 候选通过 |
| `DEVIN-DSH-NONPRIMARY-20261006` | 主配置 Opus、实际只发送 SWE，调用 `(31+9)*3` | 返回 120；父 run 主会话为 Opus，目标、工具 scope、审计与回复归属 SWE；一次 completed，7354ms，原远端与终态不变 | 候选通过 |
| 旧失败回合停止与原会话续接 | 通过原运行停止接口登记停止意图，再发新的复审请求 | 原失败保持 unknown，drained=1、cancel_requested=1；原 fair-amaryllis 不变。新请求已越过本地阻塞，被远端 -32011 额度/速率限制拒绝；工具台账为零 | 本地恢复已验证，远端复审未完成 |
| 未授权 DSH 与无延期审批 | 真实模型请求，被拒绝后不执行、不挂起，不留迟到执行入口 | 待补；源码检查不替代真实场景。SWE 安装版请求当前正常，不能把 Opus 受限当作这一项已经验证 | 未完成 |

DSH 来源为官方社区包 `@deepseek-ai/dsh-tool-calculator` 0.0.1，固定提交 `b2007a13f06bcf75bf07b9d277ee8d434a316490`，22 个源码文件。候选使用已有锁定 Node/SDK 运行时，经长度及 SHA256 验证；不依赖全局 Node 回退。

协议没有独立返回服务端解析模型 ID，`resolved_model=null`；不将 requested/effective 冒称服务端独立确认。审计未保存完整工具输出正文，因此输出值同时以真实聊天室回复及实际宿主调用事实核对。

## 正式安装版真实回归

使用 `C:/Program Files/CoolzhuAgent/bin/` 的已安装二进制，在原独立验收工作区、8767 端口运行；原安全库保留。以下请求均由聊天页面正常发送，未使用模型回包夹具。

| 标记 | 实际结果与耗时 | 宿主执行事实 |
|---|---|---|
| `DEVIN-INSTALLED-EXTENSIONS-20261006` | DSH `(27+10)*3` 返回 **111**；原生插件 `available=true`、`pythonAvailable=true`；16.4 秒 | 两个工具各一次，均 completed／审计 ok；DSH 1737ms，原生插件 589ms |
| `DEVIN-INSTALLED-SKILL-20261006` | 选用真实工作流后，新表达式 `sqrt(144)+7*9` 返回“竹简回执／表达式／宿主结果：**75**”；10.2 秒 | DSH 一次 completed／审计 ok，1083ms；没有复述格式提示；随后在页面停用测试工作流 |
| `DEVIN-INSTALLED-PLUGIN-DISABLED-20261006` | 页面停用计算器后，模型请求旧工具，被宿主明确拒绝；回复未执行；10.8 秒 | 工具台账与审计均为零，ACP 正常终态；恢复启用并完成下一轮后再次确认旧请求仍零执行 |
| `DEVIN-INSTALLED-PLUGIN-REENABLED-20261006` | 原生控制台正常恢复启用，独立新任务 `(52-10)*2` 返回 **84**；18.3 秒 | 仅一次 DSH completed／审计 ok，849ms；不补执行停用期间的旧任务 |

四轮目标均为原 SWE Agent，原远端绑定 `veiled-anise` 不变；历史增量为 0，ACP `terminal/end_turn/drained=1`，requested/effective 均为 `swe-2-medium`。正式控制台显示实际新回复，见[扩展与 SKILL 软件实拍](installed/installed-native-extension-and-skill.png)、[SKILL 页结果](installed/installed-skill-result.png)、[停用拒绝与恢复后新结果同屏](installed/installed-plugin-reenabled-native.png)及[原始事实摘要清单](installed/manifest.json)。没有使用旧候选回复截图冒称安装后执行。

恢复启用时，外部浏览器自动化输入超时，先只读核对状态，没有盲目重复提交；原生控制台完成已有插件恢复的正常确认，随后真实模型新任务成功。软件本身仍正常。停止前撤销分支的零执行结果不能替代“未授权 DSH 无延期审批”或执行中取消、超时的验收。

## Browser Use：150% 网页缩放正式实操

`BU-INSTALLED-076-20261006-zoom150` 已通过，124.6 秒。基准 Windows DPI 为 150%（新页面 DPR=1.5）；主会话只辅助打开页面、聚焦空白区域和逐步设置网页缩放至 150%（DPR=2.25），没有代模型填写、勾选或提交。结束后恢复网页原缩放；前后辅助事件与模型运行期间事件分开归档。

真实 SWE 只提交一次 `computer_use_perform`，7 步依次为聚焦点击、输入、三次滚动、勾选点击、提交点击。三次点击均 `sent/released`，其他动作 `sent/not_needed`，没有 ReleaseUnknown、重发或 Navigate。最终真实新观察确认 `FORM-PASSED-074`，`succeeded/goal_achieved=true`，标准 1/1。沿用 0.2.74 的真实 HTML 用例，所以页面标题和标记仍带 074；运行二进制确实是已核验的 0.2.76，不以页面标签冒称安装版本。

本轮 15 次 ACP 请求均 `terminal/end_turn/drained=1`，requested/effective 为 `swe-2-medium`；外层聊天原 `veiled-anise` 绑定保持、历史增量为 0。[正式软件结果实拍](installed-browser/zoom-150-passed-native.png)、[150% 起点](installed-browser/zoom-150-before-native.png)、[执行中的顶部提示](installed-browser/zoom-150-active-native.png)、[宿主与网页原始事实](installed-browser/manifest.json)均已保留。顶部提示在执行结束后消失；窗口截图只证明该提示，不冒称已经覆盖整个桌面四边泛光的所有显示器场景。

严格 `down → 整页文档替换 → up` 仍未命中。源码的点击在同一 UI 闭包连续入队按下与释放，原网页测试 pagehide 发生在 up 之后；不为了触发测试而延迟生产释放。新增的 150% 组合不替代这一项，也不覆盖多屏或双指缩放。

## 保留失败与复审

- 原生插件第一次宿主桥调用返回 unknown tool：已补接原插件执行器，原失败不追认为通过。
- 执行器接通后的插件内部仍返回 PowerShell `Depth` 参数不支持。该轮宿主完成并不等于插件任务成功；0.1.1 修复后另有成功轮。
- Opus final 第一轮没有读取成功，客户端报告 MCP 连接失败，实际工具台账为零；不算源码复审通过。定位到快照路径与错误回执问题后另起同一远端上下文复审。
- 本地首次构建编译错误、运行中 debug EXE 文件锁、测试进程继承验收工作区环境导致的目录断言失败均保留原日志。清理测试子进程环境后现有 1390 项通过、6 项跳过；跳过不算通过。
- MCP 回执回归的第一次断言失败来自测试尚未进入 submitted 状态；修正真实待测阶段后重新执行，不删除拒绝和认证断言。
- 停止入口与异常路径修复后重新离线构建通过；最新 Web 主程序检查 1390 通过、0 失败、6 跳过（46.85 秒），另有库检查 8 项、启动配置检查 1 项通过。此前远端拒绝分类的协议检查 11 项通过。工程测试不替代真实模型与安装版验收。
- 工具模块离线构建通过；模块接线 8 项通过（2.01 秒）；模型配置与插件市场前端脚本语法检查、Git 差异空白检查通过。主会话已核对冻结授权、监督器、单次登记、无延期审批和旧回合续接的职责边界，没有把远端未完成复审计为通过。
- Opus 的错误测试随后以远端响应错误中断，ACP 为 unknown/drained，工具台账为零；下一次有效路径复审又被旧回合缺少停止记录拦住。发现异常返回路径与 Drop 路径不一致，补齐停止意图，并由真实运行停止接口恢复。原失败和 unknown 保留，不修改 SQLite 伪造终态。
- `DEVIN-EXTENSIONS-RECOVERY-REVIEW-20261006` 已进入原远端会话，返回标准错误码 -32011，固定分类为账号额度或速率限制；没有原始错误正文、凭据或请求参数落日志。新失败回合 unknown/drained=1/cancel_requested=1，验证异常路径不再漏记停止意图；未自动重试，也未换模型。Opus 最终源码复审仍未通过。

候选截图与逐轮原始事实见 [candidate/](candidate/)。原生插件任务成功截图为 [兼容修复后](candidate/swe-native-plugin-compatible.png)，不能使用旧失败轮的宿主 completed 作为成功凭据；[非主目标 DSH](candidate/swe-dsh-nonprimary.png)提供另一次完整页面结果。[原会话恢复后的远端拒绝](candidate/opus-recovery-quota.png)保留当前阻塞证据。

## 仍需针对性验收的边界

- 插件停用后的拒绝、重新启用后的新调用、旧请求未迟到执行已通过；卸载、配置变化、执行中撤销、取消和超时的完整生命周期仍待补。
- Devin Goal、Relay、附件还未开放；完整记忆操作、精确模型切换、账号过期、异常退出恢复仍待验收。
- Browser 严格按住 → 整页导航 → 释放竞争、多屏、双指缩放及其它缩放组合；125% 旧安装版与 150% 本轮安装版已分别有真实验收，不代表所有组合。Paint 基础笔画沿用既有通过记录，不要求完整人物。
- 侧栏全量、更新下载安装重启、开机跳过与减少动画边界、整合计划总审核仍有遗留。微信原功能保留，不改动、不测试。
- Devin 登录后的健康诊断仍按传统 HTTP API Key 检查，可能误报缺少密钥；真实登录与 SWE 调用正常。需要按 Provider 的实际认证方式接入健康页，不能用随机 Key 掩盖问题。

## 发布与证据

- MSI 源码提交：`34928536beeb00ac1153a99b5b1bfc4977de7d5d`；源码快照：`3d1941008f69bf057b06b35fb3701b333cc9a5123f34d3179f7060ed36534289`。后续文档提交不冒称 MSI 源码提交。
- 包名 `CoolzhuAgent-0.2.76.msi`，276331138 字节，SHA256：`f4b1c1466dde291d0e5480603795a16f5c55f8ec9b9e595b4c55b7dcaa68780b`。桌面仓库 `dist/` 副本与工作树构建产物相同。
- 正常发布流程执行六项既有 gate，`release_eligible=true`、`package_safe=true`；1150 个负载文件逐项核对长度和摘要。Web SHA256 为 `87b00126cec4ac1857d6cde70cd920bea158898fc93e3a5d72a9132f444d4645`；Shell 源码未改，由正常构建收据允许复用原 producer 产物。
- 首次普通安装失败 1603，日志显示旧版移除需要管理员权限 1730；保留失败收据。随后走 Windows 正常管理员更新流程，MSI 返回 0；没有更改 UAC 或 Windows 安全配置。完整日志保留在 `tmp/`，这里只归档收据与日志摘要。
- 安装后 1150 个 Program Files 文件均与负载匹配，CLI 和[正式软件更新页](installed/installed-version.png)显示 0.2.76；正常桌面入口自检通过，原日常工作区 `C:/Users/zhupu/coolzhuagent` 与原安全库保持。日常 Qwen 选择只作为用户环境保留，没有用于本轮验收。
- [构建收据](evidence/build-identity/)、[负载核验](installed/package-076-verification.json)、[安装核验](installed/installed-076-verification.json)、[日常启动自检](installed/daily-076-selfcheck.json)。构建收据中的“尚未安装”是构建时记录，后续安装事实以独立收据为准。
- 验收结束后正常关闭独立窗口，只按记录的 EXE 路径和创建时间停止本轮后台；本轮网页服务已退出。原日常入口 8765 已恢复，[恢复后自检](installed/restored-daily-selfcheck.json)确认原工作区、安全库和源码349保持；没有停止其它项目服务。
- [PR #81](https://github.com/coolzhulike/coolzhuagent/pull/81) 已合入先前 0.2.75 上下文版本，合并时 HEAD 为 fc306ed；本轮插件源码349不在该合入范围，将由接续 PR 承接。源码 349 的 [Web console baseline](https://github.com/coolzhulike/coolzhuagent/actions/runs/37356300855/job/111919454598) 已通过。后续文档提交另按最终 HEAD 检查。
- GitHub 0.2.76 草稿发布的四项分发资产已经上传，大小与服务端 SHA256 均匹配；公开发布前核对接续 PR 与发布说明。不将现有正向结果扩展为完整验收。
