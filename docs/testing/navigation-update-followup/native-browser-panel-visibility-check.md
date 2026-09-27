# 原生浏览器右栏折叠修复：源码与内存行为检查

2026-09-27，已安装 0.2.18 在 `903×551` 贴靠窗口中出现右栏已折叠、`Example Domain` 子 WebView 仍覆盖聊天输入的情况：[原生失败截图](installed-0.2.18/16-low-height-browser-occlusion.png)。放大窗口或关闭浏览器工具后遮挡消失。本图是 0.2.18 的失败证据，不代表后续修复已通过。

`native_browser_panel.js` 原先依据 iframe 的布局矩形和浏览器工具选中状态决定子 WebView 是否可见；CSS `visibility:hidden` 的折叠栏仍保有矩形。现在对存在 `.chat-right-rail` 宿主的路径同时检查 `data-right-collapsed`、`aria-hidden`、`inert` 和计算后的可见性，并监听对应属性变化。隐藏时沿原有路径关闭子 WebView；展开时沿保留的 URL 再次导航。没有宿主的旧路径保留原判定，也没有修改原生权限与 resize 失效处理。

`node --check` 和 `git diff --check` 通过。内存假 DOM/Tauri 检查脚本 `tmp/native_browser_panel_visibility_check.js` 通过：隐藏但有矩形时关闭、展开用原 URL 重建、对话层遮挡时关闭、连续 20 次布局通知不重复导航、旧 scope 事件不影响当前页。这个检查不等于原生 WebView2 实操；连续通知去重也不证明真实快速拖窗没有抖动。销毁后重建保留 URL，不保证原网页的导航历史或未提交页面状态。后续安装包需要在真实低高度窗口重测遮挡与展开行为。
