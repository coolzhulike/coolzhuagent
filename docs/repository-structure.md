# 仓库结构

```text
coolzhu/
  Cargo.toml
  Cargo.lock
  src/
    lib.rs
  docs/
    README.md
    repository-structure.md
    interface-contracts.md
    development-standard.md
    testing-standard.md
    migration-notes.md
  tests/
    README.md
    module_linkage_smoke.rs
    manual-visual-confirmation.md
  modules/
    core-runtime/
      INTERFACE.md
      packages/
        core-runtime/
        agent-server/
        language-service/
    llm-adapter/
      INTERFACE.md
      packages/llm-adapter/
    tooling/
      INTERFACE.md
      packages/
        tool-registry/
        plugin-system/
        command-router/
        compatibility-harness/
    vision/
      INTERFACE.md
      packages/vision-service/
    computer-use/
      INTERFACE.md
      packages/computer-use-core/
    gui-web/
      INTERFACE.md
      packages/web-console/
    gui-desktop/
      INTERFACE.md
      packages/desktop-console/
    cli/
      INTERFACE.md
      packages/command-line/
    diagnostics/
      INTERFACE.md
      packages/diagnostics/
  packages/
    app-launcher/
  .coolzhu/
    plugins/                    # Cargo workspace 插件源码；允许进入公开源码包
      coolzhu-tdd-runner/
      coolzhu-git-workflow/
      coolzhu-code-review/
      coolzhu-orchestrator/
      coolzhu-docgen/
      coolzhu-db-tools/
      coolzhu-debug-diag/
      coolzhu-monitor/
      coolzhu-marketplace/
    # 其它内容是本地运行时状态，不进入公开源码包
  config/
    package-launcher.json       # 可公开的启动器配置
    package-manifest.json       # 可公开的安装包清单
  installer/
    Product.wxs
  scripts/
    build-msi.ps1
    project-delivery.ps1
```

命名规则：

- 目录使用 kebab-case。
- package 使用 `coolzhu-*`。
- 对外文档使用 `INTERFACE.md`。
- 根目录只保留集成 workspace 和联调测试。
- 模块内测试保留在模块 package 内，跨模块联调测试放根目录 `tests/`。
- `.coolzhu` 默认视为本地运行时目录；仅 `plugins/` 下被根 `Cargo.toml` 引用的插件 crate 属于有效源码。
- `target/`、`dist/`、`package/`、`tmp/`、`backups/`、模型权重、会话数据库、凭据和非白名单配置均不进入 GitHub 源码包。
