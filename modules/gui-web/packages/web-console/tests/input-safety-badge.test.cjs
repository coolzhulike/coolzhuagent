// 执行真实徽记渲染函数；只替代 DOM，不伪造模型或安全放行操作。
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function render(safety) {
  const classes = new Set();
  const attributes = new Map();
  const badge = {hidden:false, textContent:'', dataset:{},
    classList:{toggle:(name,on) => on ? classes.add(name) : classes.delete(name)},
    setAttribute:(name,value) => attributes.set(name,value),
  };
  const source = fs.readFileSync(path.join(__dirname,'../src/app.js'),'utf8');
  const start = source.indexOf('function syncSystemSafetyBadge(');
  const end = source.indexOf('\nfunction describeAttributionAndRecovery(', start);
  const context = vm.createContext({document:{querySelector:() => badge},safety});
  vm.runInContext(source.slice(start,end)+'\nsyncSystemSafetyBadge(safety);',context);
  return {badge,classes,attributes};
}

test('人工放行后资源可用，历史复核记录不再显示为当前待办', () => {
  const {badge,classes} = render({resource_state:'safe',accepts_new_input:true,human_review_required:2,
    pending_recovery_operations:0,unacknowledged_open_blocks:0,unacknowledged_legacy_runs:0});
  assert.equal(badge.hidden,true);
  assert.equal(badge.textContent,'');
  assert.equal(classes.has('needs-attention'),false);
});

test('新的未放行事项必须醒目显示数量，不能被历史放行掩盖', () => {
  const {badge,classes,attributes} = render({accepts_new_input:false,human_review_required:2,
    unacknowledged_open_blocks:1,unacknowledged_legacy_runs:2,pending_recovery_operations:0});
  assert.equal(badge.hidden,false);
  assert.equal(badge.textContent,'待人工复核 3');
  assert.equal(classes.has('needs-attention'),true);
  assert.equal(attributes.get('role'),'alert');
});

test('机器对账与不可读状态不会冒充人工待审批或已开放', () => {
  assert.equal(render({accepts_new_input:false,pending_recovery_operations:1,human_review_required:2}).badge.textContent,'恢复处理中');
  assert.equal(render({unavailable:'读取失败'}).badge.textContent,'状态未知');
  assert.equal(render({accepts_new_input:false}).badge.textContent,'输入隔离');
});
