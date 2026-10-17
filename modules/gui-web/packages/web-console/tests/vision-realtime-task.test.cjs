// 预留未请求的视觉检测资源缺口不得误报 failed 任务；真实请求/运行失败仍可告警（executionFailed 优先于 running）。
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const source = fs.readFileSync(path.join(__dirname, "../src/app.js"), "utf8");
const start = source.indexOf("function visionRealtimeTaskItem(");
const end = source.indexOf("function syncVisionRealtimeTask(");
assert(start >= 0 && end > start, "visionRealtimeTaskItem 片段未找到");
const context = vm.createContext({});
vm.runInContext(source.slice(start, end), context);
const item = (s) => context.visionRealtimeTaskItem(s);

test("预留配置缺资源但从未请求/运行：不得报 failed", () => {
  const task = item({
    detection_backend: "uidetr1", detection_model: "UI-DETR-1",
    detection_service_reachable: false, detection_model_path_exists: false,
    loop_requested: false, loop_running: false, frames_processed: 0,
    status: "reserved_realtime_perception", active_loop: "reserved_realtime_perception",
  });
  assert.ok(task);
  assert.equal(task.status, "pending");
});

test("已请求检测循环但资源缺失：真实失败告警 failed（不再误标 running）", () => {
  const task = item({
    detection_backend: "uidetr1", detection_service_reachable: false,
    detection_model_path_exists: false, loop_requested: true, loop_running: false,
  });
  assert.equal(task.status, "failed");
});

test("已请求且资源正常的在跑循环：仍显示 running", () => {
  const task = item({
    detection_backend: "uidetr1", detection_service_reachable: true,
    detection_model_path_exists: true, loop_requested: true, loop_running: true,
  });
  assert.equal(task.status, "running");
});

test("跑过（有执行痕迹）且资源缺失：仍可告警 failed", () => {
  const task = item({
    detection_backend: "uidetr1", detection_service_reachable: false,
    detection_model_path_exists: false, loop_requested: false, loop_running: false,
    started_at_ms: 1700000000000, frames_processed: 0,
  });
  assert.equal(task.status, "failed");
});

test("last_error 真实失败事实即使未在跑也告警且摘要含错误", () => {
  const task = item({
    detection_backend: "uidetr1", detection_service_reachable: true,
    detection_model_path_exists: true, last_error: "detector exited code 3",
  });
  assert.equal(task.status, "failed");
  assert.match(task.summary, /detector exited code 3/);
});

test("停止后资源正常的循环不误报任务", () => {
  const task = item({
    detection_backend: "uidetr1", detection_service_reachable: true,
    detection_model_path_exists: true, loop_requested: false, loop_running: false,
    stopped_at_ms: 1700000000000,
  });
  assert.equal(task, null);
});

test("无检测配置仍不产出任务", () => {
  assert.equal(item({}), null);
});
