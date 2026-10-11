# 聊天历史自动同步候选验收

099正式版已证实跨客户端写入后页面停留旧历史，刷新才可见。本次源码候选增加可重建SQLite房间版本投影、SSE失效通知、复用原权威分页及前端按ID合并。监听固定捕获工作区数据库，工作区请求校验；本地代数与AbortController隔离迟到响应；流式期间延后，正常结束/可见性恢复补齐；保留历史滚动锚点及选中项。它不替代outbox、会话epoch或业务恢复，也不派发模型。

## 实际结果

- 候选后台与正式099桌面壳组合，非新正式安装包。候选后台SHA `36082b471720adf7bf9c6fff3ae0ca256bf723a882d979cd97613c61649c0e6c`；原实际工程/安全库、完全访问和SWE-2-medium保持。两轮沿用唯一`island-kayak`，均completed/end_turn/drained并解锁，无新云端会话。
- 外部客户端正常发送方案附件，原页面无需刷新自动显示#675及完整#676回复，见[新消息](screenshots/external-message-auto.jpg)、[完整回复](screenshots/external-reply-auto.jpg)。不是以模型转述认定界面通过。
- 继续同一上下文发送实际代码附件；计算器真实一次completed，审计executed=true、result=5。详见[原始宿主与模型分层结果](integration-result.json)。原白名单revision35未改，net-tools维持原停用。
- 已滚动到#676中部时，新#677及完整#678持久化后，原段落和偏移保持。见[之前](screenshots/history-anchor-before.jpg)、[新消息后](screenshots/history-anchor-after-external-message.jpg)、[完整回复后](screenshots/history-anchor-after-external-reply.jpg)。后台空闲受控重启后位置仍保持，见[重启后](screenshots/history-anchor-after-backend-restart.jpg)；随后正常滚动可见#677，见[新任务](screenshots/code-review-message-after-restart.jpg)。本轮没有在断线期间追加大批历史，不外推该矩阵通过。
- 实际HTTP数字分页200/1解析正常，错误工作区409；SSE hello及首次history-changed正常，见[HTTP记录](http-probe.json)。重启后服务有原壳重新建立的连接；该连接事实与静态截图不替代断线期间新增消息补齐验收。
- 候选已退出，恢复完整Program Files正式099及其配套资源。099不包含本修补，后续正式出包后须独立复验。

## 失败与修正

1. 初次实际HTTP出现400：Serde扁平分页结构把数字limit视作字符串。已改直接字段并新增真实URI解析专项，三项Rust专项全部通过，重新offline build 48.92秒通过。
2. 首轮calculator未执行、宿主调用台账0：临时候选bin漏带固定DSH运行资源，插件声明不含计算器。原授权未变，不是用户撤权。从正式099复制并逐文件核对固定资源后，当前工具声明恢复，第二轮真实调用通过。[资源记录](dsh-runtime-copy.json)。保留首轮失败，不追写成功。
3. SWE代码审查提出流式结束缺唤醒钩子，但附件只含接线部分，遗漏实际`sendMessage`的finally。主会话核查已有`chatHistorySync?.resume()`，该结论不成立，不据此增加轮询或重复钩子。其余noop、事务快照、覆盖范围、锚点回退、ABA审查判断由实际代码及专项独立佐证。模型审查不是测试通过凭证。

## 自动检查与剩余范围

新增协调器5项针对性前端检查通过：ABA迟到与旧finally、忙时通知合并、在途/重连/删除、跨页覆盖、删除边界和游标循环。Rust3项通过：真实数字URI、跨连接提交/noop/回滚、房间删除重建。完整控制台在数字修正前为1413通过/0失败/6忽略，数字修正后已完成专项与offline build；正式构建门禁另行记录，不把旧全量计数当作新一轮全量。

尚待：正式新包独立复验；本页真实流式期间外部变化；断线期间超过200条新增/删除后的补齐；多工程/跨客户端竞争和迟到完整矩阵。当前同步是当前已加载区间的权威刷新，不提供跨页事务一致快照；持续写入由后续失效补读。删除锚点按原scrollTop有界回退，不承诺保留已删除文字的像素位置。
