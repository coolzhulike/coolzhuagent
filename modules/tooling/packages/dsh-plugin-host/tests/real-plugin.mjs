// 工程生命周期核验：必须输入真实固定来源插件目录，不构造 SDK 或模型夹具。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, readdir, realpath } from 'node:fs/promises';
import path from 'node:path';
import { loadPluginHost } from '../src/host.mjs';

const root = await realpath(process.argv[2]);
const output = process.argv[3];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const metadata = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
assert.equal(metadata.name, '@deepseek-ai/dsh-tool-calculator');
assert.equal(metadata.version, '0.0.1');
const files = [];
async function sourceFiles(directory, prefix = '') {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    assert.equal(entry.isSymbolicLink(), false);
    if (entry.name === 'node_modules' || entry.name === '.git') continue;
    const relative = prefix + entry.name;
    if (entry.isDirectory()) await sourceFiles(path.join(directory, entry.name), relative + '/');
    else if (entry.isFile()) files.push({ path: relative, sha256: hash(await readFile(path.join(directory, entry.name))) });
  }
}
await sourceFiles(root);
const receipt = { protocol: 1, name: metadata.name, version: metadata.version, entry: metadata.main,
  sdk_lock_sha256: hash(await readFile(new URL('../package-lock.json', import.meta.url))), files };
const report = { started_at: new Date().toISOString(), node: process.version,
  notice: '真实远程计算器与生产宿主接口的工程核验；不是正式市场安装或Qwen会话验收。',
  source: receipt, checks: [] };
var host;
try {
  host = await loadPluginHost({ root, receipt });
  report.manifest = host.manifest;
  assert.deepEqual(host.manifest.tools.map(t => t.name), ['calculator']);
  assert.deepEqual(host.manifest.tools[0].input_schema.required, ['expression']);
  const base = { generation: host.manifest.generation, revision: host.manifest.revision,
    name: 'calculator', arguments: { expression: '15 + 27 * sqrt(9)' }, signal: new AbortController().signal };
  const value = await host.execute({ ...base, call_id: 'real-host-normal' });
  assert.equal(value.isError, false);
  assert.equal(value.value, 96);
  report.checks.push({ name: '真实工具定义及执行', result: value });
  await assert.rejects(host.execute({ ...base, call_id: 'real-host-normal' }), { code: 'CALL_ALREADY_USED' });
  report.checks.push({ name: '已使用调用身份不可重放', passed: true });
  await assert.rejects(host.execute({ ...base, call_id: 'real-host-old-generation', generation: 'old' }), { code: 'HOST_STALE' });
  await assert.rejects(host.execute({ ...base, call_id: 'real-host-old-revision', revision: 'old' }), { code: 'HOST_STALE' });
  report.checks.push({ name: '旧宿主世代与旧来源修订拒绝', passed: true });
  const canceled = new AbortController();
  canceled.abort();
  const aborted = await host.execute({ ...base, call_id: 'real-host-pre-canceled', signal: canceled.signal });
  assert.equal(aborted.isError, true);
  assert.equal(aborted.error.info.code, 'ABORTED_BEFORE_DISPATCH');
  report.checks.push({ name: '官方AbortSignal实际预取消', result: aborted });
  const invalid = await host.execute({ ...base, call_id: 'real-host-invalid-input', arguments: { expression: 'process.exit()' } });
  assert.equal(invalid.isError, true);
  report.checks.push({ name: '真实数学解析器拒绝无效表达式', result: invalid });
  const inFlight = host.execute({ ...base, call_id: 'real-host-concurrent-first' });
  await assert.rejects(host.execute({ ...base, call_id: 'real-host-concurrent-second' }), { code: 'HOST_BUSY' });
  assert.equal((await inFlight).value, 96);
  report.checks.push({ name: '来源校验期间保持单次调用资格', passed: true });
  const checking = host.execute({ ...base, call_id: 'real-host-disposed-during-check' });
  const rejectedAfterDispose = assert.rejects(checking, { code: 'HOST_DISPOSED' });
  await host.dispose();
  await rejectedAfterDispose;
  report.checks.push({ name: '来源校验期间停用撤销执行资格并排空', passed: true });
  await assert.rejects(host.execute({ ...base, call_id: 'real-host-after-disable' }), { code: 'HOST_DISPOSED' });
  await host.dispose();
  report.checks.push({ name: '真实子fiber与服务释放、停用后拒绝、重复释放保持结果', passed: true });
  report.passed = true;
} catch (error) {
  report.passed = false;
  report.error = { code: error.code, message: error.message, cleanup_code: error.cleanupCode };
  process.exitCode = 1;
} finally {
  if (host) {
    try { await host.dispose(); } catch (error) {
      report.passed = false;
      report.cleanup_error = { code: error.code, message: error.message };
      process.exitCode = 1;
    }
  }
  report.finished_at = new Date().toISOString();
  await writeFile(output, JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks.length, file: output, error: report.error }));
}
