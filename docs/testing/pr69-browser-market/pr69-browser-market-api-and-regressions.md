# PR69 浏览器与 DSH 市场回归简报

- 验证日期：2026-09-28（Asia/Shanghai）
- 验证代码：`137546a`
- 本批范围：PR69 指定的 5 条前端结构回归、DSH 市场仓库 URL 单元回归，以及隔离端口上的真实 DSH 市场 HTTP 接口。
- 本批不包含原生 Computer Use、微信、模型调用或模型夹具；未改动生产代码、用户工程、用户模型配置和 API 密钥。

## 构建证据

以下构建均使用离线依赖，结果为成功：

- `cargo build -p coolzhu-web-console --offline`：`tmp/pr69-browser-market/cargo-build-web-console.log`
- `cargo build -p coolzhu-clawbot-sidecar --offline`：`tmp/pr69-browser-market/cargo-build-clawbot-sidecar.log`

sidecar 仅编译验证，未运行微信功能，不能当作微信实测通过。

## 5 条结构回归与 DSH URL 校验

每条测试命令均实际显示 `running 1 test`，不是空筛选。首次布局测试日志保留，修正真实多 class 选择器后用独立 `-retest` 日志复跑。

| 精确筛选 | 结果 | 证据 |
|---|---|---|
| `tools_catalog_exposes_core_plugins_skills_and_safe_actions` | 1 passed | `tmp/pr69-browser-market/test-tools_catalog_exposes_core_plugins_skills_and_safe_actions.log` |
| `web_frontend_chat_media_terminal_browser_and_self_update_panels_are_wired` | 1 passed | `tmp/pr69-browser-market/test-web_frontend_chat_media_terminal_browser_and_self_update_panels_are_wired.log` |
| `web_frontend_exposes_real_management_and_diagnostic_controls` | 1 passed | `tmp/pr69-browser-market/test-web_frontend_exposes_real_management_and_diagnostic_controls.log` |
| `web_frontend_goal_event_streams_are_limited_to_active_goals` | 1 passed | `tmp/pr69-browser-market/test-web_frontend_goal_event_streams_are_limited_to_active_goals.log` |
| `web_frontend_v3_batch2_matches_approved_compact_workbench_layout` | 首次因本轮替换时遗漏 `window-panel` class，导致新断言未匹配；修正后 1 passed | 首次：`tmp/pr69-browser-market/test-web_frontend_v3_batch2_matches_approved_compact_workbench_layout.log`；复测：`tmp/pr69-browser-market/test-web_frontend_v3_batch2_matches_approved_compact_workbench_layout-retest.log` |
| `dsh_repo_rejects_untrusted_url_shapes` | 1 passed | `tmp/pr69-browser-market/test-dsh_repo_rejects_untrusted_url_shapes.log` |

## 隔离实例与真实接口

为避免影响已安装的 8765 实例，使用绝对路径
`C:\Users\zhupu\Desktop\coolzhuagent\target\debug\coolzhu-web-console.exe`，独立运行目录
`tmp/pr69-browser-market/dsh-runtime-8766`，配置绑定 `127.0.0.1:8766`，并通过隐藏窗口方式启动。实例 PID 为 19320，所有请求均发往本机 `http://127.0.0.1:8766`；服务 stdout/stderr 分别保存在该运行目录下。测试结束后已停止且确认 8766 端口释放。

接口首次采集使用 PowerShell `Invoke-RestMethod`，HTTP 状态与结构字段正确，但中文 body 在采集器展示中出现了编码错位。为排除采集器影响，后续只对必要场景用 .NET `HttpClient` 读取原始字节，再用 `System.Text.Encoding.UTF8` 解码；`dsh-api-utf8-*` 文件是中文正文的权威证据。服务返回的中文正文正常，不能把旧采集器的乱码归因于产品。

### 接口结果

| 场景 | 实际结果 | 正文核验 |
|---|---|---|
| 首页前 24 条 | HTTP 200；总数 4377；第 1 页，每页 24；首项 `AnonyJcy/dsh-j-space` | `list.items` 数量 24，工作区标识存在 |
| 按真实条目名称搜索 | HTTP 200；返回含首项 `AnonyJcy/dsh-j-space` | 目标条目在结果中 |
| 中文搜索 `架构` | HTTP 200；第 1 页，总数 22 | 22 条结果的名称、所有者或中文描述均含“架构”；UTF‑8 正文含正常中文 |
| 分类 `agi` | HTTP 200；15 条 | 每条 `categories` 都包含 `agi` |
| 第 2 页 | HTTP 200；`page=2`，24 条 | 与第 1 页条目 ID 无交集 |
| 真实条目详情 | HTTP 200 | `detail.item.id` 与列表条目 ID 一致 |
| 真实兼容性检查 | HTTP 200；`status=incompatible`；`installable=false` | `reason` 非空，UTF‑8 正文为“仓库根目录未发现 `plugin.json` 或 `.claw-plugin/plugin.json`，当前加载器无法安装。” |
| 错误工程标识 | HTTP 409 | 按接口契约拒绝，属于预期结果 |
| 不存在的详情 ID | HTTP 404 | 按接口契约返回“市场条目不存在”，属于预期结果 |

409/404 在早期辅助摘要中 `ok=false` 只表示非 2xx 分支，不能当作测试失败；正文和预期状态均已单独断言。

完整首次响应与摘要：

- `tmp/pr69-browser-market/dsh-api-responses.json`
- `tmp/pr69-browser-market/dsh-api-summary.json`

UTF‑8 重新解码后的正文与摘要：

- `tmp/pr69-browser-market/dsh-api-utf8-list.json`
- `tmp/pr69-browser-market/dsh-api-utf8-search-chinese.json`
- `tmp/pr69-browser-market/dsh-api-utf8-detail.json`
- `tmp/pr69-browser-market/dsh-api-utf8-compatibility.json`
- `tmp/pr69-browser-market/dsh-api-utf8-summary.json`
- `tmp/pr69-browser-market/dsh-api-body-assertions.json`（归档副本：`evidence/dsh-api-body-assertions.json`）

`dsh-api-body-assertions.json` 中全部正文断言为 `true`，包括分类匹配、分页不重叠、详情 ID、兼容性状态、`installable=false`、错误状态及中文 UTF‑8 搜索。

## 测试边界

本简报只证明离线静态回归和隔离 HTTP 接口行为。它不替代原生桌面右栏、真实 Qwen 会话、微信、远程市场 UI 或 Computer Use 验收；这些项目需在对应可用环境和明确范围内单独记录。
