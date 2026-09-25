// 不向服务发送请求；核验已记录的真实工具结果而非仅信任模拟模型最终文案。
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const root=path.resolve(process.env.RELEASE_ACCEPTANCE_DIR||__dirname);
const events=JSON.parse(fs.readFileSync(path.join(root,'read-file-events.json'),'utf8'));
const result=events.find(e=>e.data.kind==='tool-result')?.data.content||'';
assert(result.includes('route: runtime-executed'));
assert(result.includes('status: completed'));
assert(result.includes('READ_FILE_RELEASE_OK_20260919'));
assert(!fs.existsSync(path.join(root,'must-not-be-created.html')));
console.log(JSON.stringify({pass:true,actual_runtime_file_read:true,forbidden_file_absent:true}));
