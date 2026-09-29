# 0.2.27 Paint 真实运行失败：输入新鲜性与规划阶段超时

真实运行 `NATIVE-PAINT-20260929-0227-A` 中，第一份截图与输入前重拍截图均为 2560×1152，窗口身份和几何未变。两图差异仅在最底部 11 行（y=1141..1151，共 18,512 个像素），完全落在已绑定客户端操作容器 `client_rect ∩ screen_rect`（本图 y=45..1140）之外。旧输入守卫比较整张 PNG 的 SHA，因而把这次窗口外缘变化判为 `stale_observation`；输入回执为 `NotSent`，并没有画出笔画。差异的外部成因尚不能确定。

一次未发送输入后的重新观察符合控制器规则。第二次规划请求已发出，约 20 秒阶段等待到期后，旧错误却称“computer-use 总剩余 0 ms、没有发送模型请求”。真实 CU 总期限当时仍有约 64 秒；迟到响应只进入请求事实记录，没有成为动作。

本次修复仅在截图画布拖拽的输入前新鲜性路径中，核对观察时记录的帧绑定、当前帧绑定、窗口身份、完整 PNG 摘要、尺寸、截图区域和客户端操作容器。解码前校验 PNG IHDR 和 16,777,216 像素上限，再比较**整个客户端操作容器**内的 RGBA 像素；容器外变化不使坐标过期。容器内任何变化、身份或几何变化、证据缺失或无效仍在输入前拒绝。其他 `FrameRef::classify` 全图语义没有放宽。规划请求已发出但等待到期时现报 `stage_timeout` 与阶段等待上限；请求前预算不足仍报 `budget_exhausted`、不发请求。迟到响应仍不执行。

本地验证：

- `cargo test -p coolzhu-web-console --offline desktop_canvas_requires_fresh_image_and_consumes_generation_once -- --nocapture`：1 passed。真实合成 PNG 覆盖容器外变化允许、容器内变化拒绝、几何/身份变化拒绝、绑定缺失拒绝、拒绝时 `NotSent`。
- `cargo test -p coolzhu-web-console --offline computer_use_frame::tests:: -- --nocapture`：9 passed。
- `sent_request_timeout_reports_only_its_stage_wait_limit`、`late_model_result_is_recorded_as_a_fact_without_a_new_action`、`minimum_stage_budget_is_a_pre_request_gate_without_rounding_up`、`zero_budget_returns_insufficient_budget_with_zero_model_requests`：逐条 `cargo test -p coolzhu-web-console --offline <名称> -- --nocapture`，各 1 passed。
- `cargo build -p coolzhu-web-console --offline`：通过；`git diff --check`：通过。

以上只证明本地守卫与错误事实的定向行为。真实 Qwen/Paint 再运行须待新完整安装候选部署后复验；本报告不将 0.2.27 的失败回溯改写为成功。
