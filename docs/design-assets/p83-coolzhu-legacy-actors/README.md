# p83-coolzhu 旧启动动画角色帧归档

归档日期：2026-10-17（仓库清理，`docs/repository-cleanup-review-2026-10-17.md` C1）。

## 内容

- `[C,H,L,O1,O2,U,Z]-k[0-4].png`：35 张旧版七字母逐帧动画角色帧（COOLZHU 每个字母 5 个关键帧）。
- `swordsman-four-poses-v2.png`：旧版左下舞剑人物四姿势合图（与 `docs/design/2026-09-27-ui-concepts/assets/swordsman-four-poses-v2.png` 字节相同，此处保留以使旧 actors 集合完整）。

## 出处与状态

原路径：`modules/gui-desktop/packages/tauri-shell/ui/assets/p83-coolzhu/actors/`。

当前生产启动页（`ui/coolzhu-seven-letter-manifest.js`）只声明 `meditating-actor-v1.png`（静坐运气人物）；
字形走 `glyphs/*.png`，背景为 `scroll-shanhe-v1.png`。以上 36 张图已不在任何播放清单或代码引用中，
属于被替换的旧美术源文件。

历史契约要求保留美术源文件，因此移入本设计归档而非删除；同时不再随 `gui-desktop.tauri-ui`
打包载荷进入安装包。若未来复用这些帧，需同时恢复资源清单声明并重新走启动动画验收。
