"""受控验收服务器；数据库只读，输出目录每轮新建，不发起模型请求。"""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def planning(database, marker):
    with closing(sqlite3.connect(Path(database).resolve().as_uri() + '?mode=ro', uri=True, timeout=1)) as connection:
        connection.execute('BEGIN')
        source = connection.execute(
            'SELECT id FROM chat_room_messages WHERE role=? AND substr(content,1,?)=? ORDER BY created_at DESC LIMIT 1',
            ('user', len(marker), marker)).fetchone()
        if not source:
            return None
        runs = connection.execute(
            "SELECT DISTINCT r.id,r.state,r.legacy_turn_id FROM runtime_runs r "
            "JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs "
            "WHERE e.event_type='chat.source_messages' AND refs.value=?", source).fetchall()
        if len(runs) != 1:
            if runs:
                raise ValueError('来源消息绑定多个运行，不选择任意一条')
            return None
        run = runs[0]
        if run[1] != 'running':
            return None
        calls = connection.execute(
            'SELECT call_id,state,action_count,current_observation_generation FROM computer_use_runs WHERE turn_id=?',
            (run[2],)).fetchall()
        if len(calls) != 1 or calls[0][1] != 'planning' or calls[0][2] != 0:
            return None
        call = calls[0]
        return dict(parent_run=run[0], parent_state=run[1], cu_call_id=call[0],
                    state=call[1], action_count=call[2], observation_generation=call[3],
                    captured_ms=time.time_ns() / 1_000_000)


def serve(database, output, marker):
    database = Path(database).resolve(strict=True)
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixtures = Path(__file__).resolve().parent
    lock = threading.Lock()
    state = {'switched': False, 'logging_failed': False}

    def record(item):
        with lock:
            envelope = dict(item, server_ns=str(time.perf_counter_ns()), received_unix_ns=str(time.time_ns()))
            try:
                with (output / 'events.jsonl').open('a', encoding='utf-8') as stream:
                    stream.write(json.dumps(envelope, ensure_ascii=False) + '\n')
                    stream.flush()
                    os.fsync(stream.fileno())
            except OSError:
                state['logging_failed'] = True
                raise

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def respond(self, payload, mime, status=200):
            self.send_response(status)
            self.send_header('Content-Type', mime)
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_GET(self):
            try:
                if self.path == '/phase':
                    with lock:
                        found = None if state['switched'] else planning(database, marker)
                        if found:
                            state['switched'] = True
                    if found:
                        record({'kind': 'planning-detected', 'facts': found})
                    self.respond(json.dumps({'ready': bool(found)}).encode(), 'application/json')
                elif self.path == '/health':
                    self.respond(json.dumps(state).encode(), 'application/json', 503 if state['logging_failed'] else 200)
                elif self.path in ('/old.html', '/new.html', '/event-probe.js'):
                    name = self.path[1:]
                    mime = 'application/javascript' if name.endswith('.js') else 'text/html'
                    self.respond((fixtures / name).read_bytes(), mime + ';charset=utf-8')
                else:
                    self.send_error(404)
            except (OSError, sqlite3.Error, ValueError):
                self.send_error(503, 'fixture evidence unavailable')

        def do_POST(self):
            try:
                size = int(self.headers.get('Content-Length', '0'))
                if self.path != '/event' or not 0 < size <= 16384:
                    self.send_error(400)
                    return
                item = json.loads(self.rfile.read(size))
                if not isinstance(item, dict) or item.get('schema') != 2 or item.get('role') not in ('OLD', 'NEW'):
                    self.send_error(400)
                    return
                record(item)
                self.respond(b'', 'application/json', 204)
            except (ValueError, OSError):
                self.send_error(503, 'fixture evidence unavailable')

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    address = {'url': f'http://127.0.0.1:{server.server_port}/old.html', 'pid': os.getpid(),
               'marker': marker, 'database_mode': 'read-only'}
    (output / 'address.json').write_text(json.dumps(address, indent=2), encoding='utf-8')
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--database', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--marker', required=True)
    args = parser.parse_args()
    if not args.marker.strip():
        parser.error('marker 不得为空')
    serve(args.database, args.output, args.marker)
