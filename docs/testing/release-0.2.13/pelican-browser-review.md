# Qwen R5 鹈鹕单车成品浏览器验收

日期：2026-09-19。结论：**通过，无需因本次浏览器验收进入第 6 轮修复。**

被测文件：`C:\Users\zhupu\Desktop\coolzhuagent\tmp\2026-09-19-qwen\evidence\pelican-r5.html`。
SHA256：`f13cb35d1fc089dbd4c629d9f6b1427e2c9c1074c6aa65b40e21a45820e97add`。

本次使用独立无头 Edge 加载本地文件，屏蔽外部 HTTP(S) 请求，实际外部请求为 0。没有写入模型产物或调用真实模型；测试前后 HTML 哈希一致。此报告是独立浏览器验收证据，不归为模型自己完成的浏览器测试。

## 结果

基础检查 7/7 通过；扩展检查 12/12 通过（部分错误与依赖检查重叠）。

| 检查 | 实测证据 |
|---|---|
| SVG 显示 | 可见非零尺寸 SVG，完整鹈鹕、单车及背景；两个时刻图像不同 |
| 暂停 | 暂停后两张 SVG 截图哈希相同 |
| 0.5× 实际速度 | 约 1.2 秒真实采样期，SVG 时钟速率 0.501415 秒/秒 |
| 2× 实际速度 | 约 1.2 秒真实采样期，SVG 时钟速率 2.005997 秒/秒 |
| 两档比例 | 4.000672，符合预期 4 倍；不是只检查滑块数值变化 |
| 恢复默认 | 速度回到 1×，SVG 时钟回到 0 附近并继续推进，实测速率 1.000144 |
| 1024×600 | 文档尺寸正好 1024×600；主 SVG 和全部控件完整可见，无横纵溢出 |
| 720×480 | 文档尺寸正好 720×480；主 SVG 和全部控件完整可见，无横纵溢出 |
| 减少动态复选框 | 勾选后时钟速率 0、画面两时刻一致；取消后重新有画面变化 |
| 减少动态后恢复默认 | 清除勾选并恢复 1× 播放，实测速率 1.008196 |
| 系统减少动态初始值 | `prefers-reduced-motion: reduce` 下自动勾选并冻结，时钟速率 0 |
| 系统偏好实时切换 | 切回 no-preference 后速率 1.004182；再次 reduce 后速率 0 |
| 错误与外部依赖 | 页面异常 0、控制台错误 0、外部资源请求 0 |
| 最终标记 | 第 46 行存在 HTML 注释 `TEST_PELICAN_QWEN_FINAL_20260919` |

速度测量直接对主 SVG 的 `getCurrentTime()` 与 `performance.now()` 做前后采样，因此覆盖该页面由 requestAnimationFrame 驱动 `setCurrentTime()` 的 SMIL 实现。Web Animations 返回空数组不会再造成速度功能无法确认。

## 人工看图

已查看普通尺寸的两时刻截图，以及 1024×600、720×480 截图。鹈鹕的白色头颈、长喙与喉囊、身体和翅膀可辨识；红色车架、双轮和双腿完整。两时刻的腿部姿态、轮子辐条、云和路边背景有变化。两个低高度窗口的主画面、播放按钮、速度条、恢复默认和减少动态控件都在可见区域内，没有出现第一轮的空白 SVG，也没有被底部裁切。

画面是简洁的二维卡通风格；本次没有提出阻断交付的视觉问题。人工检查基于上述截图，未声称完成长期循环连续录像或其他浏览器兼容性测试。

## 证据与复跑

- 基础 JSON：`pelican-r5-browser-qa/report.json`
- 扩展 JSON：`pelican-r5-extended-qa/report.json`
- 两时刻整页图：`pelican-r5-browser-qa/frame-01.png`、`frame-02.png`
- 低高度图：`pelican-r5-extended-qa/layout-1024x600.png`、`layout-720x480.png`
- 减少动态图：`manual-normal-*`、`manual-reduced-*`、`manual-restored-*`、`system-reduced-*`，均在扩展证据目录。

从仓库根执行：

```powershell
node tmp/2026-09-19-qwen/verify-pelican-artifact.cjs tmp/2026-09-19-qwen/evidence/pelican-r5.html tmp/2026-09-19-qwen/pelican-r5-browser-qa
node tmp/2026-09-19-qwen/verify-pelican-extended.cjs tmp/2026-09-19-qwen/evidence/pelican-r5.html tmp/2026-09-19-qwen/pelican-r5-extended-qa
```

扩展脚本先在稳定 R3 验证：变速、恢复默认及两种低高度布局通过；减少动态按 missing 记录。最终 R5 已具备并通过这些新增检查。测试脚本与证据均位于 tmp，没有改动模型生成的 HTML。
