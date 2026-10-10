# 执行中心总控接线与验证

模块来源 cfb9905fad9f8e86edbbe9a502aa33592ea23d5d，集成提交 328cb89。模块交付后的总控改动：统一 schema 阶梯由28推进29；四条执行 API 路由；普通与流式模型请求的冻结 Agent 工具目录；右栏执行页与编译内联资源接线。

总控额外发现并修复：右栏最终样式仍固定四列，改为五列；同路径重载工程只递增 projectWorkspaceGeneration 而不触发已有 workspace 事件，执行页旧请求失效后可能遗留 busy=true。现在工程请求失效时发出独立 project scope 事件，执行页立即 abort、清空详情与scope缓存，再读取新工程；dispose也移除该监听。

前端语法检查及基于交付异步回归的同路径重载模拟实际通过。模拟包含请求头冻结、首次网络失败后重查scope、同路径换代清理、旧详情/取消错误/轨迹废弃与dispose中断；仅DOM模拟，不是正式原生UI验收。

首次集成 offline build实际exit0，耗时1分52秒；该构建之后补充了上述UI清理和布局修正，不能当作最终源码编译证据。最终源码构建和完整回归以 tmp/analysis-execution-integrated-final-build.txt 及 tmp/analysis-execution-integrated-full-tests.txt 实际终态为准。

仍开放：完整共享模型循环、Goal阶段接入、整个后代树用量累计、稳定历史游标及跨进程控制恢复；真实provider、正式原生UI及整包安装验收。正式0.2.127进程没有被本轮替换；PR88仍未合并，未改版本标签或更新索引。
完整回归首次运行真实退出 `0xc00000fd / STATUS_STACK_OVERFLOW`，位置为 Goal 子 Agent 的停止/失效claim专项，日志 `tmp/analysis-execution-integrated-full-tests.txt`。总控将子 Agent 的模型循环future及新增生命周期包装放到堆上，减少嵌套异步future的栈占用；没有提高 RUST_MIN_STACK 或忽略失败。修正后offline build实际exit0（42.43秒），子Agent专项3通过/0失败（1.97秒），日志分别为 `tmp/analysis-execution-stack-fix-build.txt` 和 `tmp/analysis-execution-stack-fix-tests.txt`。随后重新执行完整回归，真实结果见 `tmp/analysis-execution-integrated-full-tests-retry.txt`。
第二次完整回归真实exit101：1449通过、1失败、6忽略，30.41秒。唯一失败为原右栏ARIA契约硬编码4个tab；新执行页为第5个。已把数量及逐tab面板绑定检查同步到5个，保留其余可访问性断言。最终构建/回归日志为 tmp/analysis-execution-contract-build.txt 与 tmp/analysis-execution-integrated-full-tests-final.txt。

最终构建实际exit0（24.31秒），完整Web回归实际exit0：1450通过、0失败、6忽略（30.72秒）；另browser_native_host单测1通过。当前集成代码通过离线检查，正式软件验收保持未通过/未执行边界。
