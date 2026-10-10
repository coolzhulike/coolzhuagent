# 2026-10-10 单一采集回执回归与整包复验

修正诊断任务返回值污染后，scripts/test-managed-build-capture.ps1 对成功/失败入口均增加单一对象断言。使用本轮最新helper（tmp/cargo-target-build-capture/debug/build-capture.exe），PS5.1/PS7均四项通过、真实进程exit0。证据 tmp/analysis-capture-single-receipt-ps5.txt、tmp/analysis-capture-single-receipt-ps7.txt。

上一轮修正后30次实际受管Cargo构建最终exit0、total30 failed0，结果 tmp/analysis-capture-settle-final-results.json，不能与首次返回值污染驱动exit1混淆。

完整Loader整包复验已启动，同一真实句柄26620仍存活。此前两项125用例已按预期返回PRODUCER-NOT-FOUND、ARCH-MISMATCH；尚未确认整组终态，不登记全套通过。日志 tmp/analysis-capture-integrated-loader-second.txt。package-all生产接入保持未提交；全量源码核验、正式交付与32WBS仍未闭环。
