"""联合验收证据页；只提供本地测试内容，不调用模型、不读取运行库。"""
import argparse
import hashlib
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    lock = threading.Lock()
    # 随机尾文只在本次本地文件中保存，供总控独立核对模型实际读取。
    import secrets
    nonce = secrets.token_hex(12)
    payload = "\n".join(f"record-{i:05d}: 中文连续资料 {i * 17}" for i in range(5000))
    payload += f"\nfinal_record={nonce}\n"
    raw_payload = payload.encode()
    (args.output / "expected-result.json").write_text(json.dumps({
        "sha256": hashlib.sha256(raw_payload).hexdigest(),
        "total_bytes": len(raw_payload), "final_record": nonce,
    }, ensure_ascii=False, indent=2), encoding="utf-8")
    servers = []

    def record(item):
        item.update(server_ns=time.perf_counter_ns(), received_unix_ns=time.time_ns())
        with lock, (args.output / "events.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(item, ensure_ascii=False) + "\n")

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            if self.path == "/long-result":
                body, mime = raw_payload, "text/plain;charset=utf-8"
            elif self.path in ("/old", "/new"):
                role = self.path[1:]
                destination = f"http://127.0.0.1:{servers[1].server_port}/new"
                # 只在测试页正常pointerdown处理器请求导航；不持有按键、不人为延迟up。
                html = '''<!doctype html><meta charset="utf-8"><title>Browser时序证据</title>
<body><h1>ROLE 文档</h1><button id="target">时序目标</button><input id="editor">
<p id="effect">尚无效果</p><script>
const documentId=crypto.randomUUID();let seq=0;
function report(kind,e){const t=e?.target;const a=document.activeElement;
navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,role:'ROLE',documentId,seq:++seq,
page_ms:performance.timeOrigin+performance.now(),event_ms:e?.timeStamp,trusted:e?.isTrusted,
key:e?.key,code:e?.code,repeat:e?.repeat,location:e?.location,button:e?.button,buttons:e?.buttons,
alt:e?.altKey,ctrl:e?.ctrlKey,shift:e?.shiftKey,meta:e?.metaKey,
target:t?.id||t?.tagName,active:a?.id||a?.tagName,visibility:document.visibilityState,
hasFocus:document.hasFocus(),path:e?.composedPath().slice(0,6).map(n=>n.id||n.tagName||'window')
})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','mousedown','mouseup','click','keydown','keyup','input','focusin','focusout'])
addEventListener(kind,e=>report(kind,e),true);
addEventListener('pagehide',e=>report('pagehide',e));addEventListener('load',e=>report('load',e));
target.addEventListener('click',e=>{if(e.isTrusted){effect.textContent='收到可信click';report('effect',e);}});
if('ROLE'==='old')target.addEventListener('pointerdown',e=>{if(e.isTrusted){report('navigation-requested',e);location.replace('DESTINATION');}});
report('document-script-start');</script>'''
                body = html.replace("ROLE", role).replace("DESTINATION", destination).encode()
                mime = "text/html;charset=utf-8"
            else:
                self.send_error(404)
                return
            record({"kind": "http-get", "port": self.server.server_port, "path": self.path})
            self.send_response(200)
            self.send_header("Content-Type", mime)
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            size = int(self.headers.get("Content-Length", "0"))
            if self.path != "/event" or not 0 < size <= 8192:
                self.send_error(400)
                return
            try:
                item = json.loads(self.rfile.read(size))
                if not isinstance(item, dict):
                    raise ValueError()
            except (ValueError, UnicodeError):
                self.send_error(400)
                return
            item["port"] = self.server.server_port
            record(item)
            self.send_response(204)
            self.end_headers()

    servers.extend(ThreadingHTTPServer(("127.0.0.1", 0), Handler) for _ in range(2))
    (args.output / "addresses.json").write_text(json.dumps({
        "old": f"http://127.0.0.1:{servers[0].server_port}/old",
        "new": f"http://127.0.0.1:{servers[1].server_port}/new",
        "long_result": f"http://127.0.0.1:{servers[0].server_port}/long-result",
    }, indent=2), encoding="utf-8")
    for server in servers:
        threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        while not (args.output / "stop-server").exists():
            time.sleep(0.2)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    main()
