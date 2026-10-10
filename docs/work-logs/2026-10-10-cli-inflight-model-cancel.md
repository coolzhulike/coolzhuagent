# CLI 在途模型等待取消

当前实际同步入口为command-line的DefaultRuntimeClient::stream，并非名为SyncApiClient的类型。原实现block_on等待ProviderClient::stream_message及每次next_event，没有在这些异步等待处观察根执行控制。

本轮进入block_on前读取并克隆current_execution_control；独立controlled_future模块在预先取消时不轮询请求，每20毫秒观察已有根取消/截止，正常就绪结果交付前再次检查。请求建立中断返回未确认完成/用量未知；流读取中断先合并当前usage_evidence，再以StreamFacts::incomplete保留已知/未知用量、不执行未完成工具请求且不自动重放的事实。未增加业务预算或替换父取消令牌。取消丢弃等待Future，退出流函数释放流对象；不宣称远端请求未执行或账单为零。

真实CLI构建先实际exit0（31.77秒），完成后再真实cargo build --locked --offline -j1实际exit0（6.14秒）。最终完整CLI回归串行101通过、0失败、0忽略，实际exit0。日志 tmp/analysis-cli-controlled-future-build.txt、final-build.txt、tests-serial.txt。新增预先取消零Future轮询及真实本地TCP截止测试，服务端观察EOF或Windows ConnectionReset证明拥有连接的Future被丢弃；该测试不证明真实Provider协议、连接池或远端取消确认。

首轮并行回归99通过2失败，实际exit101，日志tmp/analysis-cli-controlled-future-tests.txt保留。新增测试误只接受EOF，Windows实际连接重置，修正为EOF/RST；另一个既有rejects_unknown_allowed_tools并发失败在串行复验通过，其并发失效根因尚未定位，不声称已修复或首轮通过。

本轮仅接CLI生产适配器，ApiClient trait本身及其它入口的同步实现不自动获得取消能力。正式外部Provider在途取消、终态归属、usage迟到、权限撤销、请求建立/流读取分阶段真实协议矩阵仍开放。最新修补尚未进入release候选或安装包，32WBS与1.5/2.5不计完成。

后续真实Provider协议复验：直接使用ProviderClient::from_custom_openai_compatible和生产controlled_future::wait，本地服务器完整读取真实POST /v1/chat/completions请求后触发同一根取消。两阶段分别为不发响应头、先发有效OpenAI兼容SSE前缀且正文保持未完；后者确认至少一条协议事件实际读取，再取消后续next_event等待。两阶段均观察取消及服务器EOF/RST，Provider local_endpoint_drain仍remote_result_unknown>0且禁止自动切换。该测试证明实际请求/流对象释放及未知账务保留，未冒充远端计算停止、外部模型终态、CLI渲染/工具派发全链验收。

完整CLI串行回归实际exit0：102通过0失败0忽略；最终真实cargo build亦实际exit0。日志 tmp/analysis-cli-provider-cancel-tests.txt、build.txt。测试在新的本地随机端口执行，未使用外部账号或替换正式运行环境；本轮只增加测试，生产接线保持此前修补。

后续终态接线修补：发现此前适配器取消仅返回普通RuntimeError，ConversationRuntime依据结构化interruption区分TurnInterrupted/TurnFailed，文字取消不能代替该字段。RuntimeError新增with_interruption，保留原错误消息/轨迹；CLI两处取消映射写入实际Cancelled/TimedOut原因，不在普通Provider错误上擅自添加取消。

新增实际DefaultRuntimeClient + ConversationRuntime + ProviderClient本地HTTP/SSE测试：服务器发送部分write_file工具参数和输入7/输出2用量快照后触发根取消。断言真实连接关闭、工具执行器调用0、错误保留输入7及不完整事实、会话仅用户消息、终态TurnInterrupted/Cancelled。进一步使用生产persist_result保存错误及load_event_history恢复，确认仍返回错误且恢复唯一中断轨迹，未冒充完成。最新完整CLI串行103通过0失败0忽略，实际exit0；正常CLI实际构建exit0（12.05秒）。日志 tmp/analysis-cli-cancel-terminal-build.txt、terminal-tests.txt、persist-tests.txt。

共享RuntimeError接口的完整core-runtime串行回归同一会话4133已实际exit0：库375通过0失败1忽略，两项DSH集成各1忽略，总3忽略不计通过。日志tmp/analysis-cli-cancel-reason-core-tests.txt。实际外部Provider、完整权限/截止/迟到矩阵与候选正式交付仍开放。

后续入口审查：CLI生产源码中直接Provider请求仅DefaultRuntimeClient的上述两处等待，shared_chat另有tokio::signal::ctrl_c及远端停止确认路径；main.rs的legacy LiveCli/run_prompt/internal_prompt直接run_turn调用点未安装with_execution_control，生产当前控制可能为None。因此适配器局部能力不等于legacy prompt/REPL取消闭环，需要绑定实际宿主取消/预算并证明正常结束、连续回合和权限提示不泄漏信号；不得仅在测试注入根控制后宣称这些入口完成。

新增截止完整终态复验：同一生产适配器部分工具流测试分别使用显式取消及800毫秒根截止，两者工具零调用、输入7快照保留、不完整助手未提交；生产保存/恢复分别得到Cancelled与TimedOut的TurnInterrupted。CLI完整串行104通过0失败0忽略，真实build实际exit0，日志tmp/analysis-cli-cancel-deadline-tests.txt、build.txt。本轮仅扩展测试和核验，未补上述legacy入口真实信号来源，仍明确未闭环。

后续legacy入口接线：新增cli_control每回合范围，文本/JSON/internal_prompt三处真实run_turn先完成自有监听线程的Ctrl+C首次轮询注册，再以with_execution_control进入同步回合；沿用已有父控制及截止，无父时建立本回合令牌。正常/错误/panic离开范围均停止并join自身线程，每20毫秒观察停止；不生成新的业务截止。注册失败在执行前返回错误。后续监听错误会取消根并打印诊断，线程panic/收尾错误的完整结构化监督仍需补，不宣称完备。

真实cargo build实际exit0（8.76秒），最后再次构建exit0。完整CLI串行107通过0失败0忽略；新增真实Ctrl+C注册和上下文恢复、模拟注册失败零派发、模拟信号取消仅本回合及连续回合隔离、父截止保留。日志tmp/analysis-cli-root-control-build.txt、final-build.txt、tests.txt。模拟信号不等于真实按键触发；没有向当前共享控制台注入Ctrl+C，也没有终止其它实例。真实独立CLI进程组按键、权限提示阻塞读、退出/信号同时发生、线程监督失败和安装版仍未正式验收。Tokio底层信号注册是进程级的，停止本轮Future不证明系统处理器已注销；REPL空闲输入语义须实测。

后续监听线程异常保护：在自有线程内捕获注册/轮询 panic，撤销同一个根控制，以非阻塞 try_send 通知注册错误，避免注册已确认时阻塞错误上报。注册前 panic 不派发；注册后 panic 能结束本回合受控等待，退出范围仍 join 自有线程。新增两个永久回归，等待断言有2秒观察上限。此路径依赖 unwind，未覆盖进程 abort、阻塞权限输入和信号/完成竞态的完整监督。

真实CLI构建exit0；完整串行CLI测试109通过0失败0忽略，实际exit0。日志 tmp/analysis-cli-listener-panic-build.txt、tmp/analysis-cli-listener-panic-tests.txt。仅证明本地异常保护，真实独立进程组Ctrl+C、外部正式模型、安装包和32WBS全量门禁仍开放。

Windows权限输入取消接线：审查确认原CliPermissionPrompter直接stdin.read_line会阻塞，根令牌不保证唤醒。现经windows-process-guard安全接口，在原调用线程使用ReadConsoleW/ReadFile，独占重复的线程句柄用于CancelSynchronousIo；自有监听线程每20毫秒检查根状态，重复取消覆盖检查到读取间隙，返回/异常时停止并join，不遗留占用下一回合输入的读取线程。CLI再次检查原根原因，取消不会成为允许；输入预算8192个字节/UTF-16单位，管道UTF-8严格解码，控制台UTF-16严格解码，保留无换行末行EOF语义。

首版把Win32放CLI触犯workspace unsafe_code=forbid，真实build exit101，日志tmp/analysis-cli-permission-input-build.txt保留；已移至既有guard安全边界，未放宽workspace禁用unsafe约束。最终真实CLI build exit0（10.56秒），完整CLI串行109通过0失败0忽略；最终guard库完整59通过0失败3忽略，实际exit0。新永久真实匿名管道测试覆盖阻塞读取在写端关闭前取消、下一行不吞读、预先取消零读取、无换行EOF。日志tmp/analysis-cli-permission-input-final-build.txt、cli-tests.txt、final-guard-tests.txt；先前guard58项为EOF补验前历史。

边界：未注入共享控制台Ctrl+C，控制台ReadConsoleW路径尚无真实独立控制台实拍；取消后已排队/部分输入的跨回合归属、预算拒绝后残留行、信号与审批完成同时发生、空闲REPL和非Windows输入仍需独立验证或补实现。取消回调要求非阻塞及状态单调；不能将管道测试当成全部宿主输入证明。当前源码未进入131候选或安装版，正式模型、32WBS全量、签名发布及管理员升级仍未闭环。
