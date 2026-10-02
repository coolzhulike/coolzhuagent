# CU 结构化输出及远端检查修复审查

主会话设计、审查、实现；用户已暂停 Pro 复审且要求不使用子代理。原 Qwen 模型、Base URL、密钥及 medium 不变。

## 触发事实与方案

正式 046 的 TEXT-AO、TEXT-AR 两轮真实 qwen3.8-flash 请求在输入前因验收 JSON 语法无效停止；账本均为零动作，页面为空。只在提示词里要求 JSON 不足以保证可解析响应。NAV-AP 同包已完成一次真实导航，证明不能把格式问题混同于全部接口不可用。

依据百炼官方[结构化输出](https://www.alibabacloud.com/help/en/model-studio/qwen-structured-output)，Qwen3.8 Flash 支持 JSON Object/Schema；多模态输入不执行 Schema 约束。官方[工具调用](https://www.alibabacloud.com/help/en/model-studio/qwen-function-calling)说明思考开启时不支持强制 tool_choice object，因此不采用强制工具返回，也不关闭思考。经 agent-reach 网页读取归档于 tmp/browser-use-priority/qwen-*-official.txt。

Provider 适配器新增单次请求 ResponseFormat，与会话参数分离，不改持久配置。047 实际对所有带 schema 的 Qwen3.8 纯文本 CU 请求发送 strict=true Schema，包括根 oneOf 动作规划；图像请求和无 schema 请求发送 JSON Object。此前“无 schema 的规划请求”措辞未准确解释当前规划其实带 schema；047 连续规划停步后，下一轮改为固定验收 Schema、动作规划 JSON Object 的明确分工。其余模型保持原行为，Anthropic 协议不静默接受未实现格式。没有二次模型格式修复请求，也没有动作重放。

格式约束不表示事实正确。既有 deny_unknown_fields、标准数量与索引、证据上限、节点范围、输入前检查、动作回执和验收前后新取样继续裁决；图像仍须来自真实原图。正式新包必须重新实测文字输入、导航及 Paint，工程测试不能代替软件截图。

## 远端失败原因与最小修复

6e1312d 的 PR 检查 36810703102：1288 通过、3 失败、2 既有忽略；同提交 push 检查成功。未通过重跑掩盖失败。

- 数据库预读 user_version 在 busy_timeout 配置前执行，首次并发 WAL 初始化时可能立即 DatabaseBusy。只读预检与新连接都先设置 5 秒等待，再做前向版本检查；未修改业务写入、迁移或前向版本拒绝规则。
- CU 缺来源时原先先登记工具调用、打开默认库，导致上下文拒绝受数据库影响。现在外层派发入口先拒绝缺 provider/session/turn 的 CU，不建立匿名调用、不访问默认库；合法调用仍经原登记、预算、权限及执行链。
- Goal 宿主测试首次请求只等 3 秒，在繁忙 CI 上可能还未结束 SQLite 初始化。首次请求等待改 30 秒，取消后的响应仍保留原独立 3 秒断言。两项会临时替换全局工程/配置的测试改用 Drop 恢复，panic 也不把已删除的临时目录留给后续用例。

不改微信能力、不增加功能夹具，不减少 CI 检查项目。改完离线编译、既有工程回归，然后重新打包安装真实模型验收。最新提交的远端状态须另查，不能继承旧提交成功。

## 本轮工程验证

离线主控制台 build 通过；既有 Web 测试 1291 通过、0 失败、2 项既有忽略；模型适配器 126 通过；tool-registry 离线 check 通过；模块连接 8 通过。日志保存于 tmp/browser-use-priority/structured-output-*.log。没有删除失败用例、没有增加模型响应夹具。随后打包 047，实操结果单独归档；目前不宣称文字输入或 Paint 通过。
