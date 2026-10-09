# 0.2.111改动与测试交接

冻结源码`20facc6c2c03f8969fe592b01ea3e59e12381c2b`，快照`d090216b7bea50ecb79c422a228249a3ee0afe514a3afb079e9ae60f7c48f39e`。正常release实际子进程退出0、六项发布门通过；正常管理员安装退出0，Program Files内1159文件逐长度/SHA匹配。MSI为285818376字节，SHA256 `0a1c8ca20c73a80bfbe7d80d19b35b6b26d0ba5d46719319089503966c4824b8`。构建源码两路CI全部成功：true；确切状态见installed-validation/source-ci.json，未完成项不能继承旧提交结果。

## 改动与模块职责

会话参数DTO、serde默认值、协议与地址解析、HTTP参数校验和预算约束收敛至session_model_config。模块只读参数，不依赖HTTP handler、配置文件或数据库；主层继续负责权限、revision、密钥保护和发布。五组提取前后独立比对一致；此前候选的GUI非法值、后端400不改revision、正常保存及同EXE重启逐参数恢复已有[独立证据](../2026-10-08-session-config-boundary/report.md)。本次将该实现正式出包，未声称完整SessionConfigService、共享Runner或outbox完成。

Browser read_session回调先核资源/URL，再处理失败HRESULT，避免资源变化被通用读取错误覆盖。只调整判断顺序，不改输入、权限、预算、重试、UTF-16长度或JSON校验；没有前端调试控件。代码审查与76项既有桌面回归见[独立事实](../2026-10-08-browser-read-attribution/report.md)。失败回调与资源切换同时发生尚无正式实机证据，不能用此次打包代替。

## 正式原生实拍与边界

[主界面](native-ui.jpg)和[模型设置页](native-settings.jpg)均来自Program Files正式111原生窗口。模型设置正常读取，随后正常关闭，未保存原SWE配置。独立只读核对确认swe-2-medium、configuration_revision 51、原island-kayak绑定及无活动运行/锁定；原房间及完全访问保持。安装前确认无活动运行、终端或LSP，只停止身份、开始时间、路径与SHA全部匹配的自有110后台和壳。随后以原工作区和原安全库恢复111。

本轮模型请求0、新云端会话0；未重复简单问答、未重测Paint。配置副本保存重启证据来自候选EXE，未追认其截图来自正式111；当前正式截图只覆盖原配置读取和设置开关。

## CI失败与测试预算修补

前一提交1f59的S0运行37880503321曾失败：ACP历史重放测试握手与后续提交共用3秒期限，返回“ACP提交前已取消或超时”；完整1422通过/1失败/6忽略，原失败保留。后续只在cfg(test)中将非超时测试的握手预算改为30秒，并在握手完成后重新设本阶段测试期限，产品期限完全不变。修补后的实际offline build0、完整Web1423/0/6（另lib8/native-host1）通过。该测试修补发生于111冻结之后，不冒称111源码包含；它属于后续提交，远端另核。

## 仍未完成

严格新nativeTarget替换/跨来源commit位于down/up之间，以及确切在途观察的面板撤销仍开放。普通自然导航、登记标记或关闭后panel_unavailable均不代替这些证据。其它插件许可、附件、共享Runner/单写者、运维与Windows矩阵按总体台账继续，Goal保持active。Paint免测、微信不动、Opus暂停、不使用子代理。已公开[111预发布](https://github.com/coolzhulike/coolzhuagent/releases/tag/v0.2.111)，四资产服务端长度/SHA和实际tag=20facc6全部匹配；元数据与独立校验脚本归档于installed-validation。未签名、预发布、不标latest、不发自动升级清单。
