# Devin 统一 Agent 接入：本地第一阶段实施

日期：2026-10-02。分支：`codex/source-sync-20261002`；源码基线：`f73c49b`。用户要求先在本地实施修改；DOCX 的历史暂缓表述不覆盖本次授权。

## 实际改动

新增 `modules/gui-web/packages/web-console/src/agent_session_backend.rs`，集中识别 HTTP、Devin ACP 和预留 Cloud 后端。持久化模型参数新增可选 `backend_kind`，兼容旧配置；会话与 Agent DTO 展示后端身份。显式类型冲突拒绝，Devin 不因模型名像 GPT/Claude 而继承 HTTP 地址或进入 ProviderClient 循环。

原 Agent 服务商入口增加 Devin 配置预览，统一模型与会话页增加 Devin CLI 连接、模型获取、手工选择和保存。切换连接时仅在保存阶段清理不适用的 HTTP 参数；隐藏密钥不随 CLI 查询或 Devin 创建请求发送。思考、图片、容量和底模身份未经协商时保持未知，原 Agent 表单也不按模型名称推测能力。

新增 `devin_acp/discovery.rs`：只调用 `--version`、`models list --format json`，不自动登录或生成任务。CLI 路径来自宿主 `COOLZHU_DEVIN_CLI` 的绝对路径或绝对 PATH 项；Windows 要求原生 EXE。查询在单独临时目录运行，不使用真实工程 cwd；stdin 关闭，Windows 使用已有 Job 监督整个子树。stdout/stderr 各最多 4 MiB，版本查询 5 秒、目录查询 20 秒，同时最多两项查询。`COOLZHU_DEVIN_CLI_VERSION` 可固定完整版本输出，不匹配则拒绝继续。

目录只接受明确数组或 `models` 数组及实际 ID；未知结构、部分损坏、超限结果拒绝，不内置“可用模型”清单。目录可访问只标记 `catalog_access_confirmed`，不等价于 ACP 认证、生成或底模解析；能力仍可未知。真实 CLI schema 尚未得到本机证据。

新增 `devin_acp/protocol.rs`、`transport.rs`：ACP v1 初始化不宣告宿主文件/终端能力；根据实际 `category=model` 选项设置，完整返回配置且当前值匹配才确认 effective，resolved_model 仍可未知。JSON-RPC 有 1 MiB 帧上限、单调请求 ID、超时失效边界；权限返回 cancelled，未接入的文件/终端调用拒绝。握手连接不允许 `session/prompt`，不把通知当成工具执行事实。scope 比较覆盖工程、聊天室、Agent、run、turn、attempt、owner epoch 和 generation；这是协议基础，尚未接入真实运行台账。

新增 `GET /api/backends/devin/status`、`POST /api/backends/devin/models`。原普通/流式/接力聊天在附件和消息接纳之前返回未就绪错误；共用 HTTP client 入口也有门禁。Devin 的 `agent_execution_ready` 保持 false，不能把配置下拉框视为完整能力。

后续逐入口检查补齐了内部 Goal/演示响应、模型工具循环和直接流入口的门禁，拒绝发生在图片预处理和上下文构建之前。内部失败响应沿用根超时的旧 `model_tool_calls_executed` 控制标记来禁止工具意图兜底；实际 `tool_write_executed`、`used_real_model` 为 false，没有工具请求或执行事件。该控制标记不能作为 P1 的真实执行证据，后续统一事件服务应采用明确的执行状态。

首次完整回归发现已有 shell 失败断言只识别英文报错或旧字段，本机实际 cmd 输出中文、状态已正确为 Failed；只补中文真实报错断言，未修改生产执行逻辑。

## 验证

最终结果：

- `cargo build -p coolzhu-web-console --offline --target-dir modules/gui-web/target`：通过。产物为 `modules/gui-web/target/debug/coolzhu-web-console.exe`；既有警告及尚未接线协议基础的 unused 警告保留，不掩盖。
- `cargo test -p coolzhu-web-console --offline --target-dir modules/gui-web/target -- --test-threads=1`：主控制台 1314 通过、0 失败、2 原有忽略；包内其他两个测试目标合计 9 通过，doc-tests 通过；最终主程序复跑约 99 秒。新增配置、目录解析、协议/身份/模型确认/权限拒绝、进程超时及内部入口门禁测试包含在其中。
- 前端 `node --test .../tests/*.test.cjs`：12 通过、0 失败；`ui-control-contracts.cjs` 通过；两份修改的 JS 语法检查通过。
- `cargo check -p coolzhu-tool-registry --offline`：通过；`module_linkage_smoke`：8 通过、0 失败；均复用 `modules/gui-web/target` 缓存。
- `git diff --check`：通过。大文件仅作小范围接线，新增职责拆成独立文件，UTF-8 和既有换行保留。

最终代码日志：`tmp/analysis-devin-build-validated.log`、`tmp/analysis-devin-web-regression-validated.log`；另有 `tmp/analysis-devin-all-js-tests.log`、`tmp/analysis-devin-ui-contracts.log`、`tmp/analysis-devin-tool-registry-check.log`、`tmp/analysis-devin-module-linkage.log`。首次回归失败证据保留在 `tmp/analysis-devin-web-regression.log`，没有删除以伪装一次通过。

隔离 API 已验证：创建会话、拒绝 HTTP 密钥、保存 backend_kind、未知能力空值、拒绝采样覆盖、失败保存保持 revision/模型、三个聊天入口返回 503 且未写入会话消息、重启后恢复同一模型和后端。证据：`tmp/analysis-devin-api-smoke.json`。工程、日志和数据库在 `tmp/devin-api-smoke-*`，测试进程已退出。

前端行为测试覆盖两个配置入口：隐藏 Key 不随 Devin 查询或保存发送；查询失败保留草稿；选择模型由用户点击触发；连接切换清理不适用参数；原 HTTP 查询与一次性密钥继续工作；Devin 模型名不被推测为图片或高思考能力。既有界面契约及语法检查亦执行。使用模拟 DOM，不宣称完成可视 GUI 验收。

正式安装版后台 18860、桌面 19220 仍运行，8765 健康接口 HTTP 200。未替换正式二进制、改变工作区选择或操作正式聊天数据。

## 未完成的接入条件

这批代码是 P0 工程基础、P1 路由门禁、P2 配置的一部分，尚不是完整的 Devin 会话执行后端。本机没有已验证的 CLI 安装、认证、实际目录 schema 或 ACP 能力；模拟模型与协议样本不代表官方实际支持。

仍需实现并验证会话服务、文本事件与取消、受控工具桥、上下文/技能/记忆快照、Goal/子 Agent 父关系、重连未知态与持久化对账；并先证明原生 Windows 内建 edit/exec、配置、MCP、子 Agent 和后台进程不能绕过宿主权限。PR72 Cloud client/store 的适配、后台 observer、费用及远程产物继续按计划推进。

本轮无真实 Devin 生成、收费任务、推送或 PR 合并，未把本地修改重新打包安装。正常运行的 0.2.63 仍是修改前已核验安装版。
