# 工程文件写入的工作区绑定验收（2026-09-27）

本记录只使用 `tmp/` 下两个临时工程、私有 HTTP 端口和当前源码构建的 Web 可执行文件副本；它模拟旧请求在切换工程后才交付给服务端的顺序，**不是原生窗口实操或截图验收**。没有读取或修改用户工程、安装版 8765 或真实模型配置。两次私有服务均已在脚本退出时终止。

## 旧版误写复现

旧构建 SHA256：`585384ece96b921d5f1eefbe036e133bcdb79147e29115aac8f6b2f4c0ea23da`。临时工程 A、B 都有内容为 `same initial\n` 的 `note.txt`，因此 `GET /api/project/file/meta?path=note.txt` 返回相同内容 revision。先在 A 取得 revision，再用 `POST /api/workspace` 正常切到 B（200），最后才提交原本在 A 编辑时准备的 `PUT /api/project/file`：

```json
{"path":"note.txt","content":"A stale edit delivered after switch\n","revision":"<A 文件的内容 revision>"}
```

保存返回 **200**。A 文件仍为 `same initial\n`，B 文件变为 `A stale edit delivered after switch\n`，证实内容 revision 无法单独绑定工程。原始脱敏结果位于 `tmp/project-save-scope-race/583dc72c4a/result.json`。

## 修复后私有 HTTP 验证

新构建 SHA256：`82c0c685e5382b40cd2bdf6ac9eb481f9a61f8c9868e5e6835392b0298b325cc`。仍用两个临时工程中同路径、同内容、同 revision 的 `note.txt`，在 A 取得 revision 后切 B。四种写入请求都必须携带从当前工程响应中取得的 `expected_workspace`；客户端声明只用于核对，不能决定服务端写入 root。

| 请求 | 关键请求体字段 | 声明旧 A、空值、缺字段 | 声明当前 B |
| --- | --- | --- | --- |
| `PUT /api/project/file` | `path`, `content`, `revision`, `expected_workspace` | 均 409 | 200，B 文件保存 |
| `POST /api/project/entry` | `parent_path`, `name`, `kind`, `expected_workspace` | 均 409 | 200，B 文件创建 |
| `PATCH /api/project/entry` | `path`, `new_name`, `revision`, `expected_workspace` | 均 409 | 200，B 文件重命名 |
| `DELETE /api/project/entry` | `path`, `revision`, `recursive`, `confirm`, `expected_workspace` | 均 409 | 200，B 文件删除 |

十二个拒绝请求完成后，A/B 的 `note.txt` 都仍是 `same initial\n`，B 中也没有新建或改名文件。接着带 B 的 `expected_workspace` 顺序执行四种正常写入，均返回 200：B 的 `note.txt` 变为 `bound B save\n`，新文件完成创建、重命名、删除；A 原文件保持不变。原始脱敏结果位于 `tmp/project-mutation-scope-verify/182b786459/result.json`。

复测时可让私有 Web 以 A 为初始工程，调用 `GET /api/project/file/meta?path=note.txt` 保存 revision，再 `POST /api/workspace` 到 B。对上述四个接口分别发送旧 A 路径、空字符串和缺失的 `expected_workspace`，预期都为 409 且 A/B 文件不变；改为 B 的响应 `workspace` 后预期正常写入成功。测试文件应只建在 `tmp/`，不可对实际用户工作区做破坏性写入。

服务端绑定在 `main.rs` 的 `bound_project_mutation_root` 与四个项目写入处理器；前端 `app.js` 四处请求体均发送操作开始时捕获的工程作用域。此验证只覆盖私有 HTTP 的四个写入接口，不代表安装版原生窗口、并发网络时序或其它项目读取接口已经验收。

## 0.2.19 已安装 Web 程序副本回归

rail 已确认 0.2.19 正常升级完成后，读取已安装程序 `C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe`，SHA256 为 `3BD24917095B44ED856A6087151A0ACB3029E77A6BEC09E47E345179C5AAD042`。验证脚本先复制该程序到 `tmp/`，核对副本哈希一致，再以纯临时 A/B 工程和随机端口 `61987` 启动副本；没有接触安装版正在使用的 8765、原生窗口或用户工程。

复用上节四种 mutation 的相同请求序列，A/B 的 `note.txt` 初始同内容且 revision 相同。切 B 后，以旧 A、空字符串、缺字段的 `expected_workspace` 分别发送四种写入，共 **12 个 409**，A/B 文件均未改变。以 B 的当前工程路径发送四种写入，共 **4 个 200**；A 的 `note.txt` 保持 `same initial\n`，B 保存成 `bound B save\n`，临时文件按创建→重命名→删除后均不存在。私有进程在验证脚本退出时终止。原始结果：`tmp/project-mutation-scope-verify/86f692a4d9/result.json`。

该回归确认**实际已安装 Web 可执行文件副本**的 HTTP 写入行为，仍不等同于安装版原生窗口点击验收。
