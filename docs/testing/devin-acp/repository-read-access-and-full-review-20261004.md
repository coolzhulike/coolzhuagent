# Devin Opus 5.5 High 仓库只读访问与完整审查

## 权限与实际读取

用户明确授权指定模型读取 coolzhuagent 仓库。应用在独立工作区接通现有 MCP 工具桥，只提供 read_file、glob_search、grep_search；不开原生 Devin 文件/命令工具，不开放写入、电脑操作或子 Agent。每轮工具桥受宿主父运行、工程身份、取消、预算与工具台账约束，结束即撤除服务。只读路径必须位于当前工程，glob 的搜索模式也不能使用绝对路径或 .. 越界。

审查输入为当前工作树全部 3,739 个 Git 跟踪文件的独立快照，提交基础 8935debe47a6a2a920e97709c7592bed2c7836ef，并含只读桥本轮未提交改动。快照清单 SHA256 为 b98349a9faae28b9429bc2651e419ba08ebdc91a3b8bdaa8904bf3f42125d68a。没有复制原用户运行数据库、未跟踪凭据、.git 或构建缓存。对快照后的生产改动另行列明，不偷换被审查版本。

真实短验证 run-chat-ef92878bbe0b241777e8463f8fce33ed29f0c5fc9b4b930a 完成；requested/effective 均为 claude-opus-5-5-high，协议 end_turn，受管进程已排空。宿主 MCP tools/list 返回三项工具，read_file 实际读取 repository/Cargo.toml，83 行、3,413 字节，SHA256 为 9cc59f26d6ecdb7eeed06dd1ae5cfa6f9ebdbc48c8a208c50a3ecb75b3f8536d。截图及安全回执在 evidence/20261004-readonly/。

## 保留的失败与修复

1. 全仓首次请求在 CLI 查询阶段超时，提示未提交，没有把失败当模型完成。
2. 第二次请求模型发现 MCP 服务，但 tools/list 失败，没有读取任何源码。后续诊断证实客户端 initialize 请求 2025-11-25，而旧服务只接受请求值等于 2025-06-18，未进行协商。现按 MCP 官方生命周期返回本端支持版本；客户端接受后，后续通知/list/call 都实际携带协商后的 2025-06-18。Host、Bearer token、Origin 校验保持。
3. 诊断仅写后端协议方法、版本和校验布尔值，不写令牌、原始参数或源码；不增加前端调试条目。

官方依据：[MCP 2025-06-18 生命周期](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)。客户端可提出更新版本，不支持时服务返回自身支持版本；协商后请求携带该协议版本。

## 审查记录与边界

第一轮 room-1791124535547 / run-chat-47920848b4ad120b31cdb7de3832eb7b0ba8ee111f097e95 已完成，模型精确生效，end_turn，进程已排空。宿主计数为 65 次已完成只读调用：read_file 24、grep_search 37、glob_search 4；全部实际结果 ok。模型自述“约 63 次”仅为估计，计数以宿主台账为准。

原始报告见 docs/analysis/2026-10-04-devin-review/opus-source-review-round1.md。报告列出实际读过的段落、仅检索文件和未覆盖模块；它不是逐文件/逐行全量审计，也不是软件功能测试通过证明。

第二轮 run-chat-fac6397a4b6d80079e364fd0618287ad902da5e6c7ade350 完成 60 次宿主调用：read_file 20、grep_search 27、glob_search 13。第三轮 run-chat-89e66d8700304cd6813d622251dfa6f9c4445854529f82f8 完成 22 次：read_file 19、grep_search 3。两轮均 end_turn、进程排空，requested/effective 为 claude-opus-5-5-high；工具审计全部 ok。三轮合计 147 次（read 63、grep 67、glob 17），不含短验证与模型自述的工具发现/错名调用。第三轮报告列明指定 17 个入口及内部未深入部分；安全回执中的路径有截断，不能据此重建完整逐文件覆盖清单。

第三轮前两次在 CLI 查询阶段超时，没有提交提示，也没有 ACP attempt。官方 CLI 单独查询随后成功，返回 53 个家族、721 个模型；重启到最新开发二进制后的补查成功。尚未确定目录查询间歇超时的内部原因，未通过缓存冒充实时目录或放宽模型确认。失败请求保留在原聊天室。

第二、三轮原文分别为 opus-source-review-round2.md、opus-source-review-round3.md；主会话采纳与暂不采纳的结论写入 reviewed-execution-plan.md。已把 GitHub 分支及 PR78 链接发给审查会话作备用，同时告知当时远端尚未包含本轮未提交桥改动。真实源码读取使用本地固定快照。

主会话已确认第一轮关于“无条件提示不存在工具”的问题，并为 ACP 增加后端专用工具指南，保留宿主上下文和记忆，使用本轮实际 MCP 目录说明能力。该改动及 glob 模式边界补充发生在审查快照之后。

第三轮指出保存较窄只读白名单会扩成三项，主会话已修：载入合法子集后，保存其它参数保持该子集；新启用只读才采用三项默认清单。原文称原生工具“只靠提示词禁用”，主会话核查发现 controlled_config 的 CLI deny 和 denied_client_request 返回 cancelled 已存在，因此不采纳这个断言。模型能看见工具名称不等于拥有执行权限；真实原生拒绝尝试尚未在此次会话验证。

只读指不修改用户源码、不开写入/命令工具。宿主正常运行仍会在 .coolzhu/tool-results 生成检索溢出文件及台账；这属于运行产物，不承诺整个工作目录零写入。开发测试授权来自 dev_open_permissions，桥仍硬限制三项只读工具及工程根目录；不能由此推断普通用户工程的所有私有数据都已被审查或过滤。

## 回归检查

离线 Web build 通过。既有前端 Devin 配置/登录契约 9 通过；ACP 定向 49 通过、3 忽略；既有 Web 回归主入口 1,371 通过、6 忽略，库 8 通过、另一个入口 1 通过。工具注册 crate 检查通过、模块联接冒烟 8 通过。最后的前端白名单保存修复重新编译、9 项既有前端检查通过。未执行的真实模型忽略项不算验收。本次实际模型证据来自上述应用发送与工具台账，而非模型夹具。

远端 CI 的登录进程完成分支仍受 PowerShell 冷启动波动影响，本轮改用系统 cmd 的立即退出进程测试该分支，保留原完成时限；超时/取消分支继续测试受管 PowerShell。生产登录和受管进程保护未放宽。

0.2.69 安装包是只读桥加入前的模型配置版本，不包含此次只读增强；不能通过安装旧包宣称拥有新能力。Browser Use、Paint 与正式安装版联测仍待后续执行。
