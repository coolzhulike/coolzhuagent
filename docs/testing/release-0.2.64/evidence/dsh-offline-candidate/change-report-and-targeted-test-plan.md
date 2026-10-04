# 0.2.64 DSH 新资源 MSI 本地候选与离线核验

2026-10-02。本轮按继续授权完成独立候选构建和静态包核验；没有安装、升级、卸载、启动候选界面、关闭正式进程、调用项目模型、创建凭证或上传 GitHub。**正式安装仍为0.2.63，0.2.64只是本地未签名候选；静态通过不是软件功能验收。**

## 交付与版本来源

候选文件：[CoolzhuAgent-0.2.64.msi](../../../dist/CoolzhuAgent-0.2.64.msi)，275626523字节，SHA256：

`16052ec401960181853c5b02775e849185e225e70b11f8872a798a4e5d06688b`

分支`codex/cu-preinput-followup-20260930`，参考HEAD `8181f08ae9c50d3e41aabd32a6af85343dcd92e8`；Cargo workspace版本仍0.2.0，发布编译环境显式使用0.2.64 / 2026-10-02 / x86_64-pc-windows-msvc。候选CLI由仓库MSI脚本实际执行`--version`核对0.2.64，MSI数据库ProductVersion也为0.2.64。未提交工作区包含本轮DSH代码，**不能将包解释为干净HEAD的构建**。

源码权威为实际冻结工作树快照。三份身份分别是：

| 身份 | SHA256 | 含义 |
| --- | --- | --- |
| 源码快照 | `909a647759f9d7a21aae22273df376a6b021b3e61b39d33df27722fa235e195d` | 原声明范围内1280文件、203673354字节，含新增DSH源码与候选清单 |
| 构建输入 | `309eaf48066ace068fe2829e9cbf609f9af28b887ff9e8869f59c9498db8eb36` | 48项冻结输入、工具链、profile、版本与实际源码身份 |
| 载荷 | `265a5c3067350eead9347edf90c63964c2880a1c4055cc9bccb077029cdfaff7` | 包根1149文件、419249589字节，不含自引用清单载体 |

本次包报告`pkg-report-release-20261002-163334775-4f4d550d`，内容摘要`40351f6898cc2c12cf6afae3f4de3537bde68638553deb17a634bc4f33af96f7`。原脚本的release_eligible/Protect“released MSI”措辞只记录发布工程资格和本地文件引用，未发生外部发布，也不批准真实UI或模型验收。

## 独立构建与资源来源

沿用仓库`package-all.ps1 -Prepare → -FreezeRecordPath → build-msi.ps1 -SkipPackageBuild`流程，profile=release，所有Cargo构建使用`--locked --offline`。独立包根`tmp/candidate-064-package`，现有WiX 5.0.2，无工具安装、npm脚本或资源下载。

共享`tmp/dsh-runtime/windows-x64`首次完整核验不通过：Cordis等SDK依赖文件缺失。该目录没有被覆盖、清理或补写。将上阶段已核验的真实完整副本复制到`tmp/2026-10-02-dsh-msi/fixed-runtime-source`，再次逐文件核验；使用[独立候选清单](../../../config/package-manifest-0.2.64-candidate.json)记录实际来源，仅改变运行时source、包内清单source、对应外部资源声明，并增加候选清单构建输入。全部原artifact、安装target、源码roots、完整资源表及发布门禁保留；原清单不变。候选清单处于config根，因此自身也进入冻结源码范围。

DSH资源：官方Node24.15.0，17个SDK依赖，共290文件/93908058字节；SDK锁`af9313e4938c2c94596b702e13f48dfe430b47ec52eab98855d6707a5bdb1307`，资源锁`a64c15bd0ce8c247ddab0c361658ae78be11eb31f4ce5ba3e0602513c27238f7`。第三方资源身份单独核验，不拿载荷摘要冒充第一方源码身份。

## 已执行的最终核验

| 检查 | 结果与边界 |
| --- | --- |
| 准备/冻结 | 通过；输入变化0，生成配置0，完整48项冻结 |
| release构建与MSI | 通过；2026-10-02 08:29:08.630Z至08:34:10.319Z，约301.69秒；10项产物来源确认，6项发布工程门禁pass |
| 源码/输入稳定 | 构建前后逐文件一致，构建使用准备冻结记录；归档时再次核对1280文件一致 |
| 包安全扫描 | 实际1150文件，0发现，safe=true；未复制会话、用户配置、凭据或运行数据库 |
| 清单契约脚本 | 最终独立PowerShell子进程退出0，PASS package-manifest |
| 只读MSI/CAB | 通过，17.223秒；ProductVersion0.2.64，稳定UpgradeCode、ProgramFiles64Folder安装根正确；3个内嵌CAB实际展开，无自定义动作 |
| MSI全文件字节 | 1150文件/419502917字节，每个实际CAB文件与包根路径/大小/SHA256一致，无缺失或多余文件；较1149清单多的是253328字节自引用载体 |
| MSI内DSH资源 | `Program Files\CoolzhuAgent\bin\dsh-runtime`；实际290文件/93908058字节，解包后再用原完整核验器验证，与固定锁及冻结宿主脚本一致 |
| 正式实例保留 | 0.2.63/09dc04d、isolated、不接受新输入、1未放行block；Shell3864创建身份134353335372030593仍存活，正式Web/Shell二进制摘要不变 |
| 历史保留 | 原0.2.63包/报告/安全扫描文件未变，原修改文档摘要未变，165条原未跟踪入口均存在，三份原事故事实摘要未变 |

全量字节回执见[offline-msi-verification.json](evidence/dsh-offline-candidate/offline-msi-verification.json)，完整290文件表见[runtime-files.json](evidence/dsh-offline-candidate/runtime-files.json)。完整源范围/文件表、实际未提交源码patch和新增源码副本、输入冻结、包报告及所有执行记录均由[证据索引](evidence/dsh-offline-candidate/evidence-index.json)关联摘要。仓库自动归档的构建身份证据也保留在`evidence/build-identity/pkg-report-release-20261002-163334775-4f4d550d`。

中间失败与修正不隐藏：共享资源预检拒绝；首次离线探针错误地将MSI短/长目录名原字段直接比长名，改为解析标准`短名|长名`后完整核验通过；首次归档脚本未兼容前阶段`Path/Hash`字段，修正字段读取后保留核验通过。清单脚本首次输出PASS但外层误用残留LASTEXITCODE报失败，最终改成独立子进程确认真实退出0。没有修改MSI产品定义、资源锁或生产权限来换取通过。

本轮没有重跑此前全量工程回归；包内代码对应[第三阶段已验证源码](../../analysis/2026-10-02-dsh-web-dispatch-review.md)，其25个源文件归档时逐项保持摘要一致。此前真实Node96和握手后取消/停用属于工程场景，不升级为本候选的真实模型或GUI通过。

## 待人工和真实环境验收

1. **处理原隔离事故，再安排安装环境。** 不在当前隔离实例直接覆盖安装。人工核查原父轮/call/输入事实，停止在途输入并安排原执行者正常退出，确认鼠标键盘物理释放及无残留执行者；在可信原生窗口“运行轨迹 / 安全详情 → 连接与输入安全详情 → 放行隔离…”写依据并确认。必须以机器核对退出/屏障/持久事实且outcome=opened、accepts_new_input=true为准，未知继续隔离。不能以候选安装替代恢复或消除事故。
2. **真实安装/升级/卸载和运行布局。** 在约定验收环境执行标准MSI流程，核实注册版本、实际安装1150文件摘要，启动后的新版本身份、Node/SDK路径及完整资源核验；保留实际退出码和UI证据。当前只做CAB展开，未执行`msiexec /a`或任何安装动作。
3. **正式按钮与启用资格。** 实际市场固定来源解析、默认停用安装、明确启用确认、原审批、真实describe快照，再核停用/卸载/同版重装撤销旧资格及等待审批取消；不能用本轮静态布局替代这些UI操作。
4. **确认免费模型预算后真实Qwen闭环。** 验证真实父run/provider编号、原权限、同一根预算与取消、SDK实际回包、失败/重复/迟到拒绝，并核模型收到结果。当前没有项目模型调用，也没有付费、建凭据或改模型安全配置；Goal入口缺完整接纳身份仍拒绝。
5. **原生Browser/Paint等真实GUI。** 本会话没有可调用computer-use入口。慢事件释放、释放期间关闭及旧世代竞争、完整Paint、动画/泛光和总体仍未验；不得由工程或包测试数量替代UI成功。

GitHub上传按用户要求继续搁置；微信不改不测、Devin搁置、Pro补审暂停。原五项编号范围仍未唯一确认，本报告不宣称总体任务已完成。
