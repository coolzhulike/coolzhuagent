# 正式104：ACP大工具回执分页与真实尾文续接

本版解决正式102发现的实际失败：插件HTTP响应和磁盘原文完整，但Devin下游截短大回执，SWE看不到尾文，后续计算没有执行。正式104改为本轮回执分页，SWE通过只读接口取得尾文并完成计算。102失败记录保持原样，不能追认旧包通过。

冻结源码 `6fad837f7804591823386280ebc5c54b27c471ce`，快照 `56d0a7f71decacf61a353e3f6817d03fd75e05b2e4f0e14ecd99fab18581c471`。正常发布链真实子进程退出0、六门pass；正常MSI安装退出0，Program Files内1159个文件长度及SHA逐一匹配。MSI 285806088字节，SHA256 `cbd349c247c204612ce2b524b18bb44565ffe00b31f57542191a42b96ece8b9e`。默认不启用Browser诊断，原工作区、房间权限和安全库保持。

## 代码职责与行为

`devin_acp/result_pages.rs`负责原文对象登记及只读分页；`bridge.rs`仅声明接口、检查本轮资格并投影结果。超过6000字节的工具回执保存一次，返回带摘要、长度、偏移的首段；`tool_result_read(result_id, offset)`只能读取本轮已执行工具的固定回执，不接受文件路径，不重新执行原工具。分页按UTF-8边界切分，并限制编码后的JSON页大小。原文损坏、非法偏移、异轮ID或资格撤回明确拒绝；保存失败明确返回完整结果及下游仍可能截短的提示。

不新增第二套工具执行账本，不扩大read_file权限；原工具执行状态与isError保持。其它HTTP Provider的大结果完整回退仍属原实现，不将本版ACP分页外推为全部Provider均已闭环。[设计与边界](../../analysis/2026-09-21-integration-review/acp-tool-result-pagination-design-2026-10-08.md)。

## 实际功能验收

正式原生控制台、真实 `swe-2-medium`、唯一既有远端 `island-kayak`；无模型夹具、无新云端会话。提示词不包含随机尾文及两个随机数字。自有HTTP资料端点只供应实际插件响应，不替代模型或工具。

- net_fetch实际GET **1次**，HTTP200、truncated=false。响应体 **44498字节**，与独立资料文件逐字节一致；240条records后包含中文/补充平面字符/emoji和随机final_record。
- 完整工具回执 **111227字节**，SHA `3b5e5ad706d74d34ba72c51c47bc6ffcd833577a105c8dc4cfdfaf22cace1d50`。独立`tool.result_page_read`事件记录offset107200→111227，成功读到尾部；未重复抓取。
- 模型准确回复随机sentinel `竹林𠮷😀-aedf59f45e6e719e51c028a4`、11450、13061，调用calculator一次，最终149548450与独立算式一致。两个工具账本均completed；read_file调用0，桌面/Browser动作0。
- 单run `run-chat-dba317fc61513df2bceb7e00b697ecd706b384ba8cc542b9` completed；唯一ACP `aa952b6fe30a372d5c1d6b71d6aef101474bdcfd0bf0c6e2` end_turn、process_drained=1、唯一绑定解锁。源消息#697、最终回复#698，实际约60.3秒，无需刷新显示。
- 排空后正常撤回临时net_fetch白名单、停用验收插件、核原源码SHA，revision44→45；其余参数与执行前一致。资料服务器正常停止。

[正式软件实拍](installed-validation/large-result-tail.jpg)、[独立业务核验](large-result-pagination/verification.json)、[原事实与最终回复](large-result-pagination/result.json)、[真实HTTP网络事实](large-result-pagination/network-events.jsonl)、[完整工具原文](large-result-pagination/original-tool-result.json)、[恢复状态](large-result-pagination/restored.json)、[安装文件核验](installed-validation/installed-104-verification.json)、[发布链退出码](installed-validation/build-result.json)。

证据限制：模型最终回复把111227字节工具回执误称为HTTP响应体，正确HTTP响应体为44498字节；模型转述非法UTF-8偏移曾被拒，没有独立拒绝事件，不追认其为正式独立负例。小工具正文未另存：独立账本证明calculator完成，数值由最终回复与独立算式核对，不宣称另有持久化完整calculator回执。首次采证脚本误假设tool_calls有result_json列而失败，未改业务数据；随后读取实际schema并修正证据口径，最终核验通过。

## 已完成源码检查及后续测试设计

offline build通过；完整控制台 **1422通过、0失败、6项既有忽略**，库8项与nativehost1项通过。两项必要宿主测试覆盖Unicode/控制字符/转义的逐字还原及编码页额度、异桥ID/非法偏移/损坏拒绝、短结果不落对象和保存失败完整回退；未新增模型夹具。[原始编译及测试日志](../2026-10-08-acp-result-pages/)。

后续测试应分别设计本轮资格撤回、取消/预算耗尽、其它Provider完整回退、对象GC引用关系及压缩/图片内容场景，不以本轮尾部成功覆盖全部矩阵。Browser新原生Target替换与跨来源commit严格down/up窗口仍未命中，不能算通过；其它未完成项以四台账为准。Paint免测、微信不动、Opus暂停、无子代理。整体Goal继续。

## GitHub交付

[0.2.104预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.104)已公开；MSI、installer-report、package-safety和MSI.sha256四资产服务端长度及SHA均与本地一致，实际标签指向冻结源码6fad837。未签名、不标latest、不发布自动更新清单。[服务端原始元数据](installed-validation/github-published-metadata.json)、[实际标签](installed-validation/github-tag.json)。构建源码两路CI在本次上传时仍运行中，不提前标成功。

证据目录单独声明保留原字节；只重新暂存104的三个证据子目录，未全仓归一化。Git暂存中的34份安装与业务证据已逐长度及SHA对照独立清单，避免换行转换使原始资料不可复核。
