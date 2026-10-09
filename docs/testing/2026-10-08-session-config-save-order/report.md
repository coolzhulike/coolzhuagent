# 模型配置保存顺序修补候选验证

保存流程同一同步块持有既有会话存储锁，按store→config顺序发布参数和更新会话；旧参数Option在配置mutator内捕获，回退恢复原值或原本不存在的缺项，不覆盖其它revision。没有增加第二把全局锁、模型预算或权限绕行。SessionStore::save原有committed_state回滚保留，不重复实现。

实际offline build退出0，完整Web1423通过/0失败/6既有忽略，lib8/native-host1通过；未新增镜像单测。真实候选EXE SHA见facts.json，隔离工作区和安全库、不发模型请求、不写正式SWE库、不创建云端。configuration-save-check仅是无凭据、未发送的配置占位标识，未模拟模型响应，不能当模型连通或wire验收。

隔离SQLite创建实际INSERT失败触发器：原参数不存在及已保存各一次，POST500；内存会话、数据库会话和参数均恢复，原本缺项保持缺项。版本号单调推进两次（发布及回退），不回退revision。撤销故障后正常保存恢复。同expected_revision两个并发请求一方200/另一方409，实际获胜名称与temperature在API/数据库/配置一致。

正常页面另一次保存失败显示明确错误，草稿保留；关闭重开仍保留未保存草稿，这是前端行为，未误称数据库新值。刷新后回到原已保存名称；故障撤销后正常GUI保存成功。同EXE重启后名称、temperature和revision10保持。四张实拍覆盖[错误与草稿](ui-failure.png)、[重读原值](ui-reloaded.png)、[正常保存](ui-saved.png)和[重启恢复](ui-restarted.png)。测试进程正常由自己的停止标记清理，GUI启动指针恢复，未操作Windows审批。

仅候选源码通过，当前正式0.2.111不包含此修补。完整SessionConfigService、读快照原子性、跨文件崩溃一致性、跨进程单写者/outbox仍开放；完整交付需后续统一出包实机复验。没有用本次无模型配置验证替代真实会话长程验收。
