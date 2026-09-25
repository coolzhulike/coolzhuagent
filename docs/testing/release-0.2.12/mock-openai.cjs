// 安装后隔离验收；所有数据均为合成数据，绝不连接外部模型。
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const PORT = Number(process.env.RELEASE_MOCK_PORT || 18776);
if (!Number.isInteger(PORT) || PORT < 1024 || PORT > 65535 || PORT === 8765) throw Error('验收端口必须为非8765的有效本地端口');
const runtimeDir = path.resolve(process.env.RELEASE_ACCEPTANCE_DIR || __dirname);
fs.mkdirSync(runtimeDir, {recursive:true});
const log = path.join(runtimeDir, 'mock-requests.jsonl');
const note = path.join(runtimeDir, 'mock-note.txt');
const forbidden = path.join(runtimeDir, 'must-not-be-created.html');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const usage = { prompt_tokens: 100, completion_tokens: 20, total_tokens: 120 };
fs.writeFileSync(note, '安装版隔离验收：READ_FILE_RELEASE_OK_20260919\n仅为合成文件，无用户数据。\n');
let count = 0;
function json(res, status, data) { res.writeHead(status, {'content-type':'application/json; charset=utf-8'}); res.end(JSON.stringify(data)); }
http.createServer(async (req, res) => {
  if (req.url === '/health') return json(res, 200, {ok:true, port:PORT, pid:process.pid, requestCount:count});
  if (req.url === '/v1/models') return json(res, 200, {object:'list',data:[{id:'mock-release-2026',object:'model',owned_by:'local-test'}]});
  if (req.url !== '/v1/chat/completions' || req.method !== 'POST') return json(res, 404, {error:'unknown fixture route'});
  try {
    let raw=''; for await (const chunk of req) raw += chunk.toString('utf8');
    const body=JSON.parse(raw), sequence=++count, messages=body.messages||[], lastUserIndex=messages.findLastIndex(m=>m.role==='user');
    const lastUser=JSON.stringify(messages[lastUserIndex]?.content||''), allText=JSON.stringify(messages);
    const recovery=lastUser.includes('运行时恢复提示');
    const toolReturned=messages.slice(lastUserIndex+1).some(m=>m.role==='tool');
    const tools=(body.tools||[]).map(t=>t.function?.name), wantsTool=/模拟工具|\[mock-tool\]/i.test(lastUser);
    const pseudoFail=allText.includes('[release-pseudo-fail]'), pseudo=allText.includes('[release-pseudo-ok]')||pseudoFail;
    const forbiddenCall=allText.includes('[release-forbidden]')&&!recovery;
    const useTool=!recovery&&wantsTool&&!toolReturned&&tools.includes('read_file');
    const answer=pseudo&&(!recovery||pseudoFail)
      ? '<tool_call>'+JSON.stringify({name:'write_file',arguments:{path:forbidden,content:'must not execute'}})
      : pseudo ? '<!DOCTYPE html><html><body>RELEASE_RECOVERY_OK</body></html>'
      : toolReturned ? '已读取隔离验收文件。READ_FILE_RELEASE_OK_20260919。工具结果应显示在独立状态区。'
      : wantsTool&&!tools.includes('read_file') ? 'TOOLS_DISABLED_RELEASE_OK：没有收到 read_file 定义，未执行文件操作。'
      : 'RELEASE_STREAM_OK：思考结束，现在显示最终回复。';
    const toolCalls=[{id:`call_release_${sequence}`,type:'function',function:{name:forbiddenCall?'write_file':'read_file',arguments:JSON.stringify(forbiddenCall?{path:forbidden,content:'must not execute'}:{path:note,offset:0,limit:8})}}];
    const call=useTool||forbiddenCall;
    fs.appendFileSync(log,JSON.stringify({sequence,time:new Date().toISOString(),stream:!!body.stream,model:body.model,toolNames:tools,toolChoice:body.tool_choice||null,toolReturned,useTool,forbiddenCall,pseudo,pseudoFail,recovery,recoveryOriginalPresent:recovery&&allText.includes('[release-'),max_tokens:body.max_tokens,temperature:body.temperature,top_p:body.top_p,reasoning_effort:body.reasoning_effort,mockCredential:req.headers.authorization==='Bearer mock-key'})+'\n');
    const id=`chatcmpl-release-${sequence}`,model=body.model||'mock-release-2026';
    if(!body.stream) { await delay(250); return json(res,200,{id,object:'chat.completion',created:Math.floor(Date.now()/1000),model,choices:[{index:0,message:call?{role:'assistant',content:null,tool_calls:toolCalls}:{role:'assistant',reasoning_content:'核对隔离任务。',content:answer},finish_reason:call?'tool_calls':'stop'}],usage}); }
    res.writeHead(200,{'content-type':'text/event-stream; charset=utf-8','cache-control':'no-cache',connection:'keep-alive'});res.flushHeaders();
    const emit=(delta,finish_reason=null)=>!res.destroyed&&res.write('data: '+JSON.stringify({id,object:'chat.completion.chunk',created:Math.floor(Date.now()/1000),model,choices:[{index:0,delta,finish_reason}]})+'\n\n');
    emit({role:'assistant'});
    for(const thought of ['正在检查安装版隔离任务。','验证思考先于正文。','即将输出最终结果。']) { emit({reasoning_content:thought});await delay(700);if(res.destroyed)return; }
    if(call) {emit({tool_calls:[{index:0,...toolCalls[0]}]});emit({},'tool_calls');}
    else {for(const chunk of answer.match(/.{1,18}/gu)||[answer]) {emit({content:chunk});await delay(70);}emit({},'stop');}
    if(!res.destroyed){res.write('data: '+JSON.stringify({id,object:'chat.completion.chunk',created:Math.floor(Date.now()/1000),model,choices:[],usage})+'\n\n');res.end('data: [DONE]\n\n');}
  } catch(error) {console.error(error.message);if(!res.headersSent)json(res,500,{error:error.message});else res.end();}
}).listen(PORT,'127.0.0.1',()=>console.log(`release mock http://127.0.0.1:${PORT} pid=${process.pid}`));
