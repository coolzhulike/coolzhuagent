# 接口契约与审查规则

## 基本原则

- 跨模块接口变更必须先修改对应 `INTERFACE.md`。
- 修改公共 DTO、HTTP API、tool schema、视觉坐标语义、真实输入语义，都视为接口变更。
- 接口字段删除必须先标记 deprecated，不允许直接删除。
- 新增字段应保持可选或提供默认值。

## 当前跨模块接口

| 模块 | 接口文档 | 主要消费者 |
|---|---|---|
| core-runtime | `modules/core-runtime/INTERFACE.md` | server、cli、gui、tooling |
| llm-adapter | `modules/llm-adapter/INTERFACE.md` | core-runtime、vision、tools |
| tooling | `modules/tooling/INTERFACE.md` | core-runtime、cli、gui |
| vision | `modules/vision/INTERFACE.md` | computer-use、gui |
| computer-use | `modules/computer-use/INTERFACE.md` | gui-web、gui-desktop、tests |
| gui-web | `modules/gui-web/INTERFACE.md` | 用户、集成测试 |
| gui-desktop | `modules/gui-desktop/INTERFACE.md` | 用户 |
| cli | `modules/cli/INTERFACE.md` | 用户、自动化脚本 |
| diagnostics | `modules/diagnostics/INTERFACE.md` | 全模块 |

## 审查清单

- 是否修改了对外函数签名。
- 是否修改了 JSON 字段名或字段类型。
- 是否修改了 HTTP 路由。
- 是否修改了默认执行模式，尤其是真实键鼠输入。
- 是否修改了视觉坐标系。
- 是否补充了单元测试或联调测试。
- 是否更新了模块 `INTERFACE.md`。
