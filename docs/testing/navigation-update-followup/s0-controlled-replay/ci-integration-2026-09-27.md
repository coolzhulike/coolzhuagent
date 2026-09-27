# S0 六类受控回放的正式驱动与 CI 接入

日期：2026-09-27。此记录是[0.2.21 原型报告](report.md)的后续，不改写原版命名冲突复现、初期隔离失败或旧清单 `list_directory` 原样失败。新驱动在 [`tests/integration/s0_controlled_replay.py`](../../../../tests/integration/s0_controlled_replay.py)，继续使用真实 Web 接纳、工具派发、聊天轨迹、用量和持久化状态；本地假模型只提供受控响应，并没有另建 Agent 运行器。

## 本机完成的结果

开发态 `target/debug/coolzhu-web-console.exe` SHA-256 为 `0EF736167487C4EEE51DF45D522FB60D5F5B78E5BF3CE9FFA8B943E34FDC28ED`。执行：

```powershell
python tests/integration/s0_controlled_replay.py --web-binary target/debug/coolzhu-web-console.exe --output-dir tmp/s0-ci-local --all
```

退出码 0；[脱敏汇总](ci-debug-summary-2026-09-27.json)记录 `passed=true`、六类主场景、三种拒绝。流式 `read_file`、非流式 `glob_search`、长文件 `write_file` 均真实派发并完成两次模型往返；长文件按具名临时路径逐字节比对原定内容并记录 SHA-256。图片场景分别核对原图直传、视觉转述及请求用量归属；跨轮场景确认原始工具结果保留于审计但未污染下一轮模型请求。空最终回复的业务 run 为 `failed`，这是该负向输入的预期诊断终态，测试本身通过。

未知指纹、参数偏移、超具名临时路径三种负例均由**假模型回放白名单**拒绝，真实 Web run 为 `failed`，provider 接纳请求 0、持久化工具调用 0、外部哨兵未改变。这是测试夹具的副作用边界，**不表示产品正常运行时有同一套路径白名单沙箱**。另以 `Cargo.toml` 冒充 Web 可执行文件进行故障负控，驱动退出码为 1；脚本对报告写入和进程/服务器关闭使用嵌套 `finally`，不会因证据写入失败跳过自身清理。

每个场景使用新的临时目录、随机 localhost 端口、只含必要系统变量的子进程环境和假 Key。`USERPROFILE`、`HOME`、`APPDATA`、`LOCALAPPDATA`、`CLAW_CONFIG_HOME`、`COOLZHU_RUNTIME_DIR`、`COOLZHU_LOG_DIR`、XDG 与系统临时变量均指向该目录，工作区和日志在服务就绪后核对。不继承 `COOLZHU_WEB_STATIC_ROOT`，因此 CI 验证构建时内联的前端资源。新脚本只允许输出到仓库 `tmp`、系统临时目录或 CI 的 `RUNNER_TEMP`，并仅终止它自己启动的 Web 子进程与假模型服务器。脚本生成的 20 份证据 JSON 检查未含假 Key 明文、长文件正文、原始 provider payload 对象或用户绝对路径；产品自身隔离会话库保留在忽略的临时运行目录，不作为公开证据上传。

## CI 与边界

`.github/workflows/s0-baseline.yml` 现在在 Windows runner 构建 Web 后，使用该次 `target/debug/coolzhu-web-console.exe` 运行全部六类和三拒绝，再继续原有 Rust 回归测试；作业上限 45 分钟。驱动各 HTTP 请求有明确超时，服务就绪检查与子进程退出等待也有限。**本机已跑通，远端 CI 尚未运行**；最终源码及 0.2.22 候选包的二进制身份仍须后续构建确认。

旧 `tests/fixtures/s0-golden/manifest.json` 继续作为协议说明，旧 `list_directory` 未暴露的失败事实保留；新受控录制用当前真实 `glob_search` 覆盖同类目录查询行为，不称历史会话逐字回放。显式 text 但模型名含 image 的已确认产品缺陷，由[模型媒体路由窄修验证](../model-media-routing-narrow-fix-2026-09-27.md)单独复验，本 CI 驱动不重复其 11 个场景。这里也不证明云模型行为、原生桌面、Paint 或已安装包验收。
