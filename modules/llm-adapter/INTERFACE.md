# LLM Adapter 对外接口说明

## 模块职责

`llm-adapter` 统一智谱、阿里、百度、字节、DeepSeek、OpenAI-compatible 等模型供应商接入，向核心运行时提供统一请求、响应、流式事件和工具调用格式。

## 对外 crate

- `coolzhu-llm-adapter`，兼容 crate alias：`api`

## 稳定接口

### 核心类型

- `api::ProviderClient` - 模型客户端枚举
- `api::MessageRequest` - 请求结构
- `api::MessageResponse` - 响应结构
- `api::InputContentBlock` - 输入内容块
- `api::OutputContentBlock` - 输出内容块
- `api::ToolDefinition` - 工具定义
- `api::StreamEvent` - 流式事件

### Provider相关

- `api::ProviderKind` - Provider类型枚举
- `api::ProviderOption` - Provider选项（deprecated，使用ProviderInfo替代）
- `api::ProviderMetadata` - Provider元数据（deprecated）
- `api::ProviderInfo` - 新Provider信息结构

### Model相关

- `api::resolve_model_alias` - 模型别名解析
- `api::provider_catalog` - Provider目录（deprecated，使用ModelRegistry替代）
- `api::ModelInfo` - 模型信息结构
- `api::ModelCost` - 模型价格结构
- `api::ModelLimit` - 模型限制结构
- `api::ModelModalities` - 模型输入输出类型
- `api::ModelStatus` - 模型状态

### Registry相关（新增）

- `api::ModelRegistry` - 模型注册表
- `api::ResolvedModel` - 解析后的模型信息

### Config相关（新增）

- `api::AdapterConfig` - 配置结构
- `api::ProviderConfig` - Provider配置
- `api::ModelConfig` - 模型配置

## ProviderKind枚举成员

| Provider | 描述 | API Key环境变量 |
|----------|------|----------------|
| ClawApi | Anthropic官方API | ANTHROPIC_API_KEY |
| Anthropic | Anthropic官方API | ANTHROPIC_API_KEY |
| Xai | xAI Grok | XAI_API_KEY |
| OpenAi | OpenAI GPT | OPENAI_API_KEY |
| ZhipuAi | 智谱官方API | ZAI_API_KEY / BIGMODEL_API_KEY |
| AlibabaBailian | 阿里百炼代理 | DASHSCOPE_API_KEY |
| BaiduQianfan | 百度千帆 | QIANFAN_API_KEY |
| ByteDanceArk | 字节豆包 | ARK_API_KEY |
| DeepSeek | DeepSeek | DEEPSEEK_API_KEY |
| Custom | 自定义OpenAI兼容 | CUSTOM_API_KEY |

## 接口变更审查点

- Provider DTO 字段变更必须保持向后兼容。
- 新增 provider 必须进入 ProviderKind 枚举和 ModelRegistry。
- API key、base_url、model alias 解析不允许泄漏到 core/gui 模块。
- 新增 ModelInfo/ProviderInfo 类型不影响现有 resolve_model_alias 行为。
- deprecated 类型（ProviderOption、ProviderMetadata）保留向后兼容。

## 独立验证

```powershell
cargo fmt -p coolzhu-llm-adapter
cargo check -p coolzhu-llm-adapter --offline
cargo test -p coolzhu-llm-adapter --offline
```

## 配置文件支持（新增）

配置文件路径优先级：
1. `$HOME/.coolzhu/config.json` 或 `$USERPROFILE/.coolzhu/config.json`
2. `./coolzhu.json`
3. `./.coolzhu/config.json`

配置文件示例：
```json
{
  "providers": {
    "alibaba-bailian": {
      "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1"
    }
  },
  "models": {
    "glm-5-bailian": {
      "provider": "alibaba-bailian",
      "api_model_id": "glm-zhipu-5"
    }
  }
}
```