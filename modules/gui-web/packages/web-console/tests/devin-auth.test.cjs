const {test} = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const settle = async () => { for (let i=0;i<8;i++) await new Promise(resolve => setImmediate(resolve)); };
class Element {
  constructor() { this.listeners=new Map(); this.textContent=""; this.hidden=false; this.disabled=false; }
  addEventListener(type, fn) { this.listeners.set(type,fn); }
  removeEventListener(type) { this.listeners.delete(type); }
}
function harness() {
  const calls=[], timers=new Map(), listeners=new Set(); let serial=0;
  let data={cli_available:true,authentication:"unauthenticated",message:"Devin 尚未登录",login:null};
  let deferred=null;
  const window={
    addEventListener(type,fn) { listeners.add(fn); }, removeEventListener(type,fn) { listeners.delete(fn); },
    dispatchEvent(event) { for(const fn of [...listeners]) fn(event); },
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,"../src/devin_auth.js"),"utf8"), {
    window,document:{readyState:"complete",querySelector:()=>null},AbortController,
    CustomEvent:class {constructor(type,options){this.type=type;this.detail=options.detail;}},
    setTimeout(fn) { const id=++serial;timers.set(id,fn);return id; }, clearTimeout(id) {timers.delete(id);},
    async fetch(url,options={}) {
      calls.push({url,body:options.body && JSON.parse(options.body)});
      if(deferred){const current=deferred;deferred=null;await current.promise;return {ok:true,json:async()=>current.data};}
      if(url.endsWith("/login")) data={...data,authentication:"unknown",login:{attempt_id:"attempt-1",phase:"waiting",active:true,message:"请在官方网页授权"}};
      if(url.endsWith("/cancel")) data={...data,login:{...data.login,phase:"cancelling",message:"正在停止"}};
      const response=JSON.parse(JSON.stringify(data));
      return {ok:true,json:async()=>response};
    },
  });
  function mount(options={}) {
    const nodes=new Map(),container=new Element();
    Object.defineProperty(container,"innerHTML",{set(html){for(const match of html.matchAll(/data-devin-auth="([^"]+)"/g)) nodes.set(match[1],new Element());}});
    container.querySelector=selector=>nodes.get(selector.match(/data-devin-auth="([^"]+)"/)[1]);
    const api=window.CoolzhuDevinAuth.mount(container,options);
    return {api,nodes,container,click:action=>nodes.get(action).listeners.get("click")()};
  }
  return {calls,timers,mount,set:dataValue=>{data=dataValue;},
    defer(dataValue){let resolve;const promise=new Promise(r=>{resolve=r;});deferred={promise,data:dataValue};return resolve;},
    poll(){const pending=[...timers.values()];timers.clear();pending.forEach(fn=>fn());},
  };
}
test("界面点击登录仅提交空请求，核对完成后自动展示已登录",async()=>{
  const h=harness(),panel=h.mount();await settle();
  assert.equal(panel.nodes.get("login").disabled,false);
  await panel.click("login");await settle();
  assert.deepEqual(h.calls.find(c=>c.url.endsWith("/login")).body,{});
  assert.equal(panel.nodes.get("cancel").hidden,false);
  assert.equal(panel.nodes.get("login").disabled,true);
  h.set({cli_available:true,authentication:"authenticated",message:"Devin 已登录",login:{attempt_id:"attempt-1",phase:"completed",active:false,message:"可以获取账号模型"}});
  h.poll();await settle();
  assert.match(panel.nodes.get("status").textContent,/已登录/);
  assert.equal(panel.nodes.get("cancel").hidden,true);
  assert.equal(h.timers.size,0);panel.api.destroy();
});
test("取消携带当前流程身份，关闭或切换页面不会自动取消授权",async()=>{
  const h=harness(),panel=h.mount();await settle();await panel.click("login");
  await panel.click("cancel");
  assert.deepEqual(h.calls.find(c=>c.url.endsWith("/cancel")).body,{attempt_id:"attempt-1"});
  assert.equal(panel.nodes.get("cancel").disabled,true);
  const count=h.calls.length;panel.api.setVisible(false);panel.api.destroy();
  assert.equal(h.calls.length,count);assert.equal(h.timers.size,0);
});
test("迟到的旧状态不会覆盖新查询或已销毁页面",async()=>{
  const h=harness(),panel=h.mount();await settle();
  const release=h.defer({cli_available:true,authentication:"authenticated",message:"旧的已登录",login:null});
  const old=panel.api.refresh();await settle();
  await panel.api.refresh();release();await old;
  assert.match(panel.nodes.get("status").textContent,/尚未登录/);
  const release2=h.defer({cli_available:true,authentication:"authenticated",message:"迟到结果",login:null});
  const late=panel.api.refresh();await settle();panel.api.destroy();release2();await late;
  assert.doesNotMatch(panel.nodes.get("status").textContent,/迟到结果/);
});
test("插件页和 Agent 设置共享登录变化；隐藏的 HTTP 设置不查询 Devin",async()=>{
  const h=harness(),plugin=h.mount(),agent=h.mount({visible:false});await settle();
  const count=h.calls.length;assert.equal(agent.container.hidden,true);
  await settle();assert.equal(h.calls.length,count);
  agent.api.setVisible(true);await settle();await agent.click("login");await settle();
  assert.equal(plugin.nodes.get("cancel").hidden,false);
  assert.equal(plugin.nodes.get("login").disabled,true);
  plugin.api.destroy();agent.api.destroy();
});
