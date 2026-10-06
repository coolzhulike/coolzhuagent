# 当前验收队列（2026-10-06，0.2.81正式iframe三项回归通过）

本表汇总剩余工作，历史失败与已通过证据以各版本报告为准。当前仅主会话实施与测试，真实模型使用原SWE-2-medium会话；Opus依用户要求暂不使用，微信不改不测。Paint基础输入已通过，不再要求完整人物绘画。

| 顺序 | 事项 | 当前状态 | 完成依据 |
| --- | --- | --- | --- |
| 1 | Browser普通文字/SVG子元素点击及混合指令路由 | 0.2.78正式安装版真实SWE通过；独立遮挡负例零输入 | 正式包逐文件核验后，同一SWE会话实操截图＋宿主释放回执＋网页事件 |
| 2 | 按下期间整页变化的释放边界 | 同步整页内容替换已在0.2.78正式版通过；跨URL导航严格时序仍开放 | 先区分内容替换与新文档导航，再证明变化发生在pointerdown和pointerup之间；禁止用点击后变化替代 |
| 3 | Browser其它复杂节点与宿主资源变化 | 正式普通独立子控件正/负例、Shadow/iframe父误击拒绝、初始面板关闭负例已通过；开放Shadow子点击与父误击拒绝已在0.2.79正式版通过；验证期间关闭能撤销旧证明已通过；同进程iframe子点击、父误击和规划期间子导航旧引用拒绝已在0.2.81正式版通过；跨进程Frame、子编辑/键盘/滚动及输入前/按下中关闭替换面板仍开放 | 实际页面或面板变更截图与对应零投递/释放结果，无失败补发；不把初始关闭或渲染当时序/内部自动化通过 |
| 4 | Compute Use多屏 | 基础Paint输入已有正式通过，多屏仍未补验 | 真实显示器布局、坐标与目标软件实操截图；环境不足时保留缺口，不虚构多屏 |
| 5 | 插件剩余生命周期 | 正式卸载、固定来源重装默认停用及重新启用已通过；配置变化、执行中取消/超时、许可冻结竞态仍开放 | 当前真实插件调用与取消/超时/授权事实逐项对应；旧失败请求不能复活 |
| 6 | 会话上下文与附件边界 | SWE原远端续接已有通过；Goal/Relay附件、账号过期及模型切换尚未补全 | 正常产品入口＋真实模型结果与台账；不注销用户账号制造负例，不调用受限Opus |
| 7 | 自动升级闭环 | 检查版本等已有改动；下载、安装、重启全过程仍未正式补验 | 官方发布资产匹配，正常Windows安装流程与重启后版本/原工程核验 |
| 8 | 启动演出其它模式 | 演出及Esc/跳过按钮已有正式实操；资源失败、减弱动作、首次/恢复边界仍开放 | 实际窗口截图、一次结束/交接事实；失败和兜底不当作完整舞剑通过 |
| 9 | 四项任务总体复核 | 未完成 | 当前主会话先按最终代码、安装包及真实证据复核；Opus暂停不影响可执行工作继续推进 |

PR83已合并，当前新增修复与证据由[PR84](https://github.com/coolzhulike/coolzhuagent/pull/84)承接。0.2.77正式插件生命周期见[报告](../../testing/release-0.2.77/change-report-and-test-handoff.md)，0.2.78阶段事实见[本版改动与测试交接](../../testing/release-0.2.78/change-report-and-test-handoff.md)。

[0.2.78预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.78)已公开；四个分发资产服务端长度/SHA和实际标签源码0b已核验，正常日常启动恢复通过。

复杂节点补验、8767事件环境误判排除及正式8765地址同步反证见[078边界补验](../../testing/release-0.2.78/browser-boundary-followup.md)。开放Shadow实际缺口、职责边界、正式真实SWE正/负例及关闭时序见[079报告](../../testing/release-0.2.79/change-report-and-test-handoff.md)。

0.2.79 PR远端检查的SQLite首次并发打开锁冲突已接手：只读版本读取有界重试及阶段诊断，本地1390完整检查和10轮既有并发用例通过；不重放业务事务，不跳过失败。修补不在079包内；080正常安装及1150文件摘要通过，真实SWE正/负例31.4/21.1秒通过，cc6源码两条远端检查success。后续文档HEAD检查另核。见[080交接](../../testing/release-0.2.80/change-report-and-test-handoff.md)。

0.2.80正式证据及正常桌面入口恢复见[080交接](../../testing/release-0.2.80/change-report-and-test-handoff.md)。同进程iframe已按[文档绑定方案](native-browser-iframe-next-plan.md)在0.2.81实现并正式验收；跨进程独立Target和子编辑等仍需实施及实操。

0.2.80已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.80)：实际tag绑定cc6源码，四个上传资产的长度/SHA均匹配，见[服务端核验](../../testing/release-0.2.80/evidence/github-release-verification.json)。080交付事实保留；当前正常桌面入口已恢复到081。PR84待合入审查；同进程iframe已正式通过，跨进程和四项任务整体仍未完成。

0.2.81已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.81)：正常安装1150文件匹配，原SWE三轮25.2/34.0/32.0秒通过，d9产品源码两条远端检查success；四资产及标签源码核验、原工程日常入口恢复通过。见[081报告与原图](../../testing/release-0.2.81/change-report-and-test-handoff.md)。下一步继续Browser未验收边界，不把本轮同进程click外推为全部Browser完成。
