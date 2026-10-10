# 会话保存避免直接截断旧记录

Session::save_to_path 原先直接 fs::write，会先截断目标文件，再写新 JSON。写入失败时旧恢复数据可能已经丢失。本次同目录暂存完整 JSON、继承已有文件基本权限、sync_all 后由 tempfile 发布替换。普通失败自动清理暂存文件；只接纳可写普通文件，符号链接、目录及只读目标明确拒绝。相对单文件路径以当前目录作为暂存父目录。

新增 Windows 实机文件系统测试，持有允许读但不允许删除/替换的目标句柄，真实触发发布失败；旧字节保持一致且可恢复，目录仅剩旧会话。释放句柄后再次保存成功，恢复新消息且无残留暂存。既有 JSON 格式与工具事实 round-trip 继续通过。

离线 core-runtime/CLI build 退出 0；session 定向测试 2 通过，CLI 完整回归 91 通过。串行工具会话 79376 最终退出 0。日志 tmp/analysis-session-staged-save-build.txt、tmp/analysis-session-staged-save-runtime-tests.txt、tmp/analysis-session-staged-save-cli-tests.txt。没有将本轮定向测试宣称为最新完整 core-runtime 回归。

此修补不提供跨进程单写者/CAS、不保证 Windows 断电目录项持久化、不保留全部自定义 ACL，也不将会话消息与事件变成同一事务。进程崩溃可能残留未发布暂存文件；磁盘满/断电及完整迁移恢复矩阵仍未闭环。已安装 0.2.130 未包含本改动，32WBS 继续进行。
