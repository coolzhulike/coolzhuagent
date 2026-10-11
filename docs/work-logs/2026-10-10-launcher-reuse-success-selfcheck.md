# 启动器复用成功自检

复用预检原先将已验证证据降为 bool，转发 shell 成功后直接返回，不写本次自检。现预检返回 Option<ReuseVerification>，保留实际检查列表；转发成功后写本次成功、当前 timestamp/launch_id、已验证复用证据及实际新 shell PID。

已有后台 PID 无可靠回读则为 null；resolved 保留已解析请求快照，不伪造 observed 路径，实际复用检查记录在 reuse_verification。保存失败明确提示旧文件不能代表本次结果，不为诊断写入失败回收已有后台或已转发 shell。用户观测历史更新与转发后身份竞态仍须另行核验。

回归使用实际自检文件先写旧成功，再覆盖当前冲突失败，再覆盖复用成功，核对旧错误清除、当前 launch_id、shell PID、完整 reuse JSON 与后台 PID 为空。复用判定本身继续由既有身份正负测试覆盖，本回归不启动产品实例。

离线 launcher 构建实际退出 0；完整测试实际退出 0，库 64/0、binary 9/0，共 73 通过。日志 tmp/analysis-launcher-reuse-selfcheck-build.txt、tmp/analysis-launcher-reuse-selfcheck-tests.txt。实装 0.2.128 尚不含修复，正式复用/升级验收与 32WBS 仍未闭环。
