# UI04 与发送目标：隔离源码构建页面实操

日期：2026-09-27。此处是源码构建二进制的独立浏览器会话，不是用户已安装的 0.2.17 原生窗口。二进制 SHA-256 为 `DFD39612CD2D689E6CF4E87DE9B0C421682D1BE33137D854A4CE8F5D2479D8CE`；运行目录分别位于 `tmp/ui04-stream-572736e2eb` 和 `tmp/ui04-stream-8664f8940a`，测试服务均已关闭。模型为本地 OpenAI 兼容夹具，没有调用云模型。详情见 [源码构建身份](source-build-identity.json) 和不含密钥、正文的 [请求形状](model-request-shapes.json)。

## 页面与模型闭环

- 顶栏清空发送目标后点击发送，页面提示“尚未选择发送对象，请先在顶栏选择”，消息保持未发送；见 [01](01-empty-target-blocked.png)。随后才选择目标 B 并发起夹具的第一笔模型请求，空名单没有暗发默认目标。
- 只勾选 B 时，顶栏与发送按钮均显示 B，设置仍可编辑 A，浏览设置不改发送目标；多选 A、B 时顶栏同时列出两者。顶栏明确点击 B 单目标后，编辑对象同步为 B，再从设置选择 A，顶栏仍保持 B；见 [02](02-send-b-edit-a.png)、[03](03-multi-target-label.png)。
- 以 A 为当前管理会话、B 为唯一发送目标，隔离聊天实际调用 `read_file`，收到工具结果后模型用非流式续轮返回最终正文；页面显示 B 的回复，右栏轨迹保留 `read_file` 的完成记录；见 [04](04-tool-final-and-trace.png)。请求形状显示首轮 `stream=true`、续轮 `stream=false`，且续轮带 `assistant` 与 `tool` 消息。此处只验证本地假模型与项目宿主链，不能代替真实供应商验收。
- 下一笔请求中，夹具按一秒间隔流式发送五行 `reasoning_content`。浏览器逐次观察到 1、2、3、4、5 行；见 [05](05-thinking-line-1.png)、[06](06-thinking-line-5.png)。正文出现且回合完成后，运行过程区域隐藏、其文本清空，正文仍可见；见 [07](07-final-no-live-stream.png)。右栏独立历史展开后仍保留五行思考，先前工具轨迹也仍在；见 [08](08-history-thinking-retained.png)。
- 在第三个流式回合仍进行时尝试切房间，页面明确提示“本轮正在运行，请等待结束或中止后再切换环境”，没有允许带旧过程跨房间。回合结束后重新打开切换器可进入空房间，旧正文和临时过程均不显示；见 [09](09-switch-request-while-running.png)、[10](10-other-room-clean.png)。`read_file` 仅耗时约 8 毫秒，工具运行中的临时提示未单独截屏；此处验证的是完成后的隐藏与独立轨迹保留。

首轮夹具曾错误地对 `stream=false` 的工具续轮返回 SSE，页面因此显示“模型未返回最终文字总结”。[失败截图](12-first-fixture-protocol-mismatch.png) 与 [首轮请求形状](first-fixture-request-shapes.json) 保留此事实。修正夹具的续轮响应格式为普通 JSON 后，同一源码构建的工具和正文闭环通过；该失败不归因于产品代码。`source-build-identity.json` 中的夹具描述字段仍沿用首次文字，实际顺序以请求形状和本报告为准。

## 输入安全恢复文案：只读网络夹具

本轮源码在 `main.rs` 的 `InputSafetyRecoverySurface` 新增只读 `resource_state` 与 `accepts_new_input`，`app.js` 的 `describeInputSafetyRecovery` 新增当前输入事实与“待本人复核”提示。独立浏览器以 [明确的模拟响应](11-recovery-network-fixture.json) 替换只读 `/api/system/attribution-and-recovery`：历史未收敛 2、待本人复核 2、当前 scope 阻断 0、资源 `unknown`、`accepts_new_input=false`。页面实际显示“待本人复核 2 … 当前输入仍隔离（资源状态：未知）”；见 [11](11-recovery-network-fixture.png)。这是展示逻辑的网络夹具验证，不是真实用户工作区数据重验，更不是恢复成功或输入获放行。没有调用恢复 POST、原生确认或修改安全库。

本次没有改动测试对象之外的已安装 8765 服务、用户聊天室、Paint 画布，也没有提交代码。
