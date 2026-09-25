# 2026-08-23 安装后桌面图标修复

## 现象

桌面快捷方式显示为白色文件页/蓝色竖条的 Windows 默认图标，未显示 CZ 像素图标。原有 WiX `Shortcut` 已引用 `CoolzhuApplicationIcon`，但快捷方式目标 `COOLZHU-AGENT.exe` 自身没有 PE 图标资源；当 Shell 图标缓存失效、快捷方式被复制或由其他入口创建时会回退到默认图标。

## 修复

- `packages/app-launcher/build.rs` 使用 Windows SDK `rc.exe` 将 `docs/design-assets/coolzhu-icons-2026-08-12/coolzhu-application-icon.ico` 编译为资源，并通过 `cargo:rustc-link-arg-bin=COOLZHU-AGENT=...` 嵌入启动器 PE。
- 资源编译器查找支持 `RC`/`WINDOWS_RC` 环境变量、PATH 和 Windows Kits 目录；非 Windows 构建不要求 Windows SDK。
- `scripts/test-package-safety.ps1` 增加启动器图标资源构建契约，防止后续只修改 WiX 而再次遗漏目标程序图标。

## 验证证据

- `cargo build -p coolzhu-app-launcher --bin COOLZHU-AGENT --offline`：通过。
- 从新编译的 `COOLZHU-AGENT.exe` 提取关联图标，得到 CZ 图标：`tmp/launcher-icon-fixed.png`。
- 使用完整运行时包（补齐当前设备缺失的 WebView2Loader.dll）构建 MSI：
  - `dist/CoolzhuAgent-0.2.7.msi`
  - SHA-256：`683F0F3FC9E597F661E689D1D680D49BBDFF792D5F46A52E0EF85ED8E1F4D87E`
- 从 MSI 反编译确认 Start Menu 与 Desktop 两个 Shortcut 都仍引用 `CoolzhuApplicationIcon`，目标为 `[INSTALLDIR]COOLZHU-AGENT.exe`。
- 从 MSI staging 包中的启动器提取图标：`tmp/msi-staged-launcher-icon.png`。
- Windows 文件资源管理器大图标实机截图显示两个快捷方式均为 CZ 图标：`tmp/icon-shortcut-evidence-20260823`（embedded PE icon 与 ICO fallback）。
- 复核设备当前安装状态发现此前仍是两个 0.2.5 MSI 实例，目标 PE 提取出的确为旧蓝色默认图标；按两个已核验 ProductCode 完成卸载后，以 UAC 提权安装 `dist/CoolzhuAgent-0.2.7.msi` 成功。
- 安装后产品登记为 0.2.7，公共桌面快捷方式目标为 `C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe`，Shell IconLocation 指向 MSI 缓存的 `CoolzhuApplicationIcon`；重新提取目标 PE 得到 CZ 图标：`tmp/installed-shortcut-icon-fixed-round4.png`，完整结果记录于 `tmp/icon-install-round4-evidence.json`。

## 环境备注

完整 `build-msi.ps1` 编译过程中，设备现有构建包缺少 `modules/gui-desktop/target/tauri-shell/debug/WebView2Loader.dll`，因此使用既有完整包保留该 DLL、替换本轮所有已编译二进制后完成 WiX 打包；这不是图标资源失败。
