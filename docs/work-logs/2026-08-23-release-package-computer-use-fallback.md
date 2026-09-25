# 2026-08-23 release 安装包与 Computer Use 降级自检

本轮将当前已验证的模型思考事件、音频 readiness 修复和无 ShowUI Computer Use
降级链路整合到 release 分支，重新生成 Windows MSI，并用打包后的二进制做隔离
启动自检。当前已安装在用户设备上的 CoolzhuAgent 进程未被停止或替换。

## 构建信息

- 分支：`codex/release-20260823-computer-use`
- release 构建：`scripts/package-all.ps1 -Configuration release`
- MSI 构建：`scripts/build-msi.ps1 -Version 0.2.6 -Configuration release -SkipPackageBuild`
- 安装包：`dist/CoolzhuAgent-0.2.6.msi`
- 大小：180102656 bytes
- SHA-256：`F96C28FF5F997A037594147C6D6EAEC329610A283030E760E2E313F6ACFADF17`
- WiX：`5.0.2+aa65968c`
- 签名：未签名（`signed=false`）
- 包安全扫描：756 个文件，`safe=true`，0 条 findings

release 包中的 `coolzhu-web-console.exe` 与 `package/bin/coolzhu-web-console.exe`
字节数均为 25964544，SHA-256 均为
`F8576E8BA6B95E62874E147A2026F310A3677C1C8EEFF72EE0190D6D28E7E58F`。

## 验证结果

- `scripts/test-package-manifest.ps1`：PASS
- `scripts/test-package-safety.ps1`：PASS
- release package runtime：PASS（隔离端口 8799，启动后访问
  `/api/computer-use/capabilities`）
- 运行时能力报告：ShowUI 和本地 VLM 为 `skipped`；Browser DOM 因扩展未连接为
  `skipped`；UIA、`ocr_template`、桌面输入适配器和 `manual_confirmation` 为
  `available`。
- runtime notes 明确说明视觉模型不可用时不会伪报成功，并返回 dry-run/人工确认
  降级路径。

## 环境现象

web-console 不通过命令行 `--port` 参数覆盖监听端口，隔离自检使用
`COOLZHU_RUNTIME_DIR` 下的 `coolzhu.toml` `[web].bind_addr` 配置监听 8799；这不
影响默认 8765，也不影响安装包启动。首次误用 `--port 8778/8799` 的试运行因仍
绑定默认 8765 与已安装实例冲突，进程退出且未触碰已安装实例；随后按配置文件方式
重跑通过。
