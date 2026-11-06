"""测前端打开聊天室时实际调用的接口：耗时 + 响应体大小（每个接口 3 次取中位数）。"""
import http.client, statistics, sys, time, urllib.parse

TAG = sys.argv[1] if len(sys.argv) > 1 else "run"
ROOM = "main-room"
SESSION = "session-1792519612697-22ffea3a2956ec99"
paths = [
    "/",
    "/api/sessions",
    "/api/chat/rooms",
    f"/api/chat/rooms/{ROOM}/messages?limit=80",
    f"/api/chat/rooms/{ROOM}/roster?me={SESSION}",
    f"/api/chat/rooms/{ROOM}/handoffs?limit=20",
    f"/api/chat/rooms/{ROOM}/permissions",
    f"/api/chat/rooms/{ROOM}/todos",
    "/api/pet/state",
]
rows = []
for path in paths:
    samples, size, status = [], 0, None
    for _ in range(3):
        conn = http.client.HTTPConnection("127.0.0.1", 8765, timeout=300)
        started = time.perf_counter()
        conn.request("GET", path)
        resp = conn.getresponse()
        body = resp.read()
        samples.append(time.perf_counter() - started)
        size, status = len(body), resp.status
        conn.close()
    rows.append((path, status, statistics.median(samples), size))
out = [f"== frontend api {TAG} =="]
for path, status, median, size in rows:
    out.append(f"{median * 1000:9.1f}ms  {size / 1024:9.1f}KB  http={status}  {path}")
text = "\n".join(out)
print(text)
open(rf"D:\coolzhuagent\tmp\bench-frontend-{TAG}.txt", "w", encoding="utf-8").write(text)
