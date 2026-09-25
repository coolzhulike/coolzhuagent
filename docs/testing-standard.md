# 测试规范

## 测试分层

- 模块单元测试：放在模块 package 内。
- 模块契约测试：放在模块 package `tests/` 内。
- 跨模块联调测试：放在根目录 `tests/`。
- 视觉人工确认测试：放在根目录 `tests/manual-visual-confirmation.md` 说明，不默认自动运行。

## 根目录联调测试说明

`tests/module_linkage_smoke.rs` 覆盖：

- `computer-use` 锚点矩阵。
- `vision` grounding 坐标解析。
- `server` Axum app 构造。
- `runtime` 会话消息类型。

运行：

```powershell
cargo test --test module_linkage_smoke --offline
```

## 视觉人工确认测试

涉及真实 GUI 截图、VLM 识别、鼠标命中、窗口变化的测试不要默认自动化通过。测试过程应输出截图路径、点击坐标、ROI，并由人工确认结果是否符合预期。
