# 041只读终态源码边界核验

2026-10-01，主会话读源码核验；不是软件截图验收。既有审核会话页面只显示“极高”，完整模型版本未核，不称指定模型或四项总体验收。

| 复审要求 | 实际路径与结论 |
|---|---|
| COM线程归属 | Tauri `native_browser_observation.rs`的`view.with_webview`闭包及回调中获取controller、调用固定Accessibility.getFullAXTree。Web spawn_blocking仅等待认证broker，不传COM对象。 |
| 面板当前观察资格 | Tauri `browser_panel::input_resource`查询主窗口visible/minimized及面板active/hidden/loading/destroyed。派发前与回调后重新核验；关闭销毁变更generation。 |
| 父停止与CU成功先提交者生效 | Web store `finish_checked`取得Immediate事务；executor在该连接上读冻结父数据库关系、持久CU deadline与取消预算；同一事务写CU原版本且terminal为空的结果。已终态返回原结果，不覆写。 |
| 核验不能新开数据库 | `validate_frozen_parent_relations_on_connection`使用传入连接，调用`query_runtime_run_connection`；不再在成功判定中另开父库连接。实际main库路径canonical与冻结runtime_db_path必须匹配，不同库拒绝。 |
| 错误语义 | 父stop_requested/interrupted或内存取消→Cancelled；预算过期→TimedOut；父关系/库失效→Blocked、native_browser_parent_changed；goal=false。保留观察证据和原计数/收尾事实，不冒充verification_failed。 |
| 页面变化与取消的不同裁决点 | S2采样/匹配证明该时点页面事实；最终Immediate事务裁决成功与已提交父停止。未声称UI/内存时钟与DB联合原子，不新增第三AX、DOM镜像或运行循环。 |

两项新工程回归仅调用实际SQLite与终态入口，模型规划器/软件均未执行。覆盖父停止先提交、成功先提交、旧终态不覆写、deadline、内存取消、另库/缺父；第二连接在判定事务内写入被真实SQLite锁拒绝，提交后可写。真实Qwen取消/SPA/环境失效仍需安装后复验。
