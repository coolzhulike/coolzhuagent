## 实施前契约收紧（主代理采纳交叉审查）

以下定义覆盖候选材料里同义但不完全一致的字段名；适用于最终04与06。它们是拟议契约，尚未实现。

1. **发送事实只用三值**：`not_sent / may_have_been_sent / sent`，不存在第四个`unknown`枚举。未知是否发送统一为`may_have_been_sent`。`partial=true/false/null`为独立字段，记录已知完成点数和最后确认输入；`sent`表示已确认至少有输入被发出，不等于整段路径完成。错误发生在调用前可确认零输入才记`not_sent`；API返回不完整或崩溃且无法判断则`may_have_been_sent`。旧CU-01建议的none/partial/complete/unknown不得直接迁移为三值，必须结合每步证据，不能猜旧数据。
2. **所有权覆盖真正的输入执行进程**：桌面独占不能只跟随Web父进程的生命周期。如果helper持有输入权限并可能比父进程活得久，父进程死亡不足以证明桌面可以安全接管。实现需绑定所有能发输入的worker生命周期、代际/fencing与退出确认，或让控制句柄由实际输入executor持有；有任何前owner执行者存活/释放未知时拒绝新输入。验证父死子活、子死父活、双实例同时接管、长调用和进程恢复，不仅测父进程崩溃。操作系统句柄释放不是目标状态已知或按键已抬起的证明。
3. **到期覆盖进行中的动作**：任务deadline到达后不得启动新操作，进行中笔画须在下一可取消安全点停止，立即进入有上限的按键/鼠标释放及审计；不得等完整长笔画结束才检查时间。报告任务执行耗时与收尾耗时，写明不可中断系统调用的实际界限；无法确认释放时标记不确定并阻止后续执行。测试在down后、路径中和release时到期，检查停止延迟、无新点输入、无新规划/验收请求及有界清理，不用只覆盖“调用前deadline已过”。
4. **Paint门禁统一**：三级依赖为受控输入契约通过→最低真实Paint闭环稳定通过→海绵宝宝A/B。最低闭环未通过时可以收集复杂模型输出作为探索，但不得计入已排除Agent缺陷后的模型归因或正式成功率。每级单独定义已通过案例集及未覆盖范围，不用“可以解释失败”替代通过。
5. **计数回归为明确发布项**：用R6等价fixture的3次attempt、1次真实input、2次stale拒绝检查步骤、run聚合、API、UI、导出报告一致；重连重复投递不能重复累加。若迁移废弃旧`action_count/replan_count`，所有消费者必须改从唯一新源计算，并显示旧记录的未知来源。若保留旧列，定义其兼容口径，不能把两个0原样当可信计数。该fixture只复现记账口径，不宣称重新执行R6或定位stale子因。

### 输入事实JSON样例（拟议v1）

`partial`使用JSON布尔或null，null表示不知道；`confirmed_point_count`和`last_confirmed_position`同样可空。路径点数0表示确认尚无完成的路径采样点，不能推出零输入；down已确认、首路径点前停止时，sent与点数0可以同时成立。输入事件另计数，非路径动作的路径字段为null。`path_completed`独立记录路径完成，不能从`sent`推出。坐标对象需引用同一frame/transform的空间定义。下列为测试例，不是现有生产payload；最终契约及完整释放/迟到事实规则以04第2节为准。

```json
[
  {"case":"发送前拒绝","input_delivery":"not_sent","partial":false,"path_completed":false,"confirmed_point_count":0,"last_confirmed_position":null},
  {"case":"确认只执行部分路径","input_delivery":"sent","partial":true,"path_completed":false,"confirmed_point_count":3,"last_confirmed_position":{"frame_id":"fixture-1","space":"frame_pixels","x":30,"y":40}},
  {"case":"完整路径已发送","input_delivery":"sent","partial":false,"path_completed":true,"confirmed_point_count":8,"last_confirmed_position":{"frame_id":"fixture-1","space":"frame_pixels","x":80,"y":40}},
  {"case":"请求发给helper后失联且无发送回执","input_delivery":"may_have_been_sent","partial":null,"path_completed":null,"confirmed_point_count":null,"last_confirmed_position":null}
]
```

`sent + partial=null`也是合法组合：已确认down发出、后续helper失联，但不知道路径是否全部完成。`not_sent + confirmed_point_count>0`、`not_sent + partial=true`、`may_have_been_sent + path_completed=true`等自相矛盾组合必须拒绝。点数只适用于声明点采样含义的路径动作，不把click的down/up当成两个路径点；非路径动作另记输入事件计数。`path_completed=true`仍不证明mouseup已确认，释放状态独立记录，更不证明UI效果或目标完成。

正式PAINT-03实验预注册最低语义图形判据、样本数和独立盲判规则。允许低画质，分别评价是否在正确画布有效落笔、前置工具是否正确、目标关键结构是否出现；任意涂鸦不自动等于海绵宝宝。探索轮不并入正式A/B分母。
