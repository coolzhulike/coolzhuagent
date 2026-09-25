# 2026-08-23 最终编译、实机自检与 release 打包

## 范围

- 源码提交：`ff5ee2f910c38b3629259f50c99c7dddbf281137`
- PR：[#58](https://github.com/coolzhulike/coolzhuagent/pull/58)
- 系统：Windows；MSVC 2022 Build Tools，`link.exe` 可用
- 已配置会话：GLM-5.2、agnes（本轮隔离运行时不发送真实模型请求）
- 重点：推理过程显示、Windows UIA、工具/Computer Use dry-run、语音状态、release 包启动

本文不记录 API Key、Token 或凭据内容。

## 修复

- UIA StartButton 探测改用 `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass`，并在脚本失败时保留退出状态与 stderr；执行策略不再导致空 stdout 的误报。
- CLI 思考块测试与现有行为对齐：断言 `ReasoningDelta` 和 `[reasoning]` 输出，而不是错误地期望 thinking 被忽略。
- 对 web-console 音频/资源相关 Rust 文件完成 rustfmt，避免最终构建重新产生格式差异。

## 编译与测试

| 检查 | 结果 |
| --- | --- |
| `cargo build -p coolzhu-web-console --offline` | PASS |
| `cargo test -p coolzhu-web-console --offline` | PASS，811/811 |
| `cargo test -p coolzhu-core-runtime --offline` | PASS，181/181 |
| `cargo test -p coolzhu-computer-use-core --offline` | PASS，38/38 |
| `cargo test -p coolzhu-command-line --offline` | PASS，77/77 |
| `cargo test -p coolzhu-tool-registry --offline` | PASS，39/39 |
| `cargo test -p coolzhu-uia-resolver --offline` | PASS，2/2；真实 StartButton 坐标 `527,1008,68x72` |
| `cargo test --workspace --offline --no-fail-fast --quiet` | PASS（第二次全量重跑）；第一次并行运行中 tool-registry 进程出现一次 `STATUS_STACK_BUFFER_OVERRUN`，单独重跑与全量重跑均通过 |
| `cargo test --test module_linkage_smoke --offline` | 当前源码快照没有根 `tests/` 目录/该 test target，命令不可用，不是断言失败 |

本轮相关日志位于 `tmp/logs/`：`workspace-test-selfcheck-rerun-20260823.log`、`uia-resolver-full-selfcheck-20260823.log`、`tool-registry-test-default-20260823.log`、`web-console-build-final-selfcheck-20260823.log`。

## 实机运行自检

使用最终 source debug binary 和 package release binary 各在隔离运行时 `127.0.0.1:8766` 启动：

- `/api/system/info`、`/api/diagnostics/health`、`/api/audio/status`、`/api/computer-use/capabilities`、`/api/tools/catalog` 均返回 HTTP 200。
- release 包接口报告 `ff5ee2f910c3 · local`，工具目录 5 类/54 项，视觉 `vision.find_target` dry-run 能解析 bbox 并返回屏幕点 `(640,360)`，`executed=false`。
- UI 界面实测：标题 `COOLZHU CODE 控制台`，1280×720、DPR 1.5，聊天室/视觉实验室节点存在，浏览器 console warn/error 为 0。
- Computer Use：Windows desktop surface available；本机没有 ShowUI 本地模型，标记 `skipped`；浏览器 extension/native host 未连接，Browser surface 标记 `skipped`。未执行真实键鼠、浏览器提交或模型网络请求。
- 语音状态：本地 STT/TTS 模型文件可见，音频接口 200；非法 WebM payload 能明确返回容器校验错误，不崩溃。

隔离运行日志：`tmp/logs/live-selfcheck-final-8766-stamped.json`、`tmp/logs/packaged-release-selfcheck-8766.json`、`tmp/logs/live-selfcheck-audio-invalid-8766.json`。

已安装旧版本 `5d39cd5794b0 · 2026-08-20`（8765）恢复运行，健康接口 HTTP 200，DeepSeek、agnes、GLM-5.2 三个会话均为 ready；旧安装包尚未被覆盖。

## Release 包

- MSI：`dist/CoolzhuAgent-0.2.5-20260823-142454.msi`
- SHA-256：`AC3E1338C593BB6B7B22B89CCA25B2E5367C8372CCE9E767B599E808C4E6C932`
- WiX：`5.0.2+aa65968c`
- 包安全扫描：756 个文件，`safe=true`，`findings=[]`
- `test-package-manifest.ps1`、`test-package-safety.ps1`：PASS

尝试用 `msiexec /qn` 更新现有 per-machine 安装时，因未提升权限返回错误 1925/1603；随后 UAC 提权提示被取消，未再次强行重试。详细日志：`tmp/logs/msiexec-install-final-20260823.log`、`tmp/logs/msiexec-install-elevated-final-20260823.log`。请在设备上对 MSI 右键“以管理员身份运行”完成安装。
