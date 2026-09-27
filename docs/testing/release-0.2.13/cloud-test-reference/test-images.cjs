const fs=require('node:fs'),path=require('node:path'),t=require('./test-lib.cjs');
(async()=>{
 await t.guard();
 const file=process.argv[2],prefix=process.argv[3]||'';if(!file)throw Error('缺少图片夹具路径');
 const form=new FormData();form.set('file',new Blob([fs.readFileSync(file)],{type:'image/png'}),'visual-fixture.png');form.set('kind','image');
 const response=await fetch(t.base+'/api/attachments/upload',{method:'POST',body:form});if(!response.ok)throw Error(`上传HTTP ${response.status}`);const uploaded=await response.json();t.save(prefix+'image-upload.json',uploaded);
 const prompt='请只根据附件图片回答：从左到右有哪些图形、各是什么颜色？图片下方的英文数字标记是什么？请完整抄写标记。不要调用工具，不要根据文件名猜测。';
 const direct=await t.room('Qwen原图直传验收');await t.settings({supports_multimodal:true,enable_llm_tools:false,computer_use_enabled:false});
 await t.send(direct,prompt,prefix+'image-direct',[uploaded.attachment]);
 const fallback=await t.room('纯文本经Agnes视觉转述验收');await t.settings({supports_multimodal:false});
 await t.send(fallback,prompt,prefix+'image-via-agnes',[uploaded.attachment]);
 await t.settings({supports_multimodal:true});
})().catch(e=>{console.error(e.message);process.exitCode=1});
