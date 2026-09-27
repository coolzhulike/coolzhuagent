# 0.2.12 发布与测试证据索引

本目录可与[发布报告](../../../2026-09-19-release-0.2.12-change-and-test-report.md)一并交给其它模型。JSON/SSE仅含合成测试数据及本机文件路径，没有用户真实会话、密钥或数据库。原始安装器日志、真实运行目录备份不在此处。

## 发布与资源

| 文件 | 能证明什么 |
| --- | --- |
| `installer-report.json`、`install-result.json` | MSI版本/哈希/构建配置/签名状态与安装器成功退出 |
| `installed-state.json` | 系统登记版本、产品代码、CLI版本及无旧index/src覆盖 |
| `package-artifacts.json`、`installed-artifact-verification.json` | 10个程序文件的构建来源和安装后hash一致 |
| `package-safety.json`、`package-checks.log` | 本次包803文件扫描结果、manifest和安全检查 |
| `runtime-data-integrity-after-install.json` | 安装前后51个原运行文件内容未变；没有包含这些文件内容 |
| `source-fingerprints.json` | 关键源码指纹；Git HEAD不能唯一代表此工作区 |
| `ui-source-hashes.json` | 12个内嵌资源及3个代表性外置资源基准 |
| `embedded-byte-verification.json` | 安装EXE内12份资源完整字节的位置/长度/hash；427件assets三方一致 |
| `http-resource-verification.json` | 禁止开发源码回退后，真实HTTP返回15/15与基准一致 |

## 安装后功能

| 文件 | 对应证据 |
| --- | --- |
| `api-results.json`、`api-regression.log` | 安装版8项隔离回归，0失败 |
| `settings-readback-redacted.json` | 保存后的统一参数；省略密钥 |
| `mock-requests.jsonl` | mock实际收到的工具名/参数/恢复标记；凭据只记录合成值是否匹配的布尔值 |
| `read-file-events.json`、`evidence-verification.json` | SSE思考/正文顺序与runtime真正读取文件，不只信任mock成功文案 |
| `read-file-insights.json` | 第一轮工具调用200输入/40输出，2次模型请求 |
| `search-around.json` | 搜索唯一最终回复，around与可见索引过滤内部事件 |
| `tools-off-events.json` | 显式完全访问下，关闭模型工具仍不执行文件调用 |
| `pseudo-ok-events.json`、`pseudo-fail.json`、`forbidden-events.json` | 流式伪调用成功恢复、非流式持续伪调用失败、越界结构化调用拒绝；次数有限 |
| `final-insights.json` | API脚本结束时主房间3请求、300/60；不是浏览器补测后的最终值 |
| `ui-results.json`及四份`ui-*-verification.log` | 实际DOM、下拉、尺寸、思考临时显示、搜索定位和用量检查 |
| `release-default-permissions.json` | 新配置未设置dev_open时，安装Release缺省权限关闭 |
| `restart-persistence.json` | 安装隔离服务重启前后的参数、索引、搜索、用量与耗时比较 |
| `restart-before.json`、`restart-after.json`、`ui-restart-verification.log` | 重启前后完整合成快照与浏览器重载显示；11/11断言通过 |
| `fixture-rerun-verification.json` | docs夹具在另一全新目录/端口复跑结果 |
| `acceptance-cleanup.json` | 验收完成后核对身份并关闭隔离服务和测试浏览器 |

阶段计数说明：第一轮工具请求2次（200/40）；API脚本主房间结束3次（300/60）；浏览器再发一轮工具请求后5次（500/100）。三者是不同时间快照，没有重复计费推断。

部分快照中`services_kept_running=true`记录测试阶段交接状态；本次交付前服务已清理。18775/18776与复跑18785/18786均不是日常8765服务。

截图均来自实际安装程序，不是概念图：

- [设置1280×720](settings-installed-1280.png)、[设置1024×600](settings-installed-1024.png)
- [生成中的临时思考](thinking-installed.png)
- [搜索定位](search-installed.png)、[用量统计](usage-installed.png)、[独立工具状态](tool-status-installed.png)

负向测试故意使其它隔离房间产生失败终态，截图中顶部“需留意/任务异常”属于该合成测试数据，不是正式用户会话状态。

## 解读限制

日志中 `tool_dispatch_summary` 的少量沿用旧文案提到“computer-use 工具侧路”，而同一证据实际工具名为read_file、route为runtime-executed。本测试全局关闭Computer Use；该文案需另行修正，不能据它判定发生了桌面操作。

无源码回退HTTP验收与内嵌字节验证共同证明本机安装包的前端完整性；仍不等于在所有干净Windows机器、所有升级历史上完成兼容验证。真实模型遵循工具调用协议的能力也需要独立复测。
