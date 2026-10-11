# 宿主绑定后整工作区构建

源码提交c18dc5a7bde323381dd5a20e9f069bbe6f73a269，开始前及完成后工作树均干净。

命令 cargo build --workspace --locked --offline -j1，CARGO_BUILD_JOBS=1，CARGO_TARGET_DIR=tmp/cargo-target-integration。真实进程会话65835从启动持续等待至终态，实际exit0，耗时2分01秒；未因观察超时重启。完整输出 tmp/analysis-capture-owner-latest-workspace-build.txt。

共享windows-process-guard新增恢复前资格检查与采集器宿主句柄绑定，工作区各使用方已实际编译，包括主控制台、启动器、CLI、核心运行时、桌面控制台及agent-server等。主控制台151条警告仍存在；这是正常debug构建，未传all-features，不能外推独立Tauri/release整包、安装、外部模型或原生窗口正式验收。最新源码尚未进入0.2.131 MSI；已注册产品仍为此前观察的0.2.130。
