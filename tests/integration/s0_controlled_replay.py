"""S0 新受控录制：在具名临时目录中驱动真实 Web 与本地假模型。"""

import argparse
import hashlib
import http.server
import json
import os
import pathlib
import shutil
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import uuid
import zlib


REPO = pathlib.Path(__file__).resolve().parents[2]
CASE_IDS = (
    "BASE-STREAM-TOOL-PAIR", "BASE-NONSTREAM-TOOL-PAIR", "BASE-LONG-FILE-TASK",
    "BASE-IMAGE-ROUTING", "BASE-EMPTY-SUMMARY", "BASE-CROSS-TURN-FILTER",
)
NEGATIVE_IDS = ("unknown_fingerprint", "changed_arguments", "outside_tmp_target")
RUN = None
OUTSIDE_SENTINEL = None
WEB_PORT = MODEL_PORT = None
BASE = None
RECORDINGS = {}
REGISTRY = {}
REQUESTS = []
REJECTIONS = []
CURRENT = {"case": None, "step": 0}
ATTEMPTS = []


def digest(data):
    if isinstance(data, pathlib.Path):
        data = data.read_bytes()
    elif not isinstance(data, bytes):
        data = json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(data).hexdigest()


def evidence_value(value):
    """只把场景事实和摘要落盘；请求正文、长工具输出与临时绝对路径不进入报告。"""
    if isinstance(value, dict):
        return {key: ("<redacted>" if key in ("payload", "sse_events")
                      else evidence_value(item)) for key, item in value.items()}
    if isinstance(value, list):
        return [evidence_value(item) for item in value]
    if isinstance(value, tuple):
        return [evidence_value(item) for item in value]
    if isinstance(value, str):
        if RUN is not None:
            value = value.replace(str(RUN), "<temp-run>")
        if OUTSIDE_SENTINEL is not None:
            value = value.replace(str(OUTSIDE_SENTINEL.parent), "<temp-sentinel>")
        value = value.replace("s0-local-fake-key", "<fake-key>")
        if len(value) > 512:
            return {"redacted_long_text_sha256": digest(value.encode()), "length": len(value)}
    return value


def write_evidence(name, value):
    (RUN / name).write_text(
        json.dumps(evidence_value(value), ensure_ascii=False, indent=2), encoding="utf-8")


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def make_recordings():
    read_path = RUN / "fixture-read.txt"
    read_path.write_text("S0-READ-RESULT-UNIQUE\n", encoding="utf-8")
    listing = RUN / "listing"
    listing.mkdir()
    (listing / "listed.txt").write_text("S0-LIST-RESULT\n", encoding="utf-8")
    long_path = RUN / "long-result.txt"
    return {
        "BASE-STREAM-TOOL-PAIR": {
            "model": "s0-stream-tool-pair", "prompt": "S0REC:BASE-STREAM-TOOL-PAIR 请读取受控文件并回答。",
            "stream": True, "tool": "read_file", "call_id": "fixture-call-001",
            "arguments": {"path": str(read_path)}, "expected_result": "S0-READ-RESULT-UNIQUE",
            "final": "S0-STREAM-FINAL-OK", "allowed_paths": [str(read_path)],
        },
        "BASE-NONSTREAM-TOOL-PAIR": {
            "model": "s0-nonstream-tool-pair", "prompt": "S0REC:BASE-NONSTREAM-TOOL-PAIR 请列出受控目录。",
            "stream": False, "tool": "glob_search", "original_manifest_tool": "list_directory",
            "call_id": "fixture-call-002", "arguments": {"path": str(listing), "pattern": "*.txt"},
            "expected_result": "listed.txt",
            "final": "S0-NONSTREAM-FINAL-OK", "allowed_paths": [str(listing)],
        },
        "BASE-LONG-FILE-TASK": {
            "model": "s0-long-file-task", "prompt": "S0REC:BASE-LONG-FILE-TASK 请把受控长内容写入临时文件。",
            "stream": True, "tool": "write_file", "call_id": "fixture-call-003",
            "arguments": {"path": str(long_path), "content": "S0-LONG-CONTENT-" + "测" * 8192},
            "expected_result": "long-result.txt", "final": "S0-LONG-FINAL-OK",
            "allowed_paths": [str(long_path)],
        },
        "BASE-IMAGE-ROUTING": {
            "model": "s0-native-vision", "models": ["s0-native-vision", "s0-describe-vision", "s0-text-only"],
            "prompt": "S0REC:BASE-IMAGE-ROUTING 请说明受控像素颜色。",
            "tool": None, "arguments": None, "call_id": None, "final": "S0-IMAGE-FINAL-OK",
            "description": "S0-VISUAL-DESCRIPTION-RED-PIXEL", "allowed_paths": [],
        },
        "BASE-EMPTY-SUMMARY": {
            "model": "s0-empty-summary", "prompt": "S0REC:BASE-EMPTY-SUMMARY 请给最终答复。",
            "tool": None, "arguments": None, "call_id": None, "final": None,
            "reasoning": "S0-REASONING-ONLY-HIDDEN-MARKER", "allowed_paths": [],
        },
        "BASE-CROSS-TURN-FILTER": {
            "model": "s0-cross-turn", "prompt": "S0REC:BASE-CROSS-TURN-FILTER 请读取受控文件。",
            "next_prompt": "S0REC:BASE-CROSS-TURN-FILTER 接着简短回复，不调用工具。",
            "stream": True, "tool": "read_file", "call_id": "fixture-call-004",
            "arguments": {"path": str(RUN / "cross-turn-read.txt")},
            "expected_result": "S0-CROSS-TURN-RAW-HIDDEN-MARKER",
            "final": "S0-CROSS-TURN-FIRST-FINAL", "next_final": "S0-CROSS-TURN-SECOND-FINAL",
            "allowed_paths": [str(RUN / "cross-turn-read.txt")],
        },
    }


def prepare_recordings():
    global RECORDINGS, REGISTRY
    RECORDINGS = make_recordings()
    REGISTRY = {model: name for name, item in RECORDINGS.items()
                for model in item.get("models", [item["model"]])}
    for name, item in RECORDINGS.items():
        item["fingerprint"] = digest({"id": name, "model": item["model"], "prompt": item["prompt"],
                                      "tool": item["tool"], "arguments": item["arguments"],
                                      "call_id": item["call_id"], "final": item["final"]})
        if name == "BASE-CROSS-TURN-FILTER":
            pathlib.Path(item["arguments"]["path"]).write_text(
                item["expected_result"] + "\n", encoding="utf-8")


def gate(case_id, item, args, fingerprint):
    """假模型发出工具调用前的测试回放白名单，不代表产品权限沙箱。"""
    if case_id not in RECORDINGS or fingerprint != RECORDINGS[case_id]["fingerprint"]:
        raise ValueError("未登记的录制指纹")
    if item["tool"] is None:
        if args is not None:
            raise ValueError("无工具录制不接收参数")
        return
    target = pathlib.Path(args["path"]).resolve()
    if not target.is_relative_to(RUN.resolve()) or str(target) not in item["allowed_paths"]:
        raise ValueError("目标不在具名临时路径白名单")
    if args != item["arguments"] or item["tool"] not in ("read_file", "glob_search", "write_file"):
        raise ValueError("录制参数或工具不匹配")


class ModelHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        try:
            request = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
            model = request.get("model")
            case_id = REGISTRY.get(model)
            if case_id is None or case_id != CURRENT["case"]:
                raise ValueError("未登记的模型或场景")
            item = RECORDINGS[case_id]
            messages = json.dumps(request.get("messages", []), ensure_ascii=False)
            ATTEMPTS.append({"model": model, "stream": bool(request.get("stream")),
                             "payload_sha256": digest(request)})
            if case_id != "BASE-IMAGE-ROUTING" and item["prompt"] not in messages:
                raise ValueError("未匹配原始用户提示指纹")
            step = CURRENT["step"]
            if step not in (0, 1, 2) or (step == 2 and case_id not in
                                      ("BASE-IMAGE-ROUTING", "BASE-CROSS-TURN-FILTER")):
                raise ValueError("未登记的模型请求次数")
            gate_args = item["arguments"]
            gate_fingerprint = item["fingerprint"]
            if step == 0 and CURRENT.get("negative") == "unknown_fingerprint":
                gate_fingerprint = "0" * 64
            elif step == 0 and CURRENT.get("negative") == "changed_arguments":
                gate_args = {**item["arguments"], "unrecorded": True}
            elif step == 0 and CURRENT.get("negative") == "outside_tmp_target":
                gate_args = {**item["arguments"], "path": str(OUTSIDE_SENTINEL)}
            gate(case_id, item, gate_args, gate_fingerprint)
            if case_id == "BASE-IMAGE-ROUTING":
                expected = [
                    ("s0-native-vision", True, True, False),
                    ("s0-describe-vision", False, True, False),
                    ("s0-text-only", True, False, True),
                ][step]
                actual = (model, bool(request.get("stream")), "image_url" in messages,
                          item["description"] in messages)
                if actual != expected:
                    raise ValueError(f"图像路由请求形状不匹配: {actual!r}")
                content = ("S0-IMAGE-NATIVE-FINAL-OK" if step == 0 else
                           item["description"] if step == 1 else item["final"])
                delta = {"role": "assistant", "content": content}
                message = {"role": "assistant", "content": content}
                finish = "stop"
            elif case_id == "BASE-EMPTY-SUMMARY":
                if step != 0 or not request.get("stream"):
                    raise ValueError("空总结录制只允许一次流式请求")
                content = None
                delta = {"role": "assistant", "reasoning_content": item["reasoning"]}
                message = {"role": "assistant", "content": None, "reasoning_content": item["reasoning"]}
                finish = "stop"
            elif case_id == "BASE-CROSS-TURN-FILTER" and step == 2:
                if (item["next_prompt"] not in messages or item["final"] not in messages
                        or item["expected_result"] in messages):
                    raise ValueError("跨轮提示/最终答复缺失，或原始工具结果污染新请求")
                content = item["next_final"]
                delta = {"role": "assistant", "content": content}
                message = {"role": "assistant", "content": content}
                finish = "stop"
            elif step == 0:
                names = [tool.get("function", {}).get("name") for tool in request.get("tools", [])]
                if item["tool"] not in names:
                    raise ValueError("实际请求未暴露录制工具")
                gate(case_id, item, item["arguments"], item["fingerprint"])
                content = None
                call = {"id": item["call_id"], "type": "function", "function": {
                    "name": item["tool"], "arguments": json.dumps(item["arguments"], ensure_ascii=False)}}
                delta = {"role": "assistant", "tool_calls": [{"index": 0, **call}]}
                message = {"role": "assistant", "content": None, "tool_calls": [call]}
                finish = "tool_calls"
            else:
                if item["call_id"] not in messages or item["expected_result"] not in messages:
                    raise ValueError("工具结果未携录制 call_id 或实际结果")
                content = item["final"]
                delta = {"role": "assistant", "content": content}
                message = {"role": "assistant", "content": content}
                finish = "stop"
            CURRENT["step"] += 1
            REQUESTS.append({"case": case_id, "step": step, "payload": request,
                             "payload_sha256": digest(request)})
            usage = {"prompt_tokens": 5 + step, "completion_tokens": 3, "total_tokens": 8 + step}
            if request.get("stream"):
                event = {"id": f"s0-{case_id}-{step}", "object": "chat.completion.chunk",
                         "model": model, "choices": [{"index": 0, "delta": delta,
                                                      "finish_reason": finish}], "usage": usage}
                payload = ("data: " + json.dumps(event, ensure_ascii=False)
                           + "\n\ndata: [DONE]\n\n").encode()
                content_type = "text/event-stream"
            else:
                payload = json.dumps({"id": f"s0-{case_id}-{step}", "object": "chat.completion",
                                      "model": model, "choices": [{"index": 0, "message": message,
                                                                  "finish_reason": finish}],
                                      "usage": usage}, ensure_ascii=False).encode()
                content_type = "application/json"
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        except Exception as error:
            REJECTIONS.append(str(error))
            self.send_error(422, "fixture-rejected")


def request(path, method="GET", payload=None, timeout=30):
    data = None if payload is None else json.dumps(payload, ensure_ascii=False).encode()
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return json.load(response)


def send_stream(payload):
    req = urllib.request.Request(BASE + "/api/chat/send/stream",
                                 data=json.dumps(payload, ensure_ascii=False).encode(), method="POST")
    req.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(req, timeout=90) as response:
        raw = response.read().decode("utf-8", errors="replace")
    events = []
    for block in raw.split("\n\n"):
        kind, data = None, None
        for line in block.splitlines():
            if line.startswith("event: "):
                kind = line[7:]
            if line.startswith("data: "):
                data = line[6:]
        if data:
            try:
                events.append((kind, json.loads(data)))
            except json.JSONDecodeError:
                pass
    return events, raw


def png_bytes():
    """一像素红色合成图片；不会读取任何现有附件。"""
    def chunk(name, content):
        return (struct.pack(">I", len(content)) + name + content
                + struct.pack(">I", zlib.crc32(name + content)))
    scanline = b"\x00\xff\x00\x00\xff"
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(scanline)) + chunk(b"IEND", b""))


def upload_image():
    boundary = f"s0-image-{uuid.uuid4().hex}"
    content = png_bytes()
    (RUN / "red-one-pixel.png").write_bytes(content)
    body = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\nimage\r\n'
        f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="red-one-pixel.png"\r\n'
        "Content-Type: image/png\r\n\r\n"
    ).encode() + content + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(BASE + "/api/attachments/upload", data=body, method="POST")
    req.add_header("Content-Type", f"multipart/form-data; boundary={boundary}")
    with urllib.request.urlopen(req, timeout=20) as response:
        return json.load(response)["attachment"]


def room(name):
    return request("/api/chat/rooms", "POST", {"name": name})["room"]["id"]


def session(item, model=None, model_type="text"):
    model = model or item["model"]
    return request("/api/sessions", "POST", {
        "name": model, "provider": "Custom", "model": model,
        "model_type": model_type, "base_url": f"http://127.0.0.1:{MODEL_PORT}/v1",
        "api_key_ref": "s0-local-fake-key",
    })["session"]["id"]


def accepted_run(room_id, events):
    started = [data for kind, data in events if kind == "started"]
    if len(started) != 1:
        raise RuntimeError(f"预期唯一已接纳运行，实际事件: {[kind for kind, _ in events]}")
    run_id = started[0]["run_id"]
    trace = request(f"/api/chat/rooms/{room_id}/trace")
    run = next((item for item in trace["runs"] if item["run_id"] == run_id), None)
    if not run:
        raise RuntimeError("已接纳 run 缺少持久 trace")
    return run


def usage_for_run(run_id):
    rows = []
    for db_path in RUN.rglob("*.sqlite3"):
        with sqlite3.connect(db_path) as db:
            if db.execute("SELECT 1 FROM sqlite_master WHERE name='chat_usage_events'").fetchone():
                rows.extend(db.execute(
                    "SELECT request_kind,status,session_id,room_id,turn_id,run_id,attempt_id "
                    "FROM chat_usage_events WHERE run_id=?", (run_id,)).fetchall())
    return [{"kind": row[0], "status": row[1], "session_id": row[2], "room_id": row[3],
             "turn_id": row[4], "run_id": row[5], "attempt_id": row[6]} for row in rows]


def run_image(case_id):
    item = RECORDINGS[case_id]
    CURRENT.update({"case": case_id, "step": 0})
    attachment = upload_image()
    native = session(item, "s0-native-vision", "vision")
    native_room = room(case_id + "-native")
    native_events, native_raw = send_stream({
        "session_id": native, "target_agent_ids": [native], "chat_room_id": native_room,
        "text": item["prompt"], "attachments": [attachment],
    })
    native_run = accepted_run(native_room, native_events)
    vision = session(item, "s0-describe-vision", "vision")
    target = session(item, "s0-text-only", "text")
    request("/api/config/vision-agent", "POST", {"agent_id": vision})
    described_room = room(case_id + "-described")
    described_events, described_raw = send_stream({
        "session_id": target, "target_agent_ids": [target], "chat_room_id": described_room,
        "text": item["prompt"], "attachments": [attachment],
    })
    described_run = accepted_run(described_room, described_events)
    facts = {
        "case": case_id, "fingerprint": item["fingerprint"],
        "response_status": [native_run["status"], described_run["status"]],
        "model_request_count": len(REQUESTS),
        "final_seen": "S0-IMAGE-NATIVE-FINAL-OK" in native_raw and item["final"] in described_raw,
        "runs": [{"room_id": native_room, "run_id": native_run["run_id"],
                  "status": native_run["status"], "trace_purposes":
                  [entry["purpose"] for entry in native_run["requests"]],
                  "usage": usage_for_run(native_run["run_id"])},
                 {"room_id": described_room, "run_id": described_run["run_id"],
                  "status": described_run["status"], "trace_purposes":
                  [entry["purpose"] for entry in described_run["requests"]],
                  "usage": usage_for_run(described_run["run_id"])}],
        "provider_requests": REQUESTS.copy(), "image_sha256": digest(png_bytes()),
        "sse_event_types": [[kind for kind, _ in native_events], [kind for kind, _ in described_events]],
        "sse_events": [native_events, described_events],
    }
    write_evidence("image-partial-facts.json", facts)
    if (facts["model_request_count"] != 3 or not facts["final_seen"]
            or facts["response_status"] != ["completed", "completed"]
            or facts["runs"][0]["trace_purposes"] != ["chat"]
            or sorted(facts["runs"][1]["trace_purposes"]) != ["chat", "vision_description"]
            or [len(run["usage"]) for run in facts["runs"]] != [1, 2]):
        raise RuntimeError("图像原图与视觉转述的实际运行/归属不完整")
    return facts


def run_empty(case_id):
    item = RECORDINGS[case_id]
    CURRENT.update({"case": case_id, "step": 0})
    target = session(item)
    room_id = room(case_id)
    events, raw = send_stream({"session_id": target, "target_agent_ids": [target],
                               "chat_room_id": room_id, "text": item["prompt"]})
    run = accepted_run(room_id, events)
    messages = request(f"/api/chat/rooms/{room_id}/messages")["messages"]
    reasoning = [message for message in messages if message["kind"] == "reasoning"]
    final = [message for message in messages if message["kind"].startswith("assistant-")]
    facts = {
        "case": case_id, "fingerprint": item["fingerprint"], "room_id": room_id,
        "run_id": run["run_id"], "run_status": run["status"], "response_status": run["status"],
        "model_request_count": len(REQUESTS), "final_seen": bool(final),
        "reasoning_stored": len(reasoning) == 1 and item["reasoning"] in reasoning[0]["content"],
        "reasoning_in_final": any(item["reasoning"] in message["content"] for message in final),
        "final_messages": [{"kind": message["kind"], "content": message["content"]} for message in final],
        "diagnostic_event_seen": any(kind == "error" for kind, _ in events),
        "raw_stream_marker_seen": item["reasoning"] in raw,
        "trace_requests": [{"purpose": entry["purpose"], "attempt_id": entry["attempt_id"]}
                           for entry in run["requests"]],
        "usage": usage_for_run(run["run_id"]),
        "provider_requests": REQUESTS.copy(),
    }
    if (facts["model_request_count"] != 1 or not facts["reasoning_stored"]
            or facts["reasoning_in_final"] or not facts["diagnostic_event_seen"]
            or len(final) != 1):
        raise RuntimeError("空最终回复未满足真实诊断/轨迹契约")
    return facts


def run_cross_turn(case_id):
    item = RECORDINGS[case_id]
    CURRENT.update({"case": case_id, "step": 0})
    target = session(item)
    room_id = room(case_id)
    initial = {"session_id": target, "target_agent_ids": [target],
               "chat_room_id": room_id, "text": item["prompt"]}
    first_events, first_raw = send_stream(initial)
    first_run = accepted_run(room_id, first_events)
    second_events, second_raw = send_stream({**initial, "text": item["next_prompt"]})
    second_run = accepted_run(room_id, second_events)
    messages = request(f"/api/chat/rooms/{room_id}/messages")["messages"]
    facts = {"case": case_id, "fingerprint": item["fingerprint"],
             "room_id": room_id, "run_ids": [first_run["run_id"], second_run["run_id"]],
             "response_status": [first_run["status"], second_run["status"]],
             "model_request_count": len(REQUESTS),
             "final_seen": item["final"] in first_raw and item["next_final"] in second_raw,
             "tool_call_status": first_run.get("calls", []),
             "trace_purposes": [[entry["purpose"] for entry in run["requests"]]
                                for run in (first_run, second_run)],
             "usage": [usage_for_run(run["run_id"]) for run in (first_run, second_run)],
             "provider_requests": REQUESTS.copy(),
             "raw_tool_result_stored": any(message["kind"] == "tool-result"
                                           and item["expected_result"] in message["content"]
                                           for message in messages),
             "raw_tool_result_in_next_request": item["expected_result"] in json.dumps(
                 REQUESTS[2]["payload"], ensure_ascii=False) if len(REQUESTS) == 3 else None}
    if (facts["model_request_count"] != 3 or not facts["final_seen"]
            or facts["response_status"] != ["completed", "completed"]
            or len(facts["tool_call_status"]) != 1
            or facts["tool_call_status"][0]["status"] != "completed"
            or not facts["raw_tool_result_stored"] or facts["raw_tool_result_in_next_request"]):
        raise RuntimeError("跨轮原始工具审计投影不符合实际请求/存储契约")
    return facts


def run_negative(case_id, label):
    item = RECORDINGS[case_id]
    CURRENT.update({"case": case_id, "step": 0, "negative": label})
    target = session(item)
    room_id = room("S0-GATE-NEGATIVE-" + label)
    events, _ = send_stream({"session_id": target, "target_agent_ids": [target],
                             "chat_room_id": room_id, "text": item["prompt"]})
    run = accepted_run(room_id, events)
    persisted_call_count = 0
    for db_path in RUN.rglob("*.sqlite3"):
        with sqlite3.connect(db_path) as db:
            if db.execute("SELECT 1 FROM sqlite_master WHERE name='tool_calls'").fetchone():
                persisted_call_count += db.execute("SELECT count(*) FROM tool_calls").fetchone()[0]
    facts = {"case": case_id, "negative": label, "provider_attempts": ATTEMPTS.copy(),
             "provider_accepted_requests": len(REQUESTS), "model_rejections": REJECTIONS.copy(),
             "run_id": run["run_id"], "run_status": run["status"],
             "trace_calls": run.get("calls", []), "persisted_call_count": persisted_call_count,
             "sse_event_types": [kind for kind, _ in events],
             "outside_sentinel_unchanged": OUTSIDE_SENTINEL.read_text(encoding="utf-8") == "S0-OUTSIDE-SENTINEL-UNCHANGED"}
    expected_rejection = {
        "unknown_fingerprint": "未登记的录制指纹",
        "changed_arguments": "录制参数或工具不匹配",
        "outside_tmp_target": "目标不在具名临时路径白名单",
    }[label]
    if (len(ATTEMPTS) != 1 or len(REJECTIONS) != 1 or REQUESTS
            or run.get("calls") or persisted_call_count or run["status"] != "failed"
            or expected_rejection not in REJECTIONS[0]
            or not facts["outside_sentinel_unchanged"]):
        raise RuntimeError("fixture gate 拒绝后仍有工具派发或拒绝证据不完整")
    return facts


def extract_facts(case_id, room_id, result, raw):
    trace = request(f"/api/chat/rooms/{room_id}/trace")
    runs = trace.get("runs", [])
    if len(runs) != 1:
        raise RuntimeError(f"{case_id}: 预期一条 run，实际 {len(runs)}")
    run = runs[0]
    usage = []
    for db_path in RUN.rglob("*.sqlite3"):
        with sqlite3.connect(db_path) as db:
            if db.execute("SELECT 1 FROM sqlite_master WHERE name='chat_usage_events'").fetchone():
                usage += db.execute("SELECT request_kind,status,room_id,run_id,call_id,input_tokens,output_tokens "
                                    "FROM chat_usage_events WHERE run_id=?", (run["run_id"],)).fetchall()
    item = RECORDINGS[case_id]
    provider = [entry for entry in REQUESTS if entry["case"] == case_id]
    return {"case": case_id, "fingerprint": item["fingerprint"], "room_id": room_id,
            "run_id": run["run_id"], "run_status": run.get("status"),
            "calls": run.get("calls", []),
            "trace_requests": [{"purpose": entry.get("purpose"), "call_id": entry.get("call_id"),
                                "attempt_id": entry.get("attempt_id")} for entry in run.get("requests", [])],
            "provider_requests": provider,
            "usage": [{"kind": x[0], "status": x[1], "room_matches": x[2] == room_id,
                       "run_matches": x[3] == run["run_id"], "call_id": x[4],
                       "input": x[5], "output": x[6]} for x in usage],
            "response_status": result.get("status"), "final_seen": item["final"] in raw,
            "tool_result_seen_in_provider": len(provider) == 2 and item["expected_result"] in json.dumps(provider[1]["payload"], ensure_ascii=False),
            "call_id_seen_in_provider": len(provider) == 2 and item["call_id"] in json.dumps(provider[1]["payload"], ensure_ascii=False)}


def run_case(case_id):
    if case_id == "BASE-IMAGE-ROUTING":
        return run_image(case_id)
    if case_id == "BASE-EMPTY-SUMMARY":
        return run_empty(case_id)
    if case_id == "BASE-CROSS-TURN-FILTER":
        return run_cross_turn(case_id)
    item = RECORDINGS[case_id]
    CURRENT.update({"case": case_id, "step": 0})
    start = len(REQUESTS)
    room_id = room(case_id)
    session_id = session(item)
    payload = {"session_id": session_id, "target_agent_ids": [session_id],
               "chat_room_id": room_id, "text": item["prompt"]}
    if item["stream"]:
        events, raw = send_stream(payload)
        result = {"status": next((data.get("status") for kind, data in reversed(events)
                                  if kind in ("done", "completed")), None),
                  "sse_event_types": [kind for kind, _ in events],
                  "sse_started": [data for kind, data in events if kind == "started"]}
    else:
        result = request("/api/chat/send", "POST", payload, timeout=90)
        raw = json.dumps(result, ensure_ascii=False)
    facts = extract_facts(case_id, room_id, result, raw)
    facts["model_request_count"] = len(REQUESTS) - start
    facts["sse_event_types"] = result.get("sse_event_types", [])
    facts["sse_started"] = result.get("sse_started", [])
    facts["recorded_paths"] = item["allowed_paths"]
    if (facts["model_request_count"] != 2 or facts["run_status"] != "completed"
            or len(facts["calls"]) != 1 or facts["calls"][0]["tool_name"] != item["tool"]
            or facts["calls"][0]["status"] != "completed"
            or not facts["tool_result_seen_in_provider"] or not facts["call_id_seen_in_provider"]
            or not facts["final_seen"]):
        raise RuntimeError(f"{case_id}: 未完成真实工具循环；事实已保存")
    if case_id == "BASE-LONG-FILE-TASK":
        long_path = pathlib.Path(item["arguments"]["path"]).resolve()
        if (not long_path.is_relative_to(RUN.resolve()) or not long_path.is_file()
                or long_path.read_text(encoding="utf-8") != item["arguments"]["content"]):
            raise RuntimeError("长文件未完整写入具名临时路径")
        facts["written_file_sha256"] = digest(long_path)
        facts["written_file_bytes"] = long_path.stat().st_size
    return facts


def temp_output_root(path):
    target = pathlib.Path(path).expanduser().resolve()
    allowed = [REPO / "tmp", pathlib.Path(tempfile.gettempdir())]
    if os.environ.get("RUNNER_TEMP"):
        allowed.append(pathlib.Path(os.environ["RUNNER_TEMP"]))
    if not any(target.is_relative_to(root.resolve()) for root in allowed):
        raise ValueError("输出目录必须位于仓库 tmp、系统临时目录或 CI RUNNER_TEMP")
    target.mkdir(parents=True, exist_ok=True)
    return target


def run_suite(source_web, output_root):
    suite = output_root / ("suite-" + uuid.uuid4().hex[:12])
    suite.mkdir()
    cases = [(case_id, None) for case_id in CASE_IDS]
    cases += [("BASE-STREAM-TOOL-PAIR", label) for label in NEGATIVE_IDS]
    results = []
    for case_id, negative in cases:
        command = [sys.executable, str(pathlib.Path(__file__).resolve()),
                   "--web-binary", str(source_web), "--output-dir", str(suite),
                   "--case", case_id]
        if negative:
            command.extend(("--negative", negative))
        child = subprocess.run(command, capture_output=True, text=True)
        if child.returncode:
            summary = {"kind": "new_s0_controlled_suite", "passed": False,
                       "completed": results, "failed": {"case": case_id, "negative": negative,
                                                     "exit_code": child.returncode}}
            (suite / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
            if child.stderr:
                print(child.stderr[-3000:], file=sys.stderr)
            raise RuntimeError(f"S0 回放失败：{case_id}/{negative or 'positive'}")
        results.append(json.loads(child.stdout.strip().splitlines()[-1]))
        print(f"S0 回放通过：{case_id}/{negative or 'positive'}", flush=True)
    summary = {"kind": "new_s0_controlled_suite", "passed": True,
               "web_sha256": digest(source_web), "case_count": len(CASE_IDS),
               "negative_count": len(NEGATIVE_IDS), "results": results}
    (suite / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "suite": suite.name, "case_count": len(CASE_IDS),
                      "negative_count": len(NEGATIVE_IDS), "web_sha256": digest(source_web)},
                     ensure_ascii=False), flush=True)


def run_one(args, source_web, output_root):
    global RUN, OUTSIDE_SENTINEL, WEB_PORT, MODEL_PORT, BASE
    RUN = output_root / ("run-" + uuid.uuid4().hex[:12])
    RUN.mkdir()
    sentinel_dir = output_root / ("sentinel-" + RUN.name[4:])
    sentinel_dir.mkdir()
    OUTSIDE_SENTINEL = sentinel_dir / "outside-sentinel.txt"
    OUTSIDE_SENTINEL.write_text("S0-OUTSIDE-SENTINEL-UNCHANGED", encoding="utf-8")
    WEB_PORT = free_port()
    MODEL_PORT = free_port()
    while MODEL_PORT == WEB_PORT:
        MODEL_PORT = free_port()
    BASE = f"http://127.0.0.1:{WEB_PORT}"
    prepare_recordings()
    item = RECORDINGS[args.case]
    if args.negative and item["tool"] is None and args.negative != "unknown_fingerprint":
        raise ValueError("无工具场景不能运行参数或路径负例")
    gate(args.case, item, item["arguments"], item["fingerprint"])
    negatives = []
    negative_inputs = [("unknown_fingerprint", args.case, item["arguments"], "0" * 64)]
    if item["tool"] is None:
        negative_inputs.append(("unexpected_tool_arguments", args.case,
                                {"path": str(OUTSIDE_SENTINEL)}, item["fingerprint"]))
    else:
        negative_inputs.extend((
            ("changed_arguments", args.case,
             {**item["arguments"], "path": str(RUN / "not-recorded.txt")}, item["fingerprint"]),
            ("outside_tmp_target", args.case,
             {**item["arguments"], "path": str(OUTSIDE_SENTINEL)}, item["fingerprint"]),
        ))
    for label, case_id, arguments, fingerprint in negative_inputs:
        try:
            gate(case_id, item, arguments, fingerprint)
            raise AssertionError(f"{label} 意外获准")
        except ValueError as error:
            negatives.append({"case": label, "rejected": str(error)})

    binary = RUN / ("coolzhu-web-console.exe" if os.name == "nt" else "coolzhu-web-console")
    shutil.copy2(source_web, binary)
    if digest(binary) != digest(source_web):
        raise RuntimeError("隔离 Web 副本哈希不符")
    (RUN / "coolzhu.toml").write_text(
        f'[web]\nbind_addr = "127.0.0.1:{WEB_PORT}"\n'
        '[model]\nenable_real_llm = true\nenable_llm_tools = true\n'
        'enable_semantic_memory = false\nllm_tool_exposure = "all"\n'
        '[tool]\ndev_open_permissions = true\n[pet]\nenabled = false\n', encoding="utf-8")
    # 只继承启动本地进程需要的系统变量，避免云密钥、用户配置与静态资源覆盖泄入子进程。
    system_env = {"PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "OS",
                  "PROCESSOR_ARCHITECTURE", "NUMBER_OF_PROCESSORS", "LANG", "LC_ALL"}
    env = {key: value for key, value in os.environ.items() if key.upper() in system_env}
    env["COOLZHU_DESKTOP_SHELL_MANAGED"] = "1"
    for name, directory in {
        "COOLZHU_RUNTIME_DIR": RUN, "USERPROFILE": RUN / "home", "HOME": RUN / "home",
        "CLAW_CONFIG_HOME": RUN / ".claw", "COOLZHU_LOG_DIR": RUN / "logs",
        "APPDATA": RUN / "appdata", "LOCALAPPDATA": RUN / "localappdata",
        "XDG_CONFIG_HOME": RUN / "xdg-config", "XDG_DATA_HOME": RUN / "xdg-data",
        "XDG_CACHE_HOME": RUN / "xdg-cache", "TMP": RUN / "temp",
        "TEMP": RUN / "temp", "TMPDIR": RUN / "temp",
    }.items():
        directory.mkdir(exist_ok=True)
        env[name] = str(directory)
    env["NO_PROXY"] = "127.0.0.1,localhost"
    env["no_proxy"] = "127.0.0.1,localhost"

    model = log = web = None
    try:
        model = http.server.ThreadingHTTPServer(("127.0.0.1", MODEL_PORT), ModelHandler)
        threading.Thread(target=model.serve_forever, daemon=True).start()
        log = (RUN / "web.log").open("wb")
        web = subprocess.Popen([str(binary)], cwd=RUN, env=env, stdout=log,
                               stderr=subprocess.STDOUT,
                               creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        for _ in range(120):
            if web.poll() is not None:
                raise RuntimeError(f"隔离 Web 提前退出 {web.returncode}")
            try:
                state = request("/api/state")
                break
            except Exception:
                time.sleep(0.25)
        else:
            raise RuntimeError("隔离 Web 未就绪")
        if pathlib.Path(state["workspace"]).resolve() != RUN.resolve():
            raise RuntimeError("Web 工作区不是本次临时目录")
        if not (RUN / "logs" / "coolzhu-web-console.jsonl").is_file():
            raise RuntimeError("诊断日志未落到本次临时目录")
        if args.negative:
            facts = run_negative(args.case, args.negative)
            write_evidence("negative-result.json", facts)
            print(json.dumps({"run": RUN.name, "case": args.case,
                              "negative": args.negative, "status": facts["run_status"],
                              "persisted_call_count": facts["persisted_call_count"]},
                             ensure_ascii=False), flush=True)
            return
        facts = run_case(args.case)
        result = {"kind": "new_s0_controlled_recording", "web_sha256": digest(binary),
                  "workspace": str(RUN), "web_port": WEB_PORT, "model_port": MODEL_PORT,
                  "case": facts, "gate_negatives": negatives,
                  "outside_sentinel_unchanged": OUTSIDE_SENTINEL.read_text(encoding="utf-8") == "S0-OUTSIDE-SENTINEL-UNCHANGED",
                  "model_rejections": REJECTIONS}
        if not result["outside_sentinel_unchanged"]:
            raise RuntimeError("白名单外哨兵被修改")
        write_evidence("result.json", result)
        print(json.dumps({"run": RUN.name, "case": args.case,
                          "status": facts["response_status"],
                          "requests": facts["model_request_count"],
                          "final_seen": facts["final_seen"]}, ensure_ascii=False), flush=True)
    finally:
        try:
            write_evidence("provider-observed.json", {"requests": REQUESTS,
                                                        "attempts": ATTEMPTS,
                                                        "rejections": REJECTIONS})
        finally:
            try:
                if web is not None:
                    if web.poll() is None:
                        web.terminate()
                    try:
                        web.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        web.kill()
                        web.wait(timeout=5)
            finally:
                try:
                    if log is not None:
                        log.close()
                finally:
                    if model is not None:
                        try:
                            model.shutdown()
                        finally:
                            model.server_close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--web-binary", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--all", action="store_true", help="运行六类录制及三种白名单拒绝")
    parser.add_argument("--case", choices=CASE_IDS)
    parser.add_argument("--negative", choices=NEGATIVE_IDS)
    args = parser.parse_args()
    if args.all and (args.case or args.negative):
        parser.error("--all 不能与 --case/--negative 混用")
    if args.negative and not args.case:
        parser.error("--negative 需要 --case")
    source_web = args.web_binary.expanduser().resolve(strict=True)
    if not source_web.is_file():
        parser.error("Web 二进制不是文件")
    output_root = temp_output_root(args.output_dir)
    if args.all or not args.case:
        run_suite(source_web, output_root)
    else:
        run_one(args, source_web, output_root)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        if RUN is not None:
            write_evidence("failure.json", {"error_type": type(error).__name__,
                                            "message": str(error)})
        print(f"S0 受控录制失败：{type(error).__name__}: {error}", file=sys.stderr)
        sys.exit(1)
