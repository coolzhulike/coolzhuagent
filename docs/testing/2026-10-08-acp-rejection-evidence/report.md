# ACP 登记前拒绝证据：源码候选

正式101源码变化实测只有零派发和模型转述，宿主登记前拒绝未单独保存。此修补仅在通用dispatch入口live/tool_live拒绝时写现有运行事件，不增加执行记录、模型工具或权限，不显示前端调试信息。

事件 `tool.dispatch_rejected` 仅记录固定阶段和原因码、executed=false、冻结attempt、已声明工具名或统一占位、请求ID的SHA摘要。无参数、原始请求ID、错误自由文本或凭据。审计保存失败仍拒绝，错误明确提示证据未保存。其它RPC及CU控制入口不外推。

沿用一个现有本地宿主真实文件执行测试补少量断言：未知工具/撤销均零执行，脱敏与原attempt归属，故障注入使审计事件写入失败后仍不写目标文件、不增加tool_calls。不新增模型夹具，不以本地确定性回归代替真实SWE。

offline build成功，完整控制台1418通过/0失败/6既有忽略，另lib8及宿主1通过；[原始日志](tests-final.log)、[摘要](verification.json)。[设计与范围](../../analysis/2026-09-21-integration-review/acp-rejection-evidence-design-2026-10-08.md)。当前正式运行仍0.2.101，后续新包真实SWE源码变化竞态与独立回执实拍仍需验收，不补写旧历史、不把此候选算正式通过。
