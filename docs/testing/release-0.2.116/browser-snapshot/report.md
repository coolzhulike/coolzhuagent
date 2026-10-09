# 116 Browser纯截图驱动：严格窗口仍未命中

正式Program Files版本116，沿用SWE-2-medium/revision51和唯一island-kayak，单次实际模型/单次工具，无补发或新云会话。已有九步长程/综合链路不重复。本轮只检验新驱动是否能闭环在途观察撤销。

上一轮全历史AX取回有明显延迟，本轮外部连续只读发现登记后仅调用正常sky截图（不抓AX历史）；[登记后新鲜实拍](registered-snapshot.jpg)可见页面可信点击结果和顶层电脑使用提示。主模型检查截图后，在独立下一cell按当前截图正常点击右栏关闭；没有旧索引、同cell自动选择、宿主私有API、产品暂停或改五秒期限。

只读捕获登记后6.13ms；截图返回距登记149ms，关闭调用距登记9502ms，仍晚于五秒窗口。[正常关闭实拍](closed.jpg)不能证明撤销发生在底层等待内。说明本轮已消除AX抓取延迟，但主模型检查截图并发起下一工具的往返仍跨过短窗口。

独立SQLite核对：sent/released/effect_observed，CU succeeded，父completed；browser.observation_stopped事件0。单ACP尝试end_turn/process_drained=1、唯一绑定解锁；SSE只计事件名，未保存思考。有限watch/send/server/finalize实际退出均0。**严格撤销未通过**，不以点击普通成功替代。

后续不继续同类简单点击撞窗口；新nativeTarget替换、严格跨来源commit/down-up和HRESULT/资源竞争仍开放。基本长程能力已有证据，不能将这些窄竞态列成全功能不可用。原安全库和权限未改，Paint免测、微信不动、Opus暂停。
