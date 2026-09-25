# 2026-07-03 ClawBot HTTP Provider Adapter 设计规格

## 目标

阶段 4C-2 在现有 `coolzhu-clawbot-sidecar` 内新增可配置的外部 HTTP provider adapter，用来连接真实 ClawBot/iLink 服务。sidecar 继续对 web-console 暴露稳定 gateway 行为，真实 provider 的协议差异集中在 adapter 映射层。

## 配置

- `COOLZHU_CLAWBOT_PROVIDER_KIND=mock|http`：默认 `mock`，设置为 `http` 时启用外部 provider。
- `COOLZHU_CLAWBOT_PROVIDER_URL=http://127.0.0.1:8790`：外部 provider 基础地址。
- `COOLZHU_CLAWBOT_PROVIDER_TOKEN=<secret>`：可选 bearer token，只从环境变量读取，不写入配置文件、不输出到日志。
- `COOLZHU_CLAWBOT_ACCOUNT_ID=<id>`：sidecar 侧默认账号 id，用于补齐 provider update 未带账号的情况。

## 外部 provider HTTP 合约

adapter 使用以下最小合约，真实 ClawBot/iLink 服务可通过薄代理适配：

- `GET /health`
  - 返回 `provider`、`provider_version`、`account_id`、`online`、`last_error`。
- `POST /login/refresh`
  - 请求：`generation`、`account_id`、`now_ms`。
  - 返回 web-console gateway 兼容的 `ClawbotLoginReport`。
- `POST /login/logout`
  - 请求：`account_id`。
  - 返回成功/失败。
- `GET /updates?account_id=<id>&since_ms=<now_ms>`
  - 返回 `updates: ClawbotInboundEnvelope[]`。
- `POST /send_text`
  - 请求：outbox item 的 `account_id`、`peer_id`、`context_token`、`source_external_msg_id`、`body`。
  - 返回 `provider_message_id`。

## 安全与错误策略

- Authorization 使用 `Bearer <token>`，token 只来自环境变量。
- 任何错误文本进入日志、health、fail outbox 前都必须脱敏：移除 bearer、token、password、secret、cookie 样式内容。
- provider URL 缺失或无效时，health 返回 `online=false` 和可见错误，不 panic。
- provider HTTP 4xx/5xx、网络错误、JSON 解析错误都返回可见错误，sidecar tick 会把发送失败反馈给 web-console outbox fail。

## 验收标准

1. `HttpClawbotProvider` 能把 provider health 转成 sidecar health，并正确附带 bearer token。
2. `refresh_login()`、`poll_updates()`、`send_text()` 能访问测试 provider server 并映射 DTO。
3. `send_text()` 失败时错误已脱敏，不包含 token 原文。
4. sidecar main 能根据 `COOLZHU_CLAWBOT_PROVIDER_KIND` 选择 mock 或 http provider。
5. `cargo test -p coolzhu-clawbot-sidecar --offline`、`cargo build -p coolzhu-clawbot-sidecar --offline` 通过。
