# P6-E.4b Provider / Reasoning 兼容实现记录

## 时间边界

- 阶段始于 2026-08-31：承接已完成审核的 `llm-adapter` reasoning 能力层，Web 控制台改为消费适配器能力目录。
- 实际代码执行与实现期验证发生于 2026-09-01（Asia/Shanghai）。

## 本轮实现

- `/api/models/capabilities` 改为遍历 `api::reasoning_capability_catalog()`，返回 provider/model canonical id 与 label、上下文/最大输出、model_type、reasoning 选项明细、default、strategy、protocol、status、note、deprecated，并保留旧字段。
- 每个能力条目额外由适配器 resolver 预计算 8 个 canonical requested preflight 选项（`auto/none/minimal/low/medium/high/xhigh/max`），包含 `effective/status/reason/selectable`；unsupported 才不可选，downgraded 保留可选。UI 不接触 `preflight_wire`。
- 移除 Web 端 provider model 硬编码表与 reasoning 硬编码矩阵；供应商、模型、思考程度选择器由能力接口驱动。能力接口失败时仅保留当前会话值并将 reasoning 收敛为 `auto`。
- 正常能力加载时供应商显示名来自 DTO 的 `provider_label`；静态显示名表仅作为历史值/能力请求失败时的兜底。模型选择器过滤 deprecated，但已有会话的当前模型可兼容回显；静态 HTML 不再提供模型选项。
- 新建/更新会话对 reasoning 使用适配器 strict parser；更新在可变会话借用和字段变更前完成验证。SQLite 新 schema/缺列默认 `auto`，SQLite/JSON 读取保留 raw 字符串，summary/agent 通过 legacy resolver 暴露 canonical requested/effective resolution。
- Agent/summary 保留 `reasoning_effort`（canonical requested），增加 resolution；请求构建传 canonical requested，不再使用旧 normalizer。
- 新建选择器移除 Moonshot；Ollama 与自定义统一到 `custom` / OpenAI-compatible；deprecated 模型仅在已有会话兼容回显。
- 接入 `data-role=session-reasoning-hint`，界面只显示“配置：X · 预计：Y”，降级/不支持/legacy 时补充原因，不展示 wire/JSON/debug 信息。

## 真实验证证据

- `cargo build -p coolzhu-web-console --offline`：通过（2026-09-01）；构建输出见 `tmp/qa-p6e4b-web-impl-2026-09-01/build.log`。
- `node --check modules/gui-web/packages/web-console/src/app.js`：通过（2026-09-01）；输出见 `tmp/qa-p6e4b-web-impl-2026-09-01/node-check.log`。
- 本轮未启动应用、未触碰 8765、未执行 `cargo test` 或其它正式测试；源测试契约仅添加/更新，未运行。

## 已知边界

- reasoning 能力目录是本轮 provider/model reasoning 的事实来源；context/max/model_type 的容量值仍通过既有 token 表回退。新模型若尚未进入该表，会得到既有保守默认值，本轮未扩张双注册表范围。
- provider 旧显示名只在解析/UI 边界做最小 canonical 映射，持久层历史 provider/reasoning 原始值不被批量改写。
- preflight 的 context/max/model_type 仍允许沿用既有 token 表回退值；本轮没有扩张双注册表范围，也没有运行源测试契约。

## P6-E.4d 失败证据与 P6-E.4e 修正

- 阶段始于 2026-08-31；P6-E.4d 的独立视觉证据记录于 `tmp/qa-p6e4d-rerun-test-2026-09-01/22-visual-ui.log`。该记录显示：1280 宽设置宿主的三个分组各约 78px 且提示被裁切；快捷栏设置入口与左栏 toggle 坐标重叠，首次真实坐标点击命中左栏；Custom 自定义模型输入阶段把原 requested `medium` 重建为 `auto`；控制台有 1 条 iframe sandbox 安全 warning。此处只记录已有证据，不宣称本回合运行正式测试。
- P6-E.4e 仅改实现：settings 宿主使用 `grid-auto-rows: max-content` / `align-content: start`，由 `settings-layout` 单独纵向滚动，子 section 随内容展开；1280 桌面保留右栏网格列，overlay 断点与前端 compact 判断统一收窄到 980px；closed More 的低频入口 `display: none`，展开后回到 dock 内容流；Custom reasoning 查询缺少精确模型时回退已加载 API 的 `custom::custom-model` preflight，保留旧 requested 的兼容回显，不改全局模型类型查询；iframe sandbox 原样保留。
- 新增少量源契约，覆盖 settings/dock CSS、1280 桌面断点及 Custom reasoning 回退；未运行这些测试。

## 后续验收范围

- 独立测试会话需确认 1280 宽左右栏同时打开时中央聊天与输入区仍在网格剩余列，收起后宽度自然回收；More 展开后的设置入口可点且不命中布局 toggle；设置三组内容及 reasoning hint 可滚动完整阅读。
- 需确认 Custom 已有 `medium` 等合法 requested 在输入模型、base URL、endpoint 后仍保留为兼容回显并预计降为 `auto`；新建 Custom 仍只有可选 `auto`。
- iframe sandbox warning 作为已知非阻断限制继续记录，不以删 token 或关闭 console 监听掩盖。

## P6-E.4e 实现期检查证据

- 实际执行于 2026-09-02（Asia/Shanghai）：`cargo build -p coolzhu-web-console --offline` 通过，日志为 `tmp/qa-p6e4e-web-impl-2026-09-02/build.log`；`node --check modules/gui-web/packages/web-console/src/app.js` 通过，日志为 `tmp/qa-p6e4e-web-impl-2026-09-02/node-check.log`；`git diff --check` 通过，日志为 `tmp/qa-p6e4e-web-impl-2026-09-02/diff-check.log`。
- 本回合未运行 `cargo test`、未启动应用、未触碰 8765；视觉与行为验收仍待独立测试会话执行。

## P6-E.4f / P6-E.4g 测试稳定性记录

- P6-E.4f 独立全量复测第 1 遍为 921/921；第 2 遍唯一失败为 `chat_dispatch_routes_semantic_hotkey_to_tool_agent`，失败证据见 `tmp/qa-p6e4f-independent-test-2026-09-02/cargo-test-web-console-run2.log`。该测试未持有既有 `config_test_guard()`，可与临时替换全局 `session_store()` 的其它夹具并行，因而偶尔看不到 `mario-demo`。
- P6-E.4g 仅在该测试开头加入 `let _guard = config_test_guard();`，未改变生产逻辑、400 筛选语义或其它测试。此处记录根因与修正，不宣称本回合运行正式测试。

## P6-E.4h 最终验收记录

- 独立全量测试最终连续两遍均为 921/921，通过数 921、失败数 0；最终双通过证据仅记录在 `tmp/qa-p6e4h-final-test-2026-09-02/`。
- `tmp/qa-p6e4f-independent-test-2026-09-02/` 对应完整 UI 交互复测背景，其第二遍测试曾出现一个已修复的并发夹具失败，不与 P6h 最终双通过证据混称。
- 视觉验收通过：1280 宽侧栏不覆盖聊天，设置分组可滚动完整阅读，Custom requested 值保留；完整设置/Custom 交互来源为 `tmp/qa-p6e4f-independent-test-2026-09-02/`，P6h `tmp/qa-p6e4h-final-test-2026-09-02/` 仅作最终快速截图与状态复核。
