# DSH 工具插件宿主

固定运行资源、完整290文件信任表、Core核验与实际复制接线已工程通过；仍未打入正式MSI，也未接入市场/Qwen启用。详见[固定运行时后续审查](../../../../docs/analysis/2026-10-01-dsh-fixed-runtime-review.md)。

固定远程源码下载后续已在同一个真实官方包上工程核验：完整22文件和Git树身份核验、默认停用事务安装、实际计算96，404/下载中取消确认清理并保留旧状态。Windows扩展路径的Node入口崩溃已直接复现并修复。详见[远程下载审查与下一步接线](../../../../docs/analysis/2026-10-01-dsh-remote-download-review.md)。来源解析、正式按钮、固定运行时分发及真实Qwen仍未接入；0.2.63安装包不含这项底座。

使用锁定的官方 Cordis、ToolRuntime 与 SystemPrompt 服务加载单个 DSH 工具模块。Coolzhu 仍拥有模型循环、会话、上下文、记忆、权限、截止和轨迹。此目录已有可执行的库与一次性进程协议，**尚未接入正式市场安装按钮，也未装入 MSI**；不能把工程探针当成真实 Qwen 市场验收。

## 来源与支持范围

`package-lock.json` 固定17个官方依赖及完整性。安装依赖必须使用 `npm ci --ignore-scripts`，不执行第三方安装脚本。运行需要 Node22.19或24及以上；正式部署需随包固定可核验的 Node，不能依赖本机全局环境。

首个验证对象为真实 `@deepseek-ai/dsh-tool-calculator@0.0.1`，固定源码 commit `b2007a13f06bcf75bf07b9d277ee8d434a316490`。仅支持单个 object module、自有 apply、匹配包名、依赖 tools 的工具模块。未知服务、组合 profile、provider、UI 或 AgentLoop 依赖明确拒绝，不提供空服务维持兼容。

安装回执包含 protocol=1、name/version、entry、sdk_lock_sha256 和每个来源文件的 path/sha256。路径禁止逃逸与链接；实际注册的工具才进入清单，每次执行重新核对来源修订。安装器需另完成全树文件身份、依赖包完整性、事务与默认停用；此宿主不自行下载或安装。

`source_imports.mjs` 使用 Node 官方同步模块钩子，将首批支持的 SDK 导入固定到宿主自身依赖，包内相对导入必须属于核验回执，加载使用核验后的原始字节。安装目录不需要自己的 node_modules，也不使用工作区的同名依赖。未知裸包、Node内置模块与未核验文件导入暂不兼容；这是来源绑定，不是第三方代码的安全沙箱。模块钩子加载失败及 dispose 收尾时撤销。[Node官方接口](https://nodejs.org/api/module.html#moduleregisterhooksoptions)标注版本与稳定性，当前工程实测 Node24.15.0，正式分发仍需固定并核验完整运行时。

## 接口与生命周期

- `src/host.mjs` 导出 loadPluginHost，返回实际 manifest、execute 和 dispose。execute 要求本宿主 generation/revision、唯一 call_id 和真实 AbortSignal；来源校验开始前锁住单次执行资格，停用期间拒绝新输入。
- `src/process.mjs` 接收私有调用目录，request.json 冻结 mode、nonce、context(workspace/room/run/call)、deadline、root、receipt、config。握手 manifest.json 后，execute.json 绑定本次 live 宿主世代。cancel.json 触发 AbortController；result.json 在插件资源释放后发布。所有消息上限256KiB、原子发布，stdout 不承载协议。
- Rust `runtime::dsh_host_process` 通过受控进程 Job 调用。每次新进程的世代不同，执行前比较冻结来源修订、包身份和完整工具定义，再使用本次握手世代。取消优先于迟到结果，最多1秒协作收尾后关闭并回收整个 Windows 进程树；不自动重放。
- 子进程环境只保留系统和临时目录变量，不继承模型密钥、NODE_OPTIONS 或 NODE_PATH。进程边界与文件身份核验**不是操作系统沙箱**；启用第三方代码的批准、工具权限和外部副作用由上层实施。

## 工程核验

`tests/real-plugin.mjs` 必须输入真实固定来源插件目录，核验官方工具定义、计算96、无效表达式、预取消、世代/修订/重放拒绝、校验期间并发及停用、真实 fiber/service 释放。它不造 SDK 或模型。

Core 的 `dsh_plugin_probe` example 接收 Node绝对路径、process.mjs绝对路径、真实插件目录、来源回执JSON和输出文件，驱动同一个生产进程桥，核验实际计算96、冻结修订变更拒绝和根预取消。当前这里只验证工程链路；市场事务、正式资源打包、停用/卸载按钮、真实Qwen工具暴露和调用仍需随后完成。

Plugin-system 的 `install_dsh` 已复用现有跨进程写锁、同卷暂存/交换和恢复日志。只复制回执中的来源文件并生成独立 DSH 登记，不造原生进程工具、执行 npm 脚本或默认启用；固定来源不能通过普通更新入口转为浮动版本。`dsh_install_probe` 以真实官方包验证安装、静态文件身份及摘要失败时旧状态不变；从实际安装后的新目录通过同一 Rust/Node 进程桥计算96。远程下载、正式市场按钮、宿主启用快照和真实模型调用仍未接入，因此不能把该静态安装入口称作完整市场安装能力。
