# 0.2.112改动与测试交接

冻结源码`72be2b53f2c712ba7e7dced6686b5399f23fe3aa`，快照`d03a15531b1e0fe0a61478c235e89833cd4e4bd8f90675e899086892f75ac603`。正常release真实子进程退出0、六项发布门通过，正常安装返回0；Program Files内1159文件逐长度/SHA一致。MSI 285830664字节，SHA256 `c9891e7e5b9151b31bef06024b17b0e8bb4b08ea1a8ea9a63838ab783a85b925`。冻结源码两路远端CI 37884844905、37884840195均success，原始状态位于installed-validation。本报告不继承其它提交CI。

## 行为变化与职责

模型配置保存原先在会话读取、参数发布与SQLite更新之间释放会话锁，旧参数也在配置锁之外读取，失败时可能恢复成错误值或新增原本不存在的参数项。现在同步保存块持有原会话锁，依次完成校验、参数发布、会话更新和失败回退；旧Option在配置锁内捕获，失败时Some恢复原值、None删除新增项。锁顺序沿用store→config，响应读取await在同步块之外，无第二全局锁、不把std锁跨await。SQLite既有SessionStore失败恢复机制继续负责数据库和内存恢复。

参数校验模块继续只负责参数规则，主层负责权限、revision与持久化。不改变API Key、Base URL、工具资格或模型超时。跨资源崩溃一致性、完整SessionConfigService、outbox及共享Runner仍未完成。设计依据见[保存顺序方案](../../analysis/2026-09-21-integration-review/session-config-save-order-2026-10-08.md)。

此前ACP历史重放协议测试共用3秒阶段期限导致提交前超时。仅cfg(test)握手和提交分别使用30秒，断言保留，无重试/忽略，产品预算不变；本版首次包含此测试修补，111不包含。既有[失败与修补](../2026-10-08-acp-replay-test-budget/report.md)保留。实际offline build0、完整Web1423通过/0失败/6既有忽略（另lib8/native-host1）见[源码候选原回归](../2026-10-08-session-config-save-order/report.md)，远端本版同源CI成功。

## 正式安装版实操

使用Program Files正式112 EXE与原生壳，在独立8768工作区进行配置保存故障验证。无凭据配置占位符只用来保存参数，不发模型请求、没有模型夹具或伪造模型回复。真实SQLite触发器分别使原缺项/原有项保存返回500，参数、数据库和内存恢复，revision明确增加；撤销故障后同revision两方并发一方200、一方409，唯一获胜值一致。

正常GUI填写失败草稿并点击保存后明确显示错误；刷新读取原值，正常再次保存显示已保存。停止验收进程后用同一正式EXE重启，名称“配置失败后恢复成功”、temperature0.25和revision10恢复一致，原生截图：[错误](config-save/ui-failure.jpg)、[重读](config-save/ui-reloaded.jpg)、[正常保存](config-save/ui-saved.jpg)、[重启](config-save/ui-restarted.jpg)。触发器已撤销，隔离后台与壳均停止；没有改原运行库。

恢复原8765正式控制台，SWE-2-medium、原聊天室、revision51与唯一island-kayak未变化、无锁和活动运行；[主界面](installed-native-main.jpg)、[设置读取](installed-native-settings.jpg)为真实原生截图，读取后正常关闭，未保存原配置。本轮安装及配置验证阶段模型请求0、新云端会话0；随后Browser专项仅发起一次真实SWE请求并正常收尾，详见[在途撤销未命中记录](browser-pending/report.md)。该专项未命中严格撤销窗口，不将基础点击成功计作撤销通过。Paint免测、微信不动、Opus暂停。

## 后续针对性测试设计

1. 参数项不存在及存在两条路径，故障保存后分别确认不存在/原值，不能只看HTTP500；同时核数据库与内存名称、revision。
2. 相同expected_revision并发修改应仅一个成功，409不能覆盖赢家；正常保存后同EXE重启逐参数恢复。
3. GUI失败应保留本地草稿并明确错误，刷新后读取权威旧值；撤销故障再保存应成功，不重复发送模型请求。
4. 继续跨进程写入及config→SQLite间崩溃恢复矩阵。当前证据仅覆盖同进程保存顺序与真实SQLite失败，不外推崩溃原子性。
5. Browser严格新nativeTarget/跨来源commit发生在down/up间、确切在途观察撤销、失败HRESULT与资源变化同时发生仍缺。111关闭晚于登记5792ms只验证sent/released及效果未知，不能算严格窗口通过。

## 未完成及分发

其它插件许可/后代树、附件/免费模型凭据、动态流式/混合DPI、共享Runner/单写者、全局配额及Windows矩阵按[总体台账](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)继续，Goal active。已公开[112预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.112)，四资产服务端长度/SHA与实际tag72be2b5匹配；元数据和核验脚本位于installed-validation。未签名、预发布、不标latest、不发自动升级清单，不宣称全部验收完成。早期采证脚本路径错误、JSON时间被PowerShell转成DateTime导致身份核验拒绝均属验证驱动问题；已用同一时刻ticks准确比对并安全清理，没有放宽产品进程核验。
