// 单一 DSH 工具插件宿主；使用官方服务，不接管 Coolzhu 的会话、记忆或模型循环。
import { Context } from '@deepseek-ai/cordis';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime from '@deepseek-ai/dsh-tools';
import { createHash, randomUUID } from 'node:crypto';
import { readFile, realpath, lstat } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { bindSourceImports } from './source_imports.mjs';

const MAX_FILE_BYTES = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES = 32 * 1024 * 1024;
const MAX_JSON_BYTES = 256 * 1024;
const SUPPORTED_PLUGIN_SERVICES = new Set(['tools']);

function fail(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

function jsonSnapshot(value, limit = MAX_JSON_BYTES) {
  const text = JSON.stringify(value);
  if (typeof text !== 'string' || Buffer.byteLength(text) > limit) {
    fail('RESULT_TOO_LARGE', '插件数据无法在大小限制内序列化');
  }
  return JSON.parse(text);
}

function digest(bytes) { return createHash('sha256').update(bytes).digest('hex'); }

async function bounded(promise, timeoutMs, code) {
  var timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => { const error = new Error('插件资源未在期限内收尾'); error.code = code; reject(error); }, timeoutMs);
    })]);
  } finally { clearTimeout(timer); }
}

async function checkedFile(root, relative, maxBytes) {
  if (typeof relative !== 'string' || !relative || relative.includes('\\')
      || relative.split('/').some(p => !p || p === '.' || p === '..')
      || path.isAbsolute(relative) || relative.includes(':')) {
    fail('INVALID_SOURCE_PATH', '插件文件路径无效');
  }
  // 每一级均拒绝链接，避免父目录的重解析点绕过归属检查。
  var current = root;
  for (const part of relative.split('/')) {
    current = path.join(current, part);
    if ((await lstat(current)).isSymbolicLink()) fail('SOURCE_LINK', '插件文件不允许符号链接');
  }
  const canonical = await realpath(current);
  const inRoot = path.relative(root, canonical);
  if (!inRoot || inRoot.startsWith('..') || path.isAbsolute(inRoot)) {
    fail('SOURCE_OUTSIDE_ROOT', '插件文件超出安装目录');
  }
  const stat = await lstat(canonical);
  if (!stat.isFile() || stat.size > maxBytes) fail('SOURCE_TOO_LARGE', '插件文件无效或超出大小限制');
  const bytes = await readFile(canonical);
  if (bytes.length > maxBytes) fail('SOURCE_TOO_LARGE', '插件文件读取超出大小限制');
  return { canonical, bytes };
}

async function verifyReceipt(root, receipt) {
  if (receipt?.protocol !== 1 || !Array.isArray(receipt.files)
      || receipt.files.length < 2 || receipt.files.length > 1024
      || typeof receipt.entry !== 'string') fail('INVALID_RECEIPT', '插件安装回执不完整');
  const files = [];
  const seen = new Set();
  var total = 0;
  var entry;
  var metadata;
  for (const item of receipt.files) {
    const key = typeof item?.path === 'string' && process.platform === 'win32' ? item.path.toLowerCase() : item?.path;
    if (!item || seen.has(key) || !/^[0-9a-f]{64}$/.test(item.sha256 ?? '')) {
      fail('INVALID_RECEIPT', '插件文件身份重复或无效');
    }
    seen.add(key);
    const file = await checkedFile(root, item.path, MAX_FILE_BYTES);
    total += file.bytes.length;
    if (total > MAX_SOURCE_BYTES) fail('SOURCE_TOO_LARGE', '插件来源总大小超出限制');
    if (digest(file.bytes) !== item.sha256) fail('SOURCE_CHANGED', '插件文件已变化，需要重新检查安装身份');
    files.push({ path: item.path, sha256: item.sha256 });
    if (item.path === receipt.entry) entry = file.canonical;
    if (item.path === 'package.json') metadata = JSON.parse(file.bytes.toString('utf8'));
  }
  if (!entry || !metadata || metadata.name !== receipt.name || metadata.version !== receipt.version) {
    fail('SOURCE_IDENTITY_MISMATCH', '插件入口、包名或版本与安装回执不符');
  }
  const sdkLock = await readFile(new URL('../package-lock.json', import.meta.url));
  const sdkLockDigest = digest(sdkLock);
  if (receipt.sdk_lock_sha256 !== sdkLockDigest) fail('SDK_CHANGED', '插件依赖的宿主 SDK 身份不符');
  files.sort((a, b) => a.path.localeCompare(b.path, 'en'));
  const identity = { protocol: 1, name: metadata.name, version: metadata.version,
    entry: receipt.entry, sdk_lock_sha256: sdkLockDigest, files };
  return { entry, identity, revision: digest(JSON.stringify(identity)) };
}

function requiredServices(module) {
  const inject = module.inject ?? [];
  if (Array.isArray(inject) && inject.every(x => typeof x === 'string')) return inject;
  if (inject && typeof inject === 'object' && !Array.isArray(inject)
      && Object.keys(inject).every(x => x === 'required' || x === 'optional')
      && [inject.required ?? [], inject.optional ?? []].every(a => Array.isArray(a) && a.every(x => typeof x === 'string'))) {
    // 第一阶段也不为未知可选服务提供空实现。
    return [...(inject.required ?? []), ...(inject.optional ?? [])];
  }
  fail('INJECT_UNSUPPORTED', '插件的服务依赖格式暂不支持');
}

function toolManifest(schemas) {
  if (!Array.isArray(schemas) || !schemas.length || schemas.length > 24) {
    fail('TOOLS_UNSUPPORTED', '插件没有工具或工具数量超出当前限制');
  }
  const seen = new Set();
  return schemas.map(schema => {
    if (!schema || typeof schema.name !== 'string' || !/^[A-Za-z0-9_-]{1,57}$/.test(schema.name)
        || seen.has(schema.name) || schema.parameters?.type !== 'object') {
      fail('TOOLS_UNSUPPORTED', '实际注册的工具名称重复、无效或参数不是对象');
    }
    seen.add(schema.name);
    return jsonSnapshot({ name: schema.name, description: schema.description ?? '',
      input_schema: schema.parameters }, 64 * 1024);
  });
}

export async function loadPluginHost({ root, receipt, config = {} }) {
  const [major, minor] = process.versions.node.split('.').map(Number);
  if (!(major >= 24 || (major === 22 && minor >= 19))) fail('NODE_UNSUPPORTED', '宿主需要 Node 22.19 或24及以上版本');
  if (!path.isAbsolute(root)) fail('INVALID_ROOT', '必须指定安装目录的绝对路径');
  receipt = jsonSnapshot(receipt);
  const canonicalRoot = await realpath(root);
  const verified = await verifyReceipt(canonicalRoot, receipt);
  const imports = bindSourceImports(canonicalRoot, receipt);
  try {
    const imported = await import(pathToFileURL(verified.entry).href);
    const module = imported.default ?? imported;
    if (!module || typeof module !== 'object' || !Object.hasOwn(module, 'apply')
        || typeof module.apply !== 'function' || module.name !== receipt.name) {
      fail('MODULE_UNSUPPORTED', '包入口不是当前支持的单一 DSH 工具模块');
    }
    const required = requiredServices(module);
    const unknown = required.filter(name => !SUPPORTED_PLUGIN_SERVICES.has(name));
    if (unknown.length) fail('SERVICE_UNSUPPORTED', `宿主尚未提供服务：${unknown.join('、')}`);
    const ctx = new Context();
    const fibers = [];
    var disposed = false;
    var active = false;
    var activePromise;
    var disposePromise;
    const lifetime = new AbortController();
    const consumedCalls = new Set();
    const generation = randomUUID();
    async function dispose() {
      disposePromise ??= (async () => {
        disposed = true;
        lifetime.abort();
        try {
          if (activePromise) await bounded(activePromise.catch(() => {}), 1000, 'CALL_DRAIN_INCOMPLETE');
          // 官方根 fiber.dispose 是 restart；逐一释放真实子 fiber 并核对服务撤销。
          for (const fiber of fibers.toReversed()) await bounded(fiber.dispose(), 1000, 'DISPOSE_INCOMPLETE');
          if (fibers.some(f => f.state !== 4) || ctx.registry.size !== 0 || ctx.tools || ctx.systemPrompt) {
            fail('DISPOSE_INCOMPLETE', '插件或官方服务尚未完整释放');
          }
        } finally { imports.deregister(); }
      })();
      return await disposePromise;
    }
    try {
      fibers.push(await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false }));
      fibers.push(await ctx.plugin(ToolRuntime, { mode: 'native' }));
      fibers.push(await ctx.plugin(module, jsonSnapshot(config, 64 * 1024)));
      if (fibers.some(f => f.state !== 2)) fail('LOAD_FAILED', 'DSH 模块未进入可运行状态');
      const tools = toolManifest(ctx.tools.schemas());
      const manifest = jsonSnapshot({ protocol: 1, generation, revision: verified.revision,
        plugin: { name: receipt.name, version: receipt.version }, services: ['tools'], tools });
      return {
        manifest,
        async execute({ call_id, generation: expectedGeneration, revision, name, arguments: args, signal }) {
          if (disposed) fail('HOST_DISPOSED', '插件已停用，调用被拒绝');
          if (active) fail('HOST_BUSY', '单次插件宿主已有执行中的调用');
          if (expectedGeneration !== generation || revision !== verified.revision) fail('HOST_STALE', '插件版本或宿主世代已变化');
          if (typeof call_id !== 'string' || !call_id || call_id.length > 192 || consumedCalls.has(call_id)) {
            fail('CALL_ALREADY_USED', '调用身份无效或已使用，禁止自动重放');
          }
          if (!(signal instanceof AbortSignal)) fail('SIGNAL_REQUIRED', '插件调用必须绑定宿主取消信号');
          if (!tools.some(tool => tool.name === name)) fail('TOOL_UNAVAILABLE', '本次插件未实际注册此工具');
          const input = jsonSnapshot(args, 64 * 1024);
          // 首个 await 前占用调用资格；来源校验期间也不能插入另一调用或绕过停用收尾。
          active = true;
          activePromise = (async () => {
            // 注册清单和执行绑定同一个实际来源修订，不只比较可重复使用的语义版本号。
            if ((await verifyReceipt(canonicalRoot, receipt)).revision !== verified.revision) {
              fail('SOURCE_CHANGED', '插件身份变化，未执行');
            }
            if (disposed) fail('HOST_DISPOSED', '来源校验期间插件已停用，未执行');
            consumedCalls.add(call_id);
            const result = await ctx.tools.execute({ callId: call_id, name, arguments: input,
              signal: AbortSignal.any([signal, lifetime.signal]) });
            // 不传播 concludeTurn 或 additionalContexts 为控制指令；权限与上下文仍由 Rust 所有。
            return jsonSnapshot({ isError: result.isError, content: result.content,
              ...(result.isError ? { error: result.error } : { value: result.value }) });
          })();
          try {
            return await activePromise;
          } finally { active = false; activePromise = undefined; }
        },
        dispose,
      };
    } catch (error) {
      try { await dispose(); } catch (cleanup) { error.cleanupCode = cleanup.code ?? 'DISPOSE_INCOMPLETE'; }
      throw error;
    }
  } catch (error) {
    imports.deregister();
    throw error;
  }
}
