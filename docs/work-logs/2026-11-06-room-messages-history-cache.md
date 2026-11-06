# 2026-11-06 — 房史全量缓存（"hi"慢优化续批）

## 背景

「hi」一轮本地阶段耗时数十秒的优化分多批落地：

- `af73c5d8` perf(chat)：6 处外科修改（synchronous=NORMAL、接纳事件并事务、嵌入
  HTTP 共享 client+超时、<4 字跳过召回、桌宠 exe 派发挪后台）。
- `bcddbb61` perf(pet)：桌宠自动拉起改 opt-in（未签名壳冷扫 13.5s）。
- `7ebc5e97` revert：撤回 <4 字跳过召回（漏召回三字指令，无实测收益）。
- `eb0f6a89` perf(storage)：会话库连接池（USB 机械盘 WAL 检查点 0.4-5s/轮 ×
  12-14 串行事务）。
- `f5a5fb10` perf(memory)：memory_vectors + superseded 进程内缓存。

`af73c5d8` 列出的待办深项：`HostModelSnapshot::capture` 每目标重建工具目录与
override_models、session store 全量深比较/克隆、回合后记忆写串行挂响应路径。

## 本批：chat_room_messages 全量历史缓存

`load_chat_room_messages` 每轮按目标房间读全量房史（`HostModelSnapshot::capture`
的 context_history、全量 state load），142MB 会话库下是热路径大头。实现延续
superseded 缓存模式：进程持有 session_writer 时代锁、是唯一写者；缓存镜像与
`ORDER BY created_at, id` 一致。

- `session_db.rs`：`cached_room_messages` / `store_room_messages` /
  `room_messages_upsert_cached` / `room_messages_invalidate` /
  `room_messages_invalidate_all` / `room_messages_invalidate_everywhere`。
  cfg(test) 恒 miss，保留删库重建语义。
- `main.rs`：`load_chat_room_messages` 命中即返回（`connection.path()` 作 key，
  与 `self.path` 同一拼写）；`save` 提交后把 diff 写入行并入缓存；goal 事务、
  删除消息、清空房间、删除房间分别按全失效/按房失效。

## 本批修复（接续上一会话未完成的 WIP）

1. `room_messages_invalidate_everywhere` 重复定义 → 去重。
2. `written_room_messages` 作用域错误（声明在内层块、用在提交后）→ 提升声明。
3. `insert_goal_phase_messages_transaction` 内两次失效 → 保留提交前最后一次。
4. **正确性缺口**：`history_persistence::apply`（`HistoryEdit::Message{room:true}`
   —— goal scratch 清扫、live-\*/assistant-partial 事故面清扫）与
   `remove_orphans` 在同一 save 事务内删房消息，提交后只做 upsert 会留脏缓存。
   - `remove_orphans` 改返回删行数；>0 才 `invalidate_all(path)`（常规 save 为
     0，不付重建成本）。
   - `HistoryEdit::Message{room:true}` 按 `parent_id` 逐房失效。

写路径全覆盖核对（生产代码）：INSERT×2（goal 事务、save diff）、DELETE×3
（删消息、清房间、orphan 清扫）、房间删除级联、history_edits —— 均有失效或
并入。其余 `chat_room_messages` 写均在 `#[cfg(test)]`。

## 验证

- `cargo build -p coolzhu-web-console --offline`：exit 0（dev profile，28m）。
- `cargo test -p coolzhu-web-console --offline`：1633 通过 / 3 失败。
  - `session_db::returned_connection_comes_back_with_clean_state` 失败根因：
    `reset_connection` 先跑 temp 清理再复位 `query_only=OFF`，连接挂着
    query_only=ON 归还时 DROP 被静默拒绝 → 调回复位顺序后通过。
  - 其余 2 个失败与本次改动无关：`attachment_upload_limit_tests`（测试环境
    `workspace_projects::remember` 写默认工程选择失败）、
    `web_frontend_left_chat_rail`（前端契约期望 `const display = full || "未设置"`，
    app.js 已不含该串，契约漂移属 11-01 todo 面板批次遗留）。
  - 修复后过滤重跑：`session_db:: history_persistence:: pagination` 9/9 通过。
- 另修 `pagination_test_db` 测试 helper 返回类型（eb0f6a89 引入
  `PooledConnection` 后遗留的 `Connection` 类型不匹配）与
  `Cargo.toml` 加 `default-run="coolzhu-web-console"`：第二个 bin
  `coolzhu-browser-native-host` 导致 `cargo run -p` 无法推断目标。

## 仍待后续批次的深项

- `HostModelSnapshot::capture` 每目标重建工具目录与 override_models（只在
  goal 阶段路径，不在普通聊天热路径；override_models 可惰化/缓存）。
- session store 全量深比较/克隆（`previous`/`committed_state` diff 成本）。
- 回合后记忆写串行挂在响应路径。
