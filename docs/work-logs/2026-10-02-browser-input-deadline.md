# Browser传输期限边界

用户要求继续排查CU占用且不阻塞本地工程。只读查10月1日、2日客户端元数据：旧线程CU为local，当前失败为durable；没有明确独占/锁/端口占用记录。存活node_repl与旧线程启动时间相近不能证明独占，CIM父关系查询被系统拒绝访问，未提权、杀进程或删锁。

继续审查Browser传输，发现等待线程到期后未及时清理Pending时，领取和回执入口仍可接受过期请求。新增2项回归先失败（exit101），之后让两个入口检查同一单调时钟deadline。新增关闭/导航后原回执、迟到回执与新请求隔离的行为验证。

Web离线build退出0；输入传输模块6通过/0失败，其中4项新增。真实GUI、Qwen、Paint仍未验收，未安装候选或解除原隔离。其它既有未提交修改保留。

[修复与验证报告](../analysis/2026-10-02-browser-input-deadline-review.md)，[工程证据](../testing/browser-input-deadline-2026-10-02/evidence-index.json)。
