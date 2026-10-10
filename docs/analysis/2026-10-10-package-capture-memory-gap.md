# 整包构建记录采集与消费仍无界

当前源码直接核查确认以下消费链，不把已成功的 130 构建外推为内存边界通过。

1. scripts/package-all.ps1 的 Invoke-ArtifactBuild 用 stdout ReadToEndAsync、stderr ReadToEnd；进程结束后才 WriteAllText。长构建在此期间缺少已落盘原始输出，进程异常或宿主退出可能丢失已产生日志。
2. 完整 stdout 又经 split 保存到 script:artifactBuildMessages。后续 WebView2 Loader 导出读取该入口，不能仅删除数组而不替换消费者。
3. scripts/lib/cargo-build-evidence.ps1 的 Save-PackageCargoBuildEvidence ReadAllBytes、解码、split、ConvertFrom-IdentityJson，完整记录在多种形式下重复驻留。只替换采集不会解决归档解析无界。
4. 构建失败分类再次 Get-Content -Raw 读取 stderr；日志流式落盘之后仍需替换此消费点。

因此下一修补必须同时提供：stdout/stderr 并行、有界缓冲、原始字节增量落盘；日志创建独占；磁盘写入失败及时停止本次拥有的构建子树且等待两条采集任务收尾；退出码与采集完整性分别记录，不能用截断前缀冒成功回执。后续消费者应逐条验证 JSON 行并提取必要构建观察，保留唯一最终 build-finished 和原始文件摘要绑定，不重新驻留完整输出。

验证需真实双管道大输出与增量可见性、中文原字节、采集磁盘错误、构建非零、既有日志不覆盖、唯一成功终态，以及 PS5.1/7 兼容。不能用一个正常小日志或仅 syntax parse 宣称通过。全机 Cargo 串行及原始发布身份规则保持，不启动新正式构建冒充采集修补验证。

这是新的实现依赖审计，未修改生产采集，6.1/6.4 门禁仍开放。130 完整回执与安装证据保持历史有效，但不证明上述边界。近期 Conversation/Session 修补也没有解决此链。
