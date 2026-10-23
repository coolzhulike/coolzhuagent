# HA11 真实模型可用性探针

- 时间：2026-10-20（America/Los_Angeles）
- Room：`room-1792517847346` / `qwen-test`
- Session：`session-1792519612697-22ffea3a2956ec99`
- Provider/model：`custom` / `qwen3.8-flash`，reasoning_effort=`auto`，model_type=`text`
- 入口：`POST http://127.0.0.1:8765/api/chat/send`
- 输入：`请只回答一个数字：2+2=？不要使用工具，不要先搜索或计划。`

## 结果

- HTTP 状态：completed
- turn：`chat-turn-1792519936094-1`
- elapsed：48,402 ms
- assistant content：`4`
- 未观察到工具调用；回复附带 context usage `1472/1000000`。
- 原始响应：`runs/probe-01/response.json`

## 说明

这仅证明 qwen 会话和 API 可用，不构成 HA11 的基线/候选配对结果。B01–B05 各 3 组配对、B06–B08 故障/并发和耗时指标仍未执行；基线需从 `baseline/HA-00-20261018` 恢复并单独构建，不能把当前候选冒充基线。
