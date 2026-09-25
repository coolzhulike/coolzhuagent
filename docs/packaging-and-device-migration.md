# 打包、安装与跨设备迁移方案

更新日期：2026-05-06

推进状态：已完成 NSIS、在线激活、机器指纹绑定、资源校验/加密方向的方案落档；实现阶段冻结到 `docs/requirements-management.md` 的 G1 交互性测试闭环和 G2 第三梯队落地验证全部完成之后。冻结期间仅维护方案文档、风险评估和交互测试记录，不推进安装器、授权服务或资源镜像代码实现。

## 目标

让 COOLZHU AGENT 可以从开发目录迁移为可安装程序，并支持迁移到其他 Windows 设备后继续使用：

- 一键启动 Web GUI、后端服务和桌宠。
- 保留会话、beads 记忆、附件索引、模型配置引用和日志。
- 不泄露明文 API key。
- 无 Rust、Node、源码环境的新机器也能运行。

## 推荐方案

第一阶段采用 NSIS Windows 单机安装包，WiX 保留为后续 MSI/企业部署备选：

| 组件 | 打包策略 | 说明 |
| --- | --- | --- |
| `coolzhu-web-console.exe` | Rust release 二进制 | 本地 HTTP 服务和静态资源入口 |
| Web 静态资源 | 随 exe 同目录或嵌入包 | 当前代码使用静态文件服务，需要固定资源路径 |
| `coolzhu-tauri-shell.exe` | Tauri release 二进制 | 桌宠和 WebView 控制台壳 |
| 桌宠资源 | 随 Tauri `ui/assets` 打包 | 动画帧、气泡、图标 |
| 模型资源 | 内视觉必需资源随包/可选资源分包 | 内视觉 VLM 为 Web UI 视觉验证必需能力，后续打包阶段处理随包内置、自启动和资源加密；TTS/STT 等非首屏必需资源可按需下载或单独资源包 |
| 用户数据 | `%APPDATA%\CoolzhuAgent` | 会话、beads、附件、日志、配置 |
| 诊断工具 | 随包 | 用于首次启动和迁移后体检 |

安装器候选：

| 方案 | 优点 | 风险 | 建议 |
| --- | --- | --- | --- |
| NSIS | 简单、脚本灵活、适合快速出包；适合前置环境检测、可选组件、在线激活脚本编排 | 需要自行管理升级、卸载、权限和签名细节 | P1 已确认首选 |
| WiX Toolset | Windows Installer 规范、升级能力强 | 学习和配置成本较高 | P2 作为 MSI/企业部署备选 |
| Tauri bundler | 与桌面壳天然集成 | 当前主后端是独立 Rust Web 服务，需额外打包后端 | 可作为桌宠壳打包参考 |
| 便携 zip | 最快验证迁移 | 开始菜单、服务、升级体验弱 | 开发验收备用 |

## NSIS 安装流程草案

1. 安装器启动后执行 preflight：检测系统、硬件、WebView2、网络、端口和写入权限。
2. 将检测结果分级为 `blocker`、`warning`、`optional`：阻断项必须修复后继续，提醒项可继续安装，可选项进入组件选择。
3. 用户选择安装目录、数据目录和可选资源包；大体积模型资源默认不内置，优先按需下载。
4. 解压并校验 Web 后端、Tauri shell、桌宠资源、静态资源和诊断工具。
5. 首次启动前进入在线激活：输入账号/授权码，服务端校验设备槽位并下发签名 license token。
6. 写入开始菜单、卸载信息和本地日志路径，启动首次诊断向导。

## 安装前环境与硬件检测

检测项分为必需能力、建议能力和功能增强能力：

| 检测项 | 分级 | 处理方式 |
| --- | --- | --- |
| Windows 10/11、x64 架构 | blocker | 不满足时阻止安装并提示最低系统要求 |
| 安装目录和 `%APPDATA%\CoolzhuAgent` 写入权限 | blocker | 给出换目录、管理员权限或权限修复建议 |
| WebView2 Runtime | warning | 缺失时提示安装；若安装包内未带 runtime，给出官方下载和手动放置路径 |
| `127.0.0.1:8765` 端口占用 | warning | 自动建议备用端口并写入安装配置 |
| 磁盘空间 | blocker/warning | 核心包不足阻断；可选资源不足则禁用对应组件 |
| 内存、CPU 指令集 | warning | 低于建议值时允许安装但提示本地模型/视觉能力可能受限 |
| GPU、显存、驱动 | optional | 未满足时禁用本地 VLM/高性能推理选项，仍保留云端 provider |
| 麦克风、摄像头 | optional | 缺失时禁用语音监听/外视觉交互测试项 |
| 网络连通性、授权服务可达性 | blocker/warning | 首次在线激活必需；允许配置代理或稍后重试 |
| provider/base_url/API key 引导 | warning | 不作为安装阻断项，进入首次启动向导补全 |

检测实现原则：

- NSIS 负责安装前体验和阻断提示，复杂检测逻辑尽量复用随包的 diagnostics 可执行文件。
- 检测报告写入 `%APPDATA%\CoolzhuAgent\logs\install-preflight.log`，便于用户反馈。
- 大文件下载较慢时，不在安装器内长时间静默等待；提示用户手动下载地址和放置路径。

## 在线激活与机器指纹

授权机制用于保护自有软件授权，目标是防止一个授权在多台设备无限复用，不用于破解第三方软件或规避他人授权。

| 环节 | 方案 |
| --- | --- |
| 机器指纹 | 采集稳定字段后本地归一化并 hash，例如 Windows MachineGuid、主板 UUID、CPU 信息、系统盘卷序列等；不上传原始硬件 ID |
| 隐私保护 | 使用应用 salt 和服务端挑战值生成 fingerprint hash，服务端只保存 hash、设备别名、激活时间和最近校验时间 |
| 在线激活 | 用户输入账号/授权码，安装器或首次启动向导调用授权服务；服务端检查授权有效期和设备槽位 |
| license token | 服务端返回带过期时间、设备绑定、版本范围和功能权益的签名 token；客户端只做签名验证和权益读取 |
| 本地保护 | token 和授权缓存放入 `%APPDATA%\CoolzhuAgent\config\license.json`，P1 可用 DPAPI 或等价机制保护 |
| 防多设备复用 | 服务端按授权维护 active device slots；超过数量时要求解绑旧设备、管理员重置或购买扩展设备数 |
| 离线容忍 | 已激活设备可设置 7-14 天宽限期；宽限期结束后只保留基础/诊断能力并提示重新联网校验 |
| 迁移/换机 | 迁移包不携带可跨机复用 token；新设备导入数据后必须重新激活或完成设备迁移流程 |

注册/激活码策略建议：

- 使用“官方授权码签发 + 在线激活服务”替代离线可推导注册机，避免授权算法被客户端逆向后批量伪造。
- 授权码本身只作为兑换凭证，最终可运行状态由服务端签发的 license token 决定。
- 服务端保留撤销、设备解绑、异常激活频率限制和 token 轮换能力。

## 加密资源镜像与完整性校验

打包保护目标是提高篡改和直接复制成本，不承诺“绝对不可破解”。

| 资源 | P1 保护方式 | P2 增强 |
| --- | --- | --- |
| Web 静态资源 | manifest hash + 签名校验 | 关键 bundle 加密镜像加载 |
| 桌宠动作帧和 UI 资源 | manifest hash + 签名校验 | 资源包分块校验和修复下载 |
| TTS/STT/VLM 模型包 | 独立资源包签名校验 | 授权后解密缓存，按设备 token 派生资源密钥 |
| 配置模板 | 安装时生成，避免硬编码密钥 | DPAPI 保护本机敏感配置 |

安全边界：

- 不把解密主密钥硬编码在客户端；服务端根据 license token 下发短期资源密钥或密钥包。
- 不采用高风险、易误报的侵入式反调试/反虚拟机手段。
- 校验失败时进入诊断/修复流程，而不是静默崩溃。

## 内视觉模型资源内置与自启动方案

当前 G1 交互验证先使用用户已手动拉起的 `qwen2.5-vl-3b` OpenAI-compatible endpoint；模型资源内置、自启动和安装态环境检测并入后续打包阶段 `REQ-PACK-013`。

已确认的本机资源体积：

| 模型 | 当前位置 | 体积 | 说明 |
| --- | --- | ---: | --- |
| Qwen2.5-VL-3B-Instruct | `%USERPROFILE%\.claw\local-vlm\models\Qwen2.5-VL-3B-Instruct` | 约 7.0GB | 当前默认 `qwen2.5-vl-3b` endpoint 目标模型 |
| ShowUI-2B | `%USERPROFILE%\.claw\local-vlm\models\showui-2b` | 约 4.1GB | 可作为轻量 grounding 备选 |

打包阶段目标：

- 安装包或资源分包包含内视觉必需模型、`serve_local_vlm.py`、启动脚本和运行环境检测。
- Web UI 服务启动时自动检查本地 VLM `/health`，未运行时自动拉起；启动失败时写入安装态/运行态日志并展示可修复提示。
- 模型体积较大时可采用“核心安装包 + 内视觉模型资源包”的组合，但安装流程必须清楚提示下载地址、放置路径、校验 hash 和重试方式。
- 资源镜像需要签名校验；如后续接入授权，模型资源可纳入加密镜像/授权后解密缓存方案。
- 当前实测 Qwen 在 Windows transformers 权重加载阶段出现 access violation，打包前需完成启动稳定性修复或选择更稳定的推理后端。

## 数据目录设计

建议统一到：

```text
%APPDATA%\CoolzhuAgent\
  config\
    app.toml
    providers.toml
    model-aliases.toml
    license.json
  data\
    coolzhu.db
    attachments\
    captures\
  logs\
    web-console.log
    tauri-shell.log
    diagnostics.log
  backups\
```

迁移包建议格式：

```text
coolzhu-migration-YYYYMMDD-HHMMSS.zip
  manifest.json
  config\
  data\
  logs\optional
```

`manifest.json` 必须包含：

- 应用版本。
- 数据 schema 版本。
- 导出时间。
- 导出机器和用户名的 hash，不保存原始敏感路径。
- 包含的数据类型。
- API key 处理状态：`not-exported`、`dpapi-bound` 或 `requires-rebind`。

## 密钥处理

API key 不应以明文进入迁移包。

| 阶段 | 策略 |
| --- | --- |
| 开发态 | 继续支持环境变量和本机会话 key |
| P1 安装包 | provider 配置只迁移 base_url、model、key 引用名；迁移后要求重新录入 key |
| P2 | Windows DPAPI 保护本机密钥；迁移包只导出加密状态，不跨机解密 |
| P3 | 可选接入系统凭据库或企业密钥管理 |

## 首次启动流程

1. 检查 `%APPDATA%\CoolzhuAgent` 是否存在，不存在则初始化。
2. 检查端口 `127.0.0.1:8765` 是否被占用。
3. 检查 WebView2 runtime。
4. 检查 `coolzhu-tauri-shell.exe` 路径。
5. 检查 provider 配置和 key 状态。
6. 检查可选模型资源：TTS/STT/VLM。
7. 启动 Web 服务，写入 `coolzhu-gui-web-url.txt`。
8. 拉起桌宠，双击可显示 Web 控制台。

## 验收标准

| 场景 | 必须通过 |
| --- | --- |
| 干净 Windows 11 | 无 Rust/Node 环境，安装后能启动 Web GUI 和桌宠 |
| 干净 Windows 10 | WebView2 存在时可启动；缺失时给出可读提示 |
| 安装前检测 | 系统/硬件/权限/端口/网络/WebView2 检测能分级输出 blocker、warning、optional |
| 端口占用 | 能切换端口并让桌宠读取新 URL |
| 在线激活 | 有效授权码可激活当前设备，生成本机绑定 license token |
| 多设备复用拦截 | 同一授权超过设备槽位时，安装器或首次启动向导提示解绑/迁移/扩容 |
| 资源篡改 | 修改核心资源或模型包后校验失败，进入诊断/修复流程 |
| 数据迁移 | 导入旧设备迁移包后，会话、beads、附件索引可见 |
| 密钥安全 | 迁移包中不出现明文 API key |
| 升级 | 升级前自动备份数据目录，失败可恢复 |
| 卸载 | 默认保留用户数据，提供可选清理 |

## 任务拆分

| 任务 | 优先级 | 产物 |
| --- | --- | --- |
| 固定运行数据目录 | P1 | `app-data-dir` 工具函数和配置 |
| SQLite 数据库迁移 | P1 | `coolzhu.db` schema 和迁移脚本 |
| NSIS 安装包 | P1 | `dist/CoolzhuAgentSetup.exe`、安装/卸载脚本、开始菜单项 |
| 安装前环境与硬件检测 | P1 | NSIS preflight 页面、`install-preflight.log` |
| 首次启动诊断 | P1 | `coolzhu-diagnostics` 一键报告和启动向导 |
| 在线授权激活 | P1 | 授权服务 API、机器指纹 hash、签名 license token、本地授权缓存 |
| 核心资源完整性校验 | P1 | 资源 manifest、签名校验、失败诊断路径 |
| 导出/导入迁移包 | P1 | CLI 或 Web 设置页 |
| DPAPI 密钥保护 | P2 | 本机 provider secret store |
| 加密资源镜像 | P2 | 加密资源包、短期资源密钥、授权后解密缓存 |
| 模型/资源可选下载 | P2 | 下载 manifest、断点/重试、手动下载地址和放置路径提示 |
| 自动更新和回滚 | P2 | 版本 manifest 和备份恢复 |
| 多系统验收矩阵 | P2 | Windows 10/11 安装测试记录 |

## 风险

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 资源路径在开发态和安装态不同 | 桌宠或静态资源找不到 | 引入统一资源定位层 |
| TTS/STT/VLM 模型包体过大 | 安装包体积不可控 | 模型资源拆成可选下载包 |
| API key 跨机迁移 | 安全风险 | 默认不导出明文，迁移后重新绑定 |
| 机器指纹过于敏感或不稳定 | 隐私风险、误判换机 | 只上传 hash；多字段容错匹配；提供人工解绑/迁移流程 |
| 授权服务不可达 | 首次安装或续期失败 | 安装器支持代理/重试；已激活设备提供短期离线宽限 |
| 客户端授权逻辑被逆向 | 授权绕过风险 | 服务端签发短期 token、资源密钥服务端控制、关键资源签名校验 |
| 加密/校验触发杀软误报 | 安装失败或用户不信任 | 使用常规签名、manifest 校验和 DPAPI，避免侵入式反调试 |
| 端口和防火墙 | Web GUI 启动失败 | 仅绑定 localhost，端口占用时自动换端口并写入 URL |
| 两套 GUI 并行 | 打包复杂度增加 | 主线收敛到 Web GUI + Tauri shell |
