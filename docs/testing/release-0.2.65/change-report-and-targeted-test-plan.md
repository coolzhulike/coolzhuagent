# 0.2.65独立候选：Browser期限修复与固定DSH资源

2026-10-02。本轮仅构建和离线核验，不安装、不重启正式实例、不上传GitHub、不解除隔离。0.2.63正式包和此前0.2.64候选保留，未覆盖。MSI候选未签名。

## 交付身份

- 文件：`dist/CoolzhuAgent-0.2.65.msi`
- 大小：275647003字节。
- SHA256：`fa80c6faf13383f926f03a4617b5a406918b9b602e2fbf08dbd2f7e89c35eb19`。
- 报告ID：`pkg-report-release-20261002-193445262-4b3afcd6`。
- 源码快照：`528b32995570881438ca73e63b1430328b78ae5d6d21509fe5ae810eab9f891e`，1281文件、203702354字节。
- 冻结构建输入：`9a5e3f51f44415e6c6c5c14acbf924b9f4ed92118d7f599e28cdaf7ebd90e484`。
- 载荷摘要：`022170edf6acbf242f08488214ca474a7b51989ea8b41e3ecfedbd3930f77eae`。

当前分支`codex/cu-preinput-followup-20260930`、HEAD `8181f08ae9c50d3e41aabd32a6af85343dcd92e8`仅作参考。工作树包含未提交源码，不能把包说成干净HEAD产物；包源码身份以冻结快照为准。workspace Cargo版本仍为0.2.0，候选版本由既有发布脚本注入，CLI实测版本字段0.2.65。

## 源码、manifest与测试对应

候选包含`native_browser_input.rs`的deadline修复：宿主领取和回执结算在同一Pending互斥锁内遵守请求期限，等待线程尚未清理也不能交付过期请求或接收迟到成功。保留3秒宿主释放确认、8秒传输上限、ReleaseUnknown隔离与禁止重放。`computer_use_executor.rs`的事务测试增加取消、到期、绑定失配、派发后停止和未知释放不可覆盖场景。

两文件在源码冻结清单中的SHA256与已完成的[聚合测试证据](../browser-lifecycle-aggregate-2026-10-02/evidence-index.json)逐项一致。此前验证：Web主程序1306通过/0失败/3忽略，库8及宿主1通过，根模块链接8通过，桌面面板7通过。首次默认存储环境11项失败及临时存储复核过程保留于原证据，本轮未重新运行同一组测试，也不将debug工程通过冒充release GUI验收。

使用独立`config/package-manifest-0.2.65-candidate.json`和`tmp/candidate-065-package`。候选manifest仅改变固定DSH资源source、包内manifest指向、对应外部资源声明，并将候选manifest加入构建输入；其它artifact、target、roots、排除规则与发布门均与原manifest相等，已程序化核验。既有manifest契约通过。固定资源复制到本轮独立目录，没有改写共享旧资源。

准备阶段只读解析锁定依赖并冻结；正式阶段显式消费该冻结记录，执行既有`--locked --offline`构建。六项发布门均pass：范围声明、源码稳定、roots存在、构建输入稳定、10项产物来源、冻结输入一致。前后源码摘要相同，changed_paths=0，release_eligible=true；这个标志表示包构建资格，不表示真实功能验收完成。

## 实际MSI内容核验

只读打开MSI数据库并展开3个内嵌CAB到独立tmp目录，没有调用msiexec或任何安装动作。实际MSI **1150文件、419512141字节**，路径、集合、大小和SHA256全部与暂存包一致；其中载荷清单统计1149文件、419258813字节，不含清单自身，二者口径不能混淆。

版本字段0.2.65、既有UpgradeCode与ProgramFiles64Folder/CoolzhuAgent安装布局符合预期，CustomAction表无动作。包安全扫描1150文件，safe=true、findings为空。现有WiX5.0.2直接用于打包，未安装新工具。

从实际MSI展开后的`bin/dsh-runtime`再次执行官方锁定资源校验：290文件、93908058字节、Node与17个SDK依赖完整集合通过，锁摘要`a64c15bd0ce8c247ddab0c361658ae78be11eb31f4ce5ba3e0602513c27238f7`。逐文件内容匹配不是DSH真实模型调用成功证明。

源冻结、构建输入冻结、构建报告、实际载荷清单、逐文件MSI/CAB回执、运行资源锁、测试绑定和旧包/正式文件保留性核验见[证据索引](evidence/index.json)。

## 保留与停止点

0.2.63/0.2.64 MSI及其原报告/安全回执逐项哈希不变；正式Web和Shell二进制仍与0.2.63原哈希一致，未安装本候选。原事故文件、Codex配置保持原字节。未向旧会话发操作指令，未结束旧node/Shell进程、删除锁或改安全策略。

本候选等待本地Windows CU恢复后的实际验收：先只读枚举窗口并核对原事故；安装、原执行者退出与隔离恢复须按已明确的具体授权和正式流程另行处理。Browser必须实测按下至释放期间的Close/Cancel/navigation与回执/销毁交错，不能把8秒晚到计数或释放后关闭当成通过。Paint完整绘画、真实Qwen/DSH调用及原动画/泛光总体审核仍开放。

打包核验至此结束，停在真实CU恢复依赖点，不继续扩大功能或重复测试。
