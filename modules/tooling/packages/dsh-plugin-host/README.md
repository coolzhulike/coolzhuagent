# DSH 工具插件宿主

使用锁定的官方 Cordis、ToolRuntime 与 SystemPrompt 服务加载单个 DSH 工具模块。Coolzhu 仍拥有模型循环、会话、上下文、记忆、权限、截止和轨迹。此目录已有可执行的库与一次性进程协议，**尚未接入正式市场安装按钮，也未装入 MSI**；不能把工程探针当成真实 Qwen 市场验收。

## 来源与支持范围

`package-lock.json` 固定17个官方依赖及完整性。安装依赖必须使用 `npm ci --ignore-scripts`，不执行第三方安装脚本。运行需要 Node22.19或24及以上；正式部署需随包固定可核验的 Node，不能依赖本机全局环境。

首个验证对象为真实 `@deepseek-ai/dsh-tool-calculator@0.0.1`，固定源码 commit `b2007a13f06bcf75bf07b9d277ee8d434a316490`。仅支持单个 object module、自有 apply、匹配包名、依赖 tools 的工具模块。未知服务、组合 profile、provider、UI 或 AgentLoop 依赖明确拒绝，不提供空服务维持兼容。

安装回执包含 protocol=1、name/version、entry、sdk_lock_sha256 和每个来源文件的 path/sha256。路径禁止逃逸与链接；实际注册的工具才进入清单，每次执行重新核对来源修订。安装器需另完成全树文件身份、依赖包完整性、事务与默认停用；此宿主不自行下载或安装。

## 接口与生命周期

- `src/host.mjs` 导出 loadPluginHost，返回实际 manifest、execute 和 dispose。execute 要求本宿主 generation/revision、唯一 call_id 和真实 AbortSignal；来源校验开始前锁住单次执行资格，停用期间拒绝新输入。
- `src/process.mjs` 接收私有调用目录，request.json 冻结 mode、nonce、context(workspace/room/run/call)、deadline、root、receipt、config。握手 manifest.json 后，execute.json 绑定本次 live 宿主世代。cancel.json 触发 AbortController；result.json 在插件资源释放后发布。所有消息上限256KiB、原子发布，stdout 不承载协议。
- Rust `runtime::dsh_host_process` 通过受控进程 Job 调用。每次新进程的世代不同，执行前比较冻结来源修订、包身份和完整工具定义，再使用本次握手世代。取消优先于迟到结果，最多1秒协作收尾后关闭并回收整个 Windows 进程树；不自动重放。
- 子进程环境只保留系统和临时目录变量，不继承模型密钥、NODE_OPTIONS 或 NODE_PATH。进程边界与文件身份核验**不是操作系统沙箱**；启用第三方代码的批准、工具权限和外部副作用由上层实施。

## 工程核验

`tests/real-plugin.mjs` 必须输入真实固定来源插件目录，核验官方工具定义、计算96、无效表达式、预取消、世代/修订/重放拒绝、校验期间并发及停用、真实 fiber/service 释放。它不造 SDK 或模型。

Core 的 `dsh_plugin_probe` example 接收 Node绝对路径、process.mjs绝对路径、真实插件目录、来源回执JSON和输出文件，驱动同一个生产进程桥，核验实际计算96、冻结修订变更拒绝和根预取消。当前这里只验证工程链路；市场事务、正式资源打包、停用/卸载按钮、真实Qwen工具暴露和调用仍需随后完成。
