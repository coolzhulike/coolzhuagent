# 正式102大工具回执：失败，不计通过

marker `INSTALLED102-LARGE-RESULT-NO-READER-20261008`，原SWE-2-medium/唯一island-kayak。实际独立HTTP资料44498字节，随机尾文和两数字仅在服务器资料末尾，不在用户提示词。目标是net_fetch→读取尾文→真实calculator；只实际执行一次net_fetch。

宿主HTTP200/truncated=false、工具completed，结构化回执原文111227字节/89751字符；SHA `c98632da51978f4b2b33c1bb208839d1c4f605c27d72684afa7132c4253e52a4`。磁盘仅新增一个原文对象，body与服务器44498字节完全一致，独立GET一次。真实SWE却报告下游输出截短、读不到final_record；未猜测、未调用calculator。模型报告的具体截短长度不是宿主独立测量，不据此断言Devin的固定协议上限。

运行 `run-chat-7dd5725db1b611e62439e17c0ebbe021163ba697c8ae38ec` completed；ACP `01252e01be5eab0b0df6673fb93105d33b131503d5b3a208` end_turn/drained。**运行正常结束不等于业务验收通过**。可见#695/#696、37.8秒。临时net工具撤回、插件停用、revision41→42→43，其余参数保持、唯一绑定解锁。未读模型思考。

[正常软件失败实拍](formal-tail-unavailable.jpg)、[最终回复/宿主事实](result.json)、[完整原文](original-tool-result.json)、[独立服务器资料](body.json)、[逐字节核验](verification.json)、[独立HTTP事件](network-events.jsonl)、[恢复](restored.json)。归档脚本依赖原tmp层级，不可从此目录重放。

改进方案：[本轮工具回执只读分页](../../../analysis/2026-09-21-integration-review/acp-tool-result-pagination-design-2026-10-08.md)。新增独立result_pages模块，复用内容寻址存储；ACP只读本轮已执行工具回执、原工具资格复核、不接受任意路径。offline build通过55.31秒；完整控制台1422通过/0失败/6既有忽略、lib8/nativehost1通过，主回归42.81秒。此为源码候选，尚未正常出包和真实复验，不追认102或103大结果通过。
