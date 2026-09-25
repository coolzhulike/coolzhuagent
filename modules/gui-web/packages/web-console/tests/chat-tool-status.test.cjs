// 执行真实前端模块的公共事件入口；最小 DOM 只负责观察可见状态，不复制业务逻辑。
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function fixture() {
  class Node {
    constructor() { this.textContent = ''; this.dataset = {}; this.hidden = false; this.children = []; }
    replaceChildren(...children) { this.children = children; }
    append(...children) { this.children.push(...children); }
    addEventListener() {}
  }
  const nodes = new Map();
  const node = role => { if (!nodes.has(role)) nodes.set(role, new Node()); return nodes.get(role); };
  const context = vm.createContext({ window: {}, document: {
    querySelector: selector => node(selector.match(/data-role="([^"]+)"/)?.[1]),
    createElement: () => new Node(),
  }, activeChatRoomId: 'room-a', activeWorkspaceKey: 'default', activeSessionId: 'agent-a', chatToolWindowId: '',
  sessionRegistry: {sessions:[]}, chatMessageList: () => null, requestJson: async () => ({usage:[],timings:{},indices:{},note:''}),
  setInterval: () => 1, clearInterval: () => {}, clearTimeout: () => {}, Date, Map, Set });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/chat_experience.js'), 'utf8'), context);
  return { ui: context.window.CoolzhuChatExperience, node, context };
}

function start(ui, turn = 'turn-a', room = 'room-a') { ui.streamEvent('started', {turn_id:turn,chat_room_id:room}); }
function statusEvent(ui, data) { ui.streamEvent('tool_status', {turn_id:'turn-a',chat_room_id:'room-a',...data}); }

test('同名工具的两次调用按稳定 ID 各聚合一条，旧摘要和结果不重复计数', () => {
  const { ui, node } = fixture(); start(ui);
  for (const id of ['tool-call-a', 'tool-call-b']) {
    for (const status of ['requested', 'running', 'completed']) {
      statusEvent(ui, { call_id: id, tool_name: 'read_file', status, summary: '已读取文件' });
      const row = node('chat-tool-result-list').children.find(row => row.dataset.toolStatusId === id);
      assert.equal(row.dataset.toolStatus, status);
    }
    ui.intercept({ id, kind: 'tool-summary', content: '工具 `read_file` 已完成：RAW-AUDIT-SECRET' });
    ui.intercept({ id: id.replace('tool-call-', 'tool-result-'), kind: 'tool-result', content: `tool_call_id: ${id}\ntool_name: read_file\nstatus: completed\nRAW-AUDIT-SECRET` });
  }
  assert.equal(node('chat-tool-count').textContent, '工具调用 · 2 次');
  const rows = node('chat-tool-result-list').children;
  assert.equal(rows.length, 2);
  assert.ok(rows.every(row => row.dataset.toolStatus === 'completed' && !row.textContent.includes('RAW-AUDIT-SECRET')));
});

test('工具状态不会进入思考，正文开始与结束时移除临时思考', () => {
  const { ui, node } = fixture(); start(ui);
  ui.streamEvent('message_delta', { id: 'thought', kind: 'reasoning', delta: '正在核对文件' });
  statusEvent(ui, { call_id: 'tool-call-a', tool_name: 'write_file', status: 'requested', summary: '等待执行' });
  assert.equal(node('chat-live-reasoning-text').textContent, '正在核对文件');
  assert.equal(node('chat-live-reasoning').hidden, false);
  ui.streamEvent('message_delta', { id: 'reply', kind: 'assistant-reply', delta: '文件已完成' });
  assert.equal(node('chat-live-reasoning').hidden, true);
  assert.equal(node('chat-live-reasoning-text').textContent, '');
});

test('中止和缺失终态不会伪装为执行成功', () => {
  for (const [phase, end, expected] of [
    ['running', 'interrupted', 'interrupted'], ['requested', 'completed', 'not-executed'],
    ['running', 'completed', 'unknown'], ['running', 'failed', 'failed'],
  ]) {
    const { ui, node } = fixture(); start(ui);
    statusEvent(ui, { call_id: 'tool-call-a', tool_name: 'write_file', status: phase });
    ui.streamEvent('done', { status: end });
    assert.equal(node('chat-tool-result-list').children[0].dataset.toolStatus, expected);
  }
});

test('已保存的旧摘要与结果按同一调用回放，预览仍显示未执行', () => {
  const { ui, node } = fixture();
  ui.intercept({ id: 'tool-call-old', kind: 'tool-summary', content: '工具 `write_file` 已完成：旧的权限预览' });
  ui.intercept({ id: 'tool-result-old', kind: 'tool_result', content: 'tool_call_id: tool-call-old\ntool_name: write_file\nstatus: dry-run-only\n等待批准' });
  assert.equal(node('chat-tool-result-list').children.length, 1);
  assert.equal(node('chat-tool-result-list').children[0].dataset.toolStatus, 'not-executed');
  ui.intercept({ id: 'tool-result-ok', kind: 'tool-result', content: 'tool_call_id: tool-call-ok\ntool_name: read_file\nstatus: ok' });
  assert.equal(node('chat-tool-result-list').children[1].dataset.toolStatus, 'completed');
});

test('工具终态和旧审计中的源码字段不会渲染到状态栏', () => {
  const { ui, node } = fixture(); start(ui);
  for (const id of ['tool-call-new', 'tool-call-legacy']) {
    const summary = JSON.stringify({filePath:'artifact.html',content:'<html>RAW-ARGUMENT-MARKER</html>',oldString:'PRIVATE-OLD',newString:'PRIVATE-NEW',originalFile:'PRIVATE-FILE'});
    if (id.endsWith('new')) statusEvent(ui, {call_id:id,tool_name:'write_file',status:'completed',summary});
    else ui.intercept({id,kind:'tool-summary',content:`工具 \`write_file\` 已完成：${summary}`});
  }
  for (const row of node('chat-tool-result-list').children) {
    assert.ok(row.textContent.includes('artifact.html'));
    assert.ok(!row.textContent.includes('RAW-ARGUMENT-MARKER') && !row.textContent.includes('PRIVATE-'));
  }
});

test('超过可见窗口的调用仍计入本轮总数，重复状态不增加次数', () => {
  const { ui, node } = fixture(); start(ui);
  for (let index = 0; index < 31; index++) statusEvent(ui, {call_id:`call-${index}`,tool_name:'read_file',status:'completed'});
  statusEvent(ui, {call_id:'call-0',tool_name:'read_file',status:'completed'});
  assert.equal(node('chat-tool-count').textContent, '工具调用 · 31 次（显示最近 30 次）');
  assert.equal(node('chat-tool-result-list').children.length, 30);
});

test('结构化终态不受迟到开始事件或矛盾终态覆盖', () => {
  for (const terminal of ['completed','failed','interrupted','not-executed']) {
    const {ui,node} = fixture(); start(ui);
    statusEvent(ui,{call_id:'call-a',tool_name:'write_file',status:terminal,summary:'确定的终态'});
    for (const late of ['requested','running','completed','failed']) statusEvent(ui,{call_id:'call-a',tool_name:'write_file',status:late,summary:'迟到事件'});
    assert.equal(node('chat-tool-result-list').children[0].dataset.toolStatus,terminal);
    assert.ok(!node('chat-tool-result-list').children[0].textContent.includes('迟到事件'));
  }
});

test('旧轮和缺失作用域的状态不能污染新轮，晚到旧done不关闭新轮', () => {
  const {ui,node} = fixture(); start(ui);
  statusEvent(ui,{call_id:'old-call',tool_name:'read_file',status:'completed'});
  start(ui,'turn-b');
  statusEvent(ui,{call_id:'old-call',tool_name:'read_file',status:'running'});
  ui.streamEvent('tool_status',{call_id:'unscoped',tool_name:'read_file',status:'completed'});
  ui.streamEvent('done',{turn_id:'turn-a',status:'completed'});
  statusEvent(ui,{turn_id:'turn-b',call_id:'new-call',tool_name:'read_file',status:'running'});
  assert.equal(node('chat-tool-count').textContent,'工具调用 · 1 次');
  assert.equal(node('chat-tool-result-list').children[0].dataset.toolStatusId,'new-call');
  assert.equal(node('chat-tool-result-list').children[0].dataset.toolStatus,'running');
  ui.streamEvent('done',{turn_id:'turn-b',status:'completed'});
  statusEvent(ui,{turn_id:'turn-b',call_id:'after-done',tool_name:'read_file',status:'completed'});
  assert.equal(node('chat-tool-result-list').children.length,1);
});

test('切房和切工程后拒绝旧工具事件，当前房间新轮仍可接收', () => {
  const {ui,node,context} = fixture(); start(ui);
  context.activeChatRoomId = 'room-b'; ui.roomChanged('room-b');
  statusEvent(ui,{call_id:'old-room-call',tool_name:'read_file',status:'completed'});
  assert.equal(node('chat-tool-result-list').children.length,0);
  start(ui,'turn-b','room-b');
  statusEvent(ui,{turn_id:'turn-b',chat_room_id:'room-b',call_id:'current',tool_name:'read_file',status:'running'});
  assert.equal(node('chat-tool-result-list').children.length,1);
  context.activeWorkspaceKey = 'other-workspace';
  statusEvent(ui,{turn_id:'turn-b',chat_room_id:'room-b',call_id:'old-workspace',tool_name:'read_file',status:'completed'});
  assert.equal(node('chat-tool-result-list').children.length,1);
});
