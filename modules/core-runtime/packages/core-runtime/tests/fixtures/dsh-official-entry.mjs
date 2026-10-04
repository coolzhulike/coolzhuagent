// 仅测试：在未修改的官方calculator函数内部记录真实调用栈并注入有界调度延迟。
// 使用同进程inspector.Session，不开放调试端口，不替换SDK/工具函数或协议回包。
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {Session} from 'node:inspector';
import {createHash} from 'node:crypto';
const directory=path.dirname(fileURLToPath(import.meta.url));
const config=JSON.parse(fs.readFileSync(path.join(directory,'probe-config.json'),'utf8'));
const ipc=process.argv[2];
const request=JSON.parse(fs.readFileSync(path.join(ipc,'request.json'),'utf8'));
const publish=(name,value)=>{
  const target=path.join(directory,name),temporary=target+'.tmp';
  fs.writeFileSync(temporary,JSON.stringify(value,null,2));fs.renameSync(temporary,target);
};
const source=path.join(request.root,'lib/evaluate.js');
const bytes=fs.readFileSync(source);
const sourceHash=createHash('sha256').update(bytes).digest('hex');
if(sourceHash!==config.evaluate_sha256) throw new Error('官方函数字节不匹配');
const lines=bytes.toString('utf8').split('\n');
const line=lines.findIndex(text=>text.trim()==='let pos = 0;');
if(line<0) throw new Error('官方parse函数观察点缺失');
const session=new Session();session.connect();
const post=(method,params={})=>new Promise((resolve,reject)=>session.post(method,params,(error,result)=>error?reject(error):resolve(result)));
const scripts=new Map();session.on('Debugger.scriptParsed',({params})=>scripts.set(params.scriptId,params.url));
await post('Debugger.enable');
const breakpoint=await post('Debugger.setBreakpointByUrl',{url:pathToFileURL(source).href,lineNumber:line,columnNumber:0});
let entered=false;
session.on('Debugger.paused',({params})=>{
  try {
    if(!params.hitBreakpoints?.includes(breakpoint.breakpointId) || entered) return;
    entered=true;
    const frames=params.callFrames.map(frame=>({function:frame.functionName,url:scripts.get(frame.location.scriptId)||frame.url,
      line:frame.location.lineNumber+1,column:frame.location.columnNumber+1}));
    publish('entered.json',{pid:process.pid,mode:config.mode,entered_at_ms:Date.now(),deadline_ms:request.deadline_ms,
      context:request.context,source_sha256:sourceHash,line:line+1,source_line:lines[line].trim(),frames});
    const resumeAt=config.mode==='normal'?Date.now()+120:request.deadline_ms+(config.mode==='late'?250:4000);
    // 延迟只在真实parse已被官方SDK调用之后发生；源文件、参数和返回值不改写。
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,Math.max(0,resumeAt-Date.now()));
    publish('resumed.json',{at_ms:Date.now(),deadline_ms:request.deadline_ms,
      cancellation_file_present:fs.existsSync(path.join(ipc,'cancel.json'))});
  } catch(error) {publish('probe-error.json',{message:String(error),stack:error.stack});}
  finally {session.post('Debugger.resume');}
});
try {
  await import(pathToFileURL(config.production_entry).href);
  // 仅复制生产process.mjs已经写出的终态证据，不写回IPC、不伪造成功。
  const terminal=JSON.parse(fs.readFileSync(path.join(ipc,'result.json'),'utf8'));
  publish('terminal-copy.json',{at_ms:Date.now(),terminal});
} finally {
  session.disconnect();
}
