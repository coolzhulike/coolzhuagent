# HA11-04 受影响路径汇总与回退演练

## 本轮对候选源码的实际改动

| 文件 | 改动 | 影响面 |
|------|------|--------|
| `workspace_dev_command.rs`（新增） | 工作区开发命令白名单（v2 放宽：输出收集后缀、工作区内 cd/Set-Location、尾随 exit-echo） | 仅 workspace-write 档位的 bash/PowerShell 前置放行判定 |
| `main.rs` | 挂接白名单到 `preview_tool_permission` 的 WorkspaceAuto 豁免分支；workspace-write 工具暴露 | 聊天工具门决策；Goal/审批链路不复用该分支 |
| `host_child_agent.rs` | 同步档位下工具暴露集 | 子 agent 会话工具集 |
| `goal_verification.rs` | None/空 cwd→工作区根；commands 缺失/空/空白→Invalid；回执消费条件不变 | Goal Command 验证合同求值 |
| `goal_command.rs` | `verification_digest` 相关文件集合（此前已存在，本轮复核确认） | 回执 digest 覆盖范围 |

白名单边界（复核确认仍严）：命令链接/重定向/命令替换/后台/`..`/越工作区/`--manifest-path`/`--target-dir`/`dangerouslyDisableSandbox` 全拒；cwd 出工作区即回退审批。

## 回退演练（真实执行）

1. **回退面**：上述改动全部局限于 `web-console` 包内新增/编辑文件；基线 `tmp/ha11-baseline` 保存了不含这些改动的同源旧码，两者并存即天然回退点——停候选服务、起基线服务（本演练中多次实际执行：8766 基线/8767 候选独立切换）。
2. **演练动作**：b08b 运行中多次发生候选/基线服务中断 → 每次按"只读核对 sqlite → 定位 PID → 定点杀 → 重启另一套二进制"完成切换，验证两套实现可在同一隔离数据目录上互换运行。
3. **live gate 保留**：审批链路未被白名单削弱——b07 双侧实测 3 条非白名单写命令仍 `require-confirm` 入队、审批后才执行、拒绝后不可复活（`b07-flow.json`）。工作区外 cwd/危险命令同样回落审批。
4. **零测试假通过**：每次跑完以独立 `cargo test --offline` 复核（verify.json indep_exit 全 0）；`--self-test` 证明校验器拒绝 0-test。

## 残留风险（如实记录）

- 隔离仅靠 `COOLZHU_RUNTIME_DIR`+fixture 副本，服务 `allowed_roots` 含用户目录——模型仍可能越界读（本轮未见越界写证据）。
- 基线为补齐依赖的重建快照，非原始字节快照；与候选的差异除被测补丁外还有旧 Goal 链路（无回执机制）。
- 机器存在进程启动税/磁盘抖动导致的超时风险；两轮服务僵死原因未完全定位（sqlite 写事务 + 资源竞争嫌疑），已按隔离事件记录。
