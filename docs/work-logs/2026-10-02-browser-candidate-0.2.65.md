# Browser独立候选0.2.65

按用户要求在工程验证稳定后构建独立候选，保留063正式包和064旧候选。使用065专属manifest和包根，准备阶段冻结1281源码/资源文件，正式阶段消费同一冻结记录。六项发布门与10项产物来源均通过；当前Browser源文件哈希与聚合测试记录一致。

生成MSI 275647003字节，SHA256 `fa80c6faf13383f926f03a4617b5a406918b9b602e2fbf08dbd2f7e89c35eb19`。只读MSI数据库/CAB解包1150文件逐项匹配暂存包；290项DSH固定资源完整校验通过，安全扫描无发现。未安装、重启、上传或放行隔离。

本候选含deadline修复；旧064不含。真实GUI/模型/Paint边界仍开放，到本地CU恢复依赖点停止。

[候选报告](../testing/release-0.2.65/change-report-and-targeted-test-plan.md)，[完整身份与证据索引](../testing/release-0.2.65/evidence/index.json)。
