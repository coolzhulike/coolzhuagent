# 统一模型会话参数配置

## 原有结构与问题

- Web Console 的会话身份（名称、模型、旧 provider 标签、密钥引用、思考层级）保存在 SessionStore / SQLite。
- `coolzhu.toml` 的 `session_model_limits` 原本只保存每会话上下文与输出容量；模型请求仍依赖适配器的 provider / model 目录。
- 旧前端通过 provider 选择生成模型下拉框，未知模型缺少可直接控制的参数面。未知模型的思考设置又会被能力目录安全省略，因此仅增加一个思考下拉框不能解决问题。
- 后端 `provider_client_for_agent` 统一创建实际请求客户端；流式与非流式调用最终都进入两个原生协议适配器。

## 本轮实现

- 新增独立的 `src/model_settings.js` 与 `src/model_settings.css`。统一配置名称、模型 ID、协议、Base URL、Endpoint、密钥、模型用途、容量、温度、Top P、思考编码、思考层级或预算、工具调用开关、电脑操作开关、工具范围及可选清单。
- 模型 ID 为自由输入，允许最长 1024 字符，兼容本地模型完整路径。旧密钥编辑上限从 128 提高至 4096，已有密钥留空保留，不回显明文。
- 新增 `GET/POST /api/sessions/{session_id}/model-settings`。POST 格式为 `{ session?: UpsertSessionRequest, parameters: SessionModelLimitOverride }`，集中验证范围及协议组合。
- 扩展原 `session_model_limits` 项，保留旧 TOML 读取兼容。旧 `/model-limit` API 只修改容量，不能擦除新增字段。
- 底层保留旧 provider 身份用于迁移、环境变量密钥与默认思考策略。用户界面不再要求维护 provider 配置表。
- 请求参数由 `ProviderClient.with_request_parameters` 传入真正的协议客户端，两种流式路径与非流式路径均生效，不修改所有调用方的 `MessageRequest` 结构。
- 显式协议为 OpenAI Chat Completions 或 Anthropic Messages。显式 Endpoint 在客户端按完整路径使用，避免再次追加协议路径。
- 思考模式 `auto` 沿用能力目录；显式 `effort` / `thinking` / `adaptive` / `budget` 使用相应原生字段，因此未知新模型不受目录白名单限制。
- Anthropic 思考预算、采样互斥和协议组合会在保存前验证。显式模型参数由用户按服务端支持能力填写。
- 本地服务容量仍作为上限；用户填写更小的上下文或输出预算会真正生效。

## 集成接口

```javascript
const settings = window.CoolzhuModelSettings.mount(container, {
  sessionId: activeSessionId,
  onSaved(session, result) { /* 更新主界面会话列表 */ },
});
```

返回 `refresh(id)`、`select(id)`、`newSession()`、`destroy()`、`sessionId`、`dirty`。保存后也会冒泡 `model-settings-saved` 事件。切换编辑对象不会激活运行会话；新建会话沿用原创建 API 的激活行为。

## 验证

- `node --check src/model_settings.js` 通过。
- `cargo test -p coolzhu-llm-adapter --offline`：92 单测和 19 集成测试通过，1 个既有真实云端测试因需要密钥与外网而忽略。
- 新增真实 localhost 请求测试：两个原生协议均验证未知模型 ID、精确 Endpoint、正确鉴权及思考 / 采样请求体。
- Web Console 新增 `unified_model_settings_tests` 四条，覆盖旧配置兼容、采样/协议/预算校验、长本地模型路径、URL 限制与本地容量约束。
- 发现既有 overview 指标测试未持有共享状态测试锁，两次读取期间可能遇到其它测试临时替换 SessionStore；已补充同一 `config_test_guard`，不改变业务统计逻辑。
