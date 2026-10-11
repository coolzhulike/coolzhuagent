# 品牌Q版头像模块开发记录

从冻结SHA `40d3598471b8ae1ed88ce9c018e5d06f6442fb31` 创建独立managed worktree，分支 `codex/brand-avatars-20261010`。未读全部旧会话、未操作正式桌面/服务/运行库/云端、未创建子代理。

完整读取imagegen和agent-reach技能。实际内置image_gen逐图生成DeepSeek、ChatGPT、Qwen、Claude、Gemini、Grok、GLM和通用助手；失败比例/多余文字/背景废稿未进入资产目录。DeepSeek社区角色特征经社区项目自述核实为非官方；其他拟人方向为原创或用户指定。检索使用技能的mcporter/Exa路由，agent-reach CLI在PATH不可用。

用户明确选择“大头萌系统一八张”，明确授权仅做透明杂点清理和尺寸导出。最终完整保留生成角色，清理与主体无关的alpha杂点，等比居中统一512主图及10%安全边距，导出32/48/64px。形成八张主图、24张导出、联图、SHA/alpha来源manifest与完整提示词记录。选择风格不等于逐张成稿确认或正式软件通过。

审计两代22条旧系统头像、默认/别名/预设、模型与provider、系统回退、自定义值及实际默认Agent/列表/发送对象/消息/配置/执行页入口。建立单一解析模块：自定义优先保留；聚合平台以实际模型品牌为先；未知具体模型用通用；旧系统路径呈现时迁移，不写真实库。注意冻结基线没有单独头像上传接口，附件上传不能冒称头像上传。本次保留上传按钮纯图标布局、武侠控件与竹林背景。

主入口未编辑，提供精确可应用patch交由总控串行集成。离线Node迁移/聚合模型/自定义保留契约、JS语法、候选入口语法与patch应用检查通过；资产尺寸、alpha安全边界及路径检查通过。两次独立cargo build均因机器内存/虚拟内存资源不足exit1，保留日志并等待总控资源排队；没有把局部验证称为Rust编译或真实UI通过。

联合原生UI测试步骤、剩余事项和资源协调状态记录在集成说明与共享avatar-result.json。下一步由总控审查提交、应用入口patch和任务中心头像接线，统一整包构建/正常安装/正式UI联合测试。未合并PR88、未发布或安装。

用户新增1644.webp萌系参考，要求八张各有服装色系与头饰并少量古风。已用内置image_gen分别生成八张新版；原始输出保留，完整提示词存reference-style-prompts.json。按授权清理透明杂点并导出32/48/64px，资产校验通过；gallery-reference-style.png供用户先预览，成稿尚未获逐张确认、正式UI测试仍待总控。

用户已确认最终成稿并要求制作到头像里。入口补丁已实际应用于独立worktree，修正配置模型选项刷新后的头像预览顺序、旧消息归属断言，保持自定义头像。JS语法、22条迁移/品牌匹配契约与32图片alpha校验通过；正式软件仍由总控联合验收。

本轮offline -j1候选build实际exit0（session6564，3m01s）。头像Rust专项测试（session73384）在测试二进制编译阶段LLVM out of memory，exit101，未执行断言。完成预览顺序与断言名称调整后最终build session15278继续验证。

最终源状态cargo build -p coolzhu-web-console --offline -j1实际exit0（session15278，30.10s），日志tmp/analysis-avatar-final-build.txt。正式原生UI与安装仍待总控，未改运行库或正式服务。
