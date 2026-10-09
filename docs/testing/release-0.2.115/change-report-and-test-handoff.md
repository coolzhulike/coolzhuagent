# 0.2.115 改动与验收交接

本版将四个模型参数配置入口的同步块提取至SessionConfigService，保留原TrackedSessionStore诊断、store→config锁序与失败恢复，HTTP入口只组织输入和响应。没有第二缓存、参数表、任务队列、权限/模型或预算变更。插件目录IO留在锁外；服务暂复用Web错误适配与发布基础设施，不代表core-runtime完整领域迁移。

冻结源码 `f6f8729953a1ddb6351a4d5a9c73daa9076e10f1`，快照 `1e82d86a4ef0d5b325068ecf30360eba5f043329d34365adcfce3c3160b68a48`。正常release构建与正常管理员MSI安装均实际0，六发布门pass，Program Files 1159项逐长度/SHA一致。同源两路CI37895776447/37895770145均success。MSI 285715976字节，SHA256 `c81207e5e90be2a285ec6bbd4e7d88d522012f7bc1accbda9a24f8769c625250`。候选完整已有Web回归1423/0/6既有忽略，另lib8/native-host1通过，日志见[候选报告](../2026-10-08-session-config-service/report.md)。

## 正式安装复验与测试设计依据

第一次外层Start-Process -Wait在构建进程已结束后仍等待后代，未捕获实际子退出，不计成功。只停止路径/UTC ticks匹配的自有协调进程，未停止编译器助手；直接subprocess等待正常发布脚本重做后实际返回0。第二次同源码快照保持，dirty_against_commit=true来自第一次生成的未跟踪release证据目录，源码无修改；两次报告分别保留。此次使用最终带时间戳的MSI，不拿第一次同版本文件冒充最终包。

使用已安装EXE，四个隔离配置/安全库，无模型请求或模型回复fixture：

1. 旧容量六读者/180保存，实际1337读、混合0；本地容量8192/4096正确应用，清零恢复规则与采样0.25保留。旧API仍返回容量上限，不把小上下文的本轮请求预算视为所有字段必须相等。
2. 统一配置六读者/180整组保存，实际1150读、混合0；名称、baseURL、endpoint、参数、revision与预算属于同一捕获快照。
3. 真SQLite触发器拒绝更新：原参数缺项及已有参数两例均返回500，原会话/SQLite/参数恢复，缺项仍缺项。正常撤销故障后保存成功；同expected_revision竞争返回200/409，不能覆盖他方成功值。
4. 正常壳点击设置/内部滚动，用户上下文32768/输出1024、生效8192/1024、温度0.25，[原生设置](ui/budget.jpg)通过。同已安装EXE结束后重启，全部参数、名称、revision和两接口预算恢复；正常Ctrl+R/重新打开/滚动，[重启原生截图](ui/restarted.jpg)通过。
5. 四个SQLite库runtime_runs均0；Qwen ID仅用于参数规则，未实际切换原SWE。驱动均已消费退出0；Windows子进程Terminate返回1如实保留。

[正式主界面](installed-native-main.jpg)已恢复原SWE-2-medium、revision51、原聊天室、唯一island-kayak解锁、活动轮次0；原安全库保持。

仅关闭Web配置服务提取的正式交付子项。LLM resolve、跨工作区作用域、单写者/outbox/跨资源崩溃、SharedRunner及其它WBS矩阵继续开放。Browser严格在途撤销、跨来源commit严格down/up及新Target未因此通过；不改期限、不注入暂停、不重复简单点击碰窗口。Paint免测、微信不动、Opus暂停。ChatGPT订阅最小真实连通仍为独立实验，非正式Provider。

已公开[115预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.115)，四资产服务器长度/SHA及实际标签f6f8729一致，原始元数据已归档；未签名、不标latest、不发自动升级清单。整体Goal继续。
