# 打包回退与工作区默认值：是谁改的、是否临时方案、残留风险

> ## ⚠ 第四轮裁决对本文件表述的正式更正（覆盖下文旧表述）
>
> | 本文件的原表述 | 正式修订（以下为准） |
> | --- | --- |
> | "两个文件 mtime 相同、同一分钟 92 个文件写入 ⇒ **确定来自上游**" | "同批落盘**有线索**；来源归属**以内容、提交或同步清单确认**" |
> | "取自本次 build crate，**天然同源**" | "须由**本次构建身份、实际依赖选择和内容校验**建立关联"（Cargo 官方资料说明 `OUT_DIR` 可跨构建保留，"在 build 目录里"**不能独立证明**它由这次构建新产生） |
> | **"我那次手工 DLL 已冗余，留着无害"** | **"它可能仍参与当前选源，正式发布不得无验证地采用"**——裁决指出：当前逻辑**先检查声明路径是否存在**，因此**旧手工副本仍会遮蔽后面的回退**（这是从已记录分支得出的风险，不是已确认安装包混用了错误版本） |
> | "多候选时应提示清理 target" | "**先依据本次构建选择生产者**；无法确定时拒绝并使用**受控的独立构建目录**"（不默认建议全局 `cargo clean` 或删共享 target；历史共享 target 存在 junction 记录） |
> | "`runtime_dir` 统一是 USERPROFILE" | "`%USERPROFILE%\coolzhuagent` 是**未建立用户选择时的打包默认建议**" |
> | "缺键时按日志目录推导" | "**旧配置兼容行为**；新配置**不再**用日志位置隐式选择业务数据源" |
> | "看不见会话就是工作区丢失" | "**首先检查实际数据源**；不可见**不等于**删除" |
> | "加三条注释即可解决" | "注释**只提高可发现性**；来源校验、升级保护与安装测试**另有明确关闭条件**" |
>
> **裁决作出的三项正式决定**（本文件原为"待你确认"，现已裁决）：
> 1. **Loader**：修正产物生产与声明契约——先从**本次有效构建**确定来源，再**自动导出到稳定路径**，由 manifest 消费；**目录搜索回退不作为长期正式路径**；**当前不删除 DLL**（删除须满足静态/动态依赖核验条件）。
> 2. **打包默认工作区**：确认 `%USERPROFILE%\coolzhuagent` 作为**新用户、尚未选择工作区时的默认建议值**；**已选工作区与可确认的既有工作区优先，升级不得强制改回默认值**。
> 3. **是否同根**：**不要求同根**——保留工作区在用户选择的位置、诊断日志默认在 `%LOCALAPPDATA%\CoolzhuAgent\logs`，**把两者的来源、用途和实际路径公开呈现**。
>
> **最先应合并的两项**：① "**声明源存在也必须验证来源**"（堵住"旧文件让打包看起来成功"）② "**已选工作区失效时禁止静默回退到空目录**"（堵住"新目录让产品看起来正常启动"）。
>
> **今后每次修改默认值或回退规则，必须附一份简短决策记录**，固定回答：为什么存在 / 适用于哪些入口·目标·版本 / 输入来源与优先级 / 失败时如何处理 / 会不会改变既有用户数据位置 / 有哪些自动回归与安装证据 / 何时可以删除兼容逻辑 / 由哪个模块负责人维护。

用途：核实"上次的两处手工绕过是否已成为上游正式修复"，并评估它们是否属于**临时/非正式方案**、有哪些**会被后续埋成坑**的风险。
结论先行：**两处都来自上游同步，不是我手工绕过的残留**；两处都**不是临时 hack**（有明确边界与失败即报错的语义），但各带 1–2 个**会被后来者踩到的默认值/前置条件**，需要记录或修正。

---

## 0. 核实：它们来自上游，不是我改的

| 文件 | 当前 mtime | 判定依据 |
| --- | --- | --- |
| `config/package-launcher.json` | **2026-09-21 07:10** | 与 `scripts/package-all.ps1` **同一 mtime**；该分钟窗口内共 **92 个文件**被写入 ⇒ 批量写入 = **上游同步**（对比：`Cargo.toml` 停在 09-16，我本轮改的 `run_contract.rs` 是 09-25） |
| `scripts/package-all.ps1` | **2026-09-21 07:10** | 同上 |

⇒ 你的观察成立：`Resolve-WebView2LoaderSource` 与 `"runtime_dir": "%USERPROFILE%\\coolzhuagent"` 都是**上游带来的**。
我此前那次"手动从 `webview2-com-sys` 构建输出里捞 x64 DLL 到声明路径"是**另一件事**（把文件喂给 manifest 声明的路径），如今已被上游的**脚本级回退**取代，因此那次手工动作现在是**冗余**的——留着无害，但**不再是必需步骤**。

---

## 1. `Resolve-WebView2LoaderSource`（`scripts/package-all.ps1:237-273`）

> **现状更新（2026-09-25，PKG-01/PKG-02 已实施）**：本节描述的目录扫描回退**已整体移除**，
> 并被"构建记录 → 稳定导出 → 收据"的确定性产物链取代。
> 现行协议与失败语义见 `packaging-webview2-loader-export-protocol.md`。
> 本节保留为问题史与当初的风险评估记录。

### 1.1 问题背景

`config/package-manifest.json:86-89` 声明了一个 artifact：

```json
{ "id": "gui-desktop.webview2-loader",
  "source": "modules/gui-desktop/target/tauri-shell/{profile}/WebView2Loader.dll",
  "target": "bin/WebView2Loader.dll" }
```

即"要随包发布一个 `WebView2Loader.dll`，从 Tauri shell 的 release 目录取"。但**Tauri v2 的 WebView2 依赖常常不在该路径落文件**（loader 可能被静态链接，或只落在本次 cargo 构建的 build crate 输出里）⇒ 打包时"缺 artifact 源"。上游的处理是**加一个回退解析**，而不是改声明。

### 1.2 回退做了什么（逐条）

1. 若声明的源路径**存在** → 直接用（正常路径，无副作用）。
2. 若不存在，且 artifact id **不是** `gui-desktop.webview2-loader` → 原样返回（回退**只对这一个 artifact** 生效，不做通用搜索）。
3. 否则在 `<releaseRoot>/build/` 下找 `webview2-com-sys-*` 目录，取其中的 `out\x64\WebView2Loader.dll`。
4. **候选恰好 1 个** → 返回它，并打印一行 `fallback source ...`（**可见**，不是静默）。
5. **候选 0 个** → `throw`（"no x64 WebView2Loader.dll candidate"）。
6. **候选 >1 个** → `throw`（"ambiguous ... found N"）。

### 1.3 正式性评估：**不是临时 hack**

| 性质 | 判断 |
| --- | --- |
| 作用域 | ✅ 只对**一个** artifact id 生效，不是"全盘搜某个 DLL 名" |
| 搜索根 | ✅ 锁在**声明路径所属的 release/build 下**，不跨目录猜 |
| 失败语义 | ✅ **缺失与歧义都直接 throw**，不会静默选一个错的 |
| 可观测 | ✅ 回退时打印实际使用的源 |
| 版本一致性 | ✅ 取自**本次构建**的 build crate（同一次 cargo 构建），loader 与链接的 WebView2 依赖天然同源；不存在"跨版本混用" |

### 1.4 残留风险（会被后来者踩到）

1. **它掩盖了 manifest 声明与真实产物的不一致。** 声明的 `source` 路径**可能永远不存在**，真正可用的是构建输出。回退让打包"能过"，但**声明本身仍然是不准确的**；后来者若照声明去找产物，会再次困惑（这正是我上次手工捞 DLL 的成因）。
   → **决策点 1**：是否把 manifest 的 `source` 改成构建输出的真实位置（或直接删掉该 artifact 若确实不需要独立分发 loader），让**声明与事实一致**；回退可作为过渡保留但加注释说明"声明的源可能不存在"。
2. **回退依赖"热 target 目录"。** 它需要 `<releaseRoot>/build/webview2-com-sys-*` 存在。**全新环境（干净 checkout + 空 target）** 时该目录是否存在，取决于这次构建是否重建了该依赖——通常会有（build script 输出），但若有人用 `--no-build`/复用旧包目录，行为不同。
   → **建议**：在打包脚本/文档里明确"打包前必须有真实的 tauri release 构建"（既有流程已隐含要求，但没写明这条回退的依赖）。
3. **多版本并存会直接失败。** 依赖升级后 target 里留下多个 `webview2-com-sys-*` ⇒ 候选 >1 ⇒ 打包 throw。这是**有意的严格**（不猜），但操作上意味着"先清理 target"。
   → **建议**：在 throw 的文案里补一句可操作指引（"清理 target 后重试"），否则操作者只看到"ambiguous"。

---

## 2. `"runtime_dir": "%USERPROFILE%\\coolzhuagent"`（`config/package-launcher.json`）

### 2.1 问题背景（我亲手踩过）

`raw`/`package-launcher.json` 的 `runtime_dir` 决定**打包后 app 的工作区（workspace）落在哪里**。上游把它写成了 `%USERPROFILE%\coolzhuagent`。
若**不写**这一行，launcher 会走 `default_runtime_dir_from_log_dir(log_dir)`（`packages/app-launcher/src/lib.rs:312`）——从 `log_dir` 反推，即 `%LOCALAPPDATA%\CoolzhuAgent`（因为 `log_dir` 是 `%LOCALAPPDATA%\CoolzhuAgent\logs\package-launcher`）。

**我在上一轮实际遇到的现象**：装好的 app 起来后"看不见"真实工作区，因为它的 workspace 指向了一个几乎空的目录。当时的直接原因是该键缺失/指向别处，补上这一行后恢复正常。**现在这一行是上游的默认值**。

### 2.2 它带来的实际后果（需要知情）

1. **打包版 app 的工作区统一是 `%USERPROFILE%\coolzhuagent`**，而**日志仍在 `%LOCALAPPDATA%\CoolzhuAgent\logs\...`** ⇒ **工作区与日志分处两个根**。这是有意还是巧合，从代码看不出注释说明。
2. **`default_runtime_dir_from_log_dir` 在打包路径上是死代码**（键存在时永不触发）；只有该键被移除时才恢复"按 log_dir 推导"的行为。
3. **用户的既有工作区若不在该路径**，会被引导去**新建**一个 `%USERPROFILE%\coolzhuagent` —— 表现就是"我的会话/配置不见了"。这正是我遇到的现象。
4. **这是一个产品级默认值，却只写在 JSON 里**：改代码的人不一定看 JSON，看 JSON 的人不一定知道它覆盖了代码里的推导逻辑。

### 2.3 正式性评估：**看起来是有意的产品决定，但缺少文档痕迹**

它与 AGENTS.md 里"workspace 解析为 `%USERPROFILE%\coolzhuagent`"的描述**一致** ⇒ 更像**有意的默认值**，而非临时 hack。但同时：
- 没有任何注释/README/安装文档说明"打包版工作区在 `%USERPROFILE%\coolzhuagent`、日志在 `%LOCALAPPDATA%`"；
- 也没有说明"想用别的工作区要改这个键"。

### 2.4 决策点

- **决策点 2**：确认 `%USERPROFILE%\coolzhuagent` **是否就是期望的打包默认工作区**。若是 →
  - 在安装/使用文档（或 `package-launcher.json` 的兄弟说明文件）里写明这一点，并说明如何改；
  - 考虑给 `default_runtime_dir_from_log_dir` 加一行注释"仅当 runtime_dir 缺省时生效"，避免后来者误以为两套推导同时有效。
- 若**不是**（例如希望 `%LOCALAPPDATA%`）→ 删掉该键即可回到从 `log_dir` 推导的行为（但要注意：**已经产生的用户工作区不会自动迁移**，需要迁移说明）。
- **决策点 3**：是否需要"工作区与日志同根"（都在 `%USERPROFILE%` 或都在 `%LOCALAPPDATA%`）？现在分处两地的写法在排障时容易让人找错目录。

---

## 3. 共性教训（针对"防止给后续埋坑"）

这两处都属于同一类模式：**一个为了绕过环境差异而加的本地回退/默认值，被批量同步带成了正式行为，但没有留下"为什么存在、边界在哪、何时该去掉"的痕迹。**

建议的最小动作（都不改行为，只增加可发现性）：
1. 在 `scripts/package-all.ps1` 的回退函数上加一段注释：**为什么 Tauri v2 下声明路径可能不存在**、**回退的边界**（只此一个 artifact、唯一候选、歧义即失败）、**何时可以删掉它**（当 manifest 声明与真实产物一致时）。
2. 在 `config/package-launcher.json` 旁（或安装文档）写明：**`runtime_dir` 覆盖 launcher 的默认推导**、打包默认值是 `%USERPROFILE%\coolzhuagent`、以及如何改。
3. 在 `app-launcher/src/lib.rs` 的 `default_runtime_dir_from_log_dir` 上写明"仅在 `runtime_dir` 缺省时生效"。

这三条都不改变任何行为，只把"读过代码的人才知道的事"变成"后来者能搜到的事"。

---

## 4. 需要你确认的三点

1. **决策点 1**：是否修正 manifest 里 `gui-desktop.webview2-loader` 的 `source`（让声明与真实产物一致），还是保留回退作为长期正式路径？
2. **决策点 2**：`%USERPROFILE%\coolzhuagent` 是否确认就是打包默认工作区？（影响安装文档与用户迁移说明）
3. **决策点 3**：是否要求工作区与日志同根（现在分别在 `%USERPROFILE%` 与 `%LOCALAPPDATA%`）？

（另：本文档只做核实与风险评估，**未改任何代码或配置**。上面第 3 节的注释建议都是零行为变更，你同意的话我可以直接加。）
