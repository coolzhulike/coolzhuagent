# UPD-01 应用更新第一阶段工作记录

日期：2026-09-27。代码分支：`codex/navigation-update-followup`。

## 已实现

- 新增 `GET /api/system/app-update`：只读本进程最近检查结果，不发起网络请求。
- 新增 `POST /api/system/app-update/check`：仅在显式请求时访问固定的 `coolzhulike/coolzhuagent` GitHub 正式稳定版 Release API。请求无 URL、凭据或代理参数；HTTPS 校验、禁止重定向、连接与总超时、64 KiB 响应上限均在后端落实。保留系统代理支持。
- 使用打包时注入的 `COOLZHU_RELEASE_VERSION` 作为当前产品版本。未携带有效发行版本的开发构建显示版本未知，不把 Cargo 的 `0.2.0` 或 Git 哈希当成 MSI 版本。`build_version` 仍独立显示构建信息。
- 用 `semver` 解析和比较正式版。404、限流、非法元数据、网络失败、当前版本未知分别保持可区分的状态；只有成功取得正式版且当前版本可比较时，才可能显示 `up_to_date`。
- 发布页地址从后端固定的官方仓库路径与已验证的版本标签生成，不采用远端 JSON 中的任意链接。

本阶段仅提供检查与发布页入口。下载、安装、重启以及一键升级仍需独立完成包身份、活动任务保护和迁移恢复闭环；接口不会执行这些动作。

## 验证

- `cargo build -p coolzhu-web-console --offline` 已通过；最终编译结果见 `tmp/app-update-build-final.log`。
- `cargo test -p coolzhu-web-console --offline app_update::tests`：6 个定向测试通过，覆盖版本比较、正式发布过滤、发布页路径、HTTP 错误分类、GET 缓存和检查失败的 JSON 契约。日志见 `tmp/app-update-tests.log`。
- 最初测试发现发布页基址尾斜杠会生成 `/tag//v...`，已修复并保留精确地址断言。
- GitHub CLI 在当前机器不可用，网页工具也未能读取公开 API，因此此处不声称已核实线上是否存在正式 Release；用户显式检查时由产品接口获取实时结果。
