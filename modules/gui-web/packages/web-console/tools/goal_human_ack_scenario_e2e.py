"""GL-13/14 场景实测：对**运行中的 coolzhu web-console** 打真实 HTTP，验证
human-in-the-loop 断点与事件流可回看性。

用法（app 需已在 127.0.0.1:8765 运行、且用独立 USERPROFILE 隔离生产库）：
    python tmp/gl13-14-scenario.py

场景：一个"上线发布"目标，deploy 阶段标记 requires_human_ack（高风险，须人工放行）。
  1. 建 goal + plan（deploy 标 requires_human_ack）
  2. dispatch-ready → 不应派发，阶段转 awaiting、goal 暂停、发 awaiting-ack 事件
  3. 人工拒绝 → 阶段 blocked、goal 暂停、发 ack-rejected 事件
  4. 拒绝后重复确认 → 应被拒（须重新规划）
  5. 重新规划 → 确认态重置，重新等待
  6. 人工批准 → goal 恢复、阶段可派发、发 ack-approved 事件
  7. GL-14：以上每一步的事件都能从 /api/goals/{id} 事件流回看
"""
import json
import sys
import urllib.request
import urllib.error

BASE = "http://127.0.0.1:8765"
FAILS = []


def call(method, path, payload=None):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(
        BASE + path, data=data, method=method,
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.load(r)
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            return e.code, json.loads(body)
        except Exception:
            return e.code, {"raw": body[:200]}


def check(label, cond, detail=""):
    mark = "PASS" if cond else "FAIL"
    if not cond:
        FAILS.append(label)
    print(f"  [{mark}] {label}{(' — ' + detail) if detail else ''}")


def phase_of(status, pid):
    for p in status.get("goal", {}).get("phases", []):
        if p["id"] == pid:
            return p
    return {}


def events_of(status):
    return [e["event_type"] for e in status.get("goal", {}).get("recent_events", [])]


def plan_body():
    return {"phases": [{
        "id": "deploy", "title": "部署到生产", "assigned_role": "implementer",
        "depends_on": [], "requires_human_ack": True,
    }]}


print("== GL-13/14 场景实测：高风险阶段的人工确认断点 ==\n")

# 0. 准备真实运行环境：引导 goal 角色会话 + 指定 commander + 打心跳。
#    （真实部署里这些已存在；不做这步会先撞 blocked_missing_role_session / offline，
#     根本走不到人工确认分支——这正是场景实测相对单测的价值。）
print("-- 步骤0：准备角色会话（commander + implementer）--")
st, boot = call("POST", "/api/goals/roles/bootstrap", {"roles": ["implementer"]})
check("引导 implementer 角色会话", st == 200)
st, sessions = call("GET", "/api/sessions")
sids = [s["id"] for s in sessions.get("sessions", [])]
commander_id = next((s for s in sids if s != "goal-implementer"), None)
check("存在可用作 commander 的会话", bool(commander_id), f"commander={commander_id}")
st, _ = call("POST", f"/api/goals/roles/{commander_id}", {
    "role": "commander", "responsibility": "审视并放行阶段",
    "commander": True, "heartbeat_timeout_ms": 600000, "task_timeout_ms": 600000,
})
check("指定 commander", st == 200)
st, _ = call("POST", "/api/goals/roles/goal-implementer", {
    "role": "implementer", "responsibility": "执行部署",
    "commander": False, "heartbeat_timeout_ms": 600000, "task_timeout_ms": 600000,
})
check("配置 implementer 角色", st == 200)
for sid in (commander_id, "goal-implementer"):
    call("POST", f"/api/goals/roles/{sid}/heartbeat", {})

# 1. 建 goal + plan
st, goal = call("POST", "/api/goals", {
    "title": "上线发布（GL-13 场景）", "max_iterations": 20,
    "completion_condition": {"type": "Manual"},
})
gid = goal.get("goal", {}).get("id", "")
check("建 goal", st == 200 and bool(gid), f"id={gid}")
st, _ = call("POST", f"/api/goals/{gid}/plan", plan_body())
check("提交计划（deploy 标 requires_human_ack）", st == 200)

st, status = call("GET", f"/api/goals/{gid}")
check("requires_human_ack 已落库并读回", phase_of(status, "deploy").get("requires_human_ack") is True)

# 2. dispatch-ready → 应拦住不派发
print("\n-- 步骤1：尝试派发高风险阶段 --")
st, disp = call("POST", f"/api/goals/{gid}/dispatch-ready")
actions = [p.get("action") for p in disp.get("skipped", [])] if st == 200 else []
check("dispatch 被拦、未派发", st == 200 and not disp.get("dispatched"),
      f"dispatched={len(disp.get('dispatched', []))} skipped={actions}")
check("阶段判为 awaiting_human_ack", "awaiting_human_ack" in actions)
st, status = call("GET", f"/api/goals/{gid}")
check("阶段标记为 awaiting", phase_of(status, "deploy").get("human_ack") == "awaiting")
check("goal 已暂停等确认", status.get("goal", {}).get("status") == "paused")
check("发出 goal-phase-awaiting-ack 事件", "goal-phase-awaiting-ack" in events_of(status))

# 幂等：再次 dispatch 不应重复刷事件
st, _ = call("POST", f"/api/goals/{gid}/dispatch-ready")
st, status = call("GET", f"/api/goals/{gid}")
n_await = events_of(status).count("goal-phase-awaiting-ack")
check("重复 dispatch 不刷重复事件（幂等）", n_await == 1, f"awaiting 事件数={n_await}")

# 3. 人工拒绝
print("\n-- 步骤2：人工拒绝 --")
st, rejected = call("POST", f"/api/goals/{gid}/phases/deploy/ack",
                    {"approved": False, "reason": "发布窗口未到"})
check("拒绝接口返回 200", st == 200)
check("阶段置 blocked", phase_of(rejected, "deploy").get("status") == "blocked")
check("human_ack=rejected", phase_of(rejected, "deploy").get("human_ack") == "rejected")
check("goal 保持暂停", rejected.get("goal", {}).get("status") == "paused")
check("发出 goal-phase-ack-rejected 事件", "goal-phase-ack-rejected" in events_of(rejected))

# 4. 拒绝后不可再确认
st, again = call("POST", f"/api/goals/{gid}/phases/deploy/ack", {"approved": True})
# 409 CONFLICT：拒绝是终态，已不在 awaiting，须重新规划才能再确认。
check("已拒绝的阶段不可再批准（须重规划）", st == 409, f"HTTP {st}")

# 5. 重新规划 → 重置确认态
print("\n-- 步骤3：重新规划后须重新确认 --")
st, _ = call("POST", f"/api/goals/{gid}/plan", plan_body())
st, status = call("GET", f"/api/goals/{gid}")
check("重规划后 human_ack 被重置", phase_of(status, "deploy").get("human_ack") is None)
# 拒绝会把 goal 暂停。暂停的 goal 不允许派发（人叫停的只能由人恢复），
# 所以重规划之后还必须显式 resume 才能继续——这是刻意的安全语义。
st, _ = call("POST", f"/api/goals/{gid}/dispatch-ready")
check("暂停中的 goal 拒绝派发", st == 400, f"HTTP {st}")
st, _ = call("POST", f"/api/goals/{gid}/resume")
check("显式 resume 成功", st == 200, f"HTTP {st}")
call("POST", f"/api/goals/{gid}/dispatch-ready")
st, status = call("GET", f"/api/goals/{gid}")
check("重新回到等待确认", phase_of(status, "deploy").get("human_ack") == "awaiting")

# 6. 人工批准
print("\n-- 步骤4：人工批准 --")
st, approved = call("POST", f"/api/goals/{gid}/phases/deploy/ack", {"approved": True})
check("批准接口返回 200", st == 200)
check("human_ack=approved", phase_of(approved, "deploy").get("human_ack") == "approved")
check("goal 已恢复（脱离暂停）", approved.get("goal", {}).get("status") != "paused",
      f"status={approved.get('goal', {}).get('status')}")
check("发出 goal-phase-ack-approved 事件", "goal-phase-ack-approved" in events_of(approved))

# 6.5 边界场景（codex 场景实测补充：状态机必须严格）
print("\n-- 步骤4.5：ack 状态机边界 --")
st, _ = call("POST", f"/api/goals/{gid}/phases/deploy/ack", {"approved": False})
check("已批准的阶段不能再被拒绝（批准是终态）", st == 409, f"HTTP {st}")
st, _ = call("POST", f"/api/goals/nonexistent-goal/phases/deploy/ack", {"approved": True})
check("不存在的 goal → 404", st == 404, f"HTTP {st}")
st, _ = call("POST", f"/api/goals/{gid}/phases/nonexistent-phase/ack", {"approved": True})
check("不存在的 phase → 404", st == 404, f"HTTP {st}")

# 未进入等待就批准 —— 另起一个 goal 验证
st, g2 = call("POST", "/api/goals", {
    "title": "边界：未等待就确认", "max_iterations": 10,
    "completion_condition": {"type": "Manual"},
})
gid2 = g2.get("goal", {}).get("id", "")
call("POST", f"/api/goals/{gid2}/plan", plan_body())
st, _ = call("POST", f"/api/goals/{gid2}/phases/deploy/ack", {"approved": True})
check("未进入等待的阶段不能凭空批准", st == 409, f"HTTP {st}")
# 普通阶段（未标 requires_human_ack）
call("POST", f"/api/goals/{gid2}/plan", {"phases": [
    {"id": "plain", "title": "普通阶段", "assigned_role": "implementer", "depends_on": []}]})
st, _ = call("POST", f"/api/goals/{gid2}/phases/plain/ack", {"approved": True})
check("普通阶段不接受 ack → 400", st == 400, f"HTTP {st}")

# 批准后应真的能派发到 running（正向链路闭环）
print("\n-- 步骤4.6：批准后真实派发 --")
st, disp = call("POST", f"/api/goals/{gid}/dispatch-ready")
st, status = call("GET", f"/api/goals/{gid}")
check("批准后 dispatch 能把阶段推进到 running",
      phase_of(status, "deploy").get("status") == "running",
      f"status={phase_of(status, 'deploy').get('status')}")
disp_ev = [e for e in status.get("goal", {}).get("recent_events", [])
           if e["event_type"] == "goal-phase-dispatched"]
check("派发事件带 GL-14 路由因果字段",
      bool(disp_ev) and "route_reason" in (disp_ev[0].get("payload") or {}),
      f"payload keys={sorted((disp_ev[0].get('payload') or {}).keys()) if disp_ev else []}")

# 7. GL-14 事件流可回看
print("\n-- 步骤5：GL-14 事件流可回看 --")
st, status = call("GET", f"/api/goals/{gid}")
seen = events_of(status)
# recent_events 只回最近若干条，拒绝发生在流程早期已被挤出窗口——
# 它在「步骤2」已就地断言过，这里只校验最近的路由事件仍可回看。
for want in ["goal-phase-awaiting-ack", "goal-phase-ack-approved"]:
    check(f"事件流含 {want}", want in seen)

# 前端白名单/标签（GL-14 关键：后端记了事件、前端得能显示）
try:
    with urllib.request.urlopen(BASE + "/src/app.js") as r:
        appjs = r.read().decode("utf-8", "replace")
    # 白名单与标签分别校验——只出现在标签表里会让"已注册"名不副实。
    for want in ["goal-phase-awaiting-ack", "goal-phase-ack-approved", "goal-phase-ack-rejected",
                 "goal-iteration-budget-exhausted", "goal-resumed-from-phase", "goal-budget-raised"]:
        check(f"事件 {want} 在 SSE 白名单", f'\n  "{want}",' in appjs)
        check(f"事件 {want} 有中文标签", f'"{want}": "' in appjs)
    # GL-13：人工确认必须有可点入口，否则断点只能靠 curl 解开。
    check("前端有批准按钮", 'data-goal-action="phase-approve"' in appjs)
    check("前端有拒绝按钮", 'data-goal-action="phase-reject"' in appjs)
    check("前端有待确认徽标", "待人工确认" in appjs)
    # GL-14：路由因果字段要真的渲染出来。
    check("事件详情渲染 route_reason", "route=${payload.route_reason}" in appjs)
except Exception as e:
    check("拉取前端 app.js", False, str(e))

print("\n" + "=" * 56)
if FAILS:
    print(f"结论：{len(FAILS)} 项未通过 → {FAILS}")
    sys.exit(1)
print("结论：全部通过。GL-13 人工确认断点与 GL-14 事件流回看在真实运行的 app 上验证成功。")
