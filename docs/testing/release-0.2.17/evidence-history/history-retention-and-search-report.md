# 历史保留与检索真实基线

此前写入路径在保存前把内存工作集裁到 2,000 条，再清空 SQLite 消息表后重写工作集。独立 10,000 / 100,000 条数据库实际启动后，SQLite 本身都只剩 2,000 条，最旧消息无法定位。这不是搜索界面的分页限制，而是历史永久丢失。

父代理修改持久写入为增量保留和显式删除事务后，2026-09-26 20:33:18 构建完成以下真实 HTTP 验证：

| 验证 | 结果 |
|---|---|
| 启动 10,000 / 100,000 条历史 | SQLite 数量保持完整 |
| 每千条一个的 unique-needle-731 | 10 / 100 条全部找到 |
| 第一条、跨 2,000 条工作集边界 | 准确定位，首条 position=1 |
| Unicode CAFÉΩ 搜 caféω | 100,000 条中 50 个匹配 |
| 真实 /api/chat/send 新增一轮 | 总数 100,002，原 needle 仍 100 |
| 进程关闭后同目录重新启动 | 总数 100,002，首条可定位，needle 100、Unicode 50 |

证据保留：旧版失败目录 `history-perf-workspace`；修复后基线 `history-perf-workspace-v2/results.json`，输入数据库约 159 MB；脚本 `history_performance_validation.py`。测试 exe SHA256 为 `e4f6f1f06779785488275c759a4b55259423108fa29eda318803fcecfa862b02`。

同一二进制与隔离目录，每场景运行 6 次，后 5 次中位数：

| 场景 | 1 万条热中位数 | 10 万条热中位数 |
|---|---:|---:|
| 子串命中 | 237 ms | 2,389 ms |
| 子串不命中 | 225 ms | 2,330 ms |
| Unicode 搜索 | 255 ms | 2,278 ms |
| 最旧消息定位 | 44 ms | 318 ms |
| 跨工作集边界定位 | 98 ms | 921 ms |
| 接近最后的消息定位 | 142 ms | 976 ms |
| 最新 100 条消息分页 | 23 ms | 29 ms |
| 用量/序号 insights | 76 ms / 228 KB | 720 ms / 2.58 MB |

这些结果证明子串扫描与全量序号载荷需要优化。本轮随后增加可丢弃 FTS5 trigram 派生索引、可见序号索引，以及当前显示消息的序号请求。消息表仍是唯一真值：变化先进入普通 dirty 队列，搜索事务按 Rust Unicode lowercase 刷新索引，并在同一事务搜索完整快照。索引故障时回退完整扫描，不影响消息写入。首次构建和优化后性能待新构建实测，不能把本表当作优化结果。

本报告是实际 API 与存储证据，不替代父代理负责的真实桌面截图验收。


## FTS 第一轮真实验证（v3）

新生产构建20:49:22包含20:48:08之前的源码；目标为隔离18767，已停止并恢复主8765自动发现。`chat_insights::tests` 5/5、`history_persistence::tests` 4/4通过，后者含共享附件跨fork引用、旧历史显式删除/回滚/分支、提交失败不泄漏缓存。

首次在110,000条历史库构建完整索引并返回搜索为7,942ms。后续100,000条房间实测：

| 场景 | 热中位数 | 最大值 | 返回字节 |
|---|---:|---:|---:|
| search_hit | 263.24 ms | 268.37 ms | 32262 |
| search_miss | 206.31 ms | 217.72 ms | 62 |
| around_old | 200.23 ms | 202.75 ms | 53005 |
| around_working_set_boundary | 217.93 ms | 250.21 ms | 53262 |
| search_unicode | 259.31 ms | 265.18 ms | 32246 |
| search_turkish | 256.22 ms | 267.28 ms | 32246 |
| search_sharp_s | 269.03 ms | 279.32 ms | 32246 |
| search_emoji | 271.42 ms | 301.01 ms | 32246 |
| around_tail | 279.01 ms | 294.21 ms | 33784 |
| message_page | 24.61 ms | 52.52 ms | 129683 |
| insights | 303.19 ms | 306.56 ms | 3009 |

`İSTANBUL`搜`i̇stanbul`、`STRAẞE`搜`straße`、emoji连续子串均50个匹配；原Unicode用例亦50个。真实新增消息后100,002条、needle100；重启后首条position1、needle100、Unicode50再次通过。索引110,038行、dirty=0；测试库从约159MB增长到379MB，派生索引有真实存储成本。

证据：`history-perf-workspace-v3/results.json`、`chat-insights-tests.log`、`history-persistence-tests.log`；脚本`history_fts_performance_validation.py`。该结果是debug构建本机测量，非跨机器性能承诺。

### v3之后需要复测的两项修正

只读EXPLAIN发现SQLite计数误选旧索引，100k COUNT为147ms，指定已存在的可见覆盖索引为5.9ms，已给三处COUNT显式指定索引。首次FTS单写事务约8秒可能超过其它写请求5秒等待，已改为冷构建每500行提交、允许源写入穿插；最后短事务只在dirty已完整纳入时用FTS，否则完整扫描真实消息，不使用部分索引。**这两项增量未被v3二进制覆盖**，待新build测试及冷构建期间真实聊天并行写入验证。

### v4 预检记录（生产性能尚未开始）

21:29:48 的集成测试二进制已包含新的分批冷构建/COUNT索引，直接运行`chat_insights::tests` 5/5通过（chat-insights-v4-tests.log）。同二进制`history_persistence::tests` 3/4：最后一个附件引用删除后，旧断言要求物理删除1个文件，实际为0。主代理确认刚按04 S4.3把附件GC改为只读演练：最后引用消失只列候选，仍不物理删除。此失败是旧测试与新明确保留契约不一致，不能通过恢复物理删除来补绿；主代理将更新断言，后续再验证。此前v3的4/4仅代表旧GC契约，不视为新版覆盖。

## 分批冷构建与完整历史最终验证（v5）

本次生产构建成功（fts-v4-integrated-build.log，1m24s），exe SHA256 `d9f8ce1e5ad340c652b4265b7e3e1a965c911621bac4043703ed16d0143a44b6`，隔离端口18767，不读取用户会话。v4脚本曾把新响应错误按runtime子对象读取，实际DTO已flatten为顶层status/run_id；该次脚本中断目录与备注保留。修正脚本后使用新v5目录完整验证，没有覆盖旧证据。

- 冷索引处理与真实`/api/chat/send`并发：发送1654.78ms完成、真实run状态completed且两条消息已提交；搜索662.99ms返回房间完整10个匹配。后续首次单独搜索仍花10193.70ms完成剩余索引准备，不能把662ms宣传为全库索引建立耗时。
- 新run的结构化trace返回`source_message_id`与发送响应中的用户消息ID完全一致；跨房间around游标为404。
- 两次真实新增后100004条，旧needle仍100。停止进程，仅删除派生FTS虚表，再启动从消息重建：100004条、首条position1、needle100、Unicode50全部通过。
- Unicode CAFÉΩ、İSTANBUL→i̇stanbul、STRAẞE→straße、emoji子串均找到50条。删除/更新/rowid复用/回滚由真实SQLite窄测试覆盖。
- 最终索引110042行、dirty 0，数据库380,223,488字节；这是派生索引的实际存储代价。
- 每场景6次，后5次热中位如下。量测期间同时编译集成测试，因此保留实际数值，不宣称跨机器或完全无负载基准。

| 场景 | 1万热中位 | 10万热中位 | 10万响应字节 |
|---|---:|---:|---:|
| search_hit | 23.03 ms | 154.31 ms | 32262 |
| search_miss | 24.51 ms | 38.59 ms | 62 |
| around_old | 42.01 ms | 42.26 ms | 53005 |
| around_working_set_boundary | 41.78 ms | 63.08 ms | 53262 |
| search_unicode | 19.80 ms | 194.84 ms | 32246 |
| search_turkish | 25.19 ms | 188.36 ms | 32246 |
| search_sharp_s | 21.39 ms | 162.90 ms | 32246 |
| search_emoji | 18.13 ms | 149.96 ms | 32246 |
| around_tail | 16.03 ms | 42.40 ms | 34457 |
| message_page | 12.67 ms | 24.55 ms | 127766 |
| insights | 23.72 ms | 195.63 ms | 3043 |

代码定向验证使用21:36:23同一集成test二进制：chat_insights 5/5、history_persistence 4/4（更新为最后附件引用消失仍只读GC，不物理删除）、content_delivery 3/3（新增真实文件全hash一致范围206，范围外等长篡改仍409，损坏文件保留）、clawbot_media 1/1（真实图片共享blob、inbox无base64）、tool_result_spill 1/1（真实磁盘Unicode原文可还原，损坏对象不覆盖）。各日志 `*-final-v5-tests.log` 与 `tool-result-spill-v4-tests.log`。这些测试不替代最终安装版GUI截图。

结果原文：history-perf-workspace-v5/results.json，history-fts-v5-validation.log。18767测试进程已停止，temp发现URL已恢复8765；没有占用桌面。release资源错版门禁仍等待正式release二进制，不能由本次debug通过推断。
