const fs=require('node:fs'),t=require('./test-lib.cjs');
async function capture(label){const metadata=await t.api('/api/capture','POST');t.save(`${label}-capture.json`,metadata);const r=await fetch(t.base+'/api/capture/latest/image');if(!r.ok)throw Error(`截图HTTP ${r.status}`);fs.writeFileSync(t.evidence+`/${label}.png`,Buffer.from(await r.arrayBuffer()))}
(async()=>{
 await t.guard();const capability=await t.api('/api/computer-use/capabilities');t.save('computer-use-capabilities.json',capability);
 await t.settings({supports_multimodal:true,enable_llm_tools:true,computer_use_enabled:true,llm_tool_exposure:'all',tool_allowlist:['computer_use_perform']});
 const rid=await t.room('Qwen Paint动作与手绘验收');t.save('paint-context.json',{room:rid,fixture_prepared_by_host:true,window:'无标题 - 画图',required_method:'只使用产品正式ComputerUse，不脚本产图'});
 await t.api(`/api/chat/rooms/${rid}/diagnostics`,'PATCH',{real_llm_enabled:true,llm_tools_enabled:true,computer_use_enabled:true});
 t.save('paint-permissions.json',await t.api(`/api/chat/rooms/${rid}/permissions`));
 await capture('paint-r1-before');
 await t.send(rid,'请调用 computer_use_perform，surface=desktop。已经准备了一个新的空白画图窗口，target.window="无标题 - 画图"。先通过UIA选择铅笔或画笔工具并选择黄色，只操作这个窗口。success_criteria为画笔或铅笔已选中且黄色成为当前颜色。constraints为只使用正式computer_use_perform，不运行脚本或命令，不生成、导入、粘贴图片，不处理其它窗口；如果失败报告工具准确终态和原因，不换工具重试。','paint-r1',undefined,360000);
 await capture('paint-r1-after');
 await t.send(rid,'现在测试用户要求的手绘：调用 computer_use_perform 在当前“无标题 - 画图”的空白画布中，通过鼠标或画图形状工具画一个粗略海绵宝宝：黄色方形身体、两只眼睛、嘴、棕色短裤和四肢。不要求完成质量，优先验证真实动作。仅使用正式computer_use_perform；禁止脚本、绘图代码、图片生成、导入或粘贴图片。success_criteria为画布中出现实际手绘身体及五官的可见笔画。如果工具不支持画布拖动或路径笔画，必须如实报告能力缺口和真实执行结果，不得声称画完。','paint-r2',undefined,360000);
 await capture('paint-r2-after');
 t.save('paint-final-insights.json',await t.api(`/api/chat/rooms/${rid}/insights`));
})().catch(e=>{console.error(e.message);process.exitCode=1});
