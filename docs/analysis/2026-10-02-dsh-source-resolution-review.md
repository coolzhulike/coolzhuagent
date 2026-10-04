# 本地接续停点与DSH固定来源解析审查

2026-10-02。本轮独立接手，未向原会话发送指令。保留用户要求的gpt-6.1-sol / High / 标准速度偏好，未启用快速模式或额外代理；本轮无切换或独立核验主模型选择的接口，不宣称已调整模型。

## 接手证据

- 分支 `codex/cu-preinput-followup-20260930`，基线HEAD `8181f08ae9c50d3e41aabd32a6af85343dcd92e8`。本轮修改未提交，不借旧提交CI认定新修改通过。
- 已读取仓库AGENTS、受支持的Codex memory_summary与指定三份报告。仓库 `.agents`、`.codex` 及其skills目录不存在，未发现额外适用的子目录AGENTS。
- 原已跟踪修改只有 `docs/testing/pr69-browser-market/native-followup/release-0.2.29-handoff.md`，原SHA256 `c61b4f5cf565c5da15b70b40a6c6bfde48507e7cb2cc4e2fd03c2e348f034079`。该修改与所有历史未跟踪图片/文档均保留；原状态及源码备份在 `tmp/2026-10-02-dsh-source/`。
- 正式Web PID2572、Shell PID3864，程序路径 `C:/Program Files/CoolzhuAgent/bin`。只读API返回构建 `09dc04d0f812 · 2026-10-01`、端口8765、工程 `C:/Users/zhupu/coolzhuagent`；两个安装文件摘要与063回执一致，仍为0.2.63产物，不含本轮DSH代码。未重启、替换或升级正式应用。
- 正式只读安全API返回 `windows-session-1 / isolated / accepts_new_input=false`、1条未放行阻断、5条历史人工复核操作、0条待恢复操作。两个计数含义不同，未删改事故、库或决定，未代签复核。
- 已完整读取computer-use技能并核对本轮工具目录，没有可调用的 `node_repl` / Sky。后台存在helper进程不等于本会话有入口，未自行构造客户端协议；原生GUI验收不能执行。

现有“四项任务”台账对应总体计划、模型发现/模态、动画/UI、侧栏/扩展；交接明确Browser、Paint、DSH与泛光边界。未找到“五项问题”的原始编号清单，已异步请求补充，不推测第五项。微信不改不测、Devin搁置、Pro补审暂停；未查询或改变PR74状态。

## 实施内容

市场原有检查只找原生plugin.json，不能用来判断Node/Cordis兼容。新增独立 `dsh_source_resolve.rs`：核对目录作者/仓库与公开GitHub仓库身份，一次解析HEAD或接受明确40位commit，从固定提交根package.json取得声明包名，再交既有下载器完整核验Git树/blob与静态回执。

官方计算器目录实际上 `npm:null`，因此从固定提交根清单读取包名，不猜同名registry发行包；有目录npm声明时要求逐字一致。安装spec、其它作者、私有/替换仓库、分支路径或浮动参数拒绝。后续所有下载只使用本次已解析的固定commit。

新增 `POST /api/extension-market/dsh/source`，只接 `expected_workspace / id / optional commit`。目录、解析、下载共享120秒总预算，同进程最多两个检查；接纳、读取目录后和返回前复核工程，过期目录/弃用条目/迟到工程结果拒绝。SDK锁来自编译期宿主锁，页面不能传本机路径、SDK或shell命令。确认暂存discard后才返回source_verified，同时明确 `compatibility=unverified / installable=false`。

前端增加“检查DSH固定来源”入口，展示实际包名/版本/许可证/commit/文件数。关闭详情或工程变化中止前端请求，迟到结果不发布；后端仍依靠总预算和TempDir析构收尾，不把前端abort冒充已收到服务端取消回执。没有安装、启用、执行npm/脚本/插件或发送模型请求。

main.rs仅增加模块声明，既有下载器仅将三个共享身份/有界读取/预算函数改为crate内可见。未改变输入隔离、权限、释放时限、模型配置、插件启用设置或根预算。

## 真实工程结果及失败

生产解析/下载探针使用已审查官方仓库omdsh-dev/dsh-tool-calculator，commit `b2007a13f06bcf75bf07b9d277ee8d434a316490`，根tree `75f02eb602088262dda014b8cdbcfea70aededfa`，得到 `@deepseek-ai/dsh-tool-calculator@0.0.1 / MIT / lib/index.js`，完整22文件再核验通过；本次默认HEAD也解析到同一commit。

npm冲突返回source_invalid；预取消返回download_cancelled；显式main参数返回source_invalid。成功暂存显式discard后目录不存在，独立下载根剩余0项。未创建目录的否定路径保留cleanup_confirmed=false，不伪造确认。本次耗时14525毫秒，不安装/执行计算器，不调用Qwen。

首轮沙箱网络连接失败download_failed，无产物/剩余暂存，原错误保留；经允许的只读网络执行后实际通过，未添加凭据或改代码避开失败。来源身份核验不证明代码可信、服务兼容或运行资格。

| 验证 | 实际结果 |
| --- | --- |
| 最终Web离线build | 通过，28.49秒；首次build通过，1分14秒 |
| 首次Web回归 | 1289通过/11失败/2忽略；默认会话库打开失败、附件500及工具暴露失败均保留 |
| 第一次隔离根回归 | 1299通过/1失败/2忽略；既有默认目录用例要求末段coolzhuagent，临时根末段实际为test-runtime |
| 最终隔离根完整Web回归 | 主二进制1300通过/0失败/2既有忽略，39.47秒；另两个目标8和1通过 |
| tool-registry离线check | 通过 |
| module_linkage_smoke | 8通过/0失败，2.44秒 |
| JS语法 / diff空白 | node --check和git diff --check通过 |

最终子进程 `COOLZHU_RUNTIME_DIR` 为 `tmp/2026-10-02-dsh-source/test-final/coolzhuagent`，避开真实用户库。没有为通过而放宽测试断言/生产规则，也没有提升测试权限去写真实库。临时环境只在该子进程中生效，正式配置与安全库不变。新增三个关键来源/归属检查，没有新增模型夹具；既有受控协议测试不等于真实Qwen验收。

原始回执、所有成功/失败日志和工作树源码摘要见 [证据目录](../testing/dsh-source-resolution-2026-10-02/)。临时数据仍在tmp；证据副本不含用户配置、模型密钥或会话库。本轮工作树未查询新远端CI。

## 未完成与下一门槛

1. DSH：来源检查代码已接线并工程通过，正式安装版按钮未实拍。默认停用安装按钮与工程/操作绑定、特定提交后故障/重启恢复、真实describe持久快照、FrozenParentContext与provider原call_id/共享预算、停用/卸载、真实Qwen及固定资源MSI安装继续开放。
2. Browser：063正常点击/历史导航证据保留；8秒页面事件超过3秒释放确认得到release_unknown的失败不追认。按下至释放期间关闭、真正慢HTTP加载、旧世代事件边界仍未实操，本轮零新增输入。
3. Paint：063新增W只证明部件与输入链路，完整海绵宝宝未验收。当前资源实际隔离；需用户在可信桌面核查输入确已释放后走产品原有人工复核入口。复核后仍需可调用的原生computer-use工具。
4. 动画/泛光：历史日常结果保留；首次/减少动态/资源失败/逐姿态、混合DPI/多屏/输入期间取消未补验。
5. 总体：五项原始清单待补，现有四项台账未总体完成；未合并、发布或安装，不以工程数字替代GUI成功。

下一工程门槛是固定来源接入已有默认停用安装事务，再完成真实describe快照与冻结模型资格。下一产品门槛是包含这些代码和固定资源的候选MSI通过发布身份检查，用原Qwen与真实按钮全流程实拍。Browser/Paint需要人工输入复核与原生工具，独立DSH工程不需要解除隔离。
