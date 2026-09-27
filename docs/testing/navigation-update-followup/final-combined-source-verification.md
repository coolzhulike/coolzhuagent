# 2026-09-27 源码组合验证与视觉用量补丁复验（候选包打包前）

本记录区分两次 **debug 源码构建**，均非 0.2.18 候选 MSI。首轮 `cargo build --workspace --offline` 通过，日志为 `tmp/final-combined-workspace-build-final.log`；当时 Web SHA-256 为 `D6209492D550BB49F44B728F370F5A04D53516F6C159CC0BD77580275D138C9C`，CLI 为 `0A7C38A32C72AC8E20C0ADA29D00C3C29B4B2FBB704087DEE9EC313CFD0C1A06`。之后发现并窄修视觉转述请求缺少父运行用量关联、仅有推理无最终回复时的兜底正文复制推理问题；补丁后 Web SHA-256 为 `4AF5A03ACC09CA4F7FBCF6C14AF5E01D60D8C5105A7B7FFF8AC9FD8BC8F9F965`，验证见下文。首轮 CLI、Goal、终端实操只归属于首轮 Web 身份，不追认到补丁后的哈希。

## 首轮组合构建的离线自动测试

- `cargo test -p coolzhu-web-console --offline`：主二进制测试 1255 通过、0 失败、2 ignored；另 lib 8、browser-native-host 1 项通过。日志 `tmp/final-combined-web-tests-final.log`。两项 ignored 分别为真实云模型 CU 对照（消耗预算）和需本地官方 SDK server 的 MCP 用例；MCP 官方 SDK 已在本轮另作显式隔离验证。最初两轮旧 UI 文案/一次性 PowerShell 静态断言失败保留在 `tmp/final-combined-web-tests.log`、`tmp/final-combined-web-tests-2.log`；修正测试后 P5 定向 1/1 和最终全套通过，未为此改产品代码。
- 五个受影响底层 crate 用单线程离线全套通过：core-runtime 355、language-service 7、plugin-system 37、tool-registry 52、windows-process-guard 54 项；各自默认 ignored 计数为 1、1、0、0、3。日志 `tmp/final-combined-support-tests-serial.log`。
- `coolzhu-command-router` 20/20，通过日志 `tmp/final-combined-command-router-tests.log`；根 `module_linkage_smoke` 8/8，通过日志 `tmp/final-combined-module-linkage-smoke.log`。
- 四份变更 JS 的 `node --check` 与 `git diff --check` 通过；后者只输出 Git 工作树 CRLF 提示，无空白错误。

底层并行测试首轮 `core-runtime` 的 `prompt::tests::load_system_prompt_reads_claw_files_and_config` 在 Windows 删除临时目录时遇到占用错误 32（同轮 354 项通过），原始日志 `tmp/final-combined-support-tests.log`。该用例独立复跑 1/1 通过，五 crate 单线程全套也通过；**未找到并修复根因**。候选是别的并行测试启动的子进程继承了测试临时 cwd，特别是未设置 `workspace_root` 的 MCP 测试进程；目前只是代码层候选，不能称作本次错误的已证实原因。

## 首轮构建二进制隔离实操

均使用新的 `tmp/` 工作区、随机本机端口与本地假模型，未请求用户的 8765 服务，也未调用云模型。

- CLI → Web → Agent：CLI 退出 0，父 `done.status=completed`，Agent 工具事件 `requested → running → completed`。假模型请求 3 次，子请求用量入库恰 1 条（11 输入、7 输出 tokens）；公开 run 的轨迹包含 `chat`、`child_agent`、`tool_feedback` 各一次；父子最终标记均出现在 CLI 输出。脱敏结果 `final-combined-cli-evidence-sanitized.json`；隔离原件 `tmp/s25-cli-e2e-4e54668c64/`，日志 `tmp/final-combined-cli-agent-e2e.log`。Web 与 CLI SHA 分别匹配上文。
- Goal 工程固定：模型请求执行中切换工程返回 409、完成后返回 200；原工程未在执行中改变。Web SHA 匹配上文；结果 `tmp/s25-goal-pin-2478dda560/result.json`，日志 `tmp/final-combined-goal-pin-e2e.log`。该验收为受控模型单阶段，不代表云模型或所有 Goal 子链路。
- 终端 HTTP：默认工程写入 grant 拒绝创建，完整访问后创建、中文命令真实执行、同范围续接及输出游标正确；降回工程写入后拒绝新写入/读取输出，但允许自有 Ctrl+C/关闭；中断长命令后 shell 续用，跨房间旧句柄拒绝。Web SHA 匹配上文；结果 `tmp/s54-terminal-e2e-a1b6b30a72/result.json`，日志 `tmp/final-combined-terminal-http-e2e.log`。右栏页面此前已用相同源码的较早 debug 构建完成独立实操和截图；本轮最终二进制补 HTTP 契约，未重复页面截图。

## 补丁后 Web 构建及真实请求复验

- `cargo build -p coolzhu-web-console --offline` 通过，补丁后 Web SHA-256 `4AF5A03ACC09CA4F7FBCF6C14AF5E01D60D8C5105A7B7FFF8AC9FD8BC8F9F965`，日志 `tmp/vision-usage-reasoning-web-build.log`。只改 `main.rs` 与 `multimodal_input.rs` 的身份传递、兜底文案和最小回归；未改模型 URL、密钥或旧用量记录。
- `cargo test -p coolzhu-web-console --offline`：主二进制 1256 通过、0 失败、2 ignored；另 lib 8、browser-native-host 1 项通过，日志 `tmp/vision-usage-reasoning-web-tests.log`。新增仅推理兜底定向用例也单独 1/1 通过，日志 `tmp/vision-usage-reasoning-targeted-test.log`。其余 crate 本轮未改，不以首轮测试假称补丁后重新全工作区测试。
- 隔离 HTTP 真请求：一张受控 1×1 PNG 发送给纯文本会话时，视觉模型实际收到图片并返回文字描述，目标文本模型只收到描述而未收到图片；两次模型请求各自用量恰一条，分别为 11/7、5/3 tokens。视觉请求保持真实视觉会话 ID，两条用量同房间、同内部 provider turn、同已接纳 run；单轮轨迹 `vision_description` 与 `chat` 各一次且 attempt ID 唯一。公开 SSE turn 与内部 provider turn 本来不同，靠 run 关联，未改双身份。脱敏结果 `vision-usage-reasoning-evidence-sanitized.json`，原件 `tmp/vision-usage-run-e2e-4a7243f086/result.json`，日志 `tmp/vision-usage-reasoning-e2e.log`。首轮脚本曾错误要求两种 turn ID 相等而失败；修正测试口径后真实行为通过，保留原失败日志 `tmp/vision-usage-run-e2e.log`。
- 同一隔离测试的仅推理流式响应：过程标记保存在一条 `reasoning` 消息，最终 `assistant-error` 正文不含标记且有错误诊断事件。这一真实场景走“模型结束但无回答”分支；新增的 `assistant-fallback` 兜底函数分支由上面的定向 Rust 用例覆盖，两者不混称同一路径。

主要变动范围是 Host Agent/Goal/用量轨迹、MCP/LSP/终端宿主、插件事务、更新检查、右栏/顶栏与任务 UI。`.playwright-cli/` 是隔离实操产生的未跟踪浏览器日志，不纳入候选包或提交。此记录仅证明打包前源码构建，候选包与安装结果见对应报告。
