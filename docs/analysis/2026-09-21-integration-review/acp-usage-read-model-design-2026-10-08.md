# ACP请求统计与轨迹读取整合

真实发现：Devin ACP发送前台账持续增加，而chat_usage_events保持539条；统计页及轨迹只读取后者。此前097“539与投影一致”仅证明该旧投影一致，不等于全部模型请求完整入账。旧539条均已有精确acp:attempt_id，不能直接相加，也不能借call/run猜配。

采用查询层合并，不修改ACP状态机、锁或权限，不补写第二套持久账本、不在每个失败分支添加hooks。未使用record_acp_attempt删除。request_usage模块同时服务统计与轨迹，避免两份SQL口径再次分叉。

权威归属和状态来自devin_acp_attempts.scope_json/state/protocol_stop。匹配同一acp:attempt_id的旧投影只在工程、房间、Agent以及已知run/turn均匹配时复用真实token/mask/时间；冲突投影不重复显示，也不带入其它作用域。普通HTTP重试仍各留attempt、logical_request_id按原口径去重。旧库无ACP表照常读取HTTP；JSON损坏使查询失败，不能伪造零值。

prepared/submitted为进行中；not_sent明确未发送；terminal/end_turn完成，明确其它stop呈取消/拒绝/上限中断；unknown保持远端结果未知。prepared可直接迁移unknown，因此unknown派发计数也未知，不把drained或protocol_stop=end_turn当成状态已确定。读取层不因“时间陈旧”自行改写prepared。单条SQL获得一致读取快照，未知token不参与已知合计，不从context占比估算、不估价。台账没有创建时间的新记录返回NULL，轨迹按精确run关联；旧HTTP仍兼容原turn/call回退。

SWE方案审查已沿唯一island-kayak完成，原文见[正式100实际审查](../../testing/release-0.2.100/installed-validation/integration-result.json)。主会话取舍：采纳未知状态独立表达和精确去重；拒绝按call/turn猜配attempt或在读取层把陈旧prepared改unknown。附件没含journal完整实现不能据此断言其迁移/恢复不存在；模型自述executed=true未出现在独立原始审计，不作为原始字段证据。

验证依序为：少量数据库状态/去重/归属/旧库检查、offline build、完整控制台回归、真实既有库insights与trace核对、同一SWE从本页流式提交后的增量及实际UI、正常正式包安装后独立复验。候选与正式证据分开，不能把100追认为含统计修补。
