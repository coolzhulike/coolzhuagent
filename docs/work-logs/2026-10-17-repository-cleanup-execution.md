# 仓库清理执行记录（2026-10-17）

依据 `docs/repository-cleanup-review-2026-10-17.md` 的审核清单执行。本记录只写实际发生的动作；
未执行项与边界在各条目内注明。清理前产品候选为 0.2.136；本轮 C1/C2 实际改动按清单要求另行形成
新源码与安装包版本（见文末“版本边界”），不回写 0.2.136 证据。

## A. 清理/移出工程

| 项 | 实际动作 | 结果 |
| --- | --- | --- |
| A1 | `boundary_pose_comparison.png`、`five_pose_comparison.png`、`opening_phase_f150_168.png`、`review_and_next_execution.md` 移回动作制作项目 | 已移至 `D:\gotdot\docs\qa\taiji-juqi-kaishi-v03-6e12-review\`，4 文件共 4,013,600 字节；移动后 SHA256 不变（boundary 8153047d…、five 707b0667…、opening c709090f…、md 583c29cb…），未销毁 |
| A2 | `deploy/web-console-078/` | 源码侧全量检索（scripts/config/installer/package/.github/src/tests 及根文件）无任何 `web-console-078` 引用；该目录本就不被 Git 跟踪（`git ls-files deploy` = 0）。整体移入版本归档 `D:\coolzhu-migration\preserved-build-caches\deploy-web-console-078\`，527 文件、264,248,615 字节（含被忽略二进制，比审核时登记的 517/104MB 更全）。回退用途由 `dist/` 内保留的历次正式 MSI（0.2.79～0.2.135）承担，本副本为展开目录，不产生新版本包义务。`deploy/` 根目录保留为空目录 |
| A3 | `.playwright-cli/` 48 个历史截图/快照 | 文件在评审前已从工作树缺失；本轮把 48 条删除登记进 Git 索引（`git add -A`，状态 `D `）。`.gitignore` 已含 `/.playwright-cli/`，新增抓取继续走 `tmp/`。未恢复、未计为回收空间 |
| A4 | `submitted/release-0.2.14-agent-fixes-report.md` | 已删（与 `docs/testing/release-0.2.14-agent-fixes-report.md` SHA256 `ac4c5545…` 一致）。`04-整合改进执行计划.md` 内 2 处指向旧 C 盘绝对路径的附件链接改为仓库相对路径（本项 → `../../testing/release-0.2.14-agent-fixes-report.md`；同行的 01 附件改为 `evidence/submitted/…` 相对路径）。`submitted-manifest.json` 该附件条目补 `canonical_path` 与去重说明，提交记录不变 |
| A5 | `dsh-offline-candidate/change-report-and-targeted-test-plan.md` | 已删（与主副本 `docs/testing/release-0.2.64/change-report-and-targeted-test-plan.md` SHA256 `ba713ac7…` 一致） |
| A6 | `dsh-offline-candidate/work-log.md` | 已删（与 `docs/work-logs/2026-10-02-local-session-dsh-msi-candidate.md` SHA256 `2c5756f0…` 一致） |
| A5/A6 清单映射 | `release-0.2.66/evidence/baseline.json`、`analysis/2026-10-04-five-thread-adoption.json` | 两个清单中原 path+sha256 保留，新增 `removed_2026_10_17` 与 `canonical_path` 字段；`dsh-offline-candidate/evidence-index.json` 两条目同样补删除标记（`source` 指向权威副本，映射不断链） |
| A7 | 根目录 `SOURCE-MANIFEST.json` | `git mv` 至 `docs/releases/SOURCE-MANIFEST-0.2.0-export-20260729.json`（该文件为 0.2.0 时代导出清单，2026-07-29 提交）。`project-delivery.ps1` 只向交付目录生成新清单，不消费根副本；下一次交付生成版本专属清单 |

## B. 旧需求文档归档（保留正文，标记历史）

- B1 `docs/packaging-and-device-migration.md`：文首标记为历史提案，注明现行 WiX/MSI 链入口；在线激活等未实施内容不计完成。
- B2 `docs/plans/2026-07-22-goal-loop-issue-backlog.md`：标记历史，移出活动待办入口；Issue/PR 追溯表保留。
- B3 `docs/plans/2026-09-14-chat-tool-observability-backlog.md`：标记归档并在横幅内逐条映射当前去向（OBS-001 事件关联回放/截断终态提示归 1.5/4.2 口径继续核；OBS-002 折叠交互归 4.1/4.2；OBS-003 归 2.2，provider 缺失字段显示未知）。不按整份文档一刀切关闭。
- B4 `analysis/2026-09-21-integration-review/current-acceptance-queue.md`：标记只读历史，活动入口指向两份现行清单。
- B5 `analysis/2026-09-21-integration-review/devin-single-session-next-plan.md`：标记历史，不据此重建会话。
- B6 `AGENT.md`/`CLAUDE.md` 头部注明共同约定以 `AGENTS.md` 为准；`AGENTS.md`、`CLAUDE.md` 同步修正过期事实（`main.rs` 体积 1.36MB→≈3.6MB；Goal 现状“缺失败回退”改为已实现的回退链描述）。未删除任何工具约定文件。

## C. 被替代代码/资源

- C1：`ui/assets/p83-coolzhu/actors/` 下 35 张字母分帧 + `swordsman-four-poses-v2.png` 共 36 张（10,333,891 字节）经 `git mv` 移入 `docs/design-assets/p83-coolzhu-legacy-actors/`（附 README 说明出处与复用条件）。移动后 actors 目录仅剩 `meditating-actor-v1.png`。`coolzhu-seven-letter-manifest.js` 只引用静坐图/glyphs/背景，不受影响；`gui-desktop.tauri-ui` 按目录整体入包，移动本身即收窄了打包载荷范围。命名引用全仓检索（git grep，排除归档目录）仅有设计文档内一处文字提及，已加注归档位置；`swordsman-four-poses-v2.png` 与 `docs/design/2026-09-27-ui-concepts/assets/` 内副本字节一致，归档保留以使旧 actors 集合完整。
- C2：`modules/gui-web/packages/web-console/src/main.rs` 删除 `locate_request_from_tool_payload`、`try_parse_system_control` 两个已无生产调用入口的旧辅助函数及其专属测试 `locate_request_from_tool_payload_prefers_system_control_target`；同步移除 `ToolDispatchRequest.system_control` 字段与全部 `system_control: None` 构造点（该字段仅由被删辅助读取）。保留 `detect_system_control_from_target`（21339 行生产调用链仍在用）并移除其过期的 `#[allow(dead_code)]` 标记。`api_tool_execute` 的受控 write_file 分支、`api_vision_locate_verify` 的 410 边界均未动。
- C3 `language-service/examples/navigation_live.rs`、C4 `attachment_staging_live.rs`/`desktop_uia_live.rs`：保留为实操诊断/验收工具，未删未动。

## 验证

- `cargo build -p coolzhu-web-console --offline`：exit 0（15m08s 首次，重跑 0.54s）；150 个 warning 全部为既有项（含微信 `WechatCapability`/`WechatCatalogKind` 等），无 `system_control`/`locate_request`/`try_parse`/`never read` 相关新 warning。
- 定向测试 `cargo test -p coolzhu-web-console --offline -- dispatch semantic visual_action locate model_tool_use goal_phase_token_finalizer`：107 通过 / 0 失败 / 2 忽略；`goal_phase_token_finalizer_repair_returns_to_verifier_without_resetting_budget` 单独复跑 1 通过 / 0 失败 / 0 忽略。

## 版本边界

本轮 C1/C2 改变了打包载荷与可执行代码，按审核清单要求形成新源码版本 **0.2.137**：
`scripts/package-all.ps1 -Configuration release` + `scripts/build-msi.ps1 -Version 0.2.137`，
只复验受影响功能（启动动画静态资源清单、web-console 编译与受影响测试）。0.2.136 MSI 保持原核验字节不动。
旧移交会话/账号数据、部署副本与动作制作材料不上传；`.gitignore` 的运行态/抓取规则已在此前生效，本轮不新增。

### 0.2.137 出包结果（已完成）

- 暂存载荷：1,453 文件 / 419,531,152 字节，摘要 `0ba7eef09c5a9a8d009ab97f8761fe21f957751eb22e9c564b3fef344196f9d9`（package-all 暂存面）；MSI 输入载荷 1,186 文件 / 417,994,142 字节，摘要 `7f4211da1c01233fc8932436fcae0458be5f9c2d0d269af57daed97034be950b`。
- 源码快照摘要 `2d57ed6981101a18a4df24661599c83fd0498b126d837dd439bea9e74854cbf8`，构建前后核验一致（`live_worktree_changed=False`）。
- 许可覆盖：433 个 Cargo 观察组件 + 20 个非 Cargo 组件，`release_eligible=True`。
- 包内 `assets/p83-coolzhu/actors/` 仅剩 `meditating-actor-v1.png`（1,102,287 字节）——C1 收窄落到载荷。
- MSI：`dist/CoolzhuAgent-0.2.137.msi`，268,150,874 字节，未签名；SHA256 `a621cefc00b1e7c6c040e8ebe1b9e8f2cc26231e329fa755118f5a132f0f8ed`。
- 包报告 `pkg-report-release-20261017-064914699-0047d503`（content_sha256 `7d025546813b2daadc45dede6ebcc6bd1241b5dec5aaef0e2d91cde9bd6dce59`），构建身份证据归档于 `docs/testing/release-0.2.137/evidence/build-identity/`。
- 发布说明：`docs/releases/0.2.137.md`。
- **未做**：本版安装复验（机器级升级、后台切换、历史库保留）未执行，MSI 生成成功不代表安装可用；0.2.136 已覆盖的全量验收矩阵未重跑。

## GitHub 公开提交（2026-10-17 完成）

沿用既有公开提交流程：`scripts/project-delivery.ps1` 生成 GitHub-ready 树（1,430 个源文件，
隐私扫描 1,432 文件 0 发现），以公开基线 `25211e10` 为父叠加整理，同时把清理删除
（36 张 actors 旧图、3 个重复文档副本）与已公开清单/引用文档的更新一并同步；
36 张图被 Git 识别为 actors→design-assets 的 100% 重命名。
提交 `bff3ff05`，已推送至 `codex/release-0.2.137-cleanup-execution-20261017`（远端 ls-remote 核验一致）。
工作日志、验收证据与本机路径清单按约定不上传；`docs/design-assets/` 已在公开树中，
归档目录与 README 随移动一并公开。
其后应用户决定移除 AGENTS.md/CLAUDE.md（AGENTS.md 约定并入 AGENT.md），
公开分支追加提交 `66cfeb65`；PR #90（base main）已建立：https://github.com/coolzhulike/coolzhuagent/pull/90
注意：默认关闭的 `coolzhu-agents-md-updater` 插件若启用会重新生成 AGENTS.md。
