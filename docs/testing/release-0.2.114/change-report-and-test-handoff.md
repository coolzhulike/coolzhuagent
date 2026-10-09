# 0.2.114 改动与测试交接

本版修复旧会话容量接口没有应用本地模型容量约束的问题。冻结源码 `95c2704140f54db375fd1743c9fbdb1987ffda3c`，源码快照 `de8bd8225b2e241308b26aeb51f0d0b37af259edb278bae87bfe92c87e8ec782`；正常 release 构建/管理员安装实际退出0、六项发布门通过、Program Files 内1159文件逐长度/SHA一致。MSI 285818376字节，SHA256 `754d96f4f2ffa070b349b0f1855da599cb2db9fd7c7e1643b45f1ca1d4906483`。同源两路远端检查37892473439/37892469072均success，原状态已归档。

## 问题、行为与边界

正式113的旧GET曾返回32768，而统一页生效8192；清零旧POST返回1000000/65536，而统一页8192/4096。原失败及候选见[专项报告](../2026-10-08-legacy-model-limit/report.md)。本版旧GET在会话锁内捕获一次参数/本地容量；旧POST使用已经正常保存发布的配置构造响应，并复用纯预算规则模块。DTO接受捕获参数，避免构造时暗中重读连接。没有增加全局锁或持std锁跨await，没有改变权限、工具、超时或原模型密钥。

旧容量接口仍返回容量上限，保留旧输出字段语义；统一页的本轮输出还受请求预留约束，不能把二者在任意极小上下文下全部等同。Devin容量未协商的旧接口仍明确拒绝，不伪造其上限。本次仅关闭旧容量接口子项，完整SessionConfigService、跨工作区写者和配置/SQLite崩溃一致性仍开放。

源码实际offline build0，已有完整Web回归1423通过/0失败/6既有忽略，另lib8/native-host1通过，原始日志见候选专项。未新增镜像单测。旧容量竞态在正式113未捕获，实际复现的是本地约束遗漏；不要将代码读锁分裂风险写成已捕获混合样本。

## 正式安装实操与测试用例依据

1. 用Program Files正式web EXE、独立8768配置和安全库，正常保存用户32768/输出1024/温度0.25，本地服务8192/4096。旧GET与统一页均生效8192/1024；旧POST清零后均生效8192/4096，温度仍0.25。
2. 六路并发读取与180次交替配置保存，实际1313次读取，混合样本0。逐响应核overridden与整组预算关系，不能仅判HTTP200。原始两组参数/回执见legacy-model-limit/facts.json。
3. 再用旧POST保存32768/1024，正常原生点击设置与滚动，[设置截图](native-settings/budget.jpg)同时展示填写32768/1024、生效8192/1024和温度0.25。
4. 正常停止隔离后台，用同一已安装EXE重启。名称、全部参数、revision及两接口预算逐项保持；正常Ctrl+R、重新打开设置、滚动后的[重启截图](native-settings/restarted.jpg)通过。
5. 两个独立数据库runtime_runs均0，无模型请求、无凭据、无模型回复fixture。qwen ID仅用于参数规则检查，未将原SWE切换到Qwen。隔离后台由驱动正常terminate，Windows返回1有明确清理记录，驱动实际0；自有壳经路径/启动UTC ticks/摘要核验后停止。

[原生主界面](installed-native-main.jpg)已恢复原SWE-2-medium、聊天室、revision51、唯一island-kayak且解锁，活动轮次0。原安全库不清除，Paint免测、微信不动、Opus暂停。

## 继续开放的验收

Browser严格在途撤销上轮只读捕获登记后4.6152ms，但正常GUI关闭晚10960ms超过5秒，未通过；[原时钟及实拍](../release-0.2.112/browser-continuous/report.md)保留。本版不重跑简单点击、不改期限、不注入暂停。跨来源commit严格down/up、新原生Target、HRESULT与资源同时变化仍开放。

插件许可/后代树、附件Goal/Relay及视觉降级、流式跟随/混合DPI、多工作区调度、共享Runner/单写者、凭据/配额/故障矩阵继续按[总体台账](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)推进。ChatGPT订阅最小真实连通为独立实验，不宣称正式Provider交付。整体Goal继续，四项总体未完成。

已公开[114预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.114)，四资产服务端长度/SHA和实际tag95c2704一致，原始元数据与核验脚本见installed-validation；未签名、预发布、不标latest、不发自动升级清单。
