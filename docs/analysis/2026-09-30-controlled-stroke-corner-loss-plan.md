# Paint 连续路径拐点丢失：执行侧修复与验收

2026-09-30，主会话独立诊断、实现，不使用子代理或模型夹具代替软件实操。

## 真实证据和边界

035 G 的两个点拖动留下102像素短线，输入释放、静态四边泛光正常。035 J 的闭合多点拖动只留下少量墨迹，且是历史任务串轮，不能验收。036 L 是新的独立任务：真实 qwen3.8-flash 规划三点 L 形路径，helper 报告3点接纳、path_completed、released，但原始画布只出现起点至终点的斜线，没有折角。该轮视觉判定还返回无效 JSON（597字节、Syntax、line1/column465）；宿主如实 blocked/invalid_verification，不把 input_sent 当完成。

原路径移动使用 SetCursorPos，按下和释放使用 SendInput。中间点消失与移动事件合并相符，但截图不足以证明 Windows/Paint 内部机制。先限定修复受控连续拖动，再以真实模型新任务验证；不重放失败轮动作，不手工替模型画图。

## 实施选择

仅修改 `computer-use-core/src/input_stroke_native.cs` 的 Native.Move：发送 MOVE | MOVE_NOCOALESCE | VIRTUALDESK | ABSOLUTE 的 SendInput。Windows 官方说明 NOCOALESCE 禁止默认 WM_MOUSEMOVE 合并；VIRTUALDESK 必须配 ABSOLUTE，坐标映射整个虚拟桌面。来源：[Microsoft MOUSEINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput)。

坐标以物理像素中心映射，使用 long 防溢出，负原点、多屏边界显式拒绝。helper 初始化冻结虚拟桌面范围，移动前同时核查窗口/进程/DPI/矩形和虚拟桌面布局。取消、有限时长、许可握手、finally 释放、事实文件、帧守卫、工具协议均沿用；不以加等待时间或放宽截图守卫掩盖问题。不改变普通 click/键盘或旧输入接口。SendInput 返回1只证明接纳该事件，仍不证明应用画出了路径。

## 验证和风险

复用已有 helper 编译及释放检查，只增加纯坐标边界检查（不会注入输入），以及相关 crate 离线 build/test。随后正常新候选运行原 Qwen、原 Base URL/API_KEY/medium，重新独立规划 L 形和闭合轮廓；原始 Paint 前后截图须有对应拐点，动作记录须 released，活动提示须结束撤除。成功后再尝试简易海绵宝宝，不能把折线通过外推为头像通过。

NOCOALESCE 不保证应用处理或视觉判定正常，若仍丢点继续记录失败，不能宣称根因已完全消除。负原点的纯计算通过不能代替多显示器实操。无效视觉 JSON 是独立问题，当前只有安全投影、没有原文，不猜测或放宽判定协议。
