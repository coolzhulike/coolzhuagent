# 0.2.113 改动与测试交接

本版修复会话模型配置读取中参数、连接地址与生效预算可能来自不同版本的问题。冻结源码 `09fa80c1d5777687c7bac75b9aa1f0b6308b35e6`，源码快照 `9a68d7403ca97b1f8697662b7e0587b1d0e80f6bb231a0dcd4a5390090c8be7a`。正常 release 构建子进程退出0、六项发布门通过、正常管理员安装返回0，Program Files 内1159文件逐长度和SHA一致。MSI 285834760字节，SHA256 `f783d7847bbb61dc7dde66d420d1fe35d6061386c6228feb5bd15f96607d7cdb`。同源两路远端CI 37889528521/37889524478均success，原始状态见installed-validation。

## 行为与模块职责

原112真实并发读取捕获参数16384/1024而生效预算8192/512的混合样本，见[候选及原失败报告](../2026-10-08-config-read-snapshot/report.md)。现在model-settings GET在原会话锁内单次捕获配置，名称、revision、指定参数、Base URL、Endpoint和预算从同一捕获值构造；摘要和执行DTO接受已捕获参数，避免暗中重读连接。局部模型容量同次捕获，纯预算规则留在session_model_config模块；插件目录IO在锁外，不引入第二全局锁，不持std锁跨await。密钥状态、品牌、记忆模式仍由对应模块读取，不宣称整份响应的所有字段事务一致。

正常保存失败回退为112已有修补，本版未改变权限、工具、Base URL原配置、API Key或超时。完整SessionConfigService、跨进程写者/outbox及config与SQLite之间崩溃一致性仍开放。实现与方案见[读取快照设计](../../analysis/2026-09-21-integration-review/session-config-read-snapshot-2026-10-08.md)。源码实际offline build退出0、Web完整1423通过/0失败/6既有忽略（另lib8/native-host1）原始日志见候选报告，未新增镜像单测。

## 正式安装版实操

使用Program Files的113正式web EXE与正式原生壳，独立8768工作区、安全库、无凭据模型配置，未模拟模型回复、模型请求0。六路读取与180次名称/温度/地址/Endpoint/容量联合保存竞争，实际1149次读取均字段组合一致，混合样本0，HTTP失败0。当地服务容量8192继续约束用户32768，上限输出1024和温度0.25保持。

正常点击设置、滚动参数后[原生预算截图](config-read-snapshot/ui-budget.jpg)同时展示32768/1024输入和实际8192/1024生效值。停止隔离后台、用同一已安装EXE重启，再正常Ctrl+R刷新、重新打开设置并滚动，[重启截图](config-read-snapshot/ui-restarted.jpg)展示同一参数、生效预算与0.25；名称、revision及所有参数通过接口逐项核对保持。两次隔离后台均由驱动正常terminate清理，Windows子进程退出码1为明确停止结果，驱动实际退出0；不把子进程退出码伪写成0。自有壳经路径、启动UTC ticks、摘要核验后停止。

原8765正式113后台与壳已恢复，[原生主界面](installed-native-main.jpg)保留SWE-2-medium、原聊天室、revision51、唯一island-kayak绑定且解锁、活动轮次0。本轮新云端会话0、原安全库不清除；qwen3.8-flash仅为隔离参数配置规则测试，原模型未切换。Paint免测、微信不动、Opus暂停。

## 后续针对性测试设计与未完成项

1. 六路并发读取、交替保存两组名称/连接/温度/容量，逐响应核整组关系及生效预算，不能只检查HTTP200。真实112失败样本应保留。
2. 复验用户预算小于本地容量、读取失败/保存回退后重读、同EXE重启恢复；其它全局配置修改与旧容量端点另验。
3. 跨工作区和跨进程并发、跨资源崩溃恢复仍需统一配置服务设计，不外推本项完成。
4. Browser连续只读驱动已捕获登记后4.6152ms阶段，但正常GUI关闭晚10960ms、超过5秒窗口，严格在途撤销仍未通过，见[真实失败、时钟及实拍](../release-0.2.112/browser-continuous/report.md)。不重复简单点击碰窗口，不改产品期限或注入暂停冒充通过。
5. 插件许可/后代树、附件和其它免费模型凭据、动态流式/混合DPI、共享Runner/单写者、全局配额与Windows矩阵按[总体台账](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)继续。ChatGPT订阅最小真实连通为独立实验，不宣称正式Provider交付。

已公开[113预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.113)，四资产服务端长度/SHA及实际标签09fa80c一致，原始元数据和验证脚本见installed-validation；未签名、预发布、不标latest、不发布自动升级清单。整体Goal继续，四项总体未完成。
