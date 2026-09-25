#!/usr/bin/env python3
"""只读 CU SQLite 审计；标准库，无模型请求或桌面操作。输出详细 JSON 和 *.public.json。"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import re
import sqlite3

ACTIONS = set("navigate click double_click text_input select check submit scroll history_back history_forward drag slider_drag key_combination open_tab activate_tab close_tab".split())
STATES = set("requested classified observing planning policy_check awaiting_approval executing verifying succeeded failed blocked cancelled timed_out".split())
STEP_STATES = set("executing input_sent input_not_sent failed input_sent_observed observation_failed verified completed_unverified".split())
KINDS = {"computer_use_planning": "根据当前观察规划下一步", "computer_use_visual_description": "视觉 Agent 向纯文本规划器描述截图", "computer_use_verification": "视觉 Agent 检查目标与前后变化"}
ERRORS = set("cancelled stale_observation target_not_found target_ambiguous target_disabled target_offscreen target_not_canvas invalid_plan invalid_action invalid_stroke_path invalid_stroke_point invalid_stroke_bounds invalid_stroke_duration input_failed mouse_release_failed backend_unavailable backend_error surface_conflict unsupported_action invalid_surface invalid_target invalid_objective invalid_success_criteria policy_blocked approval_required approval_denied approval_timeout timed_out timeout no_progress action_limit replan_limit watchdog_open persistence_error verification_failed invalid_verification ambiguous_surface".split())
TOKENS = ("input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens")


def parse_json(value, default=None):
    if not isinstance(value, str):
        return value if value is not None else default
    try:
        return json.loads(value)
    except (ValueError, TypeError):
        return default


def integer(value):
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def elapsed(start, end):
    return end - start if integer(start) is not None and integer(end) is not None else None


def utc(value):
    if integer(value) is None:
        return None
    try:
        return datetime.fromtimestamp(value / 1000, timezone.utc).isoformat()
    except (ValueError, OverflowError, OSError):
        return None


def enum(value, allowed):
    return value if isinstance(value, str) and value in allowed else "other" if value is not None else None


class Aliases:
    def __init__(self):
        self.values = defaultdict(dict)

    def get(self, kind, value):
        if value is None or value == "":
            return None
        key = str(value)
        values = self.values[kind]
        if key not in values:
            values[key] = f"{kind}-{len(values) + 1:03}"
        return values[key]


def target_kind(value):
    text = str(value or "")
    if text.startswith("window-canvas:"):
        return "window_canvas"
    if text.startswith("uia-"):
        return "uia_element"
    if text.startswith("dom-") or text == "browser-tabs":
        return "browser_reference"
    return "redacted_or_other"


def action_shape(raw):
    """仅保留结构/数值几何。原始正文、URL、任意字符串不复制到任一输出。"""
    obj = parse_json(raw, {})
    if not isinstance(obj, dict):
        return {"valid_object": False}
    action = obj.get("action", obj)
    if not isinstance(action, dict):
        return {"valid_object": True, "action_field_type": type(action).__name__, "action_string_enum": enum(action, ACTIONS) if isinstance(action, str) else None, "redacted_fields": bool(obj.get("redacted_fields"))}
    args = action.get("arguments", {})
    args = args if isinstance(args, dict) else {}
    values = args.get("points")
    result = {"valid_object": True, "kind": enum(action.get("kind"), ACTIONS), "done": obj.get("done") if isinstance(obj.get("done"), bool) else None, "target_kind": target_kind(action.get("target")), "redacted_fields": bool(obj.get("redacted_fields") or action.get("redacted_fields") or args.get("redacted_fields")), "duration_ms": integer(args.get("duration_ms"))}
    if isinstance(values, list):
        valid = all(isinstance(p, list) and len(p) == 2 and all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) for v in p) for p in values)
        result.update({"point_count": len(values), "points_numeric": valid, "points_in_unit_square": valid and all(0 <= v <= 1 for p in values for v in p), "points": values if valid and len(values) <= 256 else None})
    return result


def evidence(raw, aliases, public=False):
    values = parse_json(raw, [])
    if isinstance(values, str):
        values = [values]
    if not isinstance(values, list):
        return []
    result = []
    for value in values:
        if not isinstance(value, str):
            continue
        item = {"reference": aliases.get("evidence", value)}
        match = re.search(r"screenshot:(.*?):sha256=([a-fA-F0-9]{64}):(\d+)x(\d+)", value)
        if match:
            item.update({"kind": "screenshot", "sha256": match[2].lower(), "width": int(match[3]), "height": int(match[4])})
            if not public:
                item["local_path"] = match[1]
        elif value.startswith("image_changed:") and value.split(":", 1)[1] in ("true", "false"):
            item.update({"kind": "pixel_change", "changed": value.endswith("true")})
        elif value.startswith("native_stroke:"):
            item["kind"] = "native_stroke"
            match = re.search(r":points=(\d+):duration_ms=(\d+):released", value)
            if match:
                item.update({"points": int(match[1]), "duration_ms": int(match[2]), "release_confirmed": True})
        elif "uia_snapshot:" in value:
            item["kind"] = "uia_snapshot"
            match = re.search(r":elements=(\d+)", value)
            if match:
                item["element_count"] = int(match[1])
        else:
            item["kind"] = "other_reference_or_semantic_text_redacted"
        result.append(item)
    return result


def read_rows(connection, table, fields):
    # 表名/列名均由本脚本固定声明，用户参数只用于数据库路径。
    existing = {row[1] for row in connection.execute(f'PRAGMA table_info("{table}")')}
    selected = [field for field in fields if field in existing]
    if not selected:
        return [], sorted(set(fields) - existing)
    query = "SELECT " + ",".join('"' + field + '"' for field in selected) + f' FROM "{table}"'
    return [dict(row) for row in connection.execute(query)], sorted(set(fields) - existing)


def build(database, since_ms=0):
    uri = database.resolve().as_uri() + "?mode=ro&cache=private"
    connection = sqlite3.connect(uri, uri=True, timeout=5)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA query_only=ON")
    connection.execute("BEGIN")  # WAL 也按单个一致读快照，不能使用 immutable 忽略 WAL。
    tables = {
        "computer_use_runs": "call_id provider_tool_call_id turn_id session_id chat_room_id surface state action_count replan_count no_progress_count current_observation_generation terminal_result_json created_at_ms updated_at_ms",
        "computer_use_steps": "run_id step_index observation_generation action_type normalized_target status error_code before_evidence_ref after_evidence_ref visible_progress started_at_ms completed_at_ms",
        "computer_use_step_details": "run_id step_index action_json",
        "computer_use_planner_diagnostics": "id call_id turn_id room_id session_id request_kind observation_generation model error_code response_json started_at_ms completed_at_ms",
        "chat_usage_events": "id room_id session_id created_at input_tokens output_tokens cache_read_tokens cache_write_tokens turn_id call_id request_kind",
    }
    rows, missing = {}, {}
    for table, columns in tables.items():
        rows[table], missing[table] = read_rows(connection, table, columns.split())
    schema_version = connection.execute("PRAGMA user_version").fetchone()[0]
    connection.rollback()
    connection.close()
    runs = sorted((r for r in rows["computer_use_runs"] if (r.get("created_at_ms") or 0) >= since_ms), key=lambda r: (r.get("created_at_ms") or 0, r.get("call_id") or ""))
    calls = {r.get("call_id") for r in runs}
    steps = defaultdict(list)
    details = {(r.get("run_id"), r.get("step_index")): r.get("action_json") for r in rows["computer_use_step_details"]}
    for row in rows["computer_use_steps"]:
        if row.get("run_id") in calls:
            steps[row["run_id"]].append(row)
    diagnostics = sorted((d for d in rows["computer_use_planner_diagnostics"] if d.get("call_id") in calls), key=lambda d: (d.get("started_at_ms") or 0, d.get("id") or 0))
    usage = [u for u in rows["chat_usage_events"] if (u.get("created_at") or 0) >= since_ms]
    origin = min((r.get("created_at_ms") or 0 for r in runs), default=0)
    aliases = Aliases()
    for run in runs:
        for kind, field in (("call", "call_id"), ("session", "session_id"), ("turn", "turn_id"), ("room", "chat_room_id")):
            aliases.get(kind, run.get(field))

    def make(public):
        def identity(kind, value):
            return aliases.get(kind, value) if public else value

        def timing(start, end):
            value = {"started_offset_ms": elapsed(origin, start), "completed_offset_ms": elapsed(origin, end), "elapsed_ms": elapsed(start, end)}
            if not public:
                value.update({"started_at_ms": start, "completed_at_ms": end, "started_utc": utc(start), "completed_utc": utc(end)})
            return value

        result = {"format_version": 1, "privacy": "public_allowlisted_summary" if public else "local_ids_and_screenshot_paths_no_message_bodies", "schema_version": schema_version, "read_only": True, "snapshot_consistency": "single SQLite read transaction including WAL", "missing_schema_columns": {k: v for k, v in missing.items() if v}, "notes": ["step.started_at_ms 在 adapter.act 前持久化，completed_at_ms 在 act 返回时记录；它们含重新观察及后截图耗时，不是鼠标物理 down/up 精确时间。", "visible_progress 与像素变化均不等同目标达成；以 terminal.goal_achieved 及验证状态共同判断。", "usage 是服务端已返回的非零事实；失败请求可能已有用量，没有记录不表示请求免费或未发送。", "没有 call/turn 的聊天用量单独归集，不推断归属某次 CU。", "缺少观察几何记录时不能重建窗口 rect/DPI 或屏幕绝对笔画；这里只报告记录的相对点。", "不读取会话正文，不输出 thinking、原图 data_url、凭据；公开版身份为本报告内编号。"], "runs": [], "planner_diagnostics": [], "usage_groups": []}
        if not public:
            result["database_path"] = str(database.resolve())
        for run in runs:
            terminal = parse_json(run.get("terminal_result_json"), {})
            terminal = terminal if isinstance(terminal, dict) else {}
            error = terminal.get("error") or {}
            entry = {"call": identity("call", run.get("call_id")), "session": identity("session", run.get("session_id")), "room": identity("room", run.get("chat_room_id")), "turn": identity("turn", run.get("turn_id")), "surface": enum(run.get("surface"), {"auto", "desktop", "browser"}), "state": enum(run.get("state"), STATES), "timing": timing(run.get("created_at_ms"), run.get("updated_at_ms")), "stored_action_count": run.get("action_count"), "stored_replan_count": run.get("replan_count"), "terminal": {"present": bool(terminal), "status": enum(terminal.get("status"), STATES), "goal_achieved": terminal.get("goal_achieved") is True, "attempts": integer(terminal.get("attempts")), "steps_completed": integer(terminal.get("steps_completed")), "error_code": enum(error.get("code"), ERRORS) if public else error.get("code"), "evidence": evidence(terminal.get("evidence"), aliases, public)}, "steps": []}
            for step in sorted(steps[run.get("call_id")], key=lambda s: s.get("step_index") or 0):
                shape = action_shape(details.get((step.get("run_id"), step.get("step_index"))))
                item = {"index": step.get("step_index"), "generation": step.get("observation_generation"), "action_type": enum(step.get("action_type"), ACTIONS), "status": enum(step.get("status"), STEP_STATES), "error_code": enum(step.get("error_code"), ERRORS) if public else step.get("error_code"), "target_kind": target_kind(step.get("normalized_target")), "target": identity("target", step.get("normalized_target")), "action": shape, "timing": timing(step.get("started_at_ms"), step.get("completed_at_ms")), "visible_progress": bool(step.get("visible_progress")), "before_evidence": evidence(step.get("before_evidence_ref"), aliases, public), "after_evidence": evidence(step.get("after_evidence_ref"), aliases, public)}
                entry["steps"].append(item)
            entry["recorded_step_count"] = len(entry["steps"])
            entry["recorded_action_types"] = dict(Counter(s["action_type"] for s in entry["steps"]))
            entry["audit_flags"] = []
            if any(s["timing"]["elapsed_ms"] is not None and s["timing"]["elapsed_ms"] < 0 for s in entry["steps"]):
                entry["audit_flags"].append("negative_step_elapsed")
            if entry["terminal"]["present"] and any(s["status"] == "executing" for s in entry["steps"]):
                entry["audit_flags"].append("terminal_run_has_unfinished_step")
            if entry["terminal"]["goal_achieved"] and entry["terminal"]["status"] != "succeeded":
                entry["audit_flags"].append("goal_status_inconsistent")
            if entry["terminal"]["attempts"] is not None and entry["terminal"]["attempts"] != len(entry["steps"]):
                entry["audit_flags"].append("terminal_attempts_differ_from_recorded_steps")
            result["runs"].append(entry)
        for diagnostic in diagnostics:
            kind = diagnostic.get("request_kind")
            item = {"call": identity("call", diagnostic.get("call_id")), "session": identity("session", diagnostic.get("session_id")), "room": identity("room", diagnostic.get("room_id")), "turn": identity("turn", diagnostic.get("turn_id")), "request_kind": enum(kind, KINDS), "purpose": KINDS.get(kind, "其他请求用途"), "generation": diagnostic.get("observation_generation"), "error_code": enum(diagnostic.get("error_code"), ERRORS) if public else diagnostic.get("error_code"), "response_structure": action_shape(diagnostic.get("response_json")), "timing": timing(diagnostic.get("started_at_ms"), diagnostic.get("completed_at_ms"))}
            if not public:
                item.update({"diagnostic_id": diagnostic.get("id"), "model": diagnostic.get("model")})
            result["planner_diagnostics"].append(item)
        grouped = {}
        for row in usage:
            kind = row.get("request_kind")
            scope = "cu_linked" if row.get("call_id") in calls else "cu_kind_unlinked" if kind in KINDS else "chat_or_other_unattributed"
            safe_kind = enum(kind, KINDS) if kind else "unattributed"
            key = (scope, safe_kind, row.get("room_id"), row.get("turn_id"), row.get("call_id"), row.get("session_id"))
            if key not in grouped:
                grouped[key] = {"scope": scope, "request_kind": safe_kind, "room": identity("room", row.get("room_id")), "turn": identity("turn", row.get("turn_id")), "call": identity("call", row.get("call_id")), "session": identity("session", row.get("session_id")), "recorded_requests": 0, **{k: 0 for k in TOKENS}}
            group = grouped[key]
            group["recorded_requests"] += 1
            for token in TOKENS:
                group[token] += integer(row.get(token)) or 0
        result["usage_groups"] = list(grouped.values())
        result["summary"] = {"run_count": len(runs), "run_states": dict(Counter(r["state"] for r in result["runs"])), "step_count": sum(r["recorded_step_count"] for r in result["runs"]), "diagnostic_count": len(diagnostics), "diagnostic_errors": dict(Counter(d["error_code"] for d in result["planner_diagnostics"] if d["error_code"])), "usage_record_count": len(usage), "usage_totals": {k: sum(g[k] for g in result["usage_groups"]) for k in TOKENS}, "cu_usage_totals": {k: sum(g[k] for g in result["usage_groups"] if g["scope"].startswith("cu_")) for k in TOKENS}}
        return result
    return make(False), make(True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("output", type=Path, help="本机详细 JSON；同目录自动生成 <stem>.public.json")
    parser.add_argument("--since-ms", type=int, default=0, help="仅本时间之后创建的 run/usage；缺省审计整个数据库")
    args = parser.parse_args()
    if not args.database.is_file():
        parser.error("数据库不存在；为防误建文件，审计已停止")
    public_path = args.output.with_name(args.output.stem + ".public.json")
    protected = {args.database.resolve(), Path(str(args.database.resolve()) + "-wal"), Path(str(args.database.resolve()) + "-shm")}
    if args.output.resolve() in protected or public_path.resolve() in protected:
        parser.error("输出不可覆盖数据库、WAL 或 SHM")
    local, public = build(args.database, args.since_ms)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(local, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    public_path.write_text(json.dumps(public, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps({"detail": str(args.output), "public": str(public_path), "summary": public["summary"]}, ensure_ascii=False))


if __name__ == "__main__":
    main()
