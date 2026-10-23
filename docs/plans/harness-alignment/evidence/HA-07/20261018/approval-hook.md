# HA-07 审批 hook 复核：部分实施记录

状态：**HA07-01 已有真实证据；HA07-02～05 未完成，HA-07 仍 in_progress**。

## 本轮实现

- `tool_dispatch_service::run_approval_hook`：复用启动时冻结的 HookRunner；审批消费后不重新入队，不追加第二次用户审批。
- `execute_approved_pending_record` 在真正执行前运行 pre-hook；hook 拒绝或 hook 结果未知时返回 Rejected，不产生目标文件。
- 执行后运行 post-hook；只把 hook 结果写入 evidence，不把已发生效果伪装成未执行。
- 无 hook 配置/授权时保持原行为（直接 allow），不新增全局锁或审核模型。

## 已验证

`runs/hook-recheck-01/output.log`：

- 测试 `tests::approved_tool_rechecks_pre_hook_and_does_not_execute_on_deny`。
- 真实授权 HookRunner 的 pre 命令 `echo denied>marker & exit /b 2` 运行一次，写 1 行 marker。
- 审批消费后目标 `must-not-be-written.txt` 不存在。
- `ApproveResponse.outcome.status == rejected`。
- `1 passed; 0 failed`，exit 0。

该测试同时覆盖“用户审批交互一次 + hook 后复核”；没有把 hook 拒绝当成授权，也没有新增自动审核模型请求。

生产构建：`tmp/ha07-build-01.log`，`cargo build -p coolzhu-web-console --offline` exit 0。

## 未完成（不能宣称 HA-07 accepted）

- HA07-02：同范围并发 pending 合并/一次性批准范围尚未实现；当前仍按 call_id 独立 pending。
- HA07-03：hook 改路径/命令/目标/权限、切房间/实例/版本后的授权失效尚未实现；当前只处理 hook 拒绝/未知。
- HA07-04：审批/hook/队列/领取资格前四个撤销屏障尚未实现，没有可控并发与撤销效果计数。
- HA07-05：审批拒绝/超时/重启旧 grant 生命周期、CLI/ACP 回归尚未补齐；没有通用撤销 API。
- 当前没有全局长锁；没有新增自动审核模型。

## 限制

- 测试使用进程内 HookRunner override，hook 命令是真实 cmd 进程；不是外部部署的 hook 服务。
- 只验证 pre-hook 拒绝；post-hook 只写 evidence，未证明 hook 可改变已发生效果。
