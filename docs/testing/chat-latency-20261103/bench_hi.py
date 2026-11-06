"""对隔离副本库发送 "hi"（流式路由，与前端一致），记录用户可感知时延与服务端分段。

用法: python bench_hi.py <tag> <次数>
前提: 服务已在 127.0.0.1:8765 就绪，且无其它探针进程持有会话库。
"""
import http.client, json, os, sqlite3, sys, time, re, pathlib

TAG = sys.argv[1] if len(sys.argv) > 1 else "run"
COUNT = int(sys.argv[2]) if len(sys.argv) > 2 else 3
WS = pathlib.Path(os.environ.get("BENCH_WS", r"D:\coolzhuagent\tmp\real-todo-ws"))
DB = WS / ".coolzhu" / "web-sessions.sqlite3"
ERR = WS / "err.log"
OUT = pathlib.Path(rf"D:\coolzhuagent\tmp\bench-{TAG}.txt")
BODY = {
    "session_id": "session-1792519612697-22ffea3a2956ec99",
    "chat_room_id": "room-todo-live",
    "target_agent_ids": ["session-1792519612697-22ffea3a2956ec99"],
    "text": "hi",
}
lines = []


def log(text=""):
    print(text, flush=True)
    lines.append(text)


def stream_once():
    payload = json.dumps(BODY).encode()
    conn = http.client.HTTPConnection("127.0.0.1", 8765, timeout=900)
    started = time.perf_counter()
    conn.request("POST", "/api/chat/send/stream", body=payload,
                 headers={"Content-Type": "application/json", "Accept": "text/event-stream"})
    resp = conn.getresponse()
    headers_at = time.perf_counter() - started
    firsts, current_event, body_tail = {}, "message", b""
    first_byte = None
    while True:
        line = resp.readline()
        if not line:
            break
        if first_byte is None:
            first_byte = time.perf_counter() - started
        text = line.decode("utf-8", "replace").rstrip("\r\n")
        if text.startswith("event:"):
            current_event = text[6:].strip()
            firsts.setdefault(current_event, time.perf_counter() - started)
        elif text.startswith("data:") and resp.status != 200:
            body_tail += line
        elif text == "":
            current_event = "message"
    if resp.status != 200:
        body_tail += resp.read()
    total = time.perf_counter() - started
    conn.close()
    return resp.status, headers_at, first_byte, firsts, total, body_tail[:300]


def db_report(since_ms):
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True, timeout=5)
    try:
        run = con.execute("SELECT id,state,created_at,finished_at FROM runtime_runs WHERE created_at>=? ORDER BY created_at DESC LIMIT 1", [since_ms]).fetchone()
        if not run:
            log("  (未找到 run)")
            return
        log(f"  run={run[0]} state={run[1]} run_total={(run[3] - run[2]) if run[3] else '?'}ms")
        for event_type, created in con.execute("SELECT event_type,created_at FROM runtime_run_events WHERE run_id=? ORDER BY created_at,rowid", [run[0]]):
            log(f"    +{created - run[2]:>7}ms  {event_type}")
        cols = [c[1] for c in con.execute("PRAGMA table_info(chat_usage_events)")]
        for row in con.execute("SELECT * FROM chat_usage_events WHERE created_at>=? ORDER BY created_at", [since_ms]):
            record = dict(zip(cols, row))
            keep = {k: v for k, v in record.items() if any(s in k for s in ("_at", "status", "kind", "tokens", "delay", "elapsed"))}
            base = run[2]
            for k, v in list(keep.items()):
                if k.endswith("_at") and isinstance(v, int) and v > 10**12:
                    keep[k] = f"+{v - base}ms"
            log(f"    usage: {keep}")
    finally:
        con.close()


def diag_tail(offset):
    with open(ERR, "rb") as handle:
        handle.seek(offset)
        new = handle.read().decode("utf-8", "replace")
    for line in new.splitlines():
        if re.search(r"SESSION-SQLITE\] save (start|schema|commit)|LLM-CHAIN|LLM-TOOLS\] build_request", line):
            log("    diag: " + line[:160])


log(f"== bench {TAG} @ {time.strftime('%Y-%m-%d %H:%M:%S')} ==")
for index in range(COUNT):
    if index:
        time.sleep(35)  # 超过 30s 幂等窗口，避免同文本被判重
    offset = ERR.stat().st_size if ERR.exists() else 0
    since = int(time.time() * 1000) - 1000
    status, headers_at, first_byte, firsts, total, tail = stream_once()
    log(f"[{index + 1}] http={status} headers={headers_at:.2f}s first_byte={first_byte if first_byte is None else round(first_byte, 2)}s total={total:.2f}s")
    log("    sse_first: " + ", ".join(f"{k}={v:.2f}s" for k, v in firsts.items()))
    if tail:
        log(f"    body: {tail!r}")
    time.sleep(3)
    db_report(since)
    diag_tail(offset)
OUT.write_text("\n".join(lines), encoding="utf-8")
log(f"写入 {OUT}")
