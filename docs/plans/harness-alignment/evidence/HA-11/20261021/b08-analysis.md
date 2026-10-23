# B08 真实受影响链路：验证合同与回执语义（b08b 配对）

证据目录：`runs/b08b-baseline-2/`、`runs/b08b-candidate-2/`（`b08b-flow.json` + `mock-requests.jsonl` + `tool-calls`/`room` 原始回执）。
驱动：`tmp/ha11-run/run-b08b.ps1`（真实 Goal API + mock provider，b08 模式：repair=write_file 提案）。
门命令带计数副作用：`Add-Content <gate>-count.txt x; if(!(Test-Path <marker>)){exit 1}`——每行 = 一次真实执行。

## 腿 A：同输入复用

| 步 | baseline | candidate |
|----|----------|-----------|
| run1 门失败 | gate=1，verdict=fail→implementer | gate=1，回执 `-1` failed exit=1 |
| run2 修复+复验 | gate=2，route→verifier | gate=2，repair 回执 `-2` passed digest `33b2a151fd57` |
| run3 同输入再验 | gate=3（旧版无回执机制，诚实重跑）→pass/completed | **gate 仍 2**，verdict=pass/completed，回执复用 `-2` |

候选侧同输入未产生第 3 次执行——回执 `-2` 直接满足合同；基线旧码无回执存储，如实重跑且如实通过。

## 腿 B：相关修改使回执失效

| 步 | baseline | candidate |
|----|----------|-----------|
| run1/run2 | gate=1→2，修复通过 | `-3` failed → repair `-4` passed digest `33b2a151fd57` |
| 追加 `// b08 related modification` 到 `src/lib.rs` | — | digest 变为 `36f8957ff24d` |
| run3 | gate=3 → pass/completed | gate=3，**新回执 `-5`**（digest 36f8）→ pass/completed |

候选侧：digest 变化 → 旧回执 `-4` 被拒绝复用 → 真实重执行（计数 +1、新 receipt id、新 digest）。基线无 digest 概念，行为一致地重跑。

## 腿 C/D：畸形合同（不得虚假通过）

- C 空白命令 `commands=["   "]`、D 缺 `commands` 字段：两侧 `/api/goals/{id}/plan` 均返回 `{"error":"Command.commands must contain at least one command"}`，goal 停留 planning，三次 run-next 均 ran_phase=null，无执行、无验收，cancel 干净。
- unauthorized_effects=0，false_acceptance=0。

## 腿 E：缺省 cwd（持久化为 `""`）

- baseline：门命令以 `""` cwd 在工作区根执行，3 次执行后 pass/completed。
- candidate：缺陷修复后（空/缺 cwd=工作区根），`-6` failed → repair `-7` passed（digest 36f8）→ 第 3 次验证复用 `-7` 完成，gate 共 2 次执行。

## 统计与判定

- candidate：A=2 执行/3 验证（复用 1），B=3 执行（失效应重跑），E=2 执行/3 验证（复用 1）；receipt id 递增 `-1..-7`，digest 与文件内容绑定。
- baseline：每腿 3 次执行，无复用机制亦无重复效果；终态全部如实。
- 全部效果限于隔离 ws（b08-* 文件）；无越权写、无重复效果、无错误复活、无虚假验收。
- 缺陷记录：候选侧曾发现 `cwd:""` 归一化结果被判 Invalid 的真实缺陷，修复后本系列（b08b-*-2）验证通过；此前失败归档在更早尝试与 `runs/b08-candidate-1/`。
