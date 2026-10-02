// 一次性 DSH 工具宿主协议；stdout 不兼任 IPC，模型循环与权限仍归 Rust。
import { readFile, writeFile, rename, realpath, lstat } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { loadPluginHost } from './host.mjs';

const MAX_BYTES = 256 * 1024;
const directory = await realpath(process.argv[2]);
const cancel = new AbortController();
var host;
var timer;
var watcherDone = false;
var watcher;
var request;
var terminal;

function protocolError(code, message) {
  return Object.assign(new Error(message), { code });
}

async function readMessage(name, optional = false) {
  const file = path.join(directory, name);
  try {
    const stat = await lstat(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > MAX_BYTES) {
      throw protocolError('PROTOCOL_INVALID', '协议文件类型或大小无效');
    }
    const text = await readFile(file);
    if (text.length > MAX_BYTES) throw protocolError('PROTOCOL_INVALID', '协议读取超出限制');
    return JSON.parse(text.toString('utf8'));
  } catch (error) {
    if (optional && error.code === 'ENOENT') return undefined;
    throw error;
  }
}

async function publish(name, value) {
  const bytes = Buffer.from(JSON.stringify(value));
  if (bytes.length > MAX_BYTES) throw protocolError('RESULT_TOO_LARGE', '协议结果超出限制');
  const temporary = path.join(directory, `${name}.${randomUUID()}.tmp`);
  await writeFile(temporary, bytes, { flag: 'wx', mode: 0o600 });
  await rename(temporary, path.join(directory, name));
}

function contextMatches(value) {
  return value?.protocol === 1 && value.nonce === request.nonce
    && ['workspace_id', 'room_id', 'run_id', 'call_id'].every(key => value.context?.[key] === request.context[key]);
}

function envelope(value) {
  return { protocol: 1, nonce: request?.nonce, context: request?.context, ...value };
}

try {
  request = await readMessage('request.json');
  if (request?.protocol !== 1 || !['describe', 'execute'].includes(request.mode)
      || typeof request.nonce !== 'string' || request.nonce.length < 16 || request.nonce.length > 128
      || !request.context || !['workspace_id', 'room_id', 'run_id', 'call_id'].every(key =>
        typeof request.context[key] === 'string' && request.context[key].length > 0 && request.context[key].length <= 192)
      || !Number.isSafeInteger(request.deadline_ms) || request.deadline_ms <= Date.now()
      || request.deadline_ms > Date.now() + 180000) {
    throw protocolError('PROTOCOL_INVALID', '宿主请求身份、模式或截止无效');
  }
  timer = setTimeout(() => cancel.abort(), Math.max(1, request.deadline_ms - Date.now()));
  watcher = (async () => {
    while (!watcherDone) {
      const value = await readMessage('cancel.json', true);
      if (value) {
        if (!contextMatches(value)) throw protocolError('PROTOCOL_STALE', '取消请求的父运行身份不符');
        cancel.abort();
        return;
      }
      await delay(20);
    }
  })().catch(error => { cancel.abort(); throw error; });
  // 立即注册处理，避免错误发生在插件导入期间成为未处理拒绝。
  watcher.catch(() => {});
  host = await loadPluginHost({ root: request.root, receipt: request.receipt, config: request.config });
  if (cancel.signal.aborted) throw protocolError('CANCELLED', '宿主载入期间已取消或截止');
  await publish('manifest.json', envelope({ manifest: host.manifest }));
  if (request.mode === 'describe') {
    terminal = envelope({ status: 'described', manifest: host.manifest });
  } else {
    var command;
    while (!(command = await readMessage('execute.json', true))) {
      if (cancel.signal.aborted) throw protocolError('CANCELLED', '未投递工具前已取消或截止');
      await delay(20);
    }
    if (!contextMatches(command)) throw protocolError('PROTOCOL_STALE', '执行请求的父运行身份不符');
    const result = await host.execute({ call_id: request.context.call_id,
      generation: command.generation, revision: command.revision, name: command.name,
      arguments: command.arguments, signal: cancel.signal });
    if (cancel.signal.aborted) throw protocolError('CANCELLED', '父调用已结束，迟到结果不得复活调用');
    terminal = envelope({ status: 'executed', result });
  }
} catch (error) {
  terminal = envelope({ status: 'failed', error: { code: error.code ?? 'HOST_FAILED',
    message: String(error.message).slice(0, 640) } });
  process.exitCode = 1;
} finally {
  watcherDone = true;
  clearTimeout(timer);
  var cleanupError;
  try {
    if (watcher) await watcher;
  } catch (error) { cleanupError = error; }
  // 协议观察失败不能跳过插件释放；两项都尝试，任一失败都不发布成功。
  try {
    if (host) await host.dispose();
  } catch (error) { cleanupError ??= error; }
  if (cleanupError) {
    terminal = envelope({ status: 'cleanup_failed', error: { code: cleanupError.code ?? 'DISPOSE_INCOMPLETE',
      message: String(cleanupError.message).slice(0, 640) } });
    process.exitCode = 1;
  }
  // 插件资源释放结束后才发布结果；不以函数返回冒充生命周期已收尾。
  try { await publish('result.json', terminal); }
  catch (error) {
    process.exitCode = 1;
    await publish('result.json', envelope({ status: 'failed', error: {
      code: error.code ?? 'PROTOCOL_PUBLISH_FAILED', message: '宿主无法在协议大小限制内发布结果' } }));
  }
}
