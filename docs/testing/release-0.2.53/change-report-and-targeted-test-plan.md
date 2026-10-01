# 0.2.53 改动报告与真实 Browser Use 回归

2026-10-01，主会话独立实施和测试。保持原qwen3.8-flash、medium、百炼Base URL和已有密钥；只准备初始网页、投递请求与按明确场景关闭面板，未代模型点击增加次数或绘画。微信不动不测、Devin搁置，Pro复审暂停。

## 变更与安装身份

后台对同一已认证宿主撤销资源时尚未领取的请求，在原互斥锁内按NotDispatched结算；已领取请求仍等待真实回执，不伪造未派发。桌面关闭先撤销输入资格，隐藏原视图并等待原执行清理结束，再销毁原WebView。真正未知仍隔离，不修改权限或安全库，不自动重试。详见[方案](../../analysis/2026-10-01-browser-close-input-lifecycle-review.md)。

Web与Shell离线build通过；Web完整1294通过、0失败、2既有忽略，另lib8和宿主接线1通过；Shell64通过、0失败。提交c648819的远端PR检查36826084572和push检查36826080670均success。

正常MSI安装返回0；注册唯一053，正式Shell PID15368、Web PID27476，8765唯一监听者一致，10项关键安装产物逐项匹配。MSI247012733字节、SHA256 `712e057db37139288b5ce7f9935e8ac0610cf63f4710684272945b48c2839234`；源码快照`a20bf1b7543fd8d0eaedc958eefe372a8d27aff097a3f7c9e50c85a2613afba3`，载荷摘要`d4ccc8bd36f9b1c2a00b9844035c2ad449338124af6fbfe6aa69dc481214c1ef`。6发布门、859文件/324493798字节、安全扫描通过。[安装身份](installed-native/installed-artifacts.json)，[原始构建记录](evidence/build-identity/pkg-report-release-20261001-144801162-997292e0/package-report.json)。出包installed=false保持原时点。

## 恢复与真实结果

052未知释放事故保留原记录。旧Shell3372和Web8700已正常退出、8765释放；主会话按用户明确授权执行恢复理由和可信桌面确认，并非用户手动点击。系统独立复核后返回opened/epoch48，未获放行阻断由3归零、输入开放、待人工复核徽标消失；没有手工写安全库或把旧未知改成成功。[开放提示截图](installed-native/00-native-recovery-opened.jpg)。

| 场景 | 结果 | 可复查事实 |
| --- | --- | --- |
| BQ 多步运行中关闭 | 步骤间关闭通过；交付中关闭未覆盖 | 父created1790837858866、finished1790837927036，68.170秒。关闭时点1790837916921。此前3步均sent/released/effect_observed；最后一次释放1790837905028，早于关闭。第四步1790837917552开始，10毫秒内按native_browser_panel_unavailable、not_sent/not_needed拒绝；无后续步骤、重开、重试或新增隔离。CU终态blocked/goal=false，不能称目标10达成，也不能以此冒充按下/交付期间关闭已验证。[执行中](installed-native/02-close-bq-during.jpg)、[终态](installed-native/03-close-bq-stopped.jpg)、[账本](installed-native/BU053-CLOSE-BQ-facts.json) |
| 重开页面 | 未通过，Agent可见性缺口 | 更多→浏览器重开原地址持续空白，刷新仍空白；再次点击打开网址才恢复click.html、次数0。[空白现场](installed-native/04-reopen-blank.jpg)、[显式打开后](installed-native/05-url-br-before.jpg)。新建视图没有已有视图路径的显式show；下一包修正与正式复验，不能算自动重开通过。 |
| BR 错误源URL | 零输入拒绝通过，16.501秒 | 模型真实请求target.url=text.html，原生页实际click.html。native_browser_observation_unavailable、observed page does not match the requested URL or has no facts，CUblocked、0动作、0步骤、goal=false；原URL和次数0保持，未导航、不重试。本轮无执行回执，不能杜撰sent/released；拒绝发生在观察阶段。[截图](installed-native/06-url-br-rejected.jpg)、[账本](installed-native/BU053-URL-BR-facts.json) |

## 下一包针对性测试

054仅补新建WebView显式显示，显示失败撤销资格并清理原视图。Shell离线build和既有回归后正常安装；验收关闭重开自动可见、真实Qwen新轮点击，并单独记录交付期间关闭的覆盖时点。BQ只覆盖步骤间关闭，052BP未知仍作为原失败保留。只读、滚动、文字输入和导航沿用各正式版本的真实通过证据，必要时针对新缺陷复测。

随后Paint：真实Qwen连续五点/1200—1500毫秒的闭合矩形，实际重新观察验证；再做简易海绵宝宝，联合全桌面四边泛光、准确顶部英文提示和结束撤除。动作回执释放不等于图形达成。DSH远程插件实际安装运行、开机动画和四项总体验收继续开放，PR74保持Draft。
