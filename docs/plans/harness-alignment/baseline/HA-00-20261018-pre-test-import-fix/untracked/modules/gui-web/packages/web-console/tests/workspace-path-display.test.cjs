const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

// 覆盖 Windows 路径往返显示；不改变服务器用于身份与权限判断的原始路径。
const source = fs.readFileSync(path.join(__dirname, '../src/app.js'), 'utf8');
const begin = source.indexOf('function workspaceDisplayPath(');
const end = source.indexOf('\nasync function copyOverviewWorkspacePath', begin);
assert.ok(begin >= 0 && end > begin);
const context = {};
vm.runInNewContext(source.slice(begin, end), context);
test('工程路径去掉本地盘与 UNC 扩展前缀，保留中文和普通路径', () => {
  for (const [raw, display] of [
    [String.raw`\\?\D:\中文 工程`, String.raw`D:\中文 工程`],
    [String.raw`\\?\UNC\server\share\工程`, String.raw`\\server\share\工程`],
    [String.raw`C:\Users\某人\coolzhuagent`, String.raw`C:\Users\某人\coolzhuagent`],
    ['/home/user/coolzhuagent', '/home/user/coolzhuagent'],
    [String.raw`\\.\PhysicalDrive0`, String.raw`\\.\PhysicalDrive0`],
    ['', ''],
  ]) assert.equal(context.workspaceDisplayPath(raw), display);
});
