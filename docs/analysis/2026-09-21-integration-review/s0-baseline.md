# S0 基线、证据与最小门禁

状态：实施中。本文只记录 S0 首个增量的身份、边界和验收口径；不把未执行的真实模型、桌面输入或安装包检查写成通过。

## 源码身份

| 项目 | 当前事实 | 证据等级 |
| --- | --- | --- |
| 工作区 | `C:\Users\zhupu\Desktop\coolzhuagent` | 已观察 |
| Git 基线 | **已建立**：源码对应上游 `main` 的 `91af188086705bde2779651e507d1b0ef3b9d30d`（经 codeload tar 快照获取）；本地仓库仅有标识用的空提交 `baa8e35`（提交信息内含上游 SHA），**源码文件本身仍未跟踪** | 已观察；上游 SHA 可作发行身份，本地空提交不可 |
| 依赖与成员清单 | 根 `Cargo.toml`、`Cargo.lock`、`SOURCE-MANIFEST.json` | 静态声明，待构建复核 |
| Web 生产入口 | `modules/gui-web/packages/web-console/src/main.rs` | 源码已观察 |
| 静态 Web 资源 | `index.html`、`src/app.js`、`src/styles.css` 由 `main.rs` 编译期内联 | 源码已观察 |
| MSI / 安装二进制 | **已获得**：`dist/CoolzhuAgent-{0.2.0,0.2.12,0.2.13,0.2.14}.msi`，哈希见下表；系统当前安装的是 **0.2.14** | 已观察：产物哈希与内嵌构建 SHA 均已采集 |
| 真实模型、浏览器、Paint | 本次未运行 | 未执行 |

在可提交的版本建立前，任何测试报告必须同时记录源码清单 hash 和构建产物 hash，不能仅引用本表。

### 版本 ↔ 产物哈希 ↔ 源码提交

| 版本 | MSI SHA-256 | 内嵌构建 SHA（源码提交） | 证据来源 |
| --- | --- | --- | --- |
| 0.2.0 | `794EAF150537005E8A3625257EA4B9F369FED848926DEBEBCA19572A24E56B5D` | `0a3802bada8ca6b37dad77f2e38c175cde8b2875`（本地空提交标记，**不是**上游提交） | `dist/*-installer-report.json` 的哈希；`msiexec /a` 提取 `bin/coolzhu-cli.exe --version` 读内嵌 `GIT_SHA` |
| 0.2.12 | `2403B272AF57C2D8A22D7A83F70B3869415BAA6AF0B146002FFD63A6D3AE7C23` | **未获得** | 报告未记录提交；管理安装返回 0 但未落盘（疑似复用已缓存的 admin image），未再追 |
| 0.2.13 | `88F84ECB461A8978FCA528849A732A030DE5AAE11699E28ABC8CC0D8B03B4D75` | **未获得** | 同上 |
| 0.2.14（当前已安装） | `9E13767643DE983060D6C492008D98B9E0302DFB07D57959B55872ED328EE9DB` | **`e314500a34df0b39918371feb00fb65e7855874c`**（上游 `main` 上的真实提交） | `C:\Program Files\CoolzhuAgent\bin\coolzhu-cli.exe --version` 的 `Git SHA` 字段 |

**未获得的原始证据（如实登记，不得推断）**：① 0.2.12 / 0.2.13 的源码提交；② 四个 MSI 均**未签名**（`signed=false`），无签名链可验；③ 上游仓库的完整提交历史与 CI 结果（本机只取了 `main` 的 tar 快照，`.github/workflows` 未在远端跑过）；④ 真实模型、浏览器桥、Paint 与安装包在真实桌面上的验收结果。

**已建立的一条硬对应**：0.2.14 ↔ 上游 `e314500a`。该提交属于本轮同步的 9 个上游提交之一，因此"已安装构建出自哪份源码"现在可回答。

### 2026-09-22 首个 S0 增量源码 hash

以下 SHA-256 在本次增量完成、测试开始前采集；它们标识源码，不标识 MSI 或已安装程序。

| 文件 | SHA-256 |
| --- | --- |
| `Cargo.toml` | `DE1768A6958DF689A15A63050DE673239555F182D8CC325674B3D056E84B56BB` |
| `Cargo.lock` | `FA9E1A65159077BE64D485BF1F9E0005CEE8A7BD2E6E042AFA38B5B90F1A3570` |
| `SOURCE-MANIFEST.json` | `37C21C7BD6E82B3D32F601112A6F0DC6075FBC5AC72735EF4C64F409C52232D4` |
| `web-console/src/main.rs` | `526A848420F6D370C9FF61134BE50A45C5AD150C6F1774AF49A72A4D24364E29` |
| `web-console/src/app.js` | `5DCC8ED78E38CC42A9082077DD140FE7EE659886747DD9D6358A18294EF52F60` |
| `web-console/src/styles.css` | `65AF4F23206E321B1022A4B5F7425476A4F7554108FC3D281A5599E1BA969021` |
| `web-console/index.html` | `3059FF01069A77EBFB310682B2C3B6ACBF4EAC3B84E4EB9A8F76E786342D5D2E` |
| `web-console/src/s0_fixture_replay.rs` | `785227DA611CD494846E102D5FF9254638AD2A52DC6B2A3E4146D3FAF5713B33` |
| `tests/fixtures/s0-golden/manifest.json` | `8BADC857A8852EC171718A5D3CFB03E1A10A74D5961F1B95C4404FD713849C46` |

### 2026-09-22 S1 增量后的源码快照 hash

上游同步（`91af1880`）与 S1.3–S1.5 改动后的快照。**上表已被此表取代**：`main.rs` 等多份文件在本轮改动过，旧哈希不再对应当前树。注意 `run_contract.rs` 与上一条 S1 契约文档记录的值一致（本轮未改动该文件），可作为跨文档一致性校验。

| 文件 | SHA-256 |
| --- | --- |
| `Cargo.toml` | `DE1768A6958DF689A15A63050DE673239555F182D8CC325674B3D056E84B56BB` |
| `Cargo.lock` | `FA9E1A65159077BE64D485BF1F9E0005CEE8A7BD2E6E042AFA38B5B90F1A3570` |
| `SOURCE-MANIFEST.json` | `37C21C7BD6E82B3D32F601112A6F0DC6075FBC5AC72735EF4C64F409C52232D4` |
| `modules/gui-web/packages/web-console/src/main.rs` | `A71B2EB2FA3EF1D1B84DDAE8C9E6CFDE94D3C9F82370FF8702C44C4EB7985C28` |
| `modules/gui-web/packages/web-console/src/app.js` | `5DCC8ED78E38CC42A9082077DD140FE7EE659886747DD9D6358A18294EF52F60` |
| `modules/gui-web/packages/web-console/src/styles.css` | `65AF4F23206E321B1022A4B5F7425476A4F7554108FC3D281A5599E1BA969021` |
| `modules/gui-web/packages/web-console/index.html` | `3059FF01069A77EBFB310682B2C3B6ACBF4EAC3B84E4EB9A8F76E786342D5D2E` |
| `modules/computer-use/packages/computer-use-core/src/controller.rs` | `D749FCC818FC3780D44556F880A273FC0E8A755D728CC3CDF8E80D17D44884F9` |
| `modules/computer-use/packages/computer-use-core/src/input_stroke.rs` | `270B474447FB743F14086D8C1AAE5598B6F1A7BA3BE74E14BD6F66ABA7CA51AD` |
| `modules/gui-web/packages/web-console/src/computer_use_executor.rs` | `A11D7ECB3B8B1DFE118610B47992405477C292F9772A2987F64981BFF78AAA2C` |
| `modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs` | `240FC90570D04FDB163532597B51D52BAFF9EE6C2A03B4FF4F81224F9CA15E92` |
| `modules/gui-web/packages/web-console/src/computer_use_adapters.rs` | `5179BE88FC35800D1CC1898BD4DE60CC86E5B311DBBFB09DA91997903EB3FB2E` |
| `modules/gui-web/packages/web-console/src/browser_bridge.rs` | `955A0A7D477B99F9AB5D25DFECD6394230815AFE8CA322904451CB58295F1BDF` |
| `modules/core-runtime/packages/core-runtime/src/permissions.rs` | `3785B2C94F376A85F8570F73C9755A2C6AE2727B0831E631D81D9FB0809D573F` |
| `modules/core-runtime/packages/core-runtime/src/permission_gate.rs` | `B31B5D3614FDD8FE13A9C9180A59D0D50527F03621FB1ABBD3CB773E8E5F54C1` |
| `modules/core-runtime/packages/core-runtime/src/run_contract.rs` | `CBAF072C2D21AABDA42922D0F110902C8344090F7565AD1A7AA1777A2C987868` |
| `modules/tooling/packages/tool-registry/src/lib.rs` | `18FE2B5347074CA83DA45CF873F78BF71F8A5BE470B5960C8B1F6037848427CE` |
| `modules/cli/packages/command-line/src/main.rs` | `9635C2A8F714E5106977A255EB9F32DB398AB8A71A0E6D9A3FF2F4D2FA3D5444` |

**本快照的验证状态**（与上面 hash 同一次运行）：Web 1002/1002、core-runtime 187/187、computer-use-core 50/50、tool-registry 43/43、plugin-system 28/28、command-line 81/81、llm-adapter 119/119、vision-service 36/36、windows-process-guard 1/1、S0 fixture 2/2、模块链接 4/4。这些数字只对本表哈希对应的树成立；**该快照尚未打包成 MSI**，因此没有对应的安装产物身份。

## Feature 与证据映射

| feature_id | 生产入口 | 测试/证据 | 当前状态 |
| --- | --- | --- | --- |
| S0-BASELINE-IDENTITY | 根清单、Web 编译入口 | 本文、`SOURCE-MANIFEST.json` | 部分完成：上游源码身份（`91af1880`）、当前源码快照 hash、四个 MSI 的产物哈希、0.2.14 的内嵌构建 SHA（`e314500a`）均已记录；**仍缺** 0.2.12/0.2.13 的源码提交与全部签名链 |
| S0-GOLDEN-FIXTURE | `tests/fixtures/s0-golden/manifest.json` | `tests/s0_fixture_contract.rs` | 已建立：仅合成、仅录制回放契约 |
| S0-FIXTURE-MODEL-REPLAY | `web-console/src/s0_fixture_replay.rs` | `fixture_server_replays_stream_and_nonstream_tool_pair_contracts`、`s0_fixture_server_drives_real_web_model_entrypoints_without_tool_side_effects` | 已实现：本地假模型回放 OpenAI 兼容流式/非流式响应；真实 Web 模型入口已接通，工具暴露关闭且不执行工具 |
| S0-TRUTHFUL-PLUGIN-INSTALL | `POST /api/plugins/install` | `plugin_install_endpoint_never_reports_a_noop_as_installed` | 已实现：未实现安装时返回 501 |
| S0-TRUTHFUL-STREAM-DIAGNOSTICS | `GET /api/diagnostics/stream` | `stream_diagnostics_marks_source_declaration_as_unverified` | 已实现：返回 `declared` 而非 `ok` |
| S0-MINIMUM-CI | `.github/workflows/s0-baseline.yml` | Windows 构建、Web 测试、fixture、链接测试 | 已配置，待远端首次运行 |

## Fixture 边界

`s0-golden` 包含流式与非流式工具配对、图像路由、空总结、跨轮过滤和长文件任务六类合成事件。它不包含 API key、用户路径、真实附件或真实工具输出；回放策略固定为 `recorded-only`，不得把 fixture 变成真实文件写入、云请求或键鼠执行。

它验证 fixture 的格式、覆盖范围和工具配对完整性，并已通过可复用本地假模型服务回放 OpenAI 兼容的流式与非流式响应。Web Console 的两个真实模型入口已在关闭工具暴露的隔离环境中连接该服务；这证明请求构造和响应协议可回归，但不等同于真实工具执行、完整聊天 SSE 消费或真实模型质量验证。

## 诚实状态规则

- `verified`：实际探测或测试已运行，且有对应环境与原始结果。
- `declared`：仅有源码或配置声明，不能代表运行时成功。
- `not_implemented`：入口没有执行所声称的动作，必须返回显式未实现而非成功。
- `blocked` / `not_run`：缺少产物、设备、授权或本轮尚未执行；不得归入失败率或成功率分母。

本轮的 `/api/diagnostics/stream` 属于 `declared`，并明确 `probe_executed=false`；`/api/plugins/install` 属于 `not_implemented`，不执行下载、复制、注册或加载。
