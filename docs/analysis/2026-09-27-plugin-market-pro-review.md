# 插件市场补充审查存档

来源：聊天“整合审查执行计划”，会话 `6ab013c6-8dc4-83ea-a220-a33c9940783f`，轮次 `bbb85648-6777-4577-8046-854cbcdba5f8`。本次为主会话提供代码事实后的方案审查，并非 Pro 已直接审计仓库。

主会话裁决：本地目录、可信来源浏览、安装执行分阶段接入，但全部保留在 NAV-06 未完成清单；未配置市场源展示不可用。UI 新入口可以先接本地真数据，不宣称完整插件市场已交付。复用现有 plugin-system 和权限；应用更新与插件更新保持独立。

## 返回原文

# NAV-06 插件市场最小可用路线设计审核

## 结论

根据补充源码事实，当前状态应明确拆分：

- **本地工具/插件目录能力：已有**
- **插件安装运行库能力：部分已有**
- **Web 插件市场：未完成**
- **远程插件源/目录信任链：未建立**

因此不应把当前：

- `/api/tools/catalog`
- plugin-system 库 install/update

描述为“插件市场”。

正确口径：

> 当前具备本地插件/工具管理基础，NAV-06 的未完成项是“可信插件目录 + Web 安装编排 + 状态反馈”。


---

# 一、插件市场分阶段方案

建议分三阶段，不一次引入完整市场体系。

---

# Phase 1：本地已安装视图（最低风险）

## 目标

先把已有能力产品化。

数据来源：

已有：

- 本地 tools catalog；
- plugin-system 状态。

提供：

```text
已安装插件
```

视图。

---

## 必须展示

至少：

- 插件名称；
- 版本；
- 来源类型；
- 当前状态；
- 权限声明；
- 启用/禁用状态；
- 最近错误。

---

## 不允许

不要把：

```text
本地已安装
```

显示成：

```text
官方市场插件
```

---

状态模型建议区分：

|状态|含义|
|-|-|
|Installed|本地存在|
|Enabled|当前启用|
|Disabled|安装但未启用|
|Broken|安装但加载失败|
|Unknown|状态无法确认|

---

# Phase 2：可信目录浏览（只读）

## 批准

增加：

```text
插件发现/目录
```

但必须有明确来源。

---

## 最小目录模型

不要直接返回：

```json
[
 "plugin-a",
 "plugin-b"
]
```

必须包含来源：

```json
{
 id,
 version,
 publisher,
 source,
 checksum,
 permissions,
 compatibility
}
```

---

## 来源分级

建议：

```text
SourceType:

OfficialRegistry
EnterpriseRegistry
LocalImport
Unknown
```

---

禁止：

未配置市场源时：

显示：

“暂无插件”

因为容易误导。

应显示：

```text
插件目录不可用
未配置可信来源
```

---

# 三、插件安装路线

## 不建议直接开放安装按钮

原因：

当前缺：

- Web install endpoint；
- 来源校验；
- 权限确认；
- 安装状态反馈。

---

## 最小安装闭环

安装必须经过：

```text
用户点击安装

↓

展示来源/权限/版本

↓

用户确认

↓

backend install request

↓

plugin-system install

↓

状态刷新
```

---

不能：

打开市场页面自动安装。

---

# 四、安装权限边界

必须复用已有权限系统。

不要新增：

```text
plugin_admin
plugin_manager
```

等第二套权限。

---

安装前展示：

例如：

```text
该插件请求：

- 文件访问
- 网络访问
- 模型调用
```

---

如果当前权限系统还不能表达：

插件安装先支持：

- 下载；
- 验证；
- 待启用。

不要直接加载。

---

# 五、Web API 边界建议

当前：

```text
/api/plugins/install = 501
```

这是正确状态。

不要为了 UI 打通直接返回成功。

---

建议拆：

## 1. catalog

只读：

```
GET /api/plugins/catalog
```

返回：

- 来源；
- 元数据；
- checksum；
- compatibility。

---

## 2. install plan

不要直接 install。

先：

```
POST /api/plugins/install-plan
```

返回：

- 将安装什么；
- 来源；
- 权限；
- 风险提示。

---

## 3. install execute

明确用户动作：

```
POST /api/plugins/install
```

需要：

- plan id；
- 用户确认；
- 当前权限检查。

---

## 4. status

```
GET /api/plugins/status
```

返回：

- installed；
- installing；
- failed；
- enabled。

---

# 六、必须验证的接口边界

## 1. Catalog 来源验证

必须确认：

- 目录地址是否固定；
- HTTPS；
- 是否允许重定向；
- 是否有签名/校验；
- metadata 是否可信。

不能：

远程 JSON 说：

```json
{
 permissions:["all"]
}
```

就相信。

---

# 2. 安装包验证

至少：

- hash；
- package id；
- version；
- publisher/source。

---

未来如果有签名：

再增加：

- signer identity；
- signature chain。

---

# 3. 安装失败处理

必须：

保留：

```text
install failed
```

不能：

显示：

```text
installed
```

---

需要：

- rollback；
- 临时文件清理；
- 状态恢复。

---

# 4. 插件加载边界

安装成功 ≠ 自动执行。

建议：

状态：

```text
Downloaded
Verified
Installed
Enabled
Running
```

分开。

---

# 七、浏览和安装是否分阶段？

## 必须分阶段。

原因：

两者风险不同。

---

## 浏览

风险：

低。

只需要：

- 来源；
- metadata；
- 展示。

---

## 安装

风险：

高。

涉及：

- 文件写入；
- 权限；
- 执行；
- 更新。

---

因此：

推荐：

### NAV-06A

完成：

- 本地已安装视图；
- 目录不可用状态；
- 可信目录浏览框架。

---

### NAV-06B

完成：

- install plan；
- 用户确认；
- backend install；
- plugin-system 接线。

---

### NAV-06C

未来：

- 更新；
- 自动升级；
- 签名发布。

---

# 八、与更新系统的关系

不要混合。

插件更新 ≠ 应用更新。

保持：

## App update

负责：

- MSI；
- 主程序；
- schema migration。

---

## Plugin update

负责：

- plugin package；
- plugin lifecycle。

---

不要：

插件市场直接调用：

```text
self-update
```

---

# 九、当前发布口径补充

你补充：

PR #67 已合并、CI 通过。

正确口径：

可以说：

> 代码已合并，CI 验证通过。

不能说：

> 正式发布版本。

因为缺：

- release artifact；
- 签名；
- 发布渠道；
- 更新源。

---

同样：

build_version：

```text
git hash + date
```

不能替代：

MSI 语义版本。

需要继续保持：

```text
source build identity
≠
release version
```

---

# 十、最终裁决

批准 NAV-06 最小路线：

## 第一阶段

实现：

- 本地已安装插件视图；
- 复用 plugin-system 状态；
- 明确“本地”与“市场”区别。

---

## 第二阶段

实现：

- 固定可信目录浏览；
- 来源元数据；
- checksum；
- compatibility；
- 权限展示。

---

## 第三阶段

实现：

- 用户明确确认安装；
- `/api/plugins/install`；
- plugin-system install/update 接线；
- 状态与错误反馈。

---

必须保持：

- 未配置市场源 → 显示不可用，不造空目录；
- `/api/plugins/install` 未完成 → 不伪装安装成功；
- 插件权限沿用已有权限体系；
- 浏览和安装分离；
- 安装不自动执行；
- CI 合并不等于正式发布。

当前最小正确边界：

```text
本地插件状态
        ↓
可信目录浏览
        ↓
用户确认
        ↓
安装计划
        ↓
安装执行
        ↓
状态反馈
```

不需要引入新的插件框架，只需要把已有 plugin-system 能力通过受控 Web/API 边界接出来。
