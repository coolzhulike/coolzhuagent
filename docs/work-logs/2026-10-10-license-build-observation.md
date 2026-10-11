# 许可证证据关联实际 Cargo 构建观察

export-cargo-license-evidence.py 新增可选 --build-messages：读取固定 JSONL 字节、记录 SHA256，要求恰有一个 success=true 的 build-finished 且为最后一条记录；缺失/失败/重复/后续追加或没有组件记录均拒绝。compiler-artifact 与 build-script-executed 的 package_id 必须精确存在于输入 metadata，不按名字/版本猜测映射。启用该参数后只导出实际记录出现的组件，保留目标种类、crate_types、消息原因及 fresh；不导出绝对构建路径。

本轮实际执行 cargo build -p coolzhu-core-runtime --offline -j1 --message-format=json-render-diagnostics，退出 0，耗时 2.84 秒。日志 tmp/analysis-current-core-build-messages.jsonl 与对应 stderr.log。结合上一轮冻结的当前 workspace metadata 导出 tmp/analysis-current-core-built-license-evidence.json：76 个组件、98 条观察，72 个组件找到原文。缺失原文的 4 项是 coolzhu-core-runtime、coolzhu-language-service、coolzhu-plugin-system、coolzhu-windows-process-guard；此处“缺失”仅指组件根未定位到候选原文，不推断无许可证。

独立解码验证 122 个原文文件长度与 SHA256 一致；导出 SHA256 为 fc60cf5c99f534245827a7844e918b47bf6b6ac8bd0898ff30d6831ba971ada8。Python 回归实际 exit 0，3 通过，覆盖构建终态与身份负例和此前的原文/路径/独占写入验证；日志 tmp/analysis-license-build-observation-tests.txt。

实际构建观察包括缓存、build script 与 proc macro，不等于哪些代码最终编入和分发。构建成功消息校验也不是来源签名。compiled_component_mapping_verified 继续 false，许可证义务未验证，completeness 继续 incomplete。此次是独立候选 debug 运行时构建，未替代整包正式 release 的组件记录、交付载荷绑定及其它非 Rust 组件证明，不反推已安装 0.2.128。

打包脚本已有 cargo-json-messages 捕获支路，是否覆盖每个分发入口及最终冻结包的原文携带仍需继续接线与验收。该工具可以消费此支路的真实记录，但本轮没有修改打包 manifest，也没有把候选证据发布为最终 NOTICE。
