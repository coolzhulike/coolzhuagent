# 原生 Paint 复测前的 Agent 修复（2026-09-29）

## 已确认的问题与改动

- B08：旧顶栏把工程默认开放权限显示成聊天室的“完全访问”。真实聊天室仍保存为目录权限，因此 `computer_use_perform` 被 `computer_use_room_full_access_required` 正确阻断。顶栏现按当前聊天室持久授权显示“目录权限/完全访问”；工程默认权限只在说明中解释，不自动授予桌面权限。
- 冻结聊天室兜底：执行器先核对 `originating_chat_room` 与冻结父调用的聊天室，再把同一个已核对的聊天室用于作用域、授权查询及执行。缺失或跨聊天室授权仍拒绝，不给空聊天室默认权限。
- B12：截图 helper 原来把 PNG base64 放进一条 stdout JSON；真实回执超过通用 256 KiB 保留限额后被截断，JSON 在第 262144 字节 EOF，观察阶段失败且执行步骤为 0。现在 helper 把 PNG 写进宿主生成的唯一临时目录，只在 stdout 输出尺寸、字节数和 SHA-256 元数据。宿主仍从完整的增量行读取回执，不等 EOF；只从自己指定的固定路径读取最多 80 MiB，核对长度、PNG 尺寸及摘要，再恢复原 `data_url`。临时文件在 helper 退出后清理；清理失败会显式报错。普通 stdout 的 256 KiB 上限与权限门禁均未调整。

## 已完成验证

- `cargo build -p coolzhu-computer-use-core --offline`：通过。
- `cargo test -p coolzhu-computer-use-core --offline --lib capture_receipt_transfers_png_larger_than_pipe_limit`：1/1 通过。用真实随机像素生成的 512×512 PNG（超过 256 KiB）落盘，只传短元数据行，读回后 base64 解码与原 PNG 字节完全一致。
- Windows PowerShell `Add-Type -ReferencedAssemblies System.Drawing` 编译 `input_stroke_native.cs`：通过。
- `cargo build -p coolzhu-web-console --offline`：通过；`target/debug/coolzhu-web-console.exe` SHA-256 为 `F75A3E678450D336AA3404E1D1F8DEF882603585E515C6574A187FB5E7888DB1`。
- 先前的聊天室权限与冻结聊天室定向测试及 `node --check app.js` 已通过；本轮未重复无关测试。

上述是源码和定向传输验证。真实 Qwen/Paint 场景仍需用包含本次源码的完整新安装包复测；现有安装版 B12 失败不能作为新版本结果。复测应确认：当前聊天室确有正式“完全访问”授权、截图完整且观察成功、后续操作及收尾事实可核对；不以模型返回或截图成功单独宣告整段 Paint 流程通过。
