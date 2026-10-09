# 正式0.2.106改动与测试交接

最终CU回执新增兼容旧JSON的可选`input_steps`，使模型能独立读取动作投递、释放和效果，解决105只有计数、模型无法判断released的问题。106正常安装已完成；原SWE-2-medium与唯一island-kayak的成功/失败两轮真实回执和原生软件实拍均已验证。

## 构建与安装

源码`c9c61feec75ddffce4397bf0c57d5bf7f3a64fb5`；第一方快照`9163bfe7d6bf94d0cfa2914bd0e84ca121eedbdf0e9f27be39cd539b705a3466`。正常完整发布链实际子进程退出0、六门pass，MSI安装0、Program Files内1159文件逐长度/SHA一致，默认Browser诊断关闭。分发MSI为`dist/CoolzhuAgent-0.2.106-20261008-181012.msi`，285818376字节，SHA256 `ce32bac8dba7a65ffe61f4c40030368df789d03ab6feb3c0b286193df9556848`。[安装核验](installed-validation/installed-106-verification.json)、[实际进程](installed-validation/installed-106-standard-processes.json)、[实际退出码](installed-validation/build-result.json)。生成的构建证据导致第二次报告dirty=true；第一方冻结范围和前一次相同快照，不能把全工作树称为clean。

首次构建因子PowerShell未加载Get-FileHash失败；临时包装脚本显式加载系统Utility后完整出包，但误用内部遗留LASTEXITCODE报告-1。两次事实分别保留，未据此标成功。修正包装脚本后完整重跑、实际子退出0。正式脚本保留旧同版本MSI并为新产物加时间戳；独立核验首次硬编码旧文件名失败，改为读取正式安装报告的确切路径并校验dist边界/摘要后才安装，不覆盖旧包或伪造报告。

## 模块职责与兼容

core contracts只定义序列化输入事实；Web执行器从同一运行既有SQLite步骤记录在原终态提交前投影，不新增账本、SQL列、权限闸门或补发。None表示未取得事实、空数组表示读取成功且无步骤；旧字段缺失可正常反序列化，未知枚举/NULL继续未知。投影不含输入正文、节点、网址、图片或推测时序。取消/持久化错误保留已取得事实；7个提前返回分支改为返回原finish_at_version的实际结果，保证落库与返回一致。

源码offline build通过。完整Web首次1421通过/2失败/6既有忽略，暴露上述提前返回不一致，修复后1423通过/0失败/6既有忽略；core143通过/0失败，专项1通过。[初次失败及回归摘要](../release-0.2.105/browser-observation/candidate-regression.json)。纯SQLite/序列化回归不代替真实模型验收。

## 成功回执与未命中窗口

run `run-chat-f8288b5f9a858129e012c1d59c15092aad69dda4f9af9416`，一次perform、一条步骤、单ACP end_turn/drained并解锁；终态input_steps为sent/released/partial=false/effect_observed/passed，真实SWE最终准确读取字段。旧页可信down/up/click各1、新页输入0。正常导航比CU终态晚4818ms，故只关闭成功回执字段，不计观察失效负例通过。[原事实](browser-input-positive/result.json)、[网页事件](browser-input-positive/events.jsonl)、[正常UI时间](browser-input-positive/ui-navigation-times.json)、[成功软件实拍](browser-input-positive/final-reply.jpg)。首次未命中保留，没有覆盖或追认。

## 失败回执

另一个独立网页/marker、同一远端绑定。run `run-chat-cf4fe911b509bd15d413c9cf1c61e2c34f009945e866bb42`；可信click后正常UI导航，新文档loaded在旧pointerup后6410.400ms。CU blocked/native_observation_timeout，stage verification，goal=false；input_steps仍为sent/released/partial=false，effect_status和goal_verdict为空。真实SWE正确区分投递/释放与未知效果，未把输入说成未执行；不声称按下期间导航。工具单调用、零补发、新页零输入；父failed、单end_turn/drained并解锁。[原事实与最终回复](browser-input-failure/result.json)、[网页事件](browser-input-failure/events.jsonl)、[UI时间](browser-input-failure/ui-navigation-times.json)、[正式失败终态实拍](browser-input-failure/final-reply.jpg)。

## 新发现与后续针对性验收

投递/释放最终字段缺口已闭环；观察资源变化时待处理旧观察被丢弃但接收循环仍等到通用5秒timeout，原因识别尚需改进。下一改动应沿用原资源匹配，在等待期间发现请求资源变化时明确native_browser_resource_changed，不能把它写成未发送输入。之后在正式安装版复验资源变化错误、sent/released保留及新页零输入；不修改产品输入延迟来制造时序。newTarget/跨来源commit严格down/up窗口仍开放，两轮均不是按下期间导航证据。

其余总体矩阵仍按四台账推进；Paint免测、微信不动、Opus暂停，不使用子代理，不新增云端测试会话。ChatGPT订阅一次最小文本连通已另有正式实验记录，不扩大为完整Provider。

## 后续源码候选（不包含106）

等待循环复用原FrozenPanelBinding/完整PanelResource匹配，已有请求资源在等待期间变化立即返回native_browser_resource_changed，不再等通用timeout；原回包验收也复用同一匹配函数。原50ms接收轮询与5秒总限不改，不新增权限或输入延迟。新导航仍可发起自己的合法观察。扩展已有纯匹配回归，offline build退出0、完整Web1423通过/0失败/6既有忽略。尚待107正常出包安装及真实回归，不追认106已修诊断。

106构建源码两路CI均已success，见[独立元数据](installed-validation/source-ci.json)。106尚未GitHub发布，下一候选统一验证后交付。
