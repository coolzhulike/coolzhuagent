# Browser 时序与大结果连续续读模块交付

基线为冻结 `40d3598471b8ae1ed88ce9c018e5d06f6442fb31`。本开发会话未操作正式软件、8765 服务、真实库、云端模型或安装包。以下为源码候选，真实验收全部待总控联合执行。

## 风险与改动

既有大结果协议接受任意有效 UTF-8 偏移，尾页的 complete 只证明该页结束于文件末尾；8192→69632 的中段缺口仍可能被模型遗漏。集成版累计返回区间并给出准确恢复 offset。原文、身份、摘要校验和每页 JSON 预算保持。完整覆盖只表示宿主已返回全部字节，不证明模型理解了正文。

总控审查修正：工具输出可能只是可选日志或字段查询，不能无条件强制全文消费。集成版移除普通派发、新CU和end_turn的全局drained闸门，保留定位读取；按已返回字节区间合并累计覆盖，返回连续前缀、覆盖字节数、缺口（前八项及总数）和resume_offset。complete仅表示累计全部字节已返回，page_at_end独立表示该页到尾，不能混淆。明确全文任务仍须真实逐页补齐、重建SHA并使用尾部事实；模型生成结束不是这项验收的通过证据。只读分页不增加权限、预算或重放原工具。分页审计不记录正文。

当前资格严格属于本轮 FrozenPolicy：不会将跨重启旧 ID 自动重新授权。原文已经内容寻址持久化，但连续前缀状态仍为内存；完整跨重启续接**未实现**。需要统一执行模块提供经核验的原 run/attempt/owner、原工具资格与终态恢复契约后再接回，不能扫描磁盘重新开放旧结果。本次没有新增第二套执行台账或凭据系统。

Browser 点击诊断保留队列错误 HRESULT 和 ACK 的实际 HRESULT；成功队列调用的 windows 投影返回 unit，字段留 null，不能伪造其原始成功码。confirmed 继续独立反映 ACK JSON 是否有效。没有增加生产延迟、按键持有或删除事件。ACK 仍不证明 DOM 送达/效果/commit。旧127文档负例和来源不明 BODY keydown 不变。

## 入口接线

改动均在既有 ACP 与 native Browser 模块内，无 main.rs/app.js/index.html/styles.css 接线，无新增依赖。总控已集成并修正全文消费范围；仍需最终整包构建和统一真实验收。业务目标及全文读取要求由本次明确任务与对应真实证据核验。

## 总控联合验收步骤

1. 按正式流程集成和构建安装候选，使用原 SWE-2-medium/revision57、原房间与唯一 island-kayak；开发会话未调用它们。
2. 总控启动 `python scripts/testing/browser_longrun_evidence.py --output tmp/browser-longrun-actual`。脚本仅开两个随机本地端口，不读取运行库、不调用模型。addresses.json 含测试地址，expected-result.json 含本次随机尾文和完整正文摘要。不要将这些本地服务检查宣称真实软件验收。
3. 要求真实模型 net_fetch long_result，按实际 next_offset 连续读齐所有页，再执行 Browser 与已声明 DSH 业务步骤。导出同一 result_id 的实际完整页到 JSONL，首个工具响应的第一页也必须包含；运行 verify_contiguous_result.py 核验连续还原和摘要。必须另核模型实际使用随机尾文及后续业务原始回执，不能仅看末页或口述。
   分页 sha256 对应结构化工具回执；expected-result.json 的 sha256 对应服务器正文，二者不能直接比较。需另从实际完整回执取原正文独立核验。
4. 单独采定位/缺口负例：中段未读齐时读取69632和尾页，允许定位，但complete必须false、连续前缀仍为8192且明确报告缺口。全文验收校验器必须拒绝该组不连续页；从resume_offset补齐后才能以完整页与SHA计为全文通过。可选大结果不得阻塞无关业务工具或正常结账；不可把协议end_turn冒作全文验收通过。
5. Browser old 测试页在正常可信 pointerdown 处理器立即请求跨来源导航，不给生产输入加 delay。独立页面记录 down/up/click、键值与code、修饰键、repeat、目标/焦点、documentId、页面与服务器时钟。与共享单调诊断对应，但跨时钟不能直接排序；navigation-requested、pagehide、SourceChanged 不能当精确网络 commit。未命中窄窗口保持开放。
6. 正常 UI 关闭重开产生原生 Target 替换；正常 UI 撤销正在观察的任务。核资源世代变化、输入送达、释放、效果与父终态各自证据。不重复已经通过的释放后观察取消。HRESULT 竞争若未出现，保留未命中，不用人工故障冒原生竞争。
7. 新文档严格零可信输入断言保持；若出现 keydown，保留原始 key/code/target/active/time/path 记录，与总控 UI 操作时间对照，不能删事件或放宽断言。测试页没有系统输入来源能力，无法证明外来输入出处时仍为未知。
8. 反馈上限与迟到取消沿用当前规划交换器：两次未接收反馈，第三次原样交回执行器解析；原请求取消后迟到回答拒绝。跨子目标重规划及跨重启结果续接仍需结合统一运行器正式验收。

测试输出可能含测试页键值，限本地受控证据页使用；脚本不抓其它页面、凭据或输入框内容。停止脚本时在该输出目录建立 stop-server 文件。

## 验证记录

最初两项独立 target 构建并行遭遇 LLVM 内存不足/元数据映射失败，保留 tmp/build-web-console.log、tmp/build-shell.log；随后改为 jobs=1 串行及关闭开发调试信息。最终实际命令和退出码见共享 browser-result.json。Python 语法解析通过；源码测试不是正式模型或原生软件验收。

实际桌面壳 `cargo build --manifest-path modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml --offline` 退出0，4分50秒，日志 tmp/build-shell-serial.log。控制台单任务重建仍因 `memory allocation of 1343504 bytes failed` 退出1，日志 tmp/build-web-console-serial.log；没有观察到可用整包产物，不能标为构建通过。收到全机资源协调要求后，没有启动新的 Cargo 编译或测试；后续控制台构建、result_pages 与 planning_exchange 回归交由总控排队。临时小型 harness 已准备但**未运行**，不作为验证证据。Python 分页证据校验器连续/重读/缺口断言及 git diff --check 通过。
