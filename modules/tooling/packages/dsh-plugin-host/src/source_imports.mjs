// 固定插件导入来源，避免安装目录向上查找时误用工程中的同名 SDK。不是 OS 沙箱。
import { registerHooks } from 'node:module';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

const SDK_IMPORTS = new Set(['@deepseek-ai/dsh-tools', '@deepseek-ai/cordis',
  '@deepseek-ai/schemastery', '@deepseek-ai/cosmokit']);
const HOST_PARENT = new URL('./host.mjs', import.meta.url).href;

function reject(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

export function bindSourceImports(root, receipt) {
  const source = new Map(receipt.files.map(item => [pathToFileURL(path.join(root, item.path)).href, item.sha256]));
  const prefix = pathToFileURL(root + path.sep).href;
  return registerHooks({
    resolve(specifier, context, nextResolve) {
      if (!source.has(context.parentURL)) return nextResolve(specifier, context);
      if (SDK_IMPORTS.has(specifier)) {
        // ESM 按宿主自己的固定 node_modules 解析，不走插件或工作区的搜索路径。
        return nextResolve(specifier, { ...context, parentURL: HOST_PARENT });
      }
      if (!specifier.startsWith('./') && !specifier.startsWith('../')) {
        reject('DEPENDENCY_UNSUPPORTED', `当前工具宿主尚未支持此依赖：${specifier}`);
      }
      const resolved = nextResolve(specifier, context);
      if (!source.has(resolved.url)) reject('IMPORT_OUTSIDE_RECEIPT', '插件导入不属于已核验来源文件');
      return resolved;
    },
    load(url, context, nextLoad) {
      if (!url.startsWith(prefix)) return nextLoad(url, context);
      const expected = source.get(url);
      if (!expected) reject('IMPORT_OUTSIDE_RECEIPT', '插件加载不属于已核验来源文件');
      const bytes = readFileSync(new URL(url));
      if (bytes.length > 4 * 1024 * 1024 || createHash('sha256').update(bytes).digest('hex') !== expected) {
        reject('SOURCE_CHANGED', '插件导入文件在加载前发生变化');
      }
      const loaded = nextLoad(url, context);
      if (!['module', 'commonjs', 'json'].includes(loaded.format)) {
        reject('MODULE_FORMAT_UNSUPPORTED', '插件模块格式暂不支持');
      }
      // 使用刚核验的字节，不让再次读盘造成校验内容与执行内容脱节。
      return { ...loaded, source: bytes };
    },
  });
}
