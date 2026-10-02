# 桌面应用目标定位修复审查

## 现场问题与归因

059真实Qwen请求CU059-BODY-CR仅指定target.application=mspaint.exe。窗口选择器把应用参数与窗口标题、类名做模糊匹配，没有查询可执行进程路径；中文Paint标题“无标题 - 画图”无法匹配mspaint.exe。无匹配返回None，桌面桥继续观察前台Coolzhu控制台。原图和observation_window_selection:None证明窗口路由存在Agent问题，不能把本次失败归为绘画能力。输入前旧观察校验拒绝了动作，没有实际落笔；单请求动作额度随后正确停止重规划。

## 职责与实现

UIA resolver负责进程/标题/类名匹配与唯一窗口选择，桌面桥负责核对激活目标与实际观察窗口身份，既有控制器继续负责权限、预算、规划、输入和验收。没有新增执行循环或前端诊断控件。

使用GetWindowThreadProcessId、OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)、QueryFullProcessImageNameW读取窗口所属进程路径，立即关闭查询句柄。exe参数按文件名大小写无关精确匹配；完整路径按规范化路径精确匹配。非exe的友好应用名称保留进程stem/标题/类名匹配；同时指定application与window时必须同时匹配。显式目标优先于objective的旧记事本推断。

只有未指定目标且没有旧记事本推断时仍使用前台。指定目标不存在返回target_not_found；多窗口匹配返回target_ambiguous，要求更具体的窗口提示，不任意选第一个。激活后实际观察句柄偏离目标则stale_observation，不规划错误窗口。进程路径读取失败不能把标题文本当成exe身份，也不能退回控制台。

## 风险与边界

同一进程存在多个可见窗口会明确歧义，这是保守行为变化。旧代码允许只匹配两个条件之一，现在需修正错误窗口参数。受保护进程可能无法读取路径，将明确失败；不提权、不扩大访问权限。某些UWP宿主窗口所属进程可能是ApplicationFrameHost；该方案不猜测子进程身份，必须使用窗口提示或后续有证据的专用解析，不能声称支持所有App ID。本机真实Paint所属mspaint.exe路径已只读确认。

窗口可能在观察、截图或输入前变化；新句柄检查不替代既有截图绑定、DPI、几何、许可和输入前校验。找不到目标的运行必须零规划、零输入，不能要求模型凭旧截图定位。

## 验证与正式验收

针对性工程检查覆盖中文Paint/exe匹配、聊天标题伪装exe、完整路径不同安装目录、application/window同时成立、无目标前台兼容、记事本推断、无匹配/多匹配明确失败。UIA离线build通过，lib检查12项通过；Web离线build及新安装版真实Qwen结果在060改动报告中追加。工程检查不能代替实操验收。

正式验收优先：控制台前台、Paint已打开，模型仅指定mspaint.exe自主观察与绘图；核对原图属于Paint、一次路径sent/released、最新原图实际新增闭合矩形。再指定不存在exe，确认target_not_found且没有规划/输入。Browser Use仍需同安装版单击回归。Paint完整海绵宝宝与关闭竞争等未覆盖项目保持开放。
