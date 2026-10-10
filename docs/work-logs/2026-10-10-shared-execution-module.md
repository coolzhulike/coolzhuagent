# 统一执行模块开发记录

本会话独立工作区：`shared-execution-20261010/coolzhuagent`，分支 `codex/shared-execution-20261010`，冻结基线 `40d3598471b8ae1ed88ce9c018e5d06f6442fb31`。

完成执行架构事实审查，新增共享 ExecutionService/TurnRunner，接入原 HostAgentRunner 与 scheduled poll；新增持久执行索引、真实模型会话工具目录、父子关联、独立子取消、原聊天run绑定、真实usage投影和独立右栏执行页。源头仍是原会话/凭据/上下文/记忆/权限/工具运行器，不创建第二套。原Devin入口限制保持。

离线验证过程：

- 首次 `cargo build -p coolzhu-web-console --offline` 实际完成，2m08s，exit0。此后有源代码修正，因此不能作为最终源码通过。
- 首次专项test编译 LLVM OOM，0xc0000409；保留 `tmp/analysis-execution-tests.txt`。全机多会话编译竞争由总控协调串行，未结束任何其它会话或用户进程。
- 单任务、test debug0编译产物完成，4m31s；execution_service初版2项通过。原HostAgentRunner回归该产物1通过/2失败，拒绝原因是旧版全局session_store路径校验；修正源码已改读取冻结父运行库的原房间历史，未追认通过。
- 后续审查补齐真实tool调用ID的稳定执行ID、并行spawn后的持久父关联、取消观察不抢主Notify通知、详情与结果预览分离、原运行中断和子取消后结果提交闸门。新回归包含并行父关联和多子任务取消，需最终构建后执行。
- `node --check execution_panel.js`、`git diff --check` 实际通过。属于静态验证，不等于页面/原生软件验收。

总控代验首次冻结源码：最终build exit0/1m43s，共享服务4通过；HostAgentRunner 2通过/1失败，原因是原测试取消与超时两个独立用例复用同一父工具ID，新索引按预期拒绝重放。已将超时用例分配独立来源，并新增对原取消来源拒绝重放的断言；超时错误文案明确含“超时”。首次代验失败与源码SHA256保留在 `tmp/analysis-execution-first-coordinated-validation.json`、`tmp/analysis-execution-host-first-coordinated-failure.txt`。修正后的最终build和回归仍待总控协调，不追认通过。

最后源码构建/必要模块回归等待总控分配全机编译槽位，命令和日志路径持续写入共享 `execution-result.json`。`tmp/analysis-execution-validate.ps1` 已准备顺序执行最终build、共享服务、HostAgentRunner和scheduled_delivery专项，并记录实际exit及源码SHA256。未获槽位前不启动重型Cargo。

大入口暂时仅在本独立checkout接线验证，提交时恢复自身临时入口改动；精确补丁与前端接线见 `docs/analysis/2026-10-10-shared-execution-integration.md` 及同目录 main-wiring.patch，总控统一集成。

本会话没有真实模型调用、正式服务/桌面操作、安装或发布。Goal型定时、完整Web/SSE/CLI/Goal/Relay循环收敛、安全显式重试、后代累计usage、跨进程执行控制、长期历史分页均继续开放。不能将本模块候选计为完整WBS或SWE正式通过。

已纳入总控集成审查：统一schema阶梯、workspace/config请求scope与全部迟到响应作废、切工程/dispose清空详情及取消请求三项必须修正；冻结候选尚未完成这些修正。源码不变，模块文件修改所有权待总控确认，不能把旧摘要复验结果作为可集成结论。

最终交付验证（修正工程scope及身份查询失败缓存后的冻结来源）：cargo build offline/j1/debug0 exit0，7.76秒；execution_service 6通过/0失败，host_child_agent 3通过/0失败，scheduled_delivery 3通过/0失败；原 session_schema 7通过/0失败，schema_upgrade 2通过/0失败。前端 node --check 与异步模拟均exit0；模拟覆盖首次身份失败刷新恢复、旧工程迟到失败不清新缓存、冻结请求头、切工程清理、迟到详情/取消错误/轨迹废弃及dispose清理。未自动重放取消POST。

来源摘要已与最新validation.json逐项核对一致，临时大入口接线仅作为受验证组合，交付独立模块及精确main补丁，由总控应用schema版本29、结构检查与四个执行API等入口接线。真实模型、正式软件、完整共享循环等前述开放项保持。不存在“完整WBS已通过”结论。
