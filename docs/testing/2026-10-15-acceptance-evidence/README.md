# 2026-10-15 验收证据

`manifest.json` 记录每份收据、日志和代表帧的来源、字节数与 SHA-256。这是验收归档，不是发布包、SBOM 或完整合规证明。

- `installed-0.2.133/`：安装载荷、513个静态资源、重启持久化、原生动画与历史对账。首次正常启动选到C用户目录的失败也保留；正式通过启动器持久选择D后，安装入口、原库与资源复验通过。首次/日常、减少动态、恢复不重播及Esc/跳过按钮/演出区域点击已分别留证，原生资源故障交接仍缺。
- `goal-candidate/`：安装版防环误拒反例、当前修复的构建/回归/HTTP链与完整测试。模型决定为本地受控夹具，产物由implementer真实`write_file`生成。**这个修复尚未进入已安装0.2.133。**
- `c-drive/`：只读容量、归档哈希和引用核验。16个手工候选目录的文件分配量290,115,208字节；实际删除及释放均为0，独有内容和junction目标继续保护。

当前修复源码适用完整测试为test/debug：主套件1488过/0败/6忽略，另3个子进程探针通过；沿历史口径合计1491过，微信26项按用户要求过滤，Cargo exit 0。以`goal-candidate/controller-console-full-review-20261015.json`为统计复核入口，不能把子探针的精确筛选数误算为微信排除数。

安装包的源码摘要为`e069dde16b0b9dc8fa707ecaa52d3d1f5e0b816bacc93a141d429a5e70a9aaca`；Goal候选`main.rs`摘要为`aeb17997621668388b99591a3b2cb0211529ade78a7b2fcfa34f50132eefdb09`。相同Git种子HEAD不能替代这两个不同的源码身份。

详细范围及仍缺证据见[工作记录](../../work-logs/2026-10-15-installed-133-and-goal-repair-acceptance.md)、[统一清单](../2026-10-13-goal-32wbs-acceptance-checklist.md)和[C盘手工清单](../2026-10-15-c-drive-manual-cleanup.md)。
