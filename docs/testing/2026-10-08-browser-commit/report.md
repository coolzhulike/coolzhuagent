# 跨来源提交窄时序：正式098新增实测

继续原SWE-2-medium/原聊天室/唯一island-kayak。单次真实perform、max_actions=1；网页可信pointerdown内location.replace到localhost，并有2400ms处理代码。未修改宿主down/up顺序、延迟或超时来制造通过，也未人工代点目标。

实际HTTP导航和释放通过：CU succeeded/goal_achieved=true，唯一click sent/released/effect_observed，父run completed、单attempt end_turn/drained，唯一远端解锁、internal仍空。新文档脚本在pointerdown后82.4ms执行，新文档无pointerdown/pointerup/click/input/keydown；[真实结果](result.json)、[网页事件](events.jsonl)、[正式页面实拍](after.jpg)。

**严格“提交恰在pointerdown与pointerup之间”仍不计通过**：两文档均未捕获pointerup或down-handler-finished，原文档可能已被卸载；宿主released只能证明成功释放，不能给出对应DOM事件的精确时刻。页面新脚本证明新文档已存在，不能单凭它判断与释放的先后。模型页面目标通过与这一证据缺口分开记录。

产品down/up在同一UI闭包内立即顺序入队，不等待down回调才释放。后续不通过人为延迟up、扩大超时或修改安全记录来满足时序；需独立可观察的事件事实才能关闭该窄窗口项。新nativeTarget替换也仍开放。采证脚本若重放应复制到tmp，使用自有工作区；不是模型夹具。记录器在本轮收尾正常关闭。
