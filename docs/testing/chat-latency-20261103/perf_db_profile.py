import sqlite3, time, sys

DB = r"D:\coolzhuagent\tmp\real-todo-ws\.coolzhu\web-sessions.sqlite3"
con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
try:
    print("== 表规模（行数 / 主要列字节）==")
    for table, cols in [
        ("session_messages", "length(content)+length(attachments_json)"),
        ("chat_room_messages", "length(content)+length(attachments_json)"),
        ("memory_beads", "length(summary)"),
        ("memory_vectors", "length(vector)"),
        ("attachment_refs", "0"),
        ("runtime_runs", "0"),
        ("runtime_run_events", "length(payload_json)"),
        ("chat_usage_events", "0"),
        ("tool_calls", "0"),
        ("memory_meta", "0"), ("memory_access", "0"), ("memory_edges", "0"), ("memory_jobs", "0"),
    ]:
        try:
            t = time.perf_counter()
            n, b = con.execute(f"SELECT count(*), coalesce(sum({cols}),0) FROM {table}").fetchone()
            print(f"{table:22s} rows={n:>8} bytes={b/1e6:8.2f}MB  scan={1000*(time.perf_counter()-t):7.1f}ms")
        except Exception as e:
            print(f"{table:22s} ERR {e}")

    print("\n== 最大的消息行 ==")
    for table in ("session_messages", "chat_room_messages"):
        for row in con.execute(f"SELECT id, kind, length(content), length(attachments_json) FROM {table} ORDER BY length(content)+length(attachments_json) DESC LIMIT 3"):
            print(table, row)

    print("\n== 索引 ==")
    for table in ("session_messages", "chat_room_messages", "attachment_refs"):
        for row in con.execute(f"PRAGMA index_list({table})"):
            cols = [c[2] for c in con.execute(f"PRAGMA index_info({row[1]})")]
            print(f"{table}: {row[1]} unique={row[2]} cols={cols}")

    probes = {
        "orphan session_messages": "SELECT count(*) FROM session_messages WHERE session_id NOT IN (SELECT id FROM sessions)",
        "orphan chat_room_messages": "SELECT count(*) FROM chat_room_messages WHERE room_id NOT IN (SELECT id FROM chat_rooms)",
        "orphan attachment_refs": "SELECT count(*) FROM attachment_refs WHERE (message_tbl='session_messages' AND NOT EXISTS(SELECT 1 FROM session_messages m WHERE m.id=attachment_refs.message_id)) OR (message_tbl='chat_room_messages' AND NOT EXISTS(SELECT 1 FROM chat_room_messages m WHERE m.id=attachment_refs.message_id))",
        "orphan memory_vectors": "SELECT count(*) FROM memory_vectors WHERE session_id NOT IN (SELECT id FROM sessions)",
        "load main-room": "SELECT id, author, role, target, content, kind, attachments_json, created_at FROM chat_room_messages WHERE room_id='main-room' ORDER BY created_at, id",
        "load session-1791122188340": "SELECT id, author, role, target, content, kind, attachments_json, created_at FROM session_messages WHERE session_id='session-1791122188340' ORDER BY created_at, id",
    }
    print("\n== 每次 save/load 都会执行的语句：计划 + 耗时 ==")
    for name, sql in probes.items():
        plan = " | ".join(r[3] for r in con.execute("EXPLAIN QUERY PLAN " + sql))
        t = time.perf_counter()
        rows = con.execute(sql).fetchall()
        ms = 1000 * (time.perf_counter() - t)
        result = rows[0][0] if name.startswith("orphan") else f"{len(rows)} rows"
        print(f"{name:28s} {ms:9.1f}ms result={result}\n    plan: {plan}")

    print("\n== PRAGMA ==")
    for p in ("page_size", "page_count", "freelist_count", "journal_mode", "cache_size", "mmap_size"):
        print(p, con.execute(f"PRAGMA {p}").fetchone()[0])
finally:
    con.close()
