# 0.2.109 改动与测试交接

当前正式安装0.2.109，冻结源码`09608d5f6889800d54b70ee99ec2e9163a89935f`，快照`36ef6fc9f76db5d7c55b413d37a861285330b183fcfaaa6d8f62598902b8aaf2`。正常构建实际退出0，六门pass，安装退出0，Program Files内1159文件逐长度/SHA一致。完整控制台1423通过、0失败、6既有忽略；关联8项与原生宿主1项通过。构建源码两路远端CI success，原回执见installed-validation。MSI SHA256 `fa910eb9312a209f6fdfc2a87f6654f225e69ecc1b952d8009f949296a9e07f9`。

## 改动

沿用108失败阶段轨迹，在原生观察pending登记后追加browser.observation_requested，仅含request_id与registered阶段。写入耗时计入原5秒预算，不改变输入排队、750毫秒宿主登记、权限、重试或页面行为，不向正式聊天界面增加调试内容。它不代表观察已送达或仍在等待。

## 正式实操与口径修正

两轮均使用原SWE-2-medium/revision51及唯一island-kayak；每轮仅一个computer_use_perform、一个模型attempt，单end_turn/drained，解锁且无重试补发。网页事件与SQLite真实终态独立核对，没有模型夹具。

1. 自然导航：可信click已sent/released，随后具体底层观察请求登记，网页正常跨来源导航；新页载入晚于up278.3ms，新页零输入。宿主合法返回新页观察，最终目标不可见而budget_exhausted，effect_observed/goalfailed。browser.observation_stopped为0。见[natural-navigation实拍](natural-navigation/final-reply.jpg)。**未命中等待资源更换**。
2. 正常GUI关闭右栏：可信click已sent/released，正常关闭后停止后续验证，native_browser_panel_unavailable，效果与goal verdict保持null。没有补发或第二次动作，所有收尾不变量通过。关闭动作返回比已登记观察晚约10.9秒，该观察已经进入证据，未命中在途等待；见[resource-close实拍](resource-close/final-reply.jpg)。不把登记后的关闭等同于回包之前关闭。

源码复核确认：PanelResource含generation/navigation_revision，**普通网页自身导航保留该资源**；文档新鲜度由DocumentSnapshot及真实URL另核。因此“已登记后自然导航必然触发waiting_resource_changed”的测试前提不成立。本轮没有发现应拒绝却采纳旧节点或把旧动作投到新页的证据。精确等待资源撤销分支仍需匹配请求仍在途且发生实际面板资源撤销，不能继续重复自然导航调整延时碰运气。

## 后续测试设计

Browser严格newTarget/跨来源commit处于down/up之间、精确在途资源撤销仍开放，和正常自然导航/正常关闭分开。下一轮转其它可实施矩阵，保留这些窄时序缺口，不修改产品延迟或私有宿主通道制造通过。插件许可撤回阶段、附件Goal/Relay/Agnes与账号边界、多工作区写入、日志配额/脱敏导出、WAL恢复及混合DPI仍按总体台账推进。

Paint免测、微信不动、Opus暂停、不使用子代理、不新建云端测试会话。两个网页服务正常退出0，模型活动0。发布属性与服务器资产核验另记录；没有签名或自动升级清单，不宣称全部验收完成。

## WAL迁移恢复与发布收尾

正式EXE的WAL迁移备份、独立恢复、新写入保全及真实迁移失败回滚已补正常UI实拍，通过范围和准备失败见[存储专项](wal-recovery/report.md)。其它升级环境矩阵继续开放。

109已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.109)，四资产服务器长度/SHA、实际tag与冻结09608d5一致。首次验证早于上传完成而报资产数不为4，属于采证顺序失败；上传命令实际退出0后重新验证通过，不当成产品失败，也不隐去原过程。未签名、不标latest、无自动升级清单。
