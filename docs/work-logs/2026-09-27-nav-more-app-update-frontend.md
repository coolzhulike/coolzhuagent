# 2026-09-27 导航“更多”与升级更新前端工作记录

## 范围

- `index.html`：保留首页、定时任务、设置、统计信息、微信连接五个原入口；增加“更多”折叠入口（浏览器、终端、SKILL、插件市场）及快捷栏底部“升级更新”按钮。增加 SKILL、插件市场和升级更新三个独立右栏面板。
- `src/workspace_panels.js`：为“更多”增加方向键、Home/End、Esc、外点关闭及焦点处理；子项仍走现有面板路由。浏览器和终端复用现有面板。
- `src/app.js`：SKILL 和插件市场仅读取 `/api/tools/catalog` 的 `skills`、`plugins` 分类；显示本地目录发现结果和详情，明确目录发现不等于已加载/已安装。插件市场明确远端市场与安装服务未接入，不发安装请求。
- `src/app.js`：升级更新读取 `GET /api/system/app-update`，用户按“检查更新”时发送无请求体的 `POST /api/system/app-update/check`。面板显示当前发行版本、构建标识、渠道、已发现正式版本、检查时间及状态；仅允许打开本仓库 GitHub Releases 的 HTTPS 页面，不提供下载、安装或重启入口。
- `src/workspace_panels.css`：延续玉石竹林配色，目录与更新面板响应窄屏；更新按钮固定在快捷栏底部，上方入口可纵向滚动。

## 真实性与边界

- 开发构建的 `current_version=null` 显示“未知（开发构建）”，不以 Git 构建标识伪装安装包语义版本。
- `unavailable` 仍显示“无法判断是否有更新”，即使后端已发现正式发布版本，也不声称当前版本是最新版。
- `check_failed`、`not_checked`、`checking` 不显示“最新正式版本”或发布页链接；请求失败时清掉旧检查结果。独立的检查中状态保证重新打开面板不会让只读 GET 覆盖 POST 检查结果或卡住按钮。
- SKILL 与插件卡片只写“目录已发现”或“安装状态未核验”，不把目录状态映射为已安装/已启用。

## 验证

- `node --check src/app.js`、`node --check src/workspace_panels.js` 通过。
- `node tmp/nav-update-smoke.cjs` 通过：覆盖更新检查的无 body POST、检查中重开、失败清理、开发构建状态，以及“更多”键盘、Esc、外点、路由行为。
- 四个前端文件 `git diff --check` 通过；仅出现仓库既有 CRLF/LF 转换提示。

## 联合验证待办

- 后端完成后统一执行 `cargo build -p coolzhu-web-console --offline` 与相应测试，确认新增 API、前端内联资源和路由一起编译。
- 在可用桌面测试时核对低高度快捷栏滚动、三个新面板实际布局、外链打开和发布状态文案；本轮未操作已有 8765 服务或原生窗口。
