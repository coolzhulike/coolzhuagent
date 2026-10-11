# 定稿品牌头像总控集成

交付来源0f1334a5b2766107a45842ff74d2ace52cd80931，cherry-pick为4ff0c5d。已直接核对头像会话的最新人类消息「可以了，制作到头像里」，并查看定稿八张联图；这是视觉确认及接入授权，不是正式UI验收。

原交付覆盖默认/预设/会话列表/发送对象/消息/配置/右栏，并将新默认保存为None以随实际模型匹配。总控补齐新增执行卡：品牌使用execution spec中的历史实际provider/model；自定义头像只通过真实session_id查询，不取活动会话或同名作者。新增32px执行卡图像样式。

总控发现tool-result消息未排除头像绑定，真实前端函数在传入活动session ID后返回自定义头像，原负例actual='assets/avatars/my-upload.png'，expected=null，日志tmp/analysis-avatar-tool-result-before-fix.txt。已将工具角色和全部tool-*类别排除，保留工具图标。实际前端函数/执行卡回调验证同名按ID分开、自定义值保留、历史模型优先、未知SWE用通用图、工具与用户消息不绑定Agent，修正后exit0，日志tmp/analysis-avatar-execution-contract-final.txt。

资产实际复核8主图及24小尺寸PNG，检查正方形、32/48/64尺寸、RGBA和逐文件SHA；结果tmp/analysis-integrated-avatar-assets.json。前端syntax、品牌匹配22条迁移/自定义保留契约、工程代际异步清理通过。仅本地模块/函数验证，不是原生正式UI。

首次整合offline build0/48.90秒；完整回归1451通过/1失败/6忽略/37.05秒，唯一失败为概览旧测试要求bamboo-swordsman.png，已改为新默认图准确元素路径，保留两行布局/武侠控件断言。日志tmp/analysis-avatar-integrated-build.txt与tmp/analysis-avatar-integrated-full-regression.txt。后续修正构建及回归日志tmp/analysis-avatar-integrated-final-build.txt与tmp/analysis-avatar-integrated-full-regression-final.txt；tool-result排除修正之后的最终源码验证单独记录，不能借旧构建声称已经验证最终JS。

正式验收仍开放：正式正常安装下列表/发送对象/消息/配置/执行页一致，首次/重启没有旧图闪现、404或截断，低高度及各DPI清晰，自定义路径保存重启保留。未替换正式0.2.127服务/原运行库，未调用唯一SWE、未新建云端、未改版本标签或更新索引，PR88保持open。
最终tool-result修正源码验证：offline build实际exit0，9.85秒，日志tmp/analysis-avatar-tool-icon-final-build.txt；完整回归实际exit0，1452通过/0失败/6忽略，36.65秒，日志tmp/analysis-avatar-tool-icon-full-regression.txt。
