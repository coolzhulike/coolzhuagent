# C 盘手工清理候选：2026-10-15 只读复核

本轮没有删除文件，**实际释放空间为 0 字节**。以下目录已与 `D:\old-repos\coolzhuagent-c-desktop-20261012` 下的同名目录逐文件核对内容 SHA-256，未发现目录 junction、文件符号链接或多硬链接，未发现 cwd/可执行文件落在旧 C 仓库的进程。

请先关闭旧 C 工程窗口，确认不再从旧路径运行任务，再按需要手工处理表中**精确目录**。不要扩大到它们的父目录或整个旧仓库。进程核对不包含所有内核文件句柄；若删除提示占用，应停止并查明占用者。

数字来自 `GetFileInformationByHandleEx(FileStandardInfo).AllocationSize`，共 **290,115,208 字节**，约 **276.68 MiB**。这是文件系统报告的文件分配量，不是删除后已经测得的可用空间增量；驻留文件、目录/MFT 和同时发生的其他磁盘写入会影响实际结果。

| 已核对 D 归档的 C 盘精确目录 | 文件分配字节 |
| --- | ---: |
| `C:\Users\zhupu\Desktop\coolzhuagent\.github` | 12,288 |
| `C:\Users\zhupu\Desktop\coolzhuagent\.playwright-cli` | 1,176,048 |
| `C:\Users\zhupu\Desktop\coolzhuagent\config` | 212,992 |
| `C:\Users\zhupu\Desktop\coolzhuagent\docs` | 283,957,864 |
| `C:\Users\zhupu\Desktop\coolzhuagent\installer` | 8,192 |
| `C:\Users\zhupu\Desktop\coolzhuagent\packages` | 291,304 |
| `C:\Users\zhupu\Desktop\coolzhuagent\scripts` | 983,040 |
| `C:\Users\zhupu\Desktop\coolzhuagent\skills` | 12,928 |
| `C:\Users\zhupu\Desktop\coolzhuagent\src` | 232 |
| `C:\Users\zhupu\Desktop\coolzhuagent\tests` | 98,304 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\cli\packages` | 364,544 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\computer-use\packages` | 753,912 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\core-runtime\packages` | 1,524,840 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\diagnostics\packages` | 37,080 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\llm-adapter\packages` | 537,096 |
| `C:\Users\zhupu\Desktop\coolzhuagent\modules\vision\packages` | 144,544 |

以下仍不在清理清单中：

- `C:\Users\zhupu\Desktop\coolzhuagent\tmp`、`dist`、`.coolzhu`、`.git`，以及未完整匹配的 `modules\gui-desktop\packages`、`modules\gui-web\packages`、`modules\tooling\packages`。归档中的 `tmp-partial-incomplete` 不能当完整 tmp 备份。
- 各旧 `modules\...\target` 只是构建缓存候选。本轮未证明其所有独有产物都可丢弃，不能把“可重建”直接当“已归档”。
- `C:\Users\zhupu\.codex\worktrees\db7f\gotdot`、`C:\Users\zhupu\.codex\worktrees\ab8e\gotdot`。其 `.git` 指向缺失的 Desktop 元数据，且与 `D:\gotdot` 比较仍有大量缺失内容和大小不同文件，包括视频、checkpoint、证据。
- `.codex\sessions`、Codex `LocalCache` 和 `Roaming\Codex\web`。目录大小不说明过期，不能整目录删除。
- `.codex\archived_sessions`、`.codex\backups` 及若干 coolzhuagent 工作树入口为指向 D 盘的 junction。它们的目标数据不计 C 盘回收，不能按普通目录递归清理。

历史自动审批拒绝仅表示当时的删除操作未执行，不表示这些目录永久不可清理。本轮遵循“先列清单、由用户手工处理”，没有尝试绕过拒绝。

Codex 中 C、D 两个同名项目的侧栏注册与文件清理是两件事。本轮没有可用的项目删除/归档接口，没有改写 Codex 数据库，归档会话也不能当成移除项目。

补充引用核验：未发现指向旧Desktop仓库的进程命令行引用；当前`D:\coolzhuagent`的Git工作树均登记为D路径，桌面及开始菜单两处正式快捷方式都指向`C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`。旧C仓库及D归档仍含C侧工作树登记，因此`.git`、工作树入口和整个仓库仍不在清理清单中。本轮没有读取进程环境变量或凭据，也不把上述核验冒充全部内核文件句柄审计。

审计：`D:\coolzhuagent\tmp\goal-32wbs-acceptance-20261013\controller-c-readonly-20261015.json`，exit **0**，SHA-256 `3f984a25dc1b15d96eb14bfb57e2150b149e522c003edfcb1cd54ae2fe698911`。每个目录的逐文件哈希收据在该 JSON 的 `hash_receipt` 字段。

引用审计：同目录`controller-c-reference-readonly-20261015.json`，exit **0**，SHA-256 `d3db67e6926681290cc1055f164ea97d9849bceb7582e2e2e39aeeda0e365566`。
