# 2026-10-15 继续验收：0.2.134与逐回复运行定位

沿用`D:\coolzhuagent`工作区与既有生产库，临时证据在`tmp/goal-32wbs-acceptance-20261013/`。没有使用旧321/336驱动，没有读出真实凭据，没有改写历史状态来制造通过。

## 0.2.134 已正式安装

- 唯一正常打包作业`release-134-afbb6d80717945aab46dee4515a548fb`，`scripts/build-msi.ps1 -Version 0.2.134 -Configuration release`，完整构建及MSI生成exit **0**；未使用`SkipBuild`，前后源码快照一致。
- 源码摘要：`ec6c7c043a2bbcf04d515059d300f448203c4064ba171ad01a4646302e1138e7`。其中Goal修复`main.rs`为已验收的`aeb17997621668388b99591a3b2cb0211529ade78a7b2fcfa34f50132eefdb09`。
- MSI：`dist/CoolzhuAgent-0.2.134.msi`，SHA-256 `37982448b49574c59af24ec973b656f3e1c2b5b1cdfb340daf3060c93ea256ea`。仍未签名，不能据此关闭6.4。
- 独立核对1222项载荷全部匹配；包内release程序与安装目录程序分别完成同一隔离Goal回退链。模型决定为受控夹具，产物仅由宿主`write_file`写入，三个run按failed/completed/completed收尾，Goal最终完成、retry_count仍1。
- 当前工具进程未提升，MSI声明`perMachine`。通过Windows官方`runas`进入系统提升流程，安装进程返回 **0**；没有修改安装范围、UAC或系统权限策略。
- 安装器已替换文件，但旧后台PID35284仍在运行旧映像。已核其空闲后结束旧进程，再从正常安装入口启动，避免用磁盘新文件哈希冒充旧进程已升级。
- 新后台PID **6492**、Tauri PID **26328**，均来自`C:\Program Files\CoolzhuAgent\bin`；Web Console SHA-256 `7e678c39883a6fcaabc61cad080d7e9073931a25b4ce4a77e3e3048a941d08f8`。
- D工作区、原数据库、4类静态资源、配置字节及11张历史表核验通过。升级前备份包含生产库、输入安全库和不透明配置副本；历史unknown锁原样保留。

主要收据：

- `controller-release-134-build.json`
- `controller-release-134-identity.json`
- `controller-goal-release-134-staged.json`
- `controller-release-134-preupgrade.json`
- `controller-release-134-install.json`
- `controller-release-134-runtime.json`
- `controller-goal-release-134-installed.json`

本轮推进了0.1/4.5/6.2的正式交付证据；生产模型、并行DAG、durable job与unknown恢复等整项矩阵仍开放。

## 逐回复运行定位候选

安装版原消息按钮只打开通用历史归档。已发现宿主原本就持久记录`chat.context_outputs`中的回复ID，因此候选复用这一真实归属，不按时间戳、显示作者或消息ID格式猜测run。

候选修改：

- 消息按钮向运行面板传入自身真实消息ID。
- 后端在当前工程/房间内查唯一已持久化的回复归属；跨作用域、已删除、无关联返回404，歧义返回409，冲突筛选返回400。
- 面板只展开该回复的唯一run，拒绝旧服务忽略参数后返回的通用列表。
- 保留作用域与请求取消检查；切换房间、快速点击另一条回复后，迟到结果不得覆盖当前内容。

前端6项测试已通过。唯一串行作业`controller-message-run-build.json`正在完成离线构建与`chat_insights`后端回归。这些新修改**不在0.2.134中**；当前源码的EX-01需在新候选验证后更新，不能沿用此前测试身份。

## 原生资源故障路径

已检查WebView2官方诊断接口。应用会覆盖`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，官方文档说明环境覆盖存在时注册表覆盖不生效，因此没有修改注册表策略来强行打开调试端口。

下一步准备核实当前WebView2是否接受进程级`--edge-webview-switches`诊断参数；若可用，只对本地已安装程序的启动人物图片请求做受控失败注入，观察真实`Image.onerror`及原生交接。不会伪造`finished`事件，也不会用普通浏览器或headless结果代替原生Tauri验收。
