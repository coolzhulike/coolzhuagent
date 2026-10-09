# 0.2.116 改动与定向验收交接

## 修改与风险边界

修复模型配置与工程切换并发时，名称来自旧工程、参数/容量/revision来自新工程的真实混读。SessionConfigService持有既有WorkspacePin至整个HTTP操作及响应派生结束；切换期间配置409，配置进行中切换409，完成或错误返回后释放。保持原锁序、参数规则、权限及SQLite失败恢复，无新队列或缓存。[正式115原失败、候选及方案](../2026-10-09-session-config-scope/report.md)有2041读/180成功切换与六读者48限量混读样本，首准备错误404独立保留。

源码 `aa72c2545d3178ec993a01bc8281c7bba1c0d847`，冻结快照 `1ebb4a559f0333fd2d90f6f1dbef58d10f1181122bd8850689bb38d94df94457`，dirty=false。正常release构建实际0、六门pass、正常管理员MSI安装0；Program Files 1159项逐长度/SHA一致，CLI116/源码一致；同源两路CI37902125115/37902120763均success。MSI 285728264字节，SHA256 `83b106cc9b46d3c587d3d3bf9d8b0c8d1e82a8a9fb50b0af9e7d10eb4df05a74`。候选完整Web1423/0/6既有忽略，另lib8/native-host1通过，不新增镜像实现的单元测试。

## 正式安装程序复验与用例依据

所有专项使用真实Program Files EXE，SHA `d931957b13ec7268edaa518783d43839c8a8ef32a0117e343038c318e375129f`；配置竞争零模型，不作为模型连通验证或模型回复夹具。

1. 六路读取持续忙期：154有效读、180切换均409、混读0；停止读者后正常切换200。此组只证明忙期拒绝。
2. 单读者20ms间隔真实交错：77次切换200、103次409、75次有效读取、44次读取409、混读0；读者停止后切换200。工程A/B相同会话ID但名称、温度、revision及本地容量不同，按整组核对。
3. 真SQLite写锁阻住会话更新，参数已发布期间切换409；释放后保存200。正常切到B，其名称/温度0.75/revision20均未变。非法温度400及旧版本409退出后都能正常切换，pin无泄漏。首只读观察撞Windows共享占用退出1，原cleanup保留；第二轮只改观察器，不改产品期限，不重发业务请求，完整通过。
4. 正常原生设置和滚动，显示[工程B温度0.75/容量16384](scope/save-scope2/native-b.jpg)；正常工程目录输入切换，显示[工程A温度0.35/容量8192](scope/save-scope2/native-a.jpg)。同已安装EXE结束后重启，名称、参数、revision逐项恢复，[重启原生实拍](scope/save-scope2/native-restarted.jpg)通过。复用候选驱动的输出文字仍称“候选”，以实际Program Files路径和EXE SHA为准，未拿debug结果充当正式。
5. 八个隔离SQLite库runtime_runs均0，无模型请求或新云端会话。所有成功有限驱动实际0；Windows terminate子退出1独立记录。[正式原主界面](installed-native-main.jpg)恢复SWE-2-medium/revision51/原聊天室/唯一island-kayak解锁，活动轮次0，原安全库保持。

构建协调收据first_outer_wait文字继承115模板，仅解释115历史，不表示116发生第一次失败；116只有本次正常直接子等待构建、实际0。

## 未完成项

只关闭模型配置操作进行期的工程一致性修复及正式交付。客户端迟到请求expected_workspace/ABA、完整SessionConfig/ToolDispatch/SharedRunner、跨进程单写者/outbox及跨资源崩溃继续开放。Browser复杂九步已有正式证据，但新nativeTarget、跨来源commit严格down/up、在途观察撤销及HRESULT/资源竞争仍未完整实机通过；115最新正常关闭晚于登记16721ms且父轮已完成，不计严格通过。不改时限或注入暂停，不重复简单点击。

其余供应商凭据、附件/许可/记忆/调度/多屏等以[总清单](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)为准。Paint免测、微信不动、Opus暂停。未签名、预发布、不标latest、不发自动升级清单。整体Goal继续。

已公开[0.2.116预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.116)，四份资产的GitHub长度/SHA256及实际tag指向均已独立核对，见installed-validation/github-published-metadata.json与github-tag.json。
