# 浏览器目标语义与规划停步修正审查

主会话独立设计、审查和实施，2026-10-01。用户暂停 Pro 复审；不使用子代理。触发事实见 [047 实操报告](../testing/release-0.2.47/change-report-and-targeted-test-plan.md)。

## 决策与范围

AT 的真实目标 JSON 把目的 next.html 放到 target.url，而宿主当前页为 text.html；现有观察目标匹配正确拒绝。工具定义只有“URL”及“浏览器任务用 target.url”，缺少源地址语义，因此补在参数 description、工具总说明、操作约束和内部规划说明。目的地继续由目标文字进入规划动作 arguments.url，不新增顶层 destination 字段、不自动修正错误模型参数、不改变允许导航的范围。

AS、AV、AW 的验收和规划均可解析，但规划 done=true，动作数零；AW 已明确源地址仍停步，因此不能把失败都归因于 AT 的参数问题。047 新协议把带根 oneOf 的规划 schema 也发给供应商。官方结构化输出资料没有明确确认该联合关键字的兼容性；没有 HTTP 400 或无效 JSON 证据，故不能宣称“供应商不支持”或“模型能力不足”是确定原因。

采用可复核的协议分工：固定纯文本验收发送 JsonSchema；动作规划发送 JsonObject，既有动作 schema 仍在提示词中，本地 deny_unknown_fields、动作参数验证、capabilities、最新引用接地和执行前守卫继续裁决。多模态保持 JsonObject，原模型、medium、Base URL 和密钥不变。不引入二次修复请求、自动重放、弱验收或模型切换。

为区别宿主没有引用和模型自行停步，内部诊断只新增 node_count、candidate_count、textbox_candidate_count、root_candidate_count、truncated/input_supported 和类型化能力布尔值。仅对浏览器记录；没有页面正文、控件名、输入值、网址、自由字段、节点/文档凭据或截图。仅精确已知 summary 对应 target_not_found 保留停步码，未知说明继续脱敏。使用现有诊断表，不新建查询入口或正式前端调试信息。

## 风险与验证

- JsonObject 保证 JSON，不保证合法动作。解析或接地失败必须原样停止，不能修补成动作；独立验收继续必要。
- 修改格式可能恢复规划，也可能仍停步；必须正常新包安装、真实 Qwen 输入和导航对照，不能以工程测试结论替代。
- 数量诊断不足以证明具体控件可点击/输入；当前引用、真实焦点、字段/选区与源文档仍由宿主即时检查。
- 已有节点有效期、短执行票据、冻结环境、单次许可、取消仲裁及未知输入隔离均不改。工程测试期间避免与真实 CU 抢共享输入锁。
- 新增一项纯诊断隐私检查，并扩展既有脱敏检查；不新增模型响应夹具。完成离线 build/相关回归后，正常发布、安装身份核验及软件实操单独归档。

此修正不直接处理 Paint、插件市场兼容或其它开放任务；Browser Use 闭环后继续按原优先级验收。

## 工程结果

最终主控制台离线 build 通过；完整 Web 1292 通过、0 失败、2 既有忽略，另 lib 8 项及原生宿主接线 1 项通过。诊断隐私与既有脱敏定向检查通过。日志位于 tmp/browser-use-priority/navigation-final-*.log；这些检查不代表新包 Browser Use 实操通过。
