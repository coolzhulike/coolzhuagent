# 2026-10-10 Job 自然收尾有界等待

新增helper诊断后的20次真实引导复现有3次125，诊断均为根退出后Job仍活跃（tmp/analysis-bootstrap-125-repro.json）。这证明失败分支，不单独证明计数延迟还是短暂后代。新增250ms有界自然清空等待；未自然清空仍125并回收，绝不以强杀换成功。真实cargo build offline locked -j1成功，日志 tmp/analysis-capture-settle-build.txt。

PS5真实辅助程序四项回归通过，含30秒后代根退出0仍125且自有后代回收（tmp/analysis-capture-settle-regression.txt）。首次30轮驱动发现诊断CopyToAsync.GetResult返回VoidTaskResult污染PowerShell返回对象，已[void]抑制；该轮退出1不可计通过。修正后结果另存 tmp/analysis-capture-settle-final-results.json，须以实际进程终态确认。

整包完整回归尚待重跑，正式安装、输入冻结核验及32WBS未闭环。
