# 六类 S0 真实聊天入口受控录制

> 后续状态：六类和三种拒绝已整理进[正式驱动与 CI 接入记录](ci-integration-2026-09-27.md)。下文保留 0.2.21 原型当时的结果、隔离修正过程和真实失败，不追溯改称原型已有 CI 门禁。

日期：2026-09-27。执行者为 GPT-6 Sol Max，主会话核对归档事实。使用 0.2.21 release Web 的独立副本，SHA-256 `A55CD7ED616728211FCE93F4239ED2FBE128687EBBA09FDB15FB2984A46E521B`。本地假模型、具名临时工程与独立 SQLite 经真实 Web 接纳和工具派发；不是云模型表现、历史用户会话逐字回放或原生 UI 验收。

**总体结论：不能全绿。** 六类受控主路径获得结果，图片命名冲突的额外真实复现确认产品缺陷：已保存 text 类型被模型名称覆盖，生成失败又被记 completed。该缺陷与后续修复见 [专项审查](../../../analysis/2026-09-27-explicit-model-routing-review.md)。正常名称图片正例不能替它关闭。

## 实际证据

[结构化证据](evidence.json)包含场景事实、持久化运行和工具记录、真实 provider 请求 payload。仅将受控目录替换为 `$RUN` 和 `$S0_TMP`；payload SHA 为替换前规范 JSON 摘要，不能直接拿脱敏文本计算同一摘要。所有模型返回与用量数字是受控夹具，不是云计费。

| 场景 | 实际入口和结果 | 本机录制目录 ID |
| --- | --- | --- |
| 流式工具配对 | `read_file` 真实派发，工具结果及调用 ID 进入后续 provider 请求；两次请求、两条用量，同 run；持久化 completed | `9240e915b48b` |
| 非流式工具配对 | `glob_search` 真实派发并反馈；两次请求，持久化 completed。旧清单 `list_directory` 未被产品暴露，原样失败保留；这里是等价行为的新录制 | `35e93cbce784` |
| 图片路由 | 原图直传一个 chat 请求；默认视觉转述为 vision_description 与 chat 两个请求，各自一条用量且归属同一 run；两个运行 completed。输入是 1px 合成 PNG | `46b1d7a07db6` |
| 空最终回复 | provider 有 reasoning、无最终正文；落库 run 为 failed，界面消息类型 assistant-error，有明确诊断；reasoning 单独保留且未复制到最终正文 | `872bc2fb3e40` |
| 跨轮过滤 | 两轮运行 completed，共三个请求；上一轮原始 tool-result 仍持久化但未进入下一轮 provider payload。必要用户提示、最终答复和工具执行事实保留，不是删除全部合法历史 | `1f7a83b18471` |
| 长文件任务 | `write_file` 写入具名 `long-result.txt`，真实工具完成及反馈进入第二次模型请求，运行 completed；副作用限定临时工程 | `2447fa4f8e8e` |

空最终回复的 failed 是该负向业务输入的正确收尾，不因测试通过而改记 completed。请求用量记录的 completed 表示请求自身完成，也不能代替业务 run 的终态。公开记录中工具调用关联同时保留内部 call ID、provider call ID 与 source request key；部分数据库 request_attempt_id 仍为空，不声称所有身份字段均非空。

## 回放边界负例

未知指纹 `53fe592070fa`、参数偏移 `e108282e53b0`、超出具名临时路径 `75dde97f8dbc` 均经真实 Web 请求抵达本地假模型，由**测试代理的回放白名单**返回 422。每项 provider 接收一次请求尝试、接受零次调用输出，持久化 run 为 failed，工具派发和工具调用记录均为零；超界目标是另一个具名临时哨兵，不是用户目录。

这证明受控回放不会发出未登记副作用，不证明正常产品拥有同一套路径白名单沙箱。产品权限回归属于其他报告。

## 发现过程和隔离修正

最初两轮虽然会话库、工程和工具审计位于临时目录，诊断日志仍追加到了用户 home 的既有日志。只核实路径/元数据，未读取或删除原日志；不能据此证明当时没有其他 home 配置读取。后续显式隔离 `USERPROFILE`、`HOME`、`APPDATA`、`LOCALAPPDATA`、`CLAW_CONFIG_HOME`、`COOLZHU_RUNTIME_DIR`、`COOLZHU_LOG_DIR`，确认日志实际落在各次临时目录后重跑。上表采用修正后的记录，原始失败保留。

图片正例调整名字后可通，不代表原失败只是夹具问题。后续显式 `model_type=text`、`supports_multimodal=false` 的 `s0-image-text` 复现保存在证据的 `media_name_probe`：配置显示视觉转述，实际零 provider 请求，却进入生成失败消息，run 错误完成。这是待修产品问题，不能删去失败记录。

## 后续出口

当前驱动原型在本机 `tmp/s0-controlled-replay-20260927/run.py`，没有进入正式 tests/CI；本报告不宣称已经建立 CI 回放门禁，也不关闭 Desktop04 WBS 0.2 的全部要求。路由和状态窄修后，必须使用新二进制身份复验命名冲突与受影响场景。新视觉和原生软件操作仍需单独 Computer Use 截图，不能由本报告替代。
