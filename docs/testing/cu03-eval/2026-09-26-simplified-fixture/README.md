# CU-03 试运行保留件：简化素材规划试运行（2026-09-26）

> 这是**实验证据**，不是可清理的中间产物。请勿删除；如需重新评分，见文末"重评分关系"。

## 1. 实验身份（如实记录，不把报告结果写成已独立复验）

| 项 | 值 |
| --- | --- |
| 实验 | CU-03 **简化素材**规划试运行（装置验证，**不是** Paint 收益结论） |
| 素材 | **合成占位观察**（Paint 形态：`canvas_rect` + 带状态/模式元素），**不是** R4–R6 真实录制 |
| 实际协议 | OpenAI-compatible **chat completions**（`/v1/chat/completions`） |
| 实际连接 | 本机 `ANTHROPIC_BASE_URL` 所指向的第三方 Anthropic 兼容代理 |
| 模型 | 请求值与响应声明值均为 `claude-sonnet-4-6`（原始结果里逐条保留） |
| 提示与反馈块 | **产品同一份代码**（`computer_use_planner::planning_prompt` + `bounded_step_feedback`） |
| 桌面动作 | **未执行**（纯规划请求；未注入任何输入） |
| 运行数 | 10（baseline，不带反馈）+ 10（feedback，带反馈）= 20 |

## 2. 原始结果

- `raw-results.json`：每次调用的动作、`metrics_v1` 三列、以及上一步的输入状态。
- 该次运行的**总计请求数不等于 20**：更早还有因代理响应缺顶层 `id` 而解析失败的请求（见
  `docs/analysis/.../rpr-execution-blockers.md` §B-103），失败请求不是免费请求，也未计入本目录。

## 3. 评分版本与重评分关系（不得覆盖旧结果）

- `metrics_v1`：旧三条规则（引用合法性／同目标再操作／一种可疑模式）。**已改名**，不得当作
  "目标选择正确／没有无效重复／没有不安全重放"（口径说明见
  `computer_use_eval_scorer::MetricsV1::disclaimer`）。
- `cu03-scorer-v2`：语义层（结构检查 + 三个带标签的行为指标 + 适用分母）。
- `rescore-cu03-scorer-v2.json`：**离线重评分**（不调用模型）结果。与 `raw-results.json` **并存**，
  不覆盖任何旧字段。

### 重评分的结论（如实）

| 维度 | 结果 |
| --- | --- |
| 解析 / 结构检查 | 20 / 20 通过 |
| 目标／操作选择 | **不可评分**（20/20 缺冻结标签 ⇒ insufficient_evidence） |
| 无效重复副作用 | **不可评分**（20/20 缺上一步动作类型与副作用键） |
| 未对账的危险重放 | **不可评分**（10 例缺上一步目标/副作用键；10 例不适用） |

⇒ **本次 20 次运行在语义层不可评分**：它只证明了"评测管线跑通"，**既不能**证明反馈有效，
**也不能**证明反馈无效，更**不能**作为 Paint 收益结论。这与裁决口径一致。

## 4. 复现

```bash
# 重新评分（离线、不调用模型）
cargo test -p coolzhu-web-console --offline simplified_fixture_run_is_rescored -- --nocapture
# 重跑真实试运行（消耗模型预算；需显式 --ignored）
cargo test -p coolzhu-web-console --offline cu03_model_planning_comparison -- --ignored --nocapture
```

## 5. 下一轮必须补什么（否则结论无意义）

1. **冻结素材标签**（每个样本的允许动作/目标、是否要求不动作），标签在跑之前固定；
2. **上一步事实**写进结果（动作类型、目标、副作用键、效果与进展）——否则重复与危险重放只能记"证据不足"；
3. **真实 R4–R6 录制**（`docs/testing/release-0.2.14/paint-r4..r6.json`）替换合成素材；
4. 两臂**同协议同参数**，唯一干预是"是否提供上一步事实反馈"。
