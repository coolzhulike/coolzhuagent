# 036 L：三点连续折线未完成

真实 qwen3.8-flash，原 Base URL/API_KEY/medium，正常候选/原工作区，新独立允许Paint输入的任务。不是手工重放历史动作或模型夹具。

父轮 `chat-turn-1790764115028-0`，父运行 `run-chat-dc9411046d72f7b0c65d6d279b72a11454aacce0a46e6220`。模型规划三点 `[.449,.542]→[.488,.542]→[.488,.633]`，1500ms，目标窗口 `15c08ae`。真实动作1次drag，宿主记录3点接纳、path_completed、released；原始截图只有起终点斜线，没有L形拐角。不能把接纳点数等同应用绘制的点数。

该轮最终视觉判定另有JSON语法错误（安全投影597字节、Syntax、line1/column465，没有保存原文）；结果 blocked/invalid_verification、goal_achieved=false、0 verified。模型规划成功并不等于绘图验收通过。结束后活动撤除，safe/accepts_new_input=true，待恢复0、未确认阻断0；两条已确认历史复核保留。

- [动作前原图](01-paint-before-l.png)
- [动作后原图：拐角丢失](02-paint-after-l-corner-missing.png)
- [执行期间原始桌面截图](03-desktop-l-active.png)
- [结束后原始桌面截图](04-desktop-after-l.png)
- [调用、释放、失败分类、状态及图片来源哈希](paint-l-path-and-verification-failure.json)

当前受控连续移动以SetCursorPos移动、SendInput按下/释放。观察支持事件合并/中间点未进入应用的假设，尚未证明底层机制。拟改SendInput禁止合并移动，仅限该执行接口，不放宽帧守卫、权限或取消，见[修复方案](../../../analysis/2026-09-30-controlled-stroke-corner-loss-plan.md)。新的037候选必须再次真模型实操，不把源码修复算036验收。
