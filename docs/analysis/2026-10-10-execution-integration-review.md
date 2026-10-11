# 执行模块集成审查：尚不能按完整共享执行闭环登记

开发范围当前为会话工具子Agent与scheduled poll，完整Web/SSE/CLI/Goal模型循环和Goal型定时尚未收敛。总控不把关联索引本身视为全部共享TurnRunner完成。

总控在模块独立worktree验证冻结源码：offline build实际0/1分43秒；execution_service专项4通过；host_child_agent专项2通过1失败，错误为重复execution_index.id；脚本在失败处停止。随后总控单独运行scheduled_delivery专项3通过。日志在模块worktree的`tmp/analysis-execution-final-*`，来源摘要在`tmp/analysis-execution-final-validation.json`。开发会话随后拆分取消/超时的委派调用ID并保留拒绝重放断言，源码摘要已变化，须按新源码重编译，不追认其修复通过。

入口接线前必须处理：

1. `execution_service::open` 当前每次执行列表/详情/取消/状态写入都调用`migrate`建表。应移入原统一schema阶梯并登记版本，正式请求只打开/查询既有表；隔离模块测试显式初始化。执行索引不能在查询请求隐式改变schema。
2. `execution_panel.js` 当前直接fetch，只有列表自身请求generation，没有工程切换事件处理。详情、轨迹、取消与错误响应均可在工程切换后更新旧页面；需要冻结工程及配置scope，服务端校验相应请求头，所有异步返回核对工程generation。已有app.js `coolzhu-workspace-changed`事件可复用。详情及停止不能把跨工程同ID误作用于当前工程。
3. 初次隐藏面板和文档不可见时略过轮询合理，但dispose及工程切换须中断/废弃当前请求并清空选择。旧详情/轨迹不能在新列表加载后保留。
4. `child-execution:{parent_call_id}`的来源必须带已持久领取的调用身份。真实重试应新接纳身份，不能修改持久领取以允许重放。回归调整只修正独立取消/超时测试使用了同一调用来源的错误，不削弱重放拒绝。

真实模型、已安装软件、原生窄栏、父子取消效果、重启持久账目和唯一SWE定时poll未由上述模块测试证明。Devin子Agent/Goal/Relay、嵌套后代usage、未知结果重试和跨进程执行控制仍按交接矩阵开放。
