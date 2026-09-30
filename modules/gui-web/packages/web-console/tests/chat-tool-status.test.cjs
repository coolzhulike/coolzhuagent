// 旧的“完成工具永久保留30行”用例已废弃；执行真实过程栏和聊天模块，不替代模型验收。
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function ui() {
  class Element {
    constructor() { this.textContent=''; this.dataset={}; this.hidden=false; this.children=[]; }
    replaceChildren(...nodes) { this.children=nodes; }
    append(...nodes) { this.children.push(...nodes); }
    addEventListener() {}
    setAttribute() {}
  }
  const nodes=new Map();
  const node=role => { if (!nodes.has(role)) nodes.set(role,new Element()); return nodes.get(role); };
  const context=vm.createContext({window:{getSelection:() => null},document:{
    addEventListener() {}, createElement:() => new Element(),
    querySelector:selector => {
      const role=selector.match(/data-role="([^"]+)"/)?.[1];
      return role && role!=='chat-feedback' ? node(role) : null;
    },
  },activeChatRoomId:'room-a',activeWorkspaceKey:'default',activeSessionId:'agent-a',chatToolWindowId:'',
  sessionRegistry:{sessions:[]},chatMessageList:() => null,
  requestJson:async () => ({usage:[],timings:{},indices:{},note:''}),
  setInterval:() => 1,clearInterval() {},clearTimeout() {},Date,Map,Set});
  for (const file of ['run_activity.js','chat_experience.js']) {
    vm.runInContext(fs.readFileSync(path.join(__dirname,'../src',file),'utf8'),context);
  }
  const experience=context.window.CoolzhuChatExperience;
  return {experience,node,context,
    start:turn => experience.streamEvent('started',{turn_id:turn||'turn-a',chat_room_id:'room-a'}),
    status:data => experience.streamEvent('tool_status',{turn_id:'turn-a',chat_room_id:'room-a',...data}),
  };
}

test('完成工具立即退出动态过程栏，迟到开始不能重新点亮', () => {
  const {start,status,node}=ui(); start();
  status({call_id:'a',tool_name:'read_file',status:'running'});
  assert.match(node('chat-activity-stream').textContent,/read_file · 执行中/);
  status({call_id:'a',tool_name:'read_file',status:'completed'});
  assert.equal(node('chat-activity-stream').textContent,'');
  status({call_id:'a',tool_name:'read_file',status:'running'});
  assert.equal(node('chat-activity-stream').textContent,'');
});

test('思考与工具只在本轮运行中显示，回复开始清思考、结束清全部过程', () => {
  const {start,status,experience,node}=ui(); start();
  experience.streamEvent('message_delta',{id:'thought',kind:'reasoning',delta:'核对文件'});
  status({call_id:'a',tool_name:'read_file',status:'running'});
  assert.match(node('chat-activity-stream').textContent,/核对文件/);
  experience.streamEvent('message_delta',{id:'reply',kind:'assistant-reply',delta:'结果'});
  assert.ok(!node('chat-activity-stream').textContent.includes('核对文件'));
  experience.streamEvent('done',{turn_id:'turn-a',status:'completed'});
  assert.equal(node('chat-run-activity').hidden,true);
  assert.equal(node('chat-activity-stream').textContent,'');
});

test('源码参数不泄漏到过程栏、最近只显示四项活动工具', () => {
  const {start,status,node}=ui(); start();
  for (let i=0;i<7;i++) status({call_id:'call-'+i,tool_name:'tool-'+i,status:'running',
    summary:JSON.stringify({filePath:'artifact.html',content:'<html>PRIVATE-SOURCE</html>'})});
  const text=node('chat-activity-stream').textContent;
  assert.equal(text.split('\n').length,4);
  assert.ok(text.includes('artifact.html'));
  assert.ok(!text.includes('PRIVATE-SOURCE'));
  assert.ok(!text.includes('tool-0'));
});

test('旧轮、缺失作用域和换工程后的工具事件不能污染当前过程', () => {
  const {start,status,node,context,experience}=ui(); start('turn-b');
  status({call_id:'old',tool_name:'old',status:'running'});
  experience.streamEvent('tool_status',{call_id:'unscoped',tool_name:'old',status:'running'});
  assert.equal(node('chat-activity-stream').textContent,'');
  status({turn_id:'turn-b',call_id:'new',tool_name:'current',status:'running'});
  assert.match(node('chat-activity-stream').textContent,/current/);
  context.activeWorkspaceKey='other';
  status({turn_id:'turn-b',call_id:'foreign',tool_name:'FOREIGN',status:'running'});
  assert.ok(!node('chat-activity-stream').textContent.includes('FOREIGN'));
});
