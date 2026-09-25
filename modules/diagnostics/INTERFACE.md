# Diagnostics 对外接口说明

## 模块职责

`diagnostics` 负责诊断事件、日志字段、trace id、span追踪和错误上下文。支持文件输出、console输出和GUI实时日志窗口。后续 GUI 日志窗口、server 事件和测试报告应统一消费该模块。

## 对外 crate

- `coolzhu-diagnostics`，兼容 crate alias：`diagnostics`

## 稳定接口

### 初始化

- `diagnostics::init(app: &str) -> io::Result<PathBuf>` - 初始化日志系统，返回日志文件路径
- `diagnostics::log_path() -> Option<&'static Path>` - 获取当前日志文件路径
- `diagnostics::set_gui_callback(callback: impl Fn(&LogEntry) + Send + Sync + 'static)` - 设置GUI回调

### 日志API

- `diagnostics::error(module, event, message, fields)`
- `diagnostics::warn(module, event, message, fields)`
- `diagnostics::info(module, event, message, fields)`
- `diagnostics::debug(module, event, message, fields)`
- `diagnostics::trace(module, event, message, fields)`
- `diagnostics::emit(level, module, event, message, fields)` - 统一日志输出

### Span追踪（新增）

- `diagnostics::start_span(name: &str, module: &str) -> SpanGuard` - 开始一个新的span
- `diagnostics::start_span_with_parent(name, module, parent) -> SpanGuard` - 带父span创建子span
- `diagnostics::current_context() -> Option<SpanContext>` - 获取当前span上下文
- `diagnostics::current_trace_id() -> Option<TraceId>` - 获取当前trace id
- `diagnostics::current_span_id() -> Option<SpanId>` - 获取当前span id

### SpanGuard操作

- `guard.record(key, value)` - 记录span属性
- `guard.event(name, attrs)` - 记录span内事件
- `guard.context() -> SpanContext` - 获取span上下文

### 数据类型

- `diagnostics::LogLevel` - Error/Warn/Info/Debug/Trace
- `diagnostics::LogEntry` - 日志条目结构体
- `diagnostics::SpanContext` - span上下文（trace_id, span_id, parent_id）
- `diagnostics::TraceId` - 16字节唯一trace标识
- `diagnostics::SpanId` - 8字节span标识

## 环境变量配置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| COOLZHU_LOG_DIR | 日志目录 | ~/.coolzhu/logs |
| COOLZHU_LOG_LEVEL | 最小日志级别 | INFO |
| COOLZHU_LOG_CONSOLE | 是否输出console | false |

兼容旧环境变量：
- CLAW_LOG_DIR → COOLZHU_LOG_DIR
- CLAW_LOG_LEVEL → COOLZHU_LOG_LEVEL
- CLAW_LOG_CONSOLE → COOLZHU_LOG_CONSOLE

## 日志输出格式

### Event格式（JSONL）

```json
{
  "ts_ms": 1234567890,
  "level": "INFO",
  "app": "coolzhu-agent",
  "module": "vision",
  "event": "request",
  "message": "ok",
  "trace_id": "abc123...",
  "span_id": "def456...",
  "fields": { "model": "qwen2.5-vl-3b" }
}
```

### Span格式（JSONL）

```json
{
  "ts_ms": 1234567890,
  "type": "span",
  "trace_id": "abc123...",
  "span_id": "def456...",
  "parent_id": "ghi789...",
  "name": "api_request",
  "module": "llm-adapter",
  "duration_ms": 150,
  "attributes": { "model": "glm-5" }
}
```

## 接口变更审查点

- 日志字段变化必须兼容GUI展示。
- 所有跨模块请求应携带trace id。
- 错误必须可序列化为用户可读信息。
- Span结构变更必须向后兼容。
- 环境变量命名从CLAW迁移到COOLZHU，保持兼容。

## 独立验证

```powershell
cargo fmt -p coolzhu-diagnostics
cargo check -p coolzhu-diagnostics --offline
cargo test -p coolzhu-diagnostics --offline
```