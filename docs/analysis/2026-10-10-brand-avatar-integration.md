# 品牌头像交接（2026-10-10）

基线：`40d3598471b8ae1ed88ce9c018e5d06f6442fb31`。本模块不操作正式服务、运行库、MSI 或云端。用户已选择“大头萌系统一八张”并授权透明杂点清理及尺寸导出；用户已确认最终八张成稿，联合原生软件测试尚未完成，不宣称全面替换验收通过。

## 可集成成果与实际验证

- 八张主图及24张32/48/64px导出位于 `assets/avatars/brand-v1/`，运行 `assets/avatars/manifest.json` 已在本独立worktree换成品牌列表。旧文件保留供已有路径识别，不再进入预设选择器。
- `docs/design-assets/brand-avatars-2026-10-10/` 含联图、实际alpha与SHA记录、生成提示词。所有资产非官方；用户选择不被扩大为成稿/正式软件验收。
- `brand_avatars.js`、`brand_avatars.css` 和离线契约验证已准备；入口patch通过 `git apply --check`，候选app.js通过 `node --check`。用户要求制作到头像里后，已在本worktree应用app.js/index.html/main.rs/static_resource_contract.rs入口修改；styles.css未改。
- 本worktree已应用入口补丁；总控整合时可审阅 `docs/analysis/2026-10-10-brand-avatar-entry.patch`。它同时处理白名单、初始回退、消息归属、配置预览、会话列表、后端自动默认和旧契约断言，不能只换manifest。
- 执行页由任务中心开发模块使用 `CoolzhuBrandAvatars.resolve({avatar, model: actual_model, provider: actual_provider})`；优先从执行记录的真实session获取头像。`model_id`也可解析，不用当前聊天室的model冒充后台执行来源。
- 两次独立Cargo构建真实终态均exit1：首次1455页面文件不足；`-j1`重试1453系统配额不足、windows-interface rustc崩溃。日志保留 `tmp/analysis-avatar-build*.txt`；无live构建句柄。已读资源协调JSON，后续重型构建只由总控排队。
- 联图是资产展示与实际尺寸检查，不是正式软件截图；未调用唯一云端验收会话。

## 风险与实施决策

- 只换文件会遗漏两代路径、HTML 初始回退、按序分配的后端默认和未显示头像的会话列表。独立 `brand_avatars.js` 提供一个解析入口，配合精确入口补丁。
- 匹配顺序：非系统自定义头像 → 实际模型名 → 无明确模型时 provider → 通用头像。模型名优先保证 Devin/百炼等聚合渠道运行 Claude/DeepSeek 时显示实际模型品牌；未知具体模型不冒用聚合平台品牌。
- 旧系统预设 22 个路径是明确枚举，不按整个 `assets/avatars/` 目录批量覆盖。用户上传、data/blob、绝对地址或其它自定义文件保持原值。现有服务端权限及路径校验不扩张。
- 后端目前只允许 `assets/avatars/` 下 PNG/WebP，前端又只接受 manifest 条目；本次没有找到独立头像上传接口，附件上传按钮不是头像上传入口。不得把不存在的上传功能写成已验证。前端解析和配置保存仍须保留已有自定义值。
- 新会话和 seed 不应再按 manifest 索引固定分配头像：`avatar=None` 表示随实际模型自动选择；用户显式选择新预设继续保存该路径。旧预设只在呈现时迁移，不改写真实数据库。
- 消息头像必须以 `session_id` 为首选，作者名称只作兼容回退；用户消息及工具状态图标维持原语义。
- 图像由内置 image_gen 实际逐资产生成和修正；不使用脚本绘制替代。品牌拟人均为非官方方案；DeepSeek 鲸鱼娘方向有社区二创依据，其余设定是本项目原创/用户指定。

## 旧引用覆盖清单（冻结基线行号）

| 入口 | 原位置 | 集成要求 |
| --- | --- | --- |
| 默认及第一代别名 | app.js:12–28 | 删除旧默认常量/迁移 map，使用模块 fallback 与显式旧路径集合 |
| 主 Agent 概览 | app.js:7585–7624；index.html:43 | `avatarForSession(agent)` 自动解析；初始 src 换 generic |
| manifest/允许路径 | app.js:8960–8998 | 新子目录 brand-v1；manifest 不含旧预设；加载后刷新列表和发送对象 |
| 会话/作者解析 | app.js:9122–9157 | 使用模块 resolve；消息优先 session ID；视觉/音视频模型也用品牌头像 |
| 模型配置 | app.js:9164–9218、13570；index.html:195 | 空值预览当前 provider/model；旧预设表单值变空表示自动；自定义值原样保留 |
| 房间频道成员 | app.js:12312–12317 | 共用 avatarForSession |
| 会话列表 | app.js:13010–13027 | 添加 32px 图片，保持 session ID、文字和 aria 状态 |
| 发送对象 | app.js:15552–15566 | 共用 avatarForSession；新默认始终可显示 |
| 消息新增/更新 | app.js:19656、19730、19743 | assistant/模型消息优先 session ID；用户和工具图标不能误绑定活动 Agent |
| 右栏/执行页 | index.html:768；后续任务中心模块 | 右栏随所选会话更新；执行卡使用真实执行 session/model/provider 调用 resolve |
| 后端新建/seed | main.rs:40602、50357–50380 | 新默认 None，不按列表索引轮换；保留显式头像校验 |
| 静态资源/打包 | static_resource_contract.rs、打包 assets 遍历 | 新 JS/CSS 加入 CORE；完整资产随目录打包；总控更新源码/包 manifest |
| 旧契约测试 | main.rs:67208、67232、80509 附近 | 调整旧默认/旧 manifest 断言；保留非法路径拒绝，不削弱路径校验 |

武侠图标 `WUXIA_ICON_ALIASES` 不属于头像，保留；竹林背景、玉石控件、桌宠和上传纯图标布局保留。`renderCrewCard` 的机器人编队图属于另一套桥舰场景，不是模型头像，本次不改。

## 联合 UI 验收（仅总控串行执行）

1. 集成入口补丁、静态 CORE、最终 manifest；独立离线 cargo build 成功后统一候选包，按既定流程完整正常安装。
2. 正式软件确认默认 Agent、列表、发送对象、消息、配置和右栏执行页一致；观察首次加载与重启，无旧侠客闪现、404 或截断。
3. 同一测试会话切模型，检验旧系统头像自动匹配；显式新预设保持选择。未知 SWE-2-medium → 通用；Devin+Claude → Claude；百炼+DeepSeek → DeepSeek；OpenAI-compatible+Qwen → Qwen。
4. 保留两份同名会话，用真实 session ID 检验消息与执行卡归属。用户消息不显示活动 Agent 的品牌头像，工具状态图标不被覆盖。
5. 使用现有自定义头像路径并保存配置、重启，确认路径与图像未被替换。若正式环境没有头像上传能力，只验现有自定义路径并标明限制，不把附件上传当头像上传。
6. 在 32/48/64px、低高度/不同 DPI 下查看边距与脸部辨识；白龙在浅色玉石底和深色竹林底均清晰。
7. 不操作微信、Paint，不新建云端会话、不改模型绑定；只有总控可使用既定 SWE 验收会话。

## 参考

- [DeepSeek Whale-chan 社区项目自述](https://github.com/Neko3000/deepseek-whalechan)：明确声明非官方社区二创。
- [Qwen 官网](https://chat.qwen.ai/) 与 [通义实验室](https://qianwen.aliyun.com/)：品牌入口；没有据此冒称拟人角色官方。

检索实际使用 agent-reach 技能的 `mcporter/Exa` 路由；`agent-reach doctor` 在当前 PATH 不可用。ChatGPT 白龙是用户指定方向；未获得其官方拟人设定证据。
