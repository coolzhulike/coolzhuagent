# 2026-11-07 聊天室转录可见分页（修复"消息记录全没了"）

## 现象

用户反馈安装 0.2.145 后"聊天室的消息记录都没有了"。

## 排查结论：数据未丢，是分页谓词缺陷

逐层验证（AI_temp workspace，web-sessions.sqlite3）：

- `chat_rooms=3`、`chat_room_messages=175`、`session_messages=181`、`sessions=4` —— 数据完好
- `/api/workspace/active` → `C:\Users\zhupu\Desktop\AI_temp`（工作区正确）
- `/api/chat/rooms`、`/messages`、`history-messages`、`/api/sessions`、`/api/sessions/{id}/messages` —— 全部正常返回

**根因**：`chat_insights::message_page` 对**原始消息流**分页，而前端 `shouldRenderCompletedMessage` 会隐藏 `tool-call/tool-summary/tool-result/reasoning/computer-use/vision-computer-use/goal-phase` 与 `role=tool`。

Test 房间 146 条消息中 134 条是 rea-video 后台任务的工具回执，且全部位于历史尾部。`loadChatRoomMessages` 只拉末页 80 条 → 前端过滤后 `visible=0` → `reconcileChatHistory`/`loadChatRoomMessages` 两条路径都渲染"竹简待书"空态 → 用户看到"消息全没了"。主聊天室（29 条，27 条可见）不受影响，故表现为部分房间"空了"。

后端其实早已有同义谓词：`VISIBLE_SQL`（`chat_insights.rs`）+ 部分索引 `idx_chat_visible_position_v1`，搜索与 insights 均在用；唯独转录分页没接上。

## 修复（最小改动）

- `MessageQuery` 新增 `visible: Option<bool>`（默认 false，保持原始流兼容）
- `message_page`：`visible=true` 时创建复用 `idx_chat_visible_position_v1`，查询附加 `VISIBLE_SQL` 并 `INDEXED BY` 强制走部分索引；`before` 游标查找保持不过滤（指向不可见行的旧游标仍可解析为更早可见页）
- `chat_history_sync::messages`（SSE 历史同步读窗口）服务端固定 `visible:Some(true)` —— 只服务 DOM 转录
- `app.js` `loadChatRoomMessages` URL 增加 `visible=true`

## 明确不改的消费端（需要原始流）

- `workspace_panels.js` `loadArchive`："历史思考与原始过程记录"存档，只要 reasoning/tool-* 行
- `workspace_panels.js` `readHistory`：定时任务房间历史全量渲染
- `session_db::load_chat_room_messages`：模型上下文需完整历史（含工具回执）—— 该函数与本次缓存优化均不涉转录分页
- `api_session_messages`：会话工程视图独立路径

## 验证

- 新回归 `visible_message_page_skips_tool_receipt_tail`：100 条 tool-result 塞满末页 → 原始分页 80 条全回执（兼容保留）、可见分页直接返回 `["u1","a1"]`、可见游标继续向更早翻页、不可见旧游标可解析
- `chat_insights`/`chat_history_sync`/`pagination`/`history_sync` 相关 16 个测试全过
- `cargo build -p coolzhu-web-console --offline` 通过（7m20s）
- 真实库手验：Test 房间可见页返回 8 条真实消息（hi、rea-video 分析、工具阻断问答、看护终止回执），与前端应渲染内容一致

## 交付状态

当前已安装 0.2.145 不含本修复；需重新打包安装后 Test 房间才会显示真实转录。
