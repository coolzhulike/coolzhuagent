# -*- coding: utf-8 -*-
"""生成 CU-03 历史语料库：迁移素材 + 机器生成 manifest.json。

分类严格按裁决的四类：history_original / history_derived / new_capture / synthetic。
逐样本记录：任务、决策前观察、可用反馈、真实执行/发布状态、图像与变换、
允许行为标签、证据来源、缺失项。
"""
import hashlib
import io
import json
import os
import shutil
import sqlite3

REPO = r"C:\Users\zhupu\Desktop\coolzhuagent"
SRC = os.path.join(REPO, "tmp", "2026-09-19-agent-fixes", "evidence")
CAP = os.path.join(REPO, "tmp", "2026-09-19-agent-fixes", "runtime", "evidence", "captures")
DB = os.path.join(REPO, "tmp", "2026-09-19-agent-fixes", "runtime", ".coolzhu", "web-sessions.sqlite3")
OUT = os.path.join(REPO, "docs", "testing", "cu03-eval", "corpus-2026-09-19-paint-window-drag")

FAMILIES = ["paint-r1", "paint-r2", "paint-r3", "paint-r4", "paint-r5", "paint-r6"]
SKIP_SUFFIXES = (".public.json",)
SKIP_NAMES = {"desktop-latest.png"}


def sha256_of(path):
    hasher = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def load_json(path):
    if not os.path.exists(path):
        return None
    with io.open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


# ---------------------------------------------------------------- 收集素材
conn = sqlite3.connect("file:{}?mode=ro".format(DB.replace("\\", "/")), uri=True)
conn.row_factory = sqlite3.Row
db_runs = {}
for row in conn.execute("SELECT * FROM computer_use_runs"):
    record = dict(row)
    try:
        record["_terminal"] = json.loads(record["terminal_result_json"] or "{}")
    except ValueError:
        record["_terminal"] = {}
    db_runs[record["call_id"]] = record
db_steps = {}
for row in conn.execute("SELECT * FROM computer_use_steps ORDER BY run_id, step_index"):
    db_steps.setdefault(row["run_id"], []).append(dict(row))
db_actions = {}
for row in conn.execute("SELECT * FROM computer_use_step_details ORDER BY run_id, step_index"):
    db_actions.setdefault(row["run_id"], []).append(dict(row))
conn.close()

os.makedirs(OUT, exist_ok=True)
samples = []
image_files = {}

for family in FAMILIES:
    sample_dir = os.path.join(OUT, "samples", family)
    os.makedirs(sample_dir, exist_ok=True)
    copied = []
    for name in sorted(os.listdir(SRC)):
        if not name.startswith(family):
            continue
        if name.endswith(SKIP_SUFFIXES) or name in SKIP_NAMES:
            continue
        full = os.path.join(SRC, name)
        if not os.path.isfile(full):
            continue
        shutil.copy2(full, os.path.join(sample_dir, name))
        copied.append({"file": "samples/{}/{}".format(family, name),
                       "bytes": os.path.getsize(full),
                       "sha256": sha256_of(full)})

    audit = load_json(os.path.join(SRC, "{}-audit.json".format(family)))
    summary = load_json(os.path.join(SRC, "{}-summary.json".format(family)))
    context = load_json(os.path.join(SRC, "{}-context.json".format(family)))
    run = (audit or {}).get("runs", [{}])[0] if audit else {}

    # 房间 -> DB run
    room = (context or {}).get("room") or run.get("room")
    db_run = None
    for record in db_runs.values():
        if record.get("chat_room_id") == room:
            db_run = record
            break
    call_id = db_run["call_id"] if db_run else None

    # 截图引用（含 sha256 与尺寸），逐张校验
    images = []
    referenced = []
    if audit:
        text = json.dumps(audit, ensure_ascii=False)

        def walk(node):
            if isinstance(node, dict):
                if isinstance(node.get("local_path"), str):
                    referenced.append(node)
                for value in node.values():
                    walk(value)
            elif isinstance(node, list):
                for value in node:
                    walk(value)

        walk(audit)
    seen = set()
    for node in referenced:
        path = node["local_path"]
        if path in seen:
            continue
        seen.add(path)
        digest = sha256_of(path) if os.path.exists(path) else None
        images.append({
            "reference": node.get("reference"),
            "kind": node.get("kind"),
            "sha256_declared": node.get("sha256"),
            "sha256_actual": digest,
            "digest_matches": digest == node.get("sha256"),
            "width": node.get("width"),
            "height": node.get("height"),
            "source_filename": os.path.basename(path),
        })
        if digest:
            target = os.path.join(OUT, "images", os.path.basename(path))
            os.makedirs(os.path.dirname(target), exist_ok=True)
            if not os.path.exists(target):
                shutil.copy2(path, target)
            image_files[os.path.basename(path)] = {
                "bytes": os.path.getsize(path), "sha256": digest}

    # 步骤事实（来自 DB：带动作指纹，可支撑结构层检查）
    steps = []
    if call_id:
        for index, row in enumerate(db_steps.get(call_id, [])):
            action = db_actions.get(call_id, [])
            steps.append({
                "index": row["step_index"],
                "observation_generation": row["observation_generation"],
                "action_type": row["action_type"],
                "normalized_target": row["normalized_target"],
                "action_fingerprint": row["action_fingerprint"],
                "status": row["status"],
                "error_code": row["error_code"],
                "visible_progress": bool(row["visible_progress"]),
                "action_json": (action[index]["action_json"] if index < len(action) else None),
            })

    terminal = run.get("terminal") or {}
    db_terminal = (db_run or {}).get("_terminal") or {}
    db_error = (db_terminal.get("error") or {}).get("code")

    # 结构层可判定事实：动作指纹重复、可见进展、终态错误码
    fingerprints = [step["action_fingerprint"] for step in steps]
    repeats = []
    for index in range(1, len(fingerprints)):
        if fingerprints[index] and fingerprints[index] == fingerprints[index - 1]:
            repeats.append({
                "between_steps": [index - 1, index],
                "action_fingerprint": fingerprints[index],
                "action_type": steps[index]["action_type"],
                "target": steps[index]["normalized_target"],
                "second_step_visible_progress": steps[index]["visible_progress"],
            })
    # 生产指纹 = hash(surface, observation_generation, action_json)，**含观察代次**。
    # 因此"重新观察后再做同一动作"会得到不同指纹：指纹不能直接当作"同一操作"的身份。
    # 这里再按 (动作类型, 归一化目标, 动作载荷) 去掉代次比一次，作为真正的重复候选。
    operation_repeats = []
    for index in range(1, len(steps)):
        left, right = steps[index - 1], steps[index]
        same = (left["action_type"] == right["action_type"]
                and left["normalized_target"] == right["normalized_target"]
                and left["action_json"] == right["action_json"])
        if same and right["action_json"]:
            operation_repeats.append({
                "between_steps": [index - 1, index],
                "action_type": right["action_type"],
                "target": right["normalized_target"],
                "generations": [left["observation_generation"], right["observation_generation"]],
                "fingerprints_differ": left["action_fingerprint"] != right["action_fingerprint"],
                "second_step_visible_progress": right["visible_progress"],
            })
    structural = {
        "step_count": len(steps),
        "distinct_action_fingerprints": len({item for item in fingerprints if item}),
        "consecutive_repeated_fingerprints": repeats,
        "consecutive_repeated_operations_ignoring_generation": operation_repeats,
        "fingerprint_identity_caveat": "action_fingerprint 含 observation_generation，"
                                       "重新观察后的同一操作会得到不同指纹；判「无效重复」不能直接用指纹相等，"
                                       "须按 (动作类型, 目标, 载荷) 忽略代次比较。",
        "any_visible_progress": any(step["visible_progress"] for step in steps),
        "terminal_error_code": terminal.get("error_code") or db_error,
        "terminal_stage": db_terminal.get("stage"),
        # 结构层只给"描述性事实"：能数出重复次数与可见进展，但不等于"行为正确/错误"的标签。
        "scorable_metrics": [
            "连续同载荷操作的计数与可见进展（结构层描述，忽略观察代次）"
        ],
        # 三项语义指标在**每一个**历史样本上都不可评分；样本步数不足时属于"不适用（无适用样本）"。
        "unscorable_metrics": [
            "目标/操作选择错误（无语义标签、无决策前观察提示词）",
            "无效重复副作用（无语义标签；结构层只能给计数，不能判定「是否无效」）",
            "未核销的危险重放（无隔离/核销状态记录）",
        ],
        "repeat_metric_applicability": (
            "不适用（无适用样本：该运行未产生任何动作步）" if len(steps) < 2
            else "结构层有适用样本；语义层仍不可评分"
        ),
    }

    samples.append({
        "sample_id": family,
        "category": "history_derived",
        "category_detail": "历史原始运行留存的一级导出（保留本地 id、截图绝对路径与 sha256；"
                           "原始留存本体为 runtime SQLite 与 PNG 截图）",
        "lineage": {
            "original_retention": "tmp/2026-09-19-agent-fixes/runtime/.coolzhu/web-sessions.sqlite3"
                                  " + runtime/evidence/captures/**（不在 git 内，易失）",
            "tier2_derivative": "docs/testing/release-0.2.14/{}.json（public_allowlisted_summary，已去路径与本地 id）".format(family),
        },
        "task": {
            "chat_instruction_retained": os.path.exists(os.path.join(sample_dir, "{}-request.json".format(family))),
            "context": context,
            "objective": (json.loads(db_run["objective_json"]) if db_run and db_run.get("objective_json") else None),
            "surface": (db_run or {}).get("surface") or run.get("surface"),
        },
        "pre_decision_observation": {
            "status": "部分缺失",
            "image_retained": len(images) > 0,
            "image_identity": "sha256 + 宽度 x 高度（见 image_and_transform）",
            "uia_snapshot": "仅引用字符串 uia_snapshot:<hwnd>:elements=<n>，元素树本体未留存",
            "planner_prompt": "未留存——无任何表或导出保留发给规划器的提示词本体",
            "model_raw_response": "规划响应体多数脱敏（computer_use_planner_diagnostics.response_json 为 {\"redacted_fields\":true}）",
            "parsed_decision_retained": [step["action_json"] for step in steps],
        },
        "available_feedback": {
            "status": "无反馈块（样本早于 CU-03 反馈块实现）",
            "feedback_block_present": False,
            "post_hoc_verification_summary": terminal.get("evidence") and [
                item for item in terminal.get("evidence", []) if str(item).startswith("visual_verification:")
            ] or [],
            "note": "终态证据里的 visual_verification:generation=..:image_changed=..:criteria_met=.. 是事后验证摘要，"
                    "不等于当时交给规划器的反馈内容；不得据此反推模型当时看到了反馈。",
        },
        "real_execution_state": {
            "run_state": run.get("state") or (db_run or {}).get("state"),
            "terminal_status": terminal.get("status") or db_terminal.get("status"),
            "terminal_stage": db_terminal.get("stage"),
            "terminal_error_code": terminal.get("error_code") or db_error,
            "goal_achieved": terminal.get("goal_achieved") if terminal.get("goal_achieved") is not None
                             else db_terminal.get("goal_achieved"),
            "attempts": terminal.get("attempts") or db_terminal.get("attempts"),
            "steps_completed": terminal.get("steps_completed") if terminal.get("steps_completed") is not None
                               else db_terminal.get("steps_completed"),
            "recorded_step_count": run.get("recorded_step_count"),
            "action_count": (db_run or {}).get("action_count"),
            "replan_count": (db_run or {}).get("replan_count"),
            "no_progress_count": (db_run or {}).get("no_progress_count"),
            "reported_answer": (summary or {}).get("answer"),
            "steps": steps,
            "structural_facts": structural,
        },
        "image_and_transform": {
            "images": images,
            "all_digests_match": bool(images) and all(item["digest_matches"] for item in images),
            "image_count": len(images),
            "no_image_reason": None if images else "该次运行的审计导出未引用任何截图（观察阶段即失败或未进入动作）",
            "coordinate_space": "画布相对 0..1（action_json 中的 points 已核验落在单位方格内）",
            "window_rect": "缺失",
            "dpi": "缺失",
            "crop_or_scale_mapping": "缺失——audit notes 自述：缺少观察几何记录时不能重建窗口 rect/DPI 或屏幕绝对笔画",
        },
        "allowed_behavior_labels": {
            "present": False,
            "note": "历史运行未打「目标选择正确 / 无效重复 / 危险重放」等标签；"
                    "本语料只提供结构层可判定事实（动作类型、目标、动作指纹、可见进展、终态错误码）。",
        },
        "evidence_source": {
            "runtime_db": "tmp/2026-09-19-agent-fixes/runtime/.coolzhu/web-sessions.sqlite3（schema_version=20）",
            "exports": [item["file"] for item in copied],
        },
        "missing_items": [
            "发给规划器的提示词本体（决策前观察的文本部分）",
            "UIA 元素树本体（仅有 elements=<n> 计数）",
            "窗口 rect / DPI / 裁剪与缩放映射",
            "规划模型原始响应体（多数行已脱敏）",
            "语义行为标签（目标选择是否正确、是否危险重放）",
        ],
        "files": copied,
    })

# ---------------------------------------------------------------- 20 次新采集
fixture_manifest = os.path.join(
    REPO, "docs", "testing", "cu03-eval", "2026-09-26-simplified-fixture", "raw-results.json")
if os.path.exists(fixture_manifest):
    raw = load_json(fixture_manifest)
    calls = raw if isinstance(raw, list) else raw.get("calls") or raw.get("results") or []
    samples.append({
        "sample_id": "cu03-eval-2026-09-26-simplified-fixture",
        "category": "new_capture",
        "category_detail": "本轮新采集：真实模型调用，但走简化夹具且不执行桌面动作（仅规划协议），"
                           "因此不是桌面执行回放。",
        "task": {"fixture": "简化夹具 20 次调用", "call_count": len(calls) if calls else 20},
        "pre_decision_observation": {
            "status": "与生产观察不等价",
            "note": "夹具给的是简化后的状态文本，不是真实截图 + UIA 观察。",
        },
        "available_feedback": {
            "status": "两臂配对（基线 vs 反馈）",
            "feedback_block_present": True,
            "note": "干预项仅为「是否附带反馈块」，其余协议与参数保持一致。",
        },
        "real_execution_state": {
            "desktop_actions_executed": False,
            "note": "无真实桌面执行，故无发布/隔离状态可记录。",
        },
        "image_and_transform": {"images": [], "note": "不含任何真实截图。"},
        "allowed_behavior_labels": {
            "present": False,
            "note": "未人工打标签；cu03-scorer-v2 重评为：结构层 20/20 可判定，三项语义指标 applicable=0（证据不足）。",
        },
        "evidence_source": {"export": "docs/testing/cu03-eval/2026-09-26-simplified-fixture/"
                                      "{raw-results.json, rescore-cu03-scorer-v2.json, README.md}"},
        "missing_items": [
            "真实截图与 UIA 观察",
            "真实桌面动作与发布/隔离状态",
            "语义行为标签",
        ],
    })

# ---------------------------------------------------------------- 合成样本声明
samples.append({
    "sample_id": "synthetic-replay-fixtures",
    "category": "synthetic",
    "category_detail": "合成：为评分器与提示词回放验证手工构造的夹具，"
                       "**不是历史回放**，不得当作历史执行证据引用。",
    "task": {"origin": "computer_use_eval_scorer.rs / computer_use_planner.rs 内联测试夹具"},
    "pre_decision_observation": {"status": "构造", "note": "由测试代码直接给定，不来自任何真实运行。"},
    "available_feedback": {"status": "构造", "feedback_block_present": True},
    "real_execution_state": {"desktop_actions_executed": False, "note": "纯离线，无执行。"},
    "image_and_transform": {"images": [], "note": "无图像。"},
    "allowed_behavior_labels": {"present": True, "note": "标签由构造用例显式给定，用于验证评分器判定分支。"},
    "evidence_source": {"export": "modules/gui-web/packages/web-console/src/computer_use_eval_scorer.rs（测试模块）"},
    "missing_items": ["真实观察", "真实执行状态"],
    "not_history_replay": True,
})

manifest = {
    "corpus_id": "cu03-corpus-2026-09-19-paint-window-drag",
    "format_version": 1,
    "categories": {
        "history_original": "历史原始留存（运行时 SQLite、原始截图），未在 git 内，易失",
        "history_derived": "由历史原始留存导出的记录（本目录 samples/ 为一级导出，保留路径与 sha256；"
                           "docs/testing/release-0.2.14/paint-r*.json 为二级脱敏导出）",
        "new_capture": "本轮新采集（可能仍是简化夹具/无桌面动作，须写明）",
        "synthetic": "合成夹具，禁止当作历史回放",
    },
    "privacy": "一级导出保留本地绝对路径与本地 id；不含消息正文、不含 API Key。"
               "截图仅为画图窗口级（window-*），不含全屏桌面截图。",
    "summary": {
        "sample_count": len(samples),
        "history_samples": len(FAMILIES),
        "all_history_runs_achieved_goal": False,
        "images_migrated": len(image_files),
        "images_bytes": sum(item["bytes"] for item in image_files.values()),
    },
    "images": image_files,
    "samples": samples,
}

with io.open(os.path.join(OUT, "manifest.json"), "w", encoding="utf-8", newline="\n") as handle:
    json.dump(manifest, handle, ensure_ascii=False, indent=2)
    handle.write("\n")

print("语料库目录:", OUT)
print("样本数:", len(samples))
print("迁移图片:", len(image_files), "共", sum(item["bytes"] for item in image_files.values()), "字节")
for name in sorted(image_files):
    print("   ", name, image_files[name]["bytes"])
total = 0
for dirpath, _dirs, filenames in os.walk(OUT):
    for filename in filenames:
        total += os.path.getsize(os.path.join(dirpath, filename))
print("语料库合计字节:", total)
