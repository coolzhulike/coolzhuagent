# CLI 在途模型等待取消

当前实际同步入口为command-line的DefaultRuntimeClient::stream，并非名为SyncApiClient的类型。原实现block_on等待ProviderClient::stream_message及每次next_event，没有在这些异步等待处观察根执行控制。

本轮进入block_on前读取并克隆current_execution_control；独立controlled_future模块在预先取消时不轮询请求，每20毫秒观察已有根取消/截止，正常就绪结果交付前再次检查。请求建立中断返回未确认完成/用量未知；流读取中断先合并当前usage_evidence，再以StreamFacts::incomplete保留已知/未知用量、不执行未完成工具请求且不自动重放的事实。未增加业务预算或替换父取消令牌。取消丢弃等待Future，退出流函数释放流对象；不宣称远端请求未执行或账单为零。

真实CLI构建先实际exit0（31.77秒），完成后再真实cargo build --locked --offline -j1实际exit0（6.14秒）。最终完整CLI回归串行101通过、0失败、0忽略，实际exit0。日志 tmp/analysis-cli-controlled-future-build.txt、final-build.txt、tests-serial.txt。新增预先取消零Future轮询及真实本地TCP截止测试，服务端观察EOF或Windows ConnectionReset证明拥有连接的Future被丢弃；该测试不证明真实Provider协议、连接池或远端取消确认。

首轮并行回归99通过2失败，实际exit101，日志tmp/analysis-cli-controlled-future-tests.txt保留。新增测试误只接受EOF，Windows实际连接重置，修正为EOF/RST；另一个既有rejects_unknown_allowed_tools并发失败在串行复验通过，其并发失效根因尚未定位，不声称已修复或首轮通过。

本轮仅接CLI生产适配器，ApiClient trait本身及其它入口的同步实现不自动获得取消能力。正式外部Provider在途取消、终态归属、usage迟到、权限撤销、请求建立/流读取分阶段真实协议矩阵仍开放。最新修补尚未进入release候选或安装包，32WBS与1.5/2.5不计完成。

后续真实Provider协议复验：直接使用ProviderClient::from_custom_openai_compatible和生产controlled_future::wait，本地服务器完整读取真实POST /v1/chat/completions请求后触发同一根取消。两阶段分别为不发响应头、先发有效OpenAI兼容SSE前缀且正文保持未完；后者确认至少一条协议事件实际读取，再取消后续next_event等待。两阶段均观察取消及服务器EOF/RST，Provider local_endpoint_drain仍remote_result_unknown>0且禁止自动切换。该测试证明实际请求/流对象释放及未知账务保留，未冒充远端计算停止、外部模型终态、CLI渲染/工具派发全链验收。

完整CLI串行回归实际exit0：102通过0失败0忽略；最终真实cargo build亦实际exit0。日志 tmp/analysis-cli-provider-cancel-tests.txt、build.txt。测试在新的本地随机端口执行，未使用外部账号或替换正式运行环境；本轮只增加测试，生产接线保持此前修补。
