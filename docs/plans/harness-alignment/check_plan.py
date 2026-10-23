"""只读检查实施台账及证据；不运行模型、业务命令或构建。"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import sys
import tempfile
from collections import Counter
from datetime import datetime
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[2]
TASK_STATUS = {"pending", "in_progress", "implemented", "accepted", "blocked", "needs_recheck", "deferred"}
CASE_STATUS = {"pending", "running", "passed", "failed", "blocked", "needs_recheck"}
KINDS = {"test", "integration", "benchmark", "review", "build"}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inside(root: Path, value: str) -> Path:
    path = (root / value).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError(f"证据路径越出仓库：{value}")
    return path


def time_value(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("时间缺少时区")
    return result


def check_evidence(record, root: Path):
    errors = []
    label = record.get("id", "无 ID 证据")
    if record.get("kind") not in KINDS or record.get("result") != "pass":
        errors.append(f"{label}: 当前通过条目必须引用有效类型的 pass 证据")
    if not record.get("observed", "").strip():
        errors.append(f"{label}: 缺实际观察")
    method = record.get("method", {})
    for field in ("procedure", "cwd", "actor"):
        if not method.get(field):
            errors.append(f"{label}: 缺 method.{field}")
    try:
        if time_value(method["ended_at_utc"]) < time_value(method["started_at_utc"]):
            errors.append(f"{label}: 结束早于开始")
    except (KeyError, ValueError, TypeError, AttributeError):
        errors.append(f"{label}: 缺合法开始/结束时间")
    if method.get("command") and method.get("exit_code") != 0:
        errors.append(f"{label}: 通过的命令证据必须记录 exit_code=0")
    if record.get("kind") == "test":
        if not method.get("command") or type(method.get("tests_run")) is not int or method["tests_run"] < 1:
            errors.append(f"{label}: 测试证据缺命令或实际测试数不足 1")
    if record.get("kind") == "build" and not method.get("command"):
        errors.append(f"{label}: 构建证据缺实际命令")
    for group in ("artifacts", "inputs"):
        entries = record.get(group, [])
        if not entries:
            errors.append(f"{label}: 缺 {group}")
        for entry in entries:
            try:
                path = inside(root, entry["path"])
                wanted = entry["sha256"]
                if not re.fullmatch(r"[0-9a-f]{64}", wanted) or digest(path) != wanted:
                    errors.append(f"{label}: {group} 哈希不匹配，需复核 {entry['path']}")
            except (KeyError, ValueError, OSError, TypeError) as error:
                errors.append(f"{label}: {group} 不可核实：{error}")
    return errors


def validate(data, root: Path, expected: dict[str, list[str]], complete=False):
    errors = []
    if data.get("schema_version") != 1:
        errors.append("不支持的 schema_version")
    tasks = data.get("tasks", [])
    ids = [task.get("id") for task in tasks]
    if len(ids) != len(set(ids)) or set(ids) != set(expected):
        errors.append("工作包编号缺失、重复或超出合同")
    by_id = {task.get("id"): task for task in tasks}
    evidence = data.get("evidence", [])
    evidence_ids = [item.get("id") for item in evidence]
    if any(not value for value in evidence_ids) or len(evidence_ids) != len(set(evidence_ids)):
        errors.append("证据 ID 缺失或重复")
    proofs = {item.get("id"): item for item in evidence}
    used = set()
    for task in tasks:
        key = task.get("id")
        status = task.get("status")
        required = bool(expected.get(key))
        if task.get("required") is not required or status not in TASK_STATUS:
            errors.append(f"{key}: required/status 无效")
        if (required and status == "deferred") or (not required and status != "deferred"):
            errors.append(f"{key}: 必做项不能延期，候选项不能擅自进入实施")
        cases = task.get("criteria", [])
        actual = [case.get("id") for case in cases]
        if len(actual) != len(set(actual)) or set(actual) != set(expected.get(key, [])):
            errors.append(f"{key}: 验收条目缺失、重复或超出合同")
        deps = task.get("depends_on", [])
        if len(deps) != len(set(deps)):
            errors.append(f"{key}: 依赖重复")
        for dependency in deps:
            if dependency == key or dependency not in by_id:
                errors.append(f"{key}: 无效依赖 {dependency}")
            elif status == "accepted" and by_id[dependency].get("status") != "accepted":
                errors.append(f"{key}: 依赖 {dependency} 尚未验收")
        if status in {"blocked", "needs_recheck"} and not task.get("note"):
            errors.append(f"{key}: 阻塞/复核状态缺说明")
        for case in cases:
            case_id = case.get("id")
            case_status = case.get("status")
            if case_status not in CASE_STATUS:
                errors.append(f"{case_id}: 非法状态")
            if case_status in {"failed", "blocked", "needs_recheck"} and not case.get("note"):
                errors.append(f"{case_id}: 缺失败/阻塞/复核原因")
            refs = case.get("evidence_ids", [])
            for ref in refs:
                if ref not in proofs:
                    errors.append(f"{case_id}: 证据不存在 {ref}")
            if case_status == "passed":
                if not refs:
                    errors.append(f"{case_id}: 不能无证据通过")
                used.update(ref for ref in refs if ref in proofs)
                if refs and all(proofs.get(ref, {}).get("kind") == "build" for ref in refs):
                    errors.append(f"{case_id}: 构建本身不能关闭行为验收")
        if status == "accepted" and any(case.get("status") != "passed" for case in cases):
            errors.append(f"{key}: 不能关闭未全部通过的工作包")
        if complete and required and status != "accepted":
            errors.append(f"{key}: 首轮尚未完成")
    # 拒绝环形依赖，避免接替者无合法下一项。
    visiting, visited = set(), set()

    def visit(key):
        if key in visiting:
            errors.append(f"依赖成环：{key}")
            return
        if key in visited:
            return
        visiting.add(key)
        for dependency in by_id.get(key, {}).get("depends_on", []):
            if dependency in by_id:
                visit(dependency)
        visiting.remove(key)
        visited.add(key)

    for key in by_id:
        visit(key)
    for ref in sorted(used):
        errors.extend(check_evidence(proofs[ref], root))
    handoff = data.get("handoff", {})
    for field in ("checkpoint", "last_result", "next_action"):
        if not handoff.get(field):
            errors.append(f"交接缺 {field}")
    for field in ("running_commands", "unresolved_effects"):
        if not isinstance(handoff.get(field), list):
            errors.append(f"交接缺列表 {field}")
        elif complete and handoff[field]:
            errors.append(f"仍有 {field}，不能宣布全部收口")
    for field in ("active_task", "next_task"):
        value = data.get(field)
        if value is not None and (value not in by_id or by_id[value].get("status") in {"accepted", "deferred"}):
            errors.append(f"{field} 必须指向未完成必做项或为 null")
        if complete and value is not None:
            errors.append(f"全部收口时 {field} 应为 null")
    if data.get("active_task") and not handoff.get("owner"):
        errors.append("当前执行者缺失，不能可靠交接")
    return errors


def contract():
    document = (BASE / "ACCEPTANCE.md").read_text(encoding="utf-8")
    found = re.findall(r"^\| (HA\d{2}-\d{2}) \|", document, re.M)
    if len(found) != 60 or len(set(found)) != 60:
        raise ValueError("v2 合同应包含 60 条唯一必做验收；范围调整需显式修订")
    expected = {f"HA-{index:02}": [] for index in range(13)}
    for value in found:
        expected[f"HA-{value[2:4]}"].append(value)
    expected.update({"D08-A": [], "D08-B": []})
    return expected


def self_test():
    # 仅临时隔离文件。负例必须被拒绝，不接触业务状态。
    temp_parent = ROOT / "tmp"
    temp_parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="analysis-harness-check-", dir=temp_parent) as folder:
        root = Path(folder)
        artifact = root / "actual-log.txt"
        source = root / "source.txt"
        artifact.write_text("1 test passed", encoding="utf-8")
        source.write_text("relevant input v1", encoding="utf-8")
        proof = {"id": "E1", "kind": "test", "result": "pass", "observed": "测试断言成立",
                 "method": {"command": "fixture-test", "procedure": "隔离校验器测试", "cwd": str(root),
                            "actor": "self-test", "started_at_utc": "2026-10-18T00:00:00Z",
                            "ended_at_utc": "2026-10-18T00:00:01Z", "exit_code": 0, "tests_run": 1},
                 "artifacts": [{"path": artifact.name, "sha256": digest(artifact)}],
                 "inputs": [{"path": source.name, "sha256": digest(source)}]}
        state = {"schema_version": 1, "active_task": None, "next_task": None,
                 "handoff": {"checkpoint": "test", "last_result": "test", "next_action": "none",
                             "running_commands": [], "unresolved_effects": []},
                 "evidence": [proof], "tasks": [{"id": "HA-00", "required": True, "status": "accepted",
                    "depends_on": [], "criteria": [{"id": "HA00-01", "status": "passed", "evidence_ids": ["E1"]}]}]}
        expected = {"HA-00": ["HA00-01"]}
        results = []

        def test(name, data, reject=True):
            errors = validate(data, root, expected, complete=True)
            results.append({"name": name, "passed": bool(errors) == reject})

        test("可信完整记录可以收口", state, False)
        bad = copy.deepcopy(state)
        bad["tasks"][0]["criteria"][0]["evidence_ids"] = []
        test("无证据通过被拒绝", bad)
        bad = copy.deepcopy(state)
        bad["evidence"][0]["method"]["tests_run"] = 0
        test("零项测试被拒绝", bad)
        bad = copy.deepcopy(state)
        bad["evidence"][0]["method"]["exit_code"] = 1
        test("失败命令冒充通过被拒绝", bad)
        bad = copy.deepcopy(state)
        bad["evidence"][0]["artifacts"][0]["sha256"] = "0" * 64
        test("篡改证据被拒绝", bad)
        source.write_text("changed input", encoding="utf-8")
        test("过期输入被拒绝", state)
        source.write_text("relevant input v1", encoding="utf-8")
        bad = copy.deepcopy(state)
        bad["tasks"][0]["criteria"] = []
        test("删除验收项被拒绝", bad)
        bad = copy.deepcopy(state)
        bad["tasks"][0]["depends_on"] = ["HA-00"]
        test("环形依赖被拒绝", bad)
        bad = copy.deepcopy(state)
        bad["handoff"]["unresolved_effects"] = ["effect unknown"]
        test("未知效果不得收口", bad)
        bad = copy.deepcopy(state)
        bad["tasks"][0]["status"] = "pending"
        test("未完成不得收口", bad)
        bad = copy.deepcopy(state)
        bad["evidence"][0]["artifacts"][0]["path"] = "../outside.log"
        test("越界证据路径被拒绝", bad)
        return {"self_test_passed": all(item["passed"] for item in results), "cases": results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--complete", action="store_true", help="要求所有必做项有证据并收口")
    parser.add_argument("--self-test", action="store_true", help="运行检查器的隔离负例，不验收业务")
    args = parser.parse_args()
    if args.self_test:
        result = self_test()
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result["self_test_passed"] else 1
    try:
        data = json.loads((BASE / "state.json").read_text(encoding="utf-8"))
        expected = contract()
        errors = validate(data, ROOT, expected, args.complete)
        tasks = data["tasks"]
        cases = [case for task in tasks for case in task.get("criteria", [])]
        baseline = json.loads((BASE / "baseline.json").read_text(encoding="utf-8"))
        drift = [item["path"] for item in baseline["source_files"]
                 if not inside(ROOT, item["path"]).is_file() or digest(inside(ROOT, item["path"])) != item["sha256"]]
        result = {"mode": "completion" if args.complete else "structure",
                  "structure_valid": not errors, "implementation_complete": not validate(data, ROOT, expected, True),
                  "required_tasks": sum(task["required"] for task in tasks),
                  "task_status": dict(Counter(task["status"] for task in tasks)),
                  "criteria": len(cases), "criterion_status": dict(Counter(case["status"] for case in cases)),
                  "errors": errors, "baseline_source_changes_for_review": drift,
                  "notice": "基线变化需判断相关性；本检查不证明业务行为或性能。"}
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        result = {"structure_valid": False, "implementation_complete": False, "errors": [str(error)]}
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["structure_valid"] else 1


if __name__ == "__main__":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    raise SystemExit(main())
