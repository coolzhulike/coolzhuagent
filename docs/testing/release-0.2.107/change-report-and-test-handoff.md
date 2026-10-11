# 0.2.107 改动报告与真实测试交接

当前正式安装0.2.107，冻结源码`314671b8fae7469f440046a270956c7b86234d97`，源码快照`f2661129b586f28e528f4d6e7fe88e79a1f6d1f739adf9af804a8ea087bae22a`。六门发布资格pass、真实构建子退出0、正常MSI安装0，Program Files内1159文件逐长度/SHA匹配；两路远端CI success。MSI SHA256 `e46915becd43f8509a163e23df0ba750aa5a7595881b1d7f48c19e60f90fac56`。本版仍是预发布/未签名，不提供自动升级清单。

## 改动及职责

`native_browser_host.rs`在原50毫秒观察等待循环内复用已存在的完整资源绑定检查：旧请求资源变化即结束为native_browser_resource_changed，避免旧回包被拒绝后等5秒变为通用timeout。保留同一面板发起新导航观察的能力，不增加权限、重试、输入延迟或第二账本。包含106的input_steps终态投影及提前返回一致性修补。具体分支正式实机仍未命中，不能因为源码/回归通过就关闭该项。

候选offline build退出0，完整Web1423通过/0失败/6既有忽略；原始输出在installed-validation。新增检查扩展原资源匹配测试，未新增模型夹具。

## 正式软件四轮事实

固定真实SWE-2-medium、revision51、原聊天室与唯一island-kayak。每轮只有一次computer_use_perform，max_actions=1；所有轮单end_turn/drained、无补发，最终绑定解锁。SSE仅统计事件名称，不保存思考正文。

| 场景 | 实际终态 | 判定 |
| --- | --- | --- |
| 正常地址栏导航 | succeeded；sent/released/effect_observed/passed | 导航比up晚10907.3ms，晚于工具终态；观察失效未命中，不能算负例通过。 |
| 网站点击后150ms自动跨来源跳转 | budget_exhausted；sent/released/effect_observed/failed | 新页约晚229.9ms，目标文字不可见；正常新观察合法，不是原在途资源失效分支。 |
| 处理期间关闭右栏 | budget_exhausted；sent/released/effect_observed/failed | 关闭比up晚11117.9ms，实际终态在关闭前约3577ms；未命中关闭窗口。 |
| 点击后5秒开始、7次后停止的网页连续跳转 | native_browser_observation_stale；sent/released、效果/目标未知 | 旧观察不被采纳为成功、新页零输入、无补发；失败回执事实保留子项通过，精确resource_changed等待分支仍未命中。 |

每轮独立目录含请求、事件、原终态、最终可见回复和实拍。最后一轮截图：[旧观察失效后释放事实](bounded-navigation-stale/final-reply.jpg)。所有网页变化均在释放之后，不冒称严格down/up期间，也不冒称创建新的原生Target。当前四个临时HTTP服务均正常退出0，活动模型任务0。

## 给后续测试模型的用例边界

1. 优先覆盖在途旧观察等待期间资源变更，期望精确native_browser_resource_changed、input_steps保留sent/released、效果未知、单终态、无补发；必须独立证明变化时刻落在观察请求窗口。当前未命中事实保留，不重复普通点击算通过。
2. 严格newTarget/跨来源commit落在down/up区间仍开放；页面navigation-started、释放后loaded、同步document.write和普通关闭均不能替代。
3. 106的通用timeout负例保持原始失败；本版连续跳转只关闭旧观察拒绝且释放投影保留，不外推全部Browser验收。
4. 下一轮继续插件许可/后代进程、附件入口、调度并发、DPI/启动其它模式与总体模块职责审查；详见四台账。Paint免测、微信不动、Opus暂停、不新建云端会话。

采证相对URI、列名和全库计数错误见installed-validation/collection-errors.txt；均是采证脚本错误，不记为产品测试通过或产品缺陷。


## 公开交付

107已[公开预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.107)：四资产服务器长度/SHA和实际tag=314671b核验通过；未签名、不标latest。当前正式107原生窗口保持打开，精确资源变化等待分支和总体未完成矩阵继续开放。
