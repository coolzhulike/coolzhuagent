# DSH默认停用安装与真实宿主快照工程审查

2026-10-02，承接[来源解析阶段](2026-10-02-dsh-source-resolution-review.md)。分支仍为`codex/cu-preinput-followup-20260930`，基线HEAD仍为`8181f08ae9c50d3e41aabd32a6af85343dcd92e8`，修改未提交。正式0.2.63仍是`09dc04d0f812 · 2026-10-01`，两个安装程序摘要未变化。GitHub上传已搁置，未认证、上传、发布或合并。

## 已落地

来源检查新增source_sha256，包含固定仓库、commit、逐文件源码回执和编译期SDK锁；文件顺序不影响摘要。市场详情在检查成功后允许“安装为停用状态”。安装请求只能提交工程、目录条目、明确commit和刚确认的摘要，不能传本机路径、SDK、命令或宿主清单。

安装API重新读取有效目录并全树核验同一提交，确认不一致先discard、拒绝提交。复用PluginManager原写锁、暂存交换与恢复journal，默认停用，回执使用实际operation_id。提交后暂存清理失败保留“已提交”事实和清理诊断，不报回滚或自动重装。HTTP断开后阻塞提交可能继续，并发占位保持到实际收尾；页面不自动重发，须刷新持久目录核对。

接纳、取得目录后、提交前复核工程，manager始终绑定冻结的原工程，不写新工程。提交中的切换/关闭不宣称取消。下载/核验有120秒预算和一席并发，进入既有静态提交后不丢弃阻塞任务假装回滚。

登记新增向后兼容的installation_id，取原journal操作ID，同版本重装也更换世代。旧DSH记录缺世代时拒绝快照、需重新核验安装；原生插件兼容不变。

新增真实宿主适配器：正式只认可执行文件旁固定资源、缺失拒绝；冻结安装ticket，核验资源/SDK，调用生产Node describe，结束后再次核验，再持原插件锁比较完整ticket。非法配置在启动前拒绝。快照保留完整实际工具schema、插件版本、来源修订及配置；页面无上传清单/快照接口。

enabledPlugins、dshLifecycleEpochs、dshSnapshots共用settings.json，同卷临时文件sync后替换，不新增平行开关。停用、重装、卸载撤快照并改变世代；读取复核实际安装/源码/工程/世代。普通enable不能绕过describe；新增适配器尚未挂到正式管理动作，快照不等于模型执行授权。

## 真实工程结果与失败

独立空临时目录使用已审查官方`omdsh-dev/dsh-tool-calculator`，commit`b2007a13f06bcf75bf07b9d277ee8d434a316490`、22个真实文件、包`@deepseek-ai/dsh-tool-calculator@0.0.1`。Node24.15.0，完整290个资源文件/93908058字节；资源锁`a64c15bd0ce8c247ddab0c361658ae78be11eb31f4ce5ba3e0602513c27238f7`，SDK锁`af9313e4938c2c94596b702e13f48dfe430b47ec52eab98855d6707a5bdb1307`。

最终探针35.166秒：实际工具calculator、表达式12 * (3 + 5)，精确返回isError=false,value=96；实际describe快照持久化，新manager重读逐值一致。源码摘要`d8fa5ad326a19949045904e4fea63c7318254745bf809f8877c811c7cb083b56`。

实际验证确认冲突拒绝并清理且原登记/设置字节不变、普通enable拒绝、非法配置启动前拒绝、跨工程拒绝、非法schema/锁不写设置、停用后的迟到启用拒绝、同版本重装更换安装世代并拒绝旧ticket、真实源码入口字节篡改在describe前拒绝且设置不变、恢复原字节重新核验、卸载后目录/快照/启用位移除。没有SDK替身/模型回包；明确engineering上下文只证明工程归属，不算真实父聊天轮身份。

| 验证 | 最终实际结果 |
| --- | --- |
| Core/Plugin/Web三crate离线build | 通过，39.14秒；仍有编译警告，包括未接线适配器 |
| 官方安装/快照/生产Node探针 | 通过，35.166秒，实际96及上述失败路径 |
| 聚合插件回归 | 37通过/0失败，0.89秒，含原提交故障/交换后恢复/锁竞争 |
| 聚合Web主二进制 | 1301通过/0失败/2既有忽略，40.20秒；另目标8和1通过 |
| module_linkage_smoke | 8通过/0失败，1.43秒 |
| tool-registry离线check | 通过，10.31秒 |
| 资源副本只读verify | 完整290文件/93908058字节摘要通过 |
| JS语法/diff空白 | node --check / git diff --check通过 |
| Core dsh_筛选尝试 | 0匹配、356过滤，不能计作定向测试通过；Core以build和真实生产宿主验证 |

回归子进程使用tmp/2026-10-02-dsh-activation/test-final/coolzhuagent，不写真实会话库。首轮资源参数误传父目录、旧目录权限失败、沙箱网络失败保留；旧资源获准只读核验后复制到专用目录，新副本可由默认沙箱读取。运行探针获准访问官方网络。第一次真实describe成功后因探针误用calculate返回tool_unavailable，改用实际calculator并精确断言96后通过，原失败目录/日志保留。

回执、全部成功/失败日志、当前源文件摘要与正式只读状态见[阶段证据](../testing/dsh-install-activation-2026-10-02/)。第一阶段证据对应其自身时点，不冒充当前验证。已跟踪原文档修改和165条历史未跟踪文件保留。

## Paint的一项待复核：只读说明

2026-10-02T05:42:50Z正式只读API仍是windows-session-1 / isolated / accepts_new_input=false，unacknowledged_open_blocks=1，pending=0，human_review_required=5；5是历史复核操作计数，不是五个当前阻断。当前block为native-panel-fd88ef6ac7802d80f61b9c375dbad320，latest_release仍为原epoch48。

该block逐字对应063 DH的ticket。release_unknown、input_release=unknown、quarantined=true；共享Windows输入域因此挡住Paint，不是Paint新事故。原[事实](../testing/release-0.2.63/installed-native/BU063-CLOSE-DH-facts.json)、[网页事件](../testing/release-0.2.63/installed-native/BU063-CLOSE-DH-page-events.json)、[关闭时刻](../testing/release-0.2.63/installed-native/BU063-CLOSE-DH-close-timing.json)均保留。

pointerdown=1790863289089，pointerup=1790863297091，相隔8002ms，超过3秒释放确认。随后网页计数1/click不补成可信释放回执；close_requested=1790863314079，晚于pointerup16988ms，不算按下至释放期间关闭竞争。

原执行者Shell PID3864仍存活，创建时间FILETIME134353335372030593与事故完全一致，不是PID重用。原native_recovery::commit_confirmed_release按创建身份核对，仍活着明确拒绝；晚到pointerup或关闭面板不能代替原实例退出。本轮未调用challenge、confirm_recovery或放行，未停止进程。

人工处理需要先核对事故ID/父轮/调用，停止在途输入并安排原实例正常退出，核实鼠标键盘实际释放、无残留输入执行者，再从可信桌面窗口打开“运行轨迹 / 安全详情”→“连接与输入安全详情”→“归属/恢复”旁“放行隔离…”，写明已核查依据，由原生窗口确认。普通浏览器不能代替确认。原Shell还活着时直接确认可能被拒绝；正常退出/重开及确认需要人工安排，不擅自执行或代签。

人工决定和机器开放是两步：机器核实原实例退出及恢复屏障、持久事实和资格；实际outcome=opened且允许新输入后才能开始新任务。未知/kept_isolated继续隔离，不删事故、不重置库、不改时限、不重放旧动作。本说明不表示已完成物理核查或建议直接批准。

## 仍开放与下一门槛

DSH正式启用管理动作、FrozenParentContext冻结工具定义、原provider call_id/共享预算执行、停用期间在途撤销、真实Qwen、包含新代码/资源的候选MSI和实际按钮UI未完成。DSH特定提交后故障/进程重启恢复未新增实操，只有通用事务回归，不扩大归因。未知来源实际执行、新权限/生产发布需具体确认；本轮只执行已审查官方计算器的工程目录。

Browser正常点击/历史导航证据保留，真正慢HTTP、按下至释放关闭、旧世代边界未补做。完整Paint海绵宝宝未验收，063新增W仅是部件。动画/泛光/总体仍开放，原“五项”编号清单未取得，不能推测完成率。

再次核对工具目录仍无node_repl/Sky原生computer-use入口，不以普通浏览器、旧图或测试数量替代实际GUI。下一工程门槛是快照绑定真实父轮/provider调用和预算；产品门槛是新资源候选MSI实际按钮→安装停用→真实启用→原Qwen→停用/卸载实拍。Browser/Paint须人工处理隔离和可调用原生工具；独立DSH工程无需放行。
