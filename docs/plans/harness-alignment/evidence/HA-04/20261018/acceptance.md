# HA-04 逐条验收（2026-10-18）

五条确定性合同已满足，完整命令、实际 UTC、输入哈希与退出码保存在 `runs/*/receipt.json`，原始输出及虚构 wire 数据与之同目录。历史 `../request-binding.md` 保持原样，由本记录和继续实施决策说明当前状态。

| 条目 | 实际结果 | 可复核证据 |
|---|---|---|
| HA04-01 | 普通/SSE各自暂停在服务端已收到请求后，再新增bash；旧响应的bash均未进入执行器、无文件效果。旧发送read_file集合随首响应进入共享循环；新的用户请求取得bash及temperature=0.5。started仍表示接纳，request_bound先于模型内容报告。 | web-regression-03/route-{false,true}-Add、同名-next，ha04_tests；实现patch |
| HA04-02 | OpenAI两入口及Anthropic普通/SSE真实HTTP body逐字段校验：模型、stream、工具schema/tool_choice、temperature/top_p及推理参数转换/删除均反映最终payload。摘要不保留prompt/schema正文或密钥。 | web-regression-03的raw_http_bodies；adapter-tests-01/anthropic-wire-{false,true}.json |
| HA04-03 | 两入口429/503成功：首尝试+重试共用logical ID、不同attempt；反馈为新ID并关联父请求。503耗尽为3次attempt且无效果。旧响应拒绝后的无工具重新采样只有一次、新ID/正确父链。Agent子请求只接纳1个人类输入，子调用1次、只获read_file且继承同一根截止；既有子执行取消/期限/重复拒绝测试通过。 | 14场景audit.json；Retry/Exhaust/Add JSON；actual-child-request.json；host_child_agent、identity/settlement回归 |
| HA04-04 | 两入口分别权限撤销、工具停用：收到请求→变更提交→释放旧响应→宿主拒绝，执行器进入=0、文件效果=0；未撤销对照进入=1、效果=1。撤销可先登记1条调用，但登记不等于执行。 | Revoke/Disable/Control JSON中的events及host_tool_receipts；require-confirm或会话策略拒绝 |
| HA04-05 | ACP真实桥接身份、权限、取消/重复和会话生命周期通过；订阅原生工具入口明确UnsupportedCapability，文本流完成/断流/取消保持区分；CLI两项部分流取消/期限通过。 | web-regression-03中的5项bridge及session/订阅测试；subscription-unsupported.json；cli-regression-01 |

验证：Web 70/70，adapter新增2/2、既有5/5，CLI2/2；adapter另2项live账户测试ignored。实际离线构建通过。测试未使用真实账号、付费模型或日常8765服务。HA-11质量/耗时结论仍未验证。

早期失败未删除：compile-01为旧fixture类型不匹配；web-tests-01为子角色请求不允许工具及误把登记数当执行数；web-regression-02为回执枚举断言require-approval与实际require-confirm不符。修正后web-regression-03无失败。日志含隔离诊断目录拒写提示；没有用诊断日志冒充业务成功，业务回执由独立DB及导出文件校验。

实现只在实际请求发送时摘要一次，沿既有observer/usage链保存关联；不增加审核模型请求或逐token摘要。SSE恢复请求的持久账本可追踪；不承诺每个重试/恢复都新增客户端事件。此项不赋予安装或发布授权。
