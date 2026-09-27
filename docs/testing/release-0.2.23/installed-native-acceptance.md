# 0.2.23 安装版原生界面与数据验收

日期：2026-09-28。本报告针对最终带时间戳的 `CoolzhuAgent-0.2.23-20260928-010934.msi`，大小 246,295,614 字节，SHA-256 `0BE92F1A549A8677A95742C20890E059DA54ABA18801BCC47880697EFC9EAB65`。同版较早生成的无时间戳 MSI 不在本次验收范围内。

## 安装来源

从 0.2.22 正常退出后，先取得静止升级前快照，再以 Windows 正常提权安装流程运行最终 MSI（`/passive /norestart`），安装器退出码 0；系统登记版本为 0.2.23。首次启动前的静止快照与升级前一致。安装目录 10 个关键文件的长度与 SHA-256 全部匹配同次包内清单，其中 Web 为 `D18E0CE4F1442683597550AD9D7CF913EEB21FA9DF51ED65171C398E1EF83EAC`，CLI 为 `759018CC749EC4C7F5DF2335A3F0DACB07EFC3B9E6DB5F258EEC5C2694BFCA74`。

通过系统注册的 `COOLZHU CODE Agent` 正式入口启动。8765 监听者是 `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`（本次 PID 24068），窗口壳是同目录下的 `coolzhu-tauri-shell.exe`（本次 PID 26380）；窗口可访问性文档地址为 `http://127.0.0.1:8765/`。路径、端口与文件哈希共同确认截图来自新安装版，PID 仅为本次运行的瞬时标识。

## 原生画面与权限入口

在原用户工程的既有受控验收房，正常 1443×897 窗口中，顶部中央 Logo、圆形玉饰与金色装饰完整，竹林背景和底部金框未被诊断栏遮盖；聊天室旁的权限摘要显示“权限 完全访问”，没有“调试完全访问”字样。[正常窗口](installed-native/01-installed-normal-verified.jpg)。最大化 1707×912 与精确 903×551 的原生窗口均保留圆饰比例、Logo、金框、房间及权限入口，消息和输入区可见：[最大化](installed-native/04-maximized-frame.jpg)、[903×551](installed-native/05-low-903x551.jpg)。

正常和 903×551 下均实际打开了聊天室旁的权限弹层，控件可见、可点击且未被 Logo 或窗口边框遮挡：[正常弹层](installed-native/02-permission-popover-readonly-normal.jpg)、[低窗弹层](installed-native/06-low-permission-popover-readonly.jpg)。弹层只读核验显示当前有效权限为完全访问，配置项仍为“目录权限（默认）”；详情说明了工程调试覆盖。验收未点击“应用”，未保存权限、更改全局访问策略或放行历史输入隔离。[运行轨迹与安全详情](installed-native/03-safety-trace-detail-readonly.jpg)显示独立的安全详情及 CU 入口；曾打开 CU 运行摘要查看入口，未执行新的 computer-use 动作。房间名称只是既有受控房名称，不代表本版号。

## 活动网页与窗口生命周期

右侧浏览器中实际打开安装版自己的 `http://127.0.0.1:8765/`，右栏渲染页面正文，界面显示“已在主面板内嵌”，可访问性树同时确认主文档和内嵌文档的 URL；[已加载网页](installed-native/07-webview-loaded-maximized.jpg)。网页保持加载时打开顶部权限弹层，弹层正常置顶、控件可操作且未被网页遮挡，网页仍可见；[网页与弹层](installed-native/08-webview-permission-popover.jpg)。这张图不表示弹层打开时网页被隐藏。

保持网页活动状态将原生窗口缩至 903×551，右栏自动收起，聊天、输入区与底框不被网页覆盖；[低窗自动折叠](installed-native/09-webview-auto-folded-903x551.jpg)。重新最大化后，同一 `8765` 页面和右栏恢复渲染；[放大恢复](installed-native/10-webview-restored-maximized.jpg)。点击“关闭当前工具”后聊天回到全宽、输入框无草稿；[关闭恢复](installed-native/11-webview-closed-chat-restored.jpg)。本轮没有访问外部站点，也没有把空白右栏算作网页加载成功。

## 升级与运行后数据

升级前、升级后首次启动前、最终运行后三次均用同一私有密钥清单与 SQLite 在线备份取样。升级静止前后都是 60 个规范持久文件、41 个附件（49,153,852 字节）、6 个 SQLite 库；文件哈希、数据库语义摘要及配置摘要完全一致。备份目标临时产生的 `-wal/-shm` 不作为持久文件计数；未清理或修改源用户库。

运行后仍为 60 个规范持久文件和相同的 41 个附件，附件、配置与 `web-sessions.json` 未变。只有工作区及输入安全两个 SQLite 文件发生预期运行变化：工作区库所有表中，仅 `memory_access` 保持 85 行，其中 2 行只变化 `access_count` 和 `last_accessed_at`；消息、房间、权限及附件相关表语义未变。输入安全事件从 90 增至 91，唯一新增事件类别为 `recovery_stage_advanced`；归属 epoch 表仍为 2 行，其中 1 行的接管/释放时间、实例、epoch 和持有进程字段更新。未出现人工放行或权限保存事件。CU 入口显示过运行摘要，但工作区消息表在最终快照中未变化。

原始用户数据、附件、备份、密钥及字段级差异明细仅保留在忽略目录 `tmp/candidate-0.2.23/`；公开证据只含上述受控房截图。本报告只判定 0.2.23 安装身份、指定界面/权限入口、活动 WebView 生命周期和数据保留；LSP、既有视频播放及模型路由沿用各自独立验证结果，未在本轮原生重跑。
