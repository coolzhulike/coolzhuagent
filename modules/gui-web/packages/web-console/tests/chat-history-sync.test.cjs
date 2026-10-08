const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const context = {window: {}, AbortController, DOMException, setTimeout, clearTimeout};
vm.runInNewContext(fs.readFileSync(path.join(__dirname,'../src/chat_history_sync.js'),'utf8'), context);
const {create, readWindow} = context.window.CoolzhuChatHistorySync;
const sleep = ms => new Promise(resolve => setTimeout(resolve,ms));
const deferred = () => {let resolve; const promise = new Promise(r => resolve=r); return {promise,resolve};};
function events() {const handlers=new Map(); return {closed:false, addEventListener:(event,fn)=>handlers.set(event,fn),
  emit:event=>handlers.get(event)?.(), close(){this.closed=true;}};}

test('A→B→A迟到读取与旧finally不能覆盖新代数', async () => {
  const sources=[], requests=[], applied=[];
  const sync=create({sourceFactory:()=>{const source=events();sources.push(source);return source;}, busy:()=>false,
    read:scope=>{const wait=deferred(); requests.push({scope,wait});return wait.promise;}, apply:row=>applied.push(row)});
  sync.activate({room:'a'}); sources[0].emit('hello'); await sleep(150);
  sync.activate({room:'b'}); sync.activate({room:'a'}); sources[2].emit('hello'); await sleep(150);
  requests[0].wait.resolve('旧A'); await sleep(10);
  requests[1].wait.resolve('新A'); await sleep(10);
  assert.deepEqual(applied,['新A']); assert.equal(sources[0].closed,true); sync.stop();
});

test('忙时合并通知，流式结束后只做一次权威读取', async () => {
  let busy=true, reads=0; const source=events();
  const sync=create({sourceFactory:()=>source,busy:()=>busy,read:async()=>++reads,apply:()=>{}});
  sync.activate({room:'a'}); source.emit('hello'); source.emit('history-changed'); await sleep(150);
  assert.equal(reads,0); busy=false; sync.resume(); await sleep(150); assert.equal(reads,1); sync.stop();
});

test('在途通知只补一轮，重连hello再次读取，删除关闭监听', async () => {
  const source=events(), wait=deferred(); let reads=0, removed=0;
  const sync=create({sourceFactory:()=>source,busy:()=>false,read:async()=>++reads===1?wait.promise:reads,
    apply:()=>{},missing:()=>removed++});
  sync.activate({room:'a'}); source.emit('hello'); await sleep(150);
  source.emit('history-changed'); source.emit('history-changed'); wait.resolve('首轮'); await sleep(150);
  assert.equal(reads,2); source.emit('hello'); await sleep(150); assert.equal(reads,3);
  source.emit('room-deleted'); assert.equal(removed,1); assert.equal(source.closed,true); sync.stop();
});

test('跨页补齐覆盖原边界，不制造新旧历史缺口', async () => {
  const rows=Array.from({length:430},(_,i)=>({id:`m${String(i).padStart(4,'0')}`,created_at:i,content:String(i)}));
  let reads=0;
  const result=await readWindow(async(before,limit)=>{
    reads++; const end=before?rows.findIndex(row=>row.id===before):rows.length, start=Math.max(0,end-limit);
    return {room:{id:'a'},messages:rows.slice(start,end),has_more:start>0,next_before:start>0?rows[start].id:null};
  },{id:'m0050',created_at:50},new AbortController().signal);
  assert.equal(reads,2); assert.equal(result.messages.length,380); assert.equal(result.messages[0].id,'m0050');
  assert.equal(result.messages.at(-1).id,'m0429'); assert.equal(result.next_before,'m0050');
});

test('已删除边界用稳定消息键收口；游标循环明确失败', async () => {
  const signal=new AbortController().signal;
  const result=await readWindow(async()=>({room:{id:'a'},messages:[{id:'old',created_at:1},{id:'new',created_at:3}],has_more:false}),
    {id:'deleted',created_at:2},signal);
  assert.equal(result.messages.length,1); assert.equal(result.messages[0].id,'new'); assert.equal(result.has_more,true);
  await assert.rejects(readWindow(async()=>({room:{id:'a'},messages:[{id:'new',created_at:3}],has_more:true,next_before:'same'}),
    {id:'old',created_at:1},signal),/游标未推进/);
});
