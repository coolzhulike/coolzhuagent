# 显式模型类型与媒体失败状态窄修验证

日期：2026-09-27。范围仅为 Web 控制台的模型路由、旧会话类型兼容和媒体结果终态。原缺陷、裁决与限制见 [`2026-09-27-explicit-model-routing-review.md`](../../analysis/2026-09-27-explicit-model-routing-review.md)。0.2.21 的构建与安装证据继续有效，但不包含此修复；本记录验证的是后续源码的开发态二进制，不能将结果回填为 0.2.21 安装版通过。

## 结果与身份

`cargo build -p coolzhu-web-console --offline` 完成，退出码 0。该次开发态 `target/debug/coolzhu-web-console.exe` 的 SHA-256 是 `0EF736167487C4EEE51DF45D522FB60D5F5B78E5BF3CE9FFA8B943E34FDC28ED`。以下真实 Web 场景都运行这份二进制的临时副本；最终前端源码冻结及安装候选包重建后，应以新包身份重新记录，不能沿用这个哈希。

三项定向 Rust 测试分别通过（每项 1 passed）：

| 测试 | 约束 |
| --- | --- |
| `media_route_and_image_input_respect_explicit_model_type` | 含 image/video 的模型名不能覆盖明确 `text`、`vision`、`multimodal`；普通模型名的明确 `image`/`video` 仍生成 |
| `legacy_json_missing_type_infers_without_overriding_explicit_text` | JSON 真正缺少 `model_type` 时兼容推断；明确 `text` 不被改写 |
| `legacy_sqlite_missing_type_remains_inferable_after_schema_upgrade` | SQLite 旧表缺列后连续两次加载均可推断，升级列不会抹掉缺失标记 |

实际 Web 与受控假模型的记录保存在忽略目录 `tmp/media-routing-20260927/47f0713c922d/result.json`，驱动为 `tmp/media-routing-verify-20260927.py`。驱动启动独立临时工作目录、随机 localhost 端口，将 `USERPROFILE`、`HOME`、`APPDATA`、`LOCALAPPDATA`、`CLAW_CONFIG_HOME`、`COOLZHU_RUNTIME_DIR`、`COOLZHU_LOG_DIR` 全部指向临时目录，剔除外部密钥环境变量。未接触默认 `8765`、原生桌面、用户会话库或真实模型 Key。它经真实会话配置、发送、运行状态与视频任务 API 核对了请求路径和终态，而非只调用路由函数。

| 场景 | 实测请求或状态 |
| --- | --- |
| `s0-image-text`，明确 text，`supports_multimodal=false`，发送合成 PNG | 先调用 `fixture-vision` 描述原图，再向 `s0-image-text` 发不含原图而含视觉描述的聊天请求；两条真实模型请求，run `completed`，轨迹用途为 `vision_description` 与 `chat`。原 0.2.21 复现曾误入图片生成且 provider 请求为 0 |
| 同一 text 会话，`supports_multimodal=true` | 一条含原图的聊天请求；run `completed` |
| 明确 text、模型名含 video | 一条 `/v1/chat/completions` 请求；run `completed` |
| 明确 vision 或 multimodal、模型名含 image | 各一条含原图的聊天请求；run 均 `completed` |
| 明确 image、普通模型名，缺 API Key，流式发送 | provider 请求 0，消息准确说明缺 Key，持久化 run `failed` |
| 明确 image、普通模型名，非流式成功 | 一条 `/v1/images/generations` 请求，带图片附件，run `completed` |
| 明确 image，受控 HTTP 503，非流式发送 | 一条图片生成请求，错误消息显示 503，持久化 run `failed` |
| 明确 image，在途慢请求时取消 | 中断入口返回 `interrupt_requested`，持久化 run `interrupted`，未被随后媒体结果覆盖 |
| 明确 video、普通模型名，受控成功 | 一条 `/v1/videos` 请求；父聊天 run `completed`，独立 `video_jobs` 最终 `completed`，产物 URL 为 MP4 |
| 同一 video 会话，后台缺 API Key | provider 请求 0；父聊天 run `completed`，独立 `video_jobs` 最终 `failed` 且记录缺 Key 原因 |
| 新建未指定类型，名称含 image | 创建结果推断 `model_type=image` |

## 实现边界

`agent_media_gen_kind` 只按会话已保存的 `model_type` 分派生成端点。名称推断保留于创建时缺省和真正旧记录缺类型的加载入口；JSON 缺字段不再被 serde 默认 `text` 掩盖，SQLite 旧表补列的默认值改为空字符串，加载器只对原列缺失或空值推断，同时真实读取错误继续上抛。图片输入能力独立按明确 `vision`/`multimodal` 类型及已有 `supports_multimodal` 配置处理，不因模型名包含 image/video 就强制开启，也没有将 `video` 类型臆定为图片输入能力。

图片生成帮助函数返回结构化 `MediaChatOutcome.failed`，流式及非流式入口据此写 `failed`，不解析中文错误文案。取消先发生时仍为 `interrupted`。视频提交与轮询原本就在后台：父 run 的 `completed` 仅代表 pending 消息已接纳，**不代表视频产物完成**。后台缺 Key 的失败落在独立 `video_jobs=failed`；本窄修没有把该异步失败伪装成同步父 run 失败。

兼容性边界：已经被更早版本迁移、并持久保存为 `text` 的旧记录，与用户明确选定的 `text` 无法无歧义区分，本次尊重其存储值，不按模型名称反改。未批量改写用户会话、URL 或密钥。`git diff --check` 退出码 0；现有工作树包含其他任务的长期改动，本记录只对上述 Rust 路由与媒体终态负责。
