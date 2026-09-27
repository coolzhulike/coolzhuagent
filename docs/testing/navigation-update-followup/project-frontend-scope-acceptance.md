# 工程目录前端作用域与写入绑定：开发构建验收

日期：2026-09-27。本文在 0.2.19 候选包成功构建后补记，**不属于该包的源码快照**。三张截图均取自 SHA256 为 `82C0C685E5382B40CD2BDF6AC9EB481F9A61F8C9868E5E6835392B0298B325CC` 的开发构建，**不是安装版截图**。所有页面实验只连接随机私有端口及 `tmp/` 工程，没有连接安装版 8765、真实模型或原生桌面。

## 身份与结论边界

- 第一轮前端源码构建 SHA256 `ADEFF6B30B3991A86B03635B7F2D9B02B9AFABC188C7885E19633F8F713B81B`：验证旧工程 tree/open 的延迟成功及错误响应不会回写新工程；同工程文件打开倒序保留最后选择；按工程内存缓存保留未保存草稿且同名路径不串用；保存期间继续编辑保持 dirty。
- 第二轮中间源码构建 SHA256 `585384ECE96B921D5F1EEFBE036E133BCDB79147E29115AAC8F6B2F4C0EA23DA`：验证 A-diff 切 B-empty/view 不沿用旧对比路径；旧搜索与行窗口响应切工程后不覆盖新显示；旧 rename 成功迟到只更新旧工程缓存标签。它仍不含后来的四写接口 `expected_workspace` 强制绑定，不能用于证明旧请求发送后的安全性。
- 最终源码候选 Web 构建 SHA256 `82C0C685E5382B40CD2BDF6AC9EB481F9A61F8C9868E5E6835392B0298B325CC`：包含上述页面保护、四处前端写请求的发起时工程快照及四个后端处理器的强制绑定。`cargo build -p coolzhu-web-console --offline` 成功，日志 `tmp/project-scope-bound-build.log`。此身份是开发构建，不能代替后续候选 MSI 或安装版身份。

## 最终源码构建的实测

同一最终源码构建的私有页面使用 `tmp/s54-terminal-ui-1baa6aa2fe` 工程及端口 59661。调用现有 UI 方法完成创建 `bound.txt`、保存内容、重命名为 `renamed-bound.txt`、删除；每步得到正常结果，最终标签和选中路径清空。[CRUD 收尾截图](project-frontend-scope-evidence/01-crud-finished.png)只展示测试文件删除后的页面状态；工程目录仍有配置及日志文件，不能单凭图证明此前三个步骤。实际调用结果已在本轮 Playwright CLI 输出记录。

第二次页面操作在 B 工程创建并编辑 `conflict-bound.txt`，编辑器中保留“草稿仍在旧工程，未保存”。仅通过后端 API 把活动工程切到 C，页面仍持 B 的已确认作用域，然后点击保存：返回 false，标签 dirty 仍为 true，编辑器原文保留，放弃修改确认框调用次数为 0，页面明确显示“工程目录已变化，请刷新工程文件后重试”。[409 与草稿同屏截图](project-frontend-scope-evidence/03-workspace-409-dirty-preserved.png)显示错误、黄色未保存标签和草稿；[编辑器近景](project-frontend-scope-evidence/02-dirty-editor.png)显示打开目录前的草稿。两次私有浏览器会话及服务 PID 29096 均已关闭。

后端直接 HTTP 验收由另一代理在同 SHA 构建的独立端口完成，原始脱敏结果 `tmp/project-mutation-scope-verify/182b786459/result.json`：四写接口分别使用旧 A、空值、缺失的 `expected_workspace` 共 12 个请求全部 409 且零写；声明当前 B 的保存、新建、重命名、删除四个请求全部 200；A 原文件不变。完整步骤已记在[工程写入归属验收](project-mutation-workspace-acceptance.md)。

四条现有 Rust 定向用例通过，各 1/1：`project_save_preserves_bom_crlf_and_refreshes_revision`、`project_save_detects_external_revision_conflict`、`project_mutation_status_contract_covers_400_403_409_413`、`project_rename_and_delete_open_file_backend_contract`；日志 `tmp/project-scope-bound-tests.log`。`node --check` 检查 `app.js` 及 `git diff --check` 均 exit 0。

本轮页面实操证明最终源码构建的请求字段和工程变化提示，未在新候选安装版上重复原生窗口操作；后续包与安装验收须另记其实际哈希。页面内 Map 仅在当前页面存未保存正文；刷新或重启后不能从 sessionStorage 恢复 dirty 草稿，sessionStorage 仅按已确认工程保存标签元数据。
