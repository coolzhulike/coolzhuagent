# 0.2.128 正式验收状态

2026-10-10：正常整包与 MSI 构建实际 exit 0，安装实际 exit 0。源码提交为 b849767302bf2d2909848add923673fb52f8a71e。六项整包门禁通过。

实装逐文件核对 1192 个载荷文件：长度和 SHA-256 全部匹配，未发现清单外残留文件（清单自身载体单独排除）。证据见 evidence/install/installed-verification.json。安装版 CLI 报告版本 0.2.128、上述完整源码 SHA、x86_64-pc-windows-msvc。

安装注册表确认 COOLZHU CODE Agent DisplayVersion 为 0.2.128。独立核对最终 MSI 摘要、载荷清单文件摘要、整包报告文件摘要与 installer report 一致；载荷摘要相同；文件级 SBOM 的 1192 个条目逐项名称、长度和 SHA-256 与载荷清单一致，没有遗漏或额外项。证据见 evidence/install/delivery-binding-verification.json。MSI、文件级 SBOM、安全扫描及安装报告均保持各自证据范围；安全扫描通过不替代运行验收。

原验收配置仍绑定 127.0.0.1:8765。恢复原验收后台的 Start-Process 命令被自动审批策略拒绝；工具未提供更具体的拒绝原因。不得据安装和文件校验宣称原生 UI、真实模型、Browser 零输入负例、Goal 取消或恢复验收通过。原失败记录继续有效。

完整许可证/NOTICE、签名及其余 32WBS 未闭环项继续开放。文件级 SBOM 不等同于完整组件 SBOM。本版本尚未完成正式交付验收。
