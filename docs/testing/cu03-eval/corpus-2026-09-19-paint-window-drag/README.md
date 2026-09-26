# CU-03 历史语料库：2026-09-19 画图窗口拖拽（paint-r1..r6）

本目录是 `CU03-CORPUS` 的交付物：把散落在 `tmp/`（已被 `.gitignore` 忽略、随时可能被清理）
的历史 CU 运行素材，按裁决要求的四类归档，并**逐样本**记录溯源信息与缺失项。

> 本目录的结论口径：**历史素材不足就明确写缺失，不补造、不外推**。任何"合成"内容都显式标为
> `synthetic`，**不得当作历史回放引用**。

## 1. 为什么需要这个语料库

裁决要求 CU-03 的语义层指标必须建立在**有真实来源的样本**上；而在做 `CU03-SCORER` 时已经确认：
`docs/testing/cu03-eval/2026-09-26-simplified-fixture/` 的 20 次调用虽然结构层 20/20 可判定，
但三项语义指标全部 `applicable = 0`（证据不足）。因此需要回头把**历史上真实执行过桌面动作**的
素材找出来，判断它们能支撑哪些指标。

本目录就是这次定位的结果：历史素材确实找得到（含真实截图），但**仍然不足以支撑语义层指标**，
原因是缺"决策前观察的文本本体 / UIA 元素树 / 几何与变换 / 语义标签"。以下逐项写明。

## 2. 素材来源与派生层级

同一批运行留下了**三层**记录，本目录只迁移一层，另两层保留在原处并在此登记：

| 层级 | 内容 | 位置 | 是否在 git 内 |
| --- | --- | --- | --- |
| 历史原始留存 | 运行时 SQLite（`schema_version=20`）与 PNG 截图 | `tmp/2026-09-19-agent-fixes/runtime/.coolzhu/web-sessions.sqlite3`、`runtime/evidence/captures/**` | 否（`tmp/` 被忽略，**易失**） |
| 历史派生（一级） | 保留本地 id、截图绝对路径与 `sha256` 的导出 | **本目录 `samples/`（已迁移）** | 是 |
| 历史派生（二级） | `public_allowlisted_summary`：已去路径与本地 id | `docs/testing/release-0.2.14/paint-r3..r6.json` | 是 |

一级与二级的**事实内容同源**：`docs/testing/release-0.2.14/paint-r4.json` 与本目录
`samples/paint-r4/paint-r4-audit.json` 的 `summary` 字段逐值一致（同为 1 次 blocked 运行、
2 步、6 条诊断、8 条用量），差别只在 `privacy` 取值、是否带 `database_path`/`local_path`。
因此**二级导出可用于对外引用，但无法用于核对截图**（路径已被剥除）。

## 3. 迁移内容

- `samples/paint-r1..r6/`：每次运行的 `audit / live / events / insights / context / request /
  run / summary / answer.txt / .sse` 等一级导出（**不含** `*.public.json`，避免与二级重复）。
- `images/`：**只迁移被审计记录实际引用到的窗口级截图**，共 8 张、1,708,685 字节。
  文件名即其 `sha256`（`window-<hwnd>-<sha256>.png`），可自校验。
- `tools/build_corpus.py`：语料库生成脚本（从原始留存重建 `manifest.json` 与迁移清单）。

**刻意未迁移**（在此登记，避免被误当成"不存在"）：

| 文件 | 原因 |
| --- | --- |
| `desktop-latest.png` | 与 `paint-r6-after.png` **字节相同**（同为 120,736 字节、`sha256=be647e42…`），重复内容不再存一份 |
| `window-25508-2dd872b7…png`、`window-9624-b930fd73…png` | 未被任何 `paint-r*-audit/live/events` 引用；`9624` 还是另一个窗口句柄 |

## 4. 分类与逐样本事实

分类口径（与裁决四类一致）见 `manifest.json` 的 `categories`。历史样本均为
`history_derived`（一级导出），其原始留存按上表登记。

**六次运行全部未达成目标**（`goal_achieved=false`）——本语料库是一个**失败语料库**，
其价值在于覆盖了真实的失败面，而不是成功演示：

| 样本 | 运行终态 | 失败阶段 | 错误码 | 步数 | 截图 | 语义标签 |
| --- | --- | --- | --- | --- | --- | --- |
| paint-r1 | failed | observation | `input_failed`（窗口身份/位置/DPI 已变化） | 0 | 0 | 无 |
| paint-r2 | blocked | intent_guard | `invalid_tool_input`（缺 `success_criteria`） | 0 | 0 | 无 |
| paint-r3 | blocked | supervisor | `no_progress` | 2（2× click） | 1 | 无 |
| paint-r4 | blocked | supervisor | `no_progress` | 2（2× drag） | 3 | 无 |
| paint-r5 | failed | planning | `planner_backend_unavailable`（规划器 20s 超时） | 2（2× drag） | 2 | 无 |
| paint-r6 | failed | execution | `stale_observation`（画布已变化需重新观察） | 3（click/click/drag） | 3 | 无 |

> `paint-r2` 的**聊天层**总结写的是 `recursive_call_blocked`，而**运行时权威记录**
> （`computer_use_runs.terminal_result_json`）是 `intent_guard / invalid_tool_input`。
> 本表与 `manifest.json` 采用运行时记录，聊天层措辞仅供参考。

逐样本的完整溯源（任务、决策前观察、可用反馈、真实执行/发布状态、图像与变换、
允许行为标签、证据来源、缺失项）在 `manifest.json` 的 `samples[]` 中，字段名即口径。

## 5. 共性缺失项（决定"能不能算语义指标"）

| 需要的证据 | 历史素材里的状态 |
| --- | --- |
| 决策前观察：**截图** | ✅ 有，且 `sha256` + 尺寸可校验（8/8 校验通过） |
| 决策前观察：**发给规划器的提示词本体** | ❌ 未留存——没有任何表或导出保留提示词 |
| 决策前观察：**UIA 元素树** | ❌ 仅有引用字符串 `uia_snapshot:<hwnd>:elements=<n>`，元素本体未留存 |
| 规划模型**原始响应体** | ❌ 多数已脱敏（`computer_use_planner_diagnostics.response_json` 为 `{"redacted_fields":true}`） |
| 可用反馈（当时交给规划器的反馈块） | ❌ 历史运行早于 CU-03 反馈块，确认**不存在** |
| 图像 → 物理坐标的映射（窗口 rect / DPI / 裁剪与缩放） | ❌ 全部缺失；审计 notes 自述"缺少观察几何记录时不能重建窗口 rect/DPI 或屏幕绝对笔画" |
| 语义行为标签（目标选择是否正确、是否危险重放） | ❌ 无 |

**因此**：历史样本只参与结构层指标；三项语义指标（目标/操作选择错误、无效重复副作用、
未核销的危险重放）在本语料库上**不可评分**，不得以任何方式补零或反推。
`manifest.json` 每个样本的 `structural_facts.scorable_metrics / unscorable_metrics` 逐条列出该判定。

⚠️ 一个容易误用的点：终态证据里的 `visual_verification:generation=..:image_changed=..:criteria_met=..`
是**事后验证摘要**，不是当时交给规划器的反馈内容。不得据此回填"模型当时看到了什么反馈"。

## 6. 归档过程中得到的两个硬结论

**(1) `action_fingerprint` 不能当作"同一操作"的身份。**
生产代码是 `hash(surface, observation_generation, action_json)`（见
`computer_use_executor.rs:230-241`）——**含观察代次**。所以"重新观察后再做同一动作"
必然得到不同指纹。用真实数据可验证：`paint-r3` 两次点击的动作载荷完全相同
（`{"arguments":{},"kind":"click","target":"uia-951f959f29c11d9f"}`），但指纹为
`c2ca9e5c00db712f` / `3ca13a695e4eac5c`——**按指纹相等判定重复，得到 0 次重复；**
忽略代次比较（动作类型 + 目标 + 载荷）才得到那 1 次真实重复。
`manifest.json` 同时给出两种视图（`consecutive_repeated_fingerprints` 与
`consecutive_repeated_operations_ignoring_generation`），以免这一差异被静默吞掉。

**(2) 既有身份绑定里已有"图像版本"的雏形。** `observation_generation` 参与指纹、
`before/after_evidence_ref` 里带 `screenshot:<path>:sha256=<digest>:<W>x<H>`——即
帧绑定已经有"观察代次 + 内容摘要 + 尺寸"，缺的是**窗口 rect / DPI / 裁剪与缩放**这一段
映射。这为 `FRAME-BINDING` 的"先审查既有绑定、能复用就复用"提供了具体落点。

## 7. 使用规则

1. 引用历史样本时，须同时写出其**类别**（`history_derived`）与**缺失项**；不得表述为
   "完整回放"或"含决策前观察"。
2. `synthetic` 样本（见 `manifest.json` 末条）**禁止**被当作历史执行证据引用。
3. 语义层指标在本语料库上写"不适用／证据不足"，**不写 0%**。
4. 截图仅含画图窗口（`window-*`），不含全屏桌面；导出不含消息正文与 API Key。

## 8. 可执行守卫与变异验证

本目录的口径不只写在文档里，还有 7 个可执行守卫把它们钉住：
`modules/gui-web/packages/web-console/src/computer_use_eval_corpus.rs`

| 守卫 | 钉住的口径 |
| --- | --- |
| `corpus_categories_are_the_four_ruled_kinds_and_synthetic_is_never_history` | 类别只能是裁决四类；合成样本必须显式声明不是历史回放，且不得引用历史运行时库作来源 |
| `history_samples_declare_missing_evidence_and_absence_of_labels` | 历史样本必须写明缺失项、必须如实写「无语义标签」「当时不存在反馈块」 |
| `corpus_does_not_claim_goal_achievement` | 失败语料库不得被写成达成目标；每个样本必须有终态错误码 |
| `history_samples_mark_semantic_metrics_unscorable_without_zero_filling` | 三项语义指标必须列为不可评分，且不得出现 `0%` / `rate` 式补零 |
| `migrated_images_are_self_verifying` | 截图文件名必须内嵌其 `sha256`；有图样本摘要必须全部校验通过；不得声称有窗口 rect/裁剪映射 |
| `migrated_sample_files_match_recorded_digests_byte_for_byte` | 迁移素材必须逐字节等于清单记录的摘要与体积（防行尾归一化静默改写） |
| `fingerprint_identity_caveat_is_recorded_with_a_real_counter_example` | 必须留下「指纹含观察代次」的口径与 `paint-r3` 这个真实反例 |

**字节保真为什么要单独设防**：本机 `core.autocrlf=true`，仓库根 `.gitattributes` 又有
`*.json text eol=lf` —— 检出/提交时的行尾归一化会改写字节，让清单里记录的 `sha256` 失效。
本目录用自己的 `.gitattributes`（`samples/** -text`、`tools/** -text`、`images/** binary`）
把字节钉住（更深层的属性按 git「就近优先」规则胜出），上面的逐字节守卫则是那个钉子的检查者。

**变异验证（证明守卫不是永远通过）**：对 `manifest.json` 施加 11 种"把话说满"的破坏，
每一种都被上表对应守卫捕获（11/11）：

| 变异 | 命中守卫 |
| --- | --- |
| 历史样本改成 `synthetic` 但不声明非回放 | 类别守卫 |
| 清空缺失项清单 | 缺失项守卫 |
| 声称有语义标签 | 缺失项守卫 |
| 声称截图摘要不匹配 | 图像守卫 |
| 声称达成目标 / 汇总写成已达成 | 目标守卫 |
| 声称当时存在反馈块 | 缺失项守卫 |
| 声称有窗口 rect | 图像守卫 |
| 抹掉语义指标不可评分声明 | 语义层守卫 |
| 删除合成样本的非回放声明 | 类别守卫 |
| 改掉某迁移素材的记录摘要 | 逐字节守卫 |

> 变异脚本：`tmp/mutate-corpus-guard.py`（临时工具，不随仓库分发）。它只改测试读取的清单并在
> `finally` 中还原，不改动 `samples/` 与 `images/` 内容。

## 9. 复现

```powershell
# 需要原始留存仍在 tmp/（易失）。重建 manifest.json 与 samples/images 清单：
python docs/testing/cu03-eval/corpus-2026-09-19-paint-window-drag/tools/build_corpus.py
```

校验截图完整性：`manifest.json` 的 `images.<文件名>.sha256` 应与文件名中的摘要一致，
且 `samples[].image_and_transform.all_digests_match` 为 `true`。
