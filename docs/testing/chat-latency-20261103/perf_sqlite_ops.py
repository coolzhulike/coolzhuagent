"""复刻 open_session_connection + initialize_session_schema + 单行提交，对比 USB 机械盘与 NVMe。
只写 metadata 表的 zz_bench 键并在结束时删除；仅用于隔离副本库。"""
import sqlite3, statistics, sys, time

DBS = {
    "USB-HDD(D:)": r"D:\coolzhuagent\tmp\real-todo-ws\.coolzhu\web-sessions.sqlite3",
    "NVMe(C:)": sys.argv[1],
}
VALIDATE = [
    "SELECT id,base_url,endpoint,model_type,context_reset_at FROM sessions LIMIT 0",
    "SELECT room_id,id,content,attachments_json FROM chat_room_messages LIMIT 0",
    "SELECT session_id,id,origin_message_id,origin_table FROM memory_beads LIMIT 0",
    "SELECT active_run_id,claim_token,retry_count,human_ack FROM goal_phases LIMIT 0",
    "SELECT id,state FROM runtime_runs LIMIT 0",
    "SELECT record_seq,record_subject FROM fact_log_records LIMIT 0",
    "SELECT provider_tool_call_id,source_request_key FROM tool_calls LIMIT 0",
    "SELECT id,workspace_id,spec_json,state,run_id FROM execution_index LIMIT 0",
    "SELECT room_id,session_id,todos_json,source,revision FROM chat_room_todos LIMIT 0",
]


def open_like_app(path, synchronous):
    pre = sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=5)
    pre.execute("PRAGMA user_version").fetchone()
    pre.close()
    con = sqlite3.connect(path, timeout=5, isolation_level=None)
    con.execute("PRAGMA user_version").fetchone()
    con.execute(f"PRAGMA foreign_keys=ON")
    con.execute(f"PRAGMA synchronous={synchronous}")
    con.execute("PRAGMA journal_mode").fetchone()
    con.execute("PRAGMA user_version").fetchone()
    con.execute("SELECT value FROM metadata WHERE key='schema_contract_version'").fetchone()
    for sql in VALIDATE:
        con.execute(sql).fetchall()
    return con


def bench(path, synchronous, rounds=8):
    opens, commits, reused = [], [], []
    for index in range(rounds):
        t = time.perf_counter()
        con = open_like_app(path, synchronous)
        opens.append(time.perf_counter() - t)
        t = time.perf_counter()
        con.execute("BEGIN IMMEDIATE")
        con.execute("INSERT INTO metadata(key,value) VALUES('zz_bench',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [str(index)])
        con.execute("COMMIT")
        commits.append(time.perf_counter() - t)
        con.close()
    con = open_like_app(path, synchronous)
    for index in range(rounds):
        t = time.perf_counter()
        con.execute("BEGIN IMMEDIATE")
        con.execute("INSERT INTO metadata(key,value) VALUES('zz_bench',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [str(index)])
        con.execute("COMMIT")
        reused.append(time.perf_counter() - t)
    con.execute("DELETE FROM metadata WHERE key='zz_bench'")
    con.close()
    med = lambda values: statistics.median(values) * 1000
    return med(opens), med(commits), med(reused)


lines = ["== sqlite 单次操作中位数（ms）=="]
for name, path in DBS.items():
    for synchronous in ("FULL", "NORMAL"):
        o, c, r = bench(path, synchronous)
        lines.append(f"{name:12s} sync={synchronous:6s} open+schema={o:8.1f}  open后提交={c:8.1f}  复用连接提交={r:8.1f}")
text = "\n".join(lines)
print(text)
open(r"D:\coolzhuagent\tmp\analysis-sqlite-ops.txt", "w", encoding="utf-8").write(text)
