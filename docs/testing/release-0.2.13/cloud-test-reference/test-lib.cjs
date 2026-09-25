const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=__dirname,runtime=path.join(root,'runtime'),evidence=path.join(root,'evidence'),base='http://127.0.0.1:18795',qid='session-1779459149988';
fs.mkdirSync(evidence,{recursive:true});
const save=(name,data)=>fs.writeFileSync(path.join(evidence,name),typeof data==='string'?data:JSON.stringify(data,null,2));
async function api(route,method='GET',body){const r=await fetch(base+route,{method,headers:body?{'content-type':'application/json'}:{},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(30000)});const t=await r.text();assert(r.ok,`${method} ${route} HTTP ${r.status}: ${t.slice(0,400)}`);return JSON.parse(t)}
async function guard(){assert(fs.existsSync(path.join(runtime,'.qwen-test-fixture')));const w=await api('/api/workspace');assert.equal(path.resolve(w.workspace).toLowerCase(),runtime.toLowerCase());const s=await api(`/api/sessions/${qid}/model-settings`);assert.equal(s.session.model,'qwen3.8-flash');return s}
async function settings(parameters,session){const prior=await api(`/api/sessions/${qid}/model-settings`);await api(`/api/sessions/${qid}/model-settings`,'POST',{parameters:{...prior.parameters,...parameters},...(session?{session}: {})});return api(`/api/sessions/${qid}/model-settings`)}
async function room(name){const r=(await api('/api/chat/rooms','POST',{name})).room.id;await api(`/api/chat/rooms/${r}/permissions`,'PATCH',{permission_profile:'full-access',risk_acknowledged:true,confirmed_twice:true});return r}
async function send(rid,text,label,attachments,timeout=240000){
 const request={session_id:qid,target_agent_ids:[qid],chat_room_id:rid,text,...(attachments?{attachments}:{})};save(`${label}-request.json`,request);
 const start=Date.now(),events=[],decoder=new TextDecoder();let buffer='',raw='',failure=null;
 try{const response=await fetch(base+'/api/chat/send/stream',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(request),signal:AbortSignal.timeout(timeout)});assert(response.ok,`HTTP ${response.status} ${await (!response.ok?response.text():Promise.resolve(''))}`);
 for await(const chunk of response.body){const next=decoder.decode(chunk,{stream:true});raw+=next;buffer+=next;let m;while((m=buffer.match(/\r?\n\r?\n/))){const frame=buffer.slice(0,m.index).replace(/\r/g,'');buffer=buffer.slice(m.index+m[0].length);const type=frame.split('\n').find(l=>l.startsWith('event:'))?.slice(6).trim();const data=frame.split('\n').filter(l=>l.startsWith('data:')).map(l=>l.slice(5).trimStart()).join('\n');if(data){let parsed;try{parsed=JSON.parse(data)}catch{parsed={raw:data}}events.push({type,at_ms:Date.now()-start,data:parsed});save(`${label}-events.json`,events)}}}
 }catch(e){failure=e.message}
 save(`${label}.sse`,raw);save(`${label}-events.json`,events);
 const answer=events.filter(e=>e.type==='message_done'&&e.data.role==='assistant'&&!['reasoning','tool-call','tool-result','tool-summary'].includes(e.data.kind)).map(e=>e.data.content).join('\n');save(`${label}-answer.txt`,answer);
 const started=events.find(e=>e.type==='started'||e.type==='start')?.data||events.find(e=>e.data.run_id)?.data;
 if(failure&&started?.run_id){await api(`/api/runs/${started.run_id}/interrupt`,'POST',{reason:'隔离验收达到时间上限'}).catch(()=>{});}
 const run=started?.run_id?await api(`/api/runs/${started.run_id}`).catch(e=>({error:e.message})):null;
 const insights=await api(`/api/chat/rooms/${rid}/insights`);save(`${label}-insights.json`,insights);save(`${label}-run.json`,run);
 const summary={label,room:rid,elapsed_ms:Date.now()-start,answer,errors:events.filter(e=>e.type==='error').map(e=>e.data),failure,run,events:events.length,tool_calls:events.filter(e=>e.data.kind==='tool-call').length,tool_results:events.filter(e=>e.data.kind==='tool-result').length,usage:insights.usage};save(`${label}-summary.json`,summary);console.log(JSON.stringify(summary));return {summary,events,answer};
}
module.exports={root,runtime,evidence,base,qid,save,api,guard,settings,room,send};
