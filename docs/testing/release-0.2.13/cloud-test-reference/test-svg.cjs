const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),t=require('./test-lib.cjs');
(async()=>{
 await t.guard();await t.settings({supports_multimodal:true,enable_llm_tools:true,computer_use_enabled:false,llm_tool_exposure:'all',tool_allowlist:['read_file','write_file','edit_file','glob_search','grep_search']});
 const rid=await t.room('Qwen SVG鹈鹕骑车长程任务');t.save('svg-context.json',{room:rid});
 const output=path.join(t.runtime,'pelican-bicycle.html');
 const prompts=[
  `请完成一个文件创作任务：用HTML内嵌SVG制作鹈鹕骑自行车的2D循环动画。请使用真实文件工具把完整作品写到 ${output}。要求鹈鹕具有可辨认的长喙、喉囊、翅膀；自行车双轮辐条旋转、脚踏与双腿运动同步，身体轻微起伏，地面和背景移动。单HTML无外部资源。先规划再实际写文件，可自行读取检查。不要在回复中仅输出代码或假称已经写入。完成后简短报告文件路径。`,
  `继续修改刚才同一个文件，保留鹈鹕和所有动画。仔细检查轮子、踏板与双腿：左右腿应有相位差，脚尽量贴合踏板，避免关节脱离。根据实际代码修正可发现的问题，禁止重建成简单占位图。读取后使用文件工具修改，并说明本轮具体变更。`,
  `继续在同一个文件添加播放/暂停按钮、0.5到2倍速度滑块和恢复默认按钮。暂停应冻结全部轮子、腿、背景与身体动画；速度控制同时影响所有相关动画。保留前两轮细节。请真实修改文件。`,
  `继续检查同一个HTML的低高度布局：1024×600与720×480下主要动画和控件应可用，避免被裁切。加一个减少动态选项，兼顾 prefers-reduced-motion。保留已经完成的播放/暂停、速度和恢复默认以及完整鹈鹕轮廓。请实际读取修改文件。`,
  `最后做一次完整审查并按需修改：对照前四轮的所有要求检查同一个文件，确保HTML闭合、SVG无外链、无依赖网络、按钮和速度控件仍在。加入HTML注释 TEST_PELICAN_QWEN_FINAL_20260919。如果没有浏览器工具，请明确说明只做了源码检查，不得声称已目视或浏览器运行验证。简洁列出最终功能。`
 ];
 const outcomes=[];
 for(let i=0;i<prompts.length;i++){
  const result=await t.send(rid,prompts[i],`svg-r${i+1}`,undefined,300000);
  const exists=fs.existsSync(output),stat=exists?fs.statSync(output):null;
  const info={round:i+1,exists,bytes:stat?.size||0,failed:!!result.summary.failure||result.summary.errors.length>0,run_state:result.summary.run?.state};
  if(exists){const bytes=fs.readFileSync(output);info.sha256=crypto.createHash('sha256').update(bytes).digest('hex');fs.writeFileSync(path.join(t.evidence,`pelican-r${i+1}.html`),bytes)}
  outcomes.push(info);t.save('svg-rounds.json',outcomes);
  if(info.failed||!exists||result.summary.run?.state!=='completed')break;
 }
})().catch(e=>{console.error(e.message);process.exitCode=1});
