# P6-D 图标与朱红印记细化

日期：2026-08-30

## 本轮范围

- 工程工具栏的对比、保存、新文件、新目录、重命名、删除、刷新、索引八个动作统一使用 `assets/icons-wuxia/*.svg`，保留中文标签与原有动作入口，并为装饰图片补充 `aria-hidden="true"`。
- `wuxiaIconElement()` 使用固定白名单，仅允许当前动态调用的 `alert-triangle`、`chat`、`chevron`、`delete`、`stop`；非法名称统一回退到有效的 `alert-triangle.svg`，避免拼接不存在的资源路径。
- 目标中心无真实目标时只显示可操作语义明确的空态，不再伪造 `REQ-GOAL-*` 阶段或“暂未接入”调试文案。
- 朱红印记改为指针透明的装饰容器：空白印章图片作为底层，DOM 字形层根据审批/失败状态显示“批”/“错”。失败反馈优先于待审批反馈，保留 250ms 一次性动画与 reduced-motion 规则。

## 边界

本轮不调整 P6-C 已确认的窗口布局几何，不修改后端 Run/Queue 逻辑，不生成新资源，不启动浏览器或服务。`main.rs` 增加对应前端静态契约，覆盖图标、空态、朱印状态优先级和装饰可访问性。

## 验证

验证结果记录在 `tmp/qa-p6d-impl-2026-08-30/implementation-summary.md`，包括 Node 语法检查、`web_frontend_` 契约测试、离线构建、`git diff --check` 及构建产物信息。
