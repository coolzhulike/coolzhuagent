# 项目目录清理审核清单

审核基准：2026-10-17 当前工作树，产品候选 0.2.136。本文是待审核建议，**未执行删除或移动**。原本已缺失的 `.playwright-cli` 文件另列，不算本轮清理成果。这里只将重复、被替代或不属于当前工程的内容列为候选，不因日期旧或没有文本引用就删除。

## 盘点范围与结论

盘点了 Git 已跟踪文件和未被忽略的新增文件：10,157 个跟踪路径、848 个新增路径。排除 `target/`、`tmp/`、安装包、用户数据库等被忽略的内容，因此下列大小是这个范围的逻辑字节，不是整个磁盘占用或可立即回收空间。

- `docs/testing/`：8,492 个现存文件，594,414,575 字节，主要是历次版本的原始证据；不能整目录删除。
- `docs/work-logs/`：193 个文件，797,610 字节；空间收益很小，主要问题是历史状态容易被当成当前计划。
- 已确认 3 组正文完全相同的 Markdown，可去重 24,559 字节，但需要先调整引用/证据索引。
- 旧启动动画资源 36 张、11,324,119 字节已不在当前启动素材清单中，但历史契约明确要求保留美术源文件，建议先移入设计归档。
- 本轮没有确认任何整个 Rust crate 可以安全删除。

## A. 建议清理或移出工程的文件

| 编号 | 精确路径/范围 | 建议 | 依据及处理前提 |
| --- | --- | --- | --- |
| A1 | `boundary_pose_comparison.png`、`five_pose_comparison.png`、`opening_phase_f150_168.png`、`review_and_next_execution.md` | 移回所属动作制作项目，不直接销毁 | 四个未跟踪文件共 4,013,600 字节；文档讨论角色骨架、动作帧和游戏导出，三个图片由该文档引用，不是本应用实现或验收材料。 |
| A2 | `deploy/web-console-078/` | 核实回退用途后移入版本归档，之后删除重复展开目录 | 非忽略内容已有 517 文件、104,163,047 字节；还可能含被忽略二进制，未计入本数。属于旧部署副本，非 Cargo 源码入口；本次读取的运行程序位于正式安装目录，不使用该路径。需先确认旧回退副本和启动引用，不能直接删除整个 `deploy/`。 |
| A3 | `.playwright-cli/element-2026-09-27*.png`、`.playwright-cli/page-2026-09-27*.png`、`.playwright-cli/page-2026-09-27*.yml` | 审核后登记历史删除；新增抓取结果统一进临时目录 | 当前索引显示 48 个文件已经缺失，均为历史截图/快照。本轮没有删除它们，也不会恢复或把它们计为新回收空间。 |
| A4 | `docs/analysis/2026-09-21-integration-review/evidence/submitted/release-0.2.14-agent-fixes-report.md` | 删除副本、改指向 `docs/testing/release-0.2.14-agent-fixes-report.md` | 两份 14,841 字节，SHA256 完全相同；提交材料的历史清单应留映射，避免证据链断裂。 |
| A5 | `docs/testing/release-0.2.64/evidence/dsh-offline-candidate/change-report-and-targeted-test-plan.md` | 删除副本、保留 `docs/testing/release-0.2.64/change-report-and-targeted-test-plan.md` | 两份 8,001 字节完全相同；先改证据路径引用。 |
| A6 | `docs/testing/release-0.2.64/evidence/dsh-offline-candidate/work-log.md` | 删除副本、保留 `docs/work-logs/2026-10-02-local-session-dsh-msi-candidate.md` | 两份 1,717 字节完全相同；先维护原清单映射。 |
| A7 | 根目录 `SOURCE-MANIFEST.json` | 归档为旧导出清单；下一次交付生成新的版本专属清单 | 它是历史导出的产物，当前目录已大量变化。`scripts/project-delivery.ps1` 的清单生成目标是交付目录，而不是把此旧文件当当前源码权威。不能用旧清单验收本次版本。 |

## B. 建议归档/合并的旧需求文档

| 编号 | 路径 | 为什么过时 | 应保留的内容及替代入口 |
| --- | --- | --- | --- |
| B1 | `docs/packaging-and-device-migration.md` | 仍写安装器实现冻结、NSIS 为首选；当前实际使用 WiX/MSI | 保留迁移与授权方案背景，标记为历史提案；现行打包入口是 `scripts/package-all.ps1`、`scripts/build-msi.ps1`、`installer/Product.wxs` 和版本发布说明。在线激活等未实施内容不能因 MSI 可用而直接宣称完成或删除。 |
| B2 | `docs/plans/2026-07-22-goal-loop-issue-backlog.md` | 文内已明确 GL-01～15 实现并提 PR，迁移号和源码行号漂移 | 保留 Issue/PR 追溯表，移出活动待办入口；当前状态以统一关键验收清单为准。相关 Goal 实现和失败回退测试保留。 |
| B3 | `docs/plans/2026-09-14-chat-tool-observability-backlog.md` | 仍以动画之后排期、尚未实施口吻描述；回复定位/运行记录已有新实现和验收 | 逐条映射后合并进当前清单；搜索超时、用量等不能按整份文档一刀切关闭。 |
| B4 | `docs/analysis/2026-09-21-integration-review/current-acceptance-queue.md` | 累积旧版本增量、旧执行流程和“当前”指令，不适合继续作为当前调度计划 | 改为只读历史，活动入口统一为 `docs/testing/2026-10-16-active-acceptance-plan.md` 及 `docs/testing/2026-10-13-goal-32wbs-acceptance-checklist.md`；原失败和证据链接保留。 |
| B5 | `docs/analysis/2026-09-21-integration-review/devin-single-session-next-plan.md` | 记录特定历史绑定的恢复和执行方案，不是现在的执行安排 | 归档历史方案，不据此重新创建/恢复任何会话。保留单会话桥接架构说明。 |
| B6 | `AGENT.md`、`AGENTS.md`、`CLAUDE.md` | 多入口维护容易漂移；`AGENTS.md` 中部分文件规模、会话位置、Goal 现状及工具协议说明已旧 | 建议集中维护共同工程约束、其余入口链接过去；本轮不删除工具约定文件，也不以旧说明覆盖当前运行事实。 |

`docs/testing/release-*` 共 5,665 文件、426,650,874 字节，可另做离线证据归档，但不是可直接删除的清单。保留最终版本报告、原失败、签名/摘要、许可证与源码身份；大截图、完整构建日志是否可移出 Git 应按索引核对。单纯删除工作树文件不会自动缩小 Git 历史，不在本次做历史重写或强制推送。

## C. 已被替代的代码/资源候选

| 编号 | 路径/符号 | 核对结果 | 建议 |
| --- | --- | --- | --- |
| C1 | `modules/gui-desktop/packages/tauri-shell/ui/assets/p83-coolzhu/actors/` 下 `[C,H,L,O1,O2,U,Z]-k[0-4].png` 及 `swordsman-four-poses-v2.png` | 35 张旧分帧加 1 张舞剑图；当前 `coolzhu-seven-letter-manifest.js` 只声明 `meditating-actor-v1.png`，生产页仍走当前播放器和桥接 | 审核后移入设计归档，再收窄打包资源范围；保留静坐图、字形、背景、播放器与桥。历史契约要求保留美术源文件，因此不能直接全删 actors。 |
| C2 | `modules/gui-web/packages/web-console/src/main.rs`：`locate_request_from_tool_payload`、`try_parse_system_control` | 本次源码引用检查只剩旧辅助链与其单元测试，没有生产调用入口；前者定义约 24948 行，测试约 60652 行 | 可单独删除旧辅助链及其专属测试，或迁到测试模块；改动后定向编译验证。**不能连带删除 `detect_system_control_from_target`**，它仍在约 21819 行被正式定位流程调用。 |
| C3 | `modules/core-runtime/packages/language-service/examples/navigation_live.rs` | 一次真实 LSP 故障诊断入口，直接包含生产客户端并依赖仓库内固定文件；不是运行时发布入口 | 可改为明确的诊断示例或移到验收工具目录；有真实复现价值，不作为无用源码直接删。 |
| C4 | `modules/gui-web/packages/web-console/examples/attachment_staging_live.rs` | 通过引用生产存储模块运行隔离测试，`main` 虽为空但测试模块有用途 | 保留测试用途；不可仅因空 main 删除。`desktop_uia_live.rs` 同样是实操验收工具。 |

## D. 明确保留

- `api_vision_locate_verify` 的 410 拒绝处理是旧接口的安全边界；`api_tool_execute` 还保留受控文件写入分支，不是整段死代码。
- Provider 的 deprecated 类型、旧模型别名与兼容解析仍被导出/调用，服务于历史配置，不能按标签直接删除。
- SQLite 迁移、已退役绑定记录、取消与重试、权限拒绝、通知去重等历史兼容逻辑及对应反例测试。
- 当前安装包、许可证、构建身份、验收摘要及仍阻塞事项的原始记录；运行数据库、会话、账号配置、私钥和本机状态不属于垃圾文件。
- `vermilion-seal-blank-v1.png` 仍被聊天室状态界面引用；它与用户要求去掉的开机朱印不是同一用途。

## 本次交付边界

本轮只提交公开源码、必要回归测试和去除本机身份的发布说明；本地验收现场、账号/会话数据、部署副本及动作制作材料不上传。新增忽略规则防止运行态和浏览器抓取文件再次混入提交，不改变这些文件本身。

0.2.136 MSI 保持此前已核验的字节，不因整理文档重新构建；清理候选全部保留待审核。后续若批准 C1/C2 等实际删除，应形成新的源码和安装包版本，并只验证受影响功能，不能继续沿用本包摘要。
