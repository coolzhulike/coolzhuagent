// 使用真实 SDK 与真实远程插件，验证依赖、注册、执行、取消和释放。
import { Context } from '@deepseek-ai/cordis';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime from '@deepseek-ai/dsh-tools';
import * as Calculator from './real-calculator/lib/index.js';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';

const ctx = new Context();
const loadedFibers = [];
const receipt = {startedAt: new Date().toISOString(), node: process.version,
  scope: '真实插件 SDK 依赖探针；不代表正式安装或 Qwen 模型调用通过',
  module: {name: Calculator.name, inject: Calculator.inject}, operations: []};
try {
  const promptFiber = await ctx.plugin(SystemPrompt, {
    includeHarnessIdentity: false, includeRuntimeContext: false,
  });
  loadedFibers.push(promptFiber);
  const toolsFiber = await ctx.plugin(ToolRuntime, {mode: 'native'});
  loadedFibers.push(toolsFiber);
  const pluginFiber = await ctx.plugin(Calculator);
  loadedFibers.push(pluginFiber);
  assert.equal(promptFiber.state, 2);
  assert.equal(toolsFiber.state, 2);
  assert.equal(pluginFiber.state, 2);
  const schemas = ctx.tools.schemas();
  receipt.schemas = schemas;
  assert.equal(schemas.length, 1);
  assert.equal(schemas[0].name, 'calculator');
  receipt.operations.push({name: '真实服务加载', states: [promptFiber.state, toolsFiber.state, pluginFiber.state]});

  const normal = await ctx.tools.execute({callId: 'p1-calculator-normal', name: 'calculator',
    arguments: {expression: '15 + 27 * sqrt(9)'}, signal: new AbortController().signal});
  receipt.operations.push({name: '真实工具执行', result: normal});
  assert.equal(normal.isError, false);
  assert.equal(normal.value, 96);
  assert.equal(normal.content[0].text, '96');

  const invalid = await ctx.tools.execute({callId: 'p1-calculator-invalid', name: 'calculator',
    arguments: {expression: 'process.exit()'}, signal: new AbortController().signal});
  receipt.operations.push({name: '实际表达式拒绝', result: invalid});
  assert.equal(invalid.isError, true);

  const aborted = new AbortController();
  aborted.abort();
  const canceled = await ctx.tools.execute({callId: 'p1-calculator-aborted', name: 'calculator',
    arguments: {expression: '15 + 27 * sqrt(9)'}, signal: aborted.signal});
  receipt.operations.push({name: '真实 AbortSignal 预取消', result: canceled});
  assert.equal(canceled.error.info.code, 'ABORTED_BEFORE_DISPATCH');

  await pluginFiber.dispose();
  const remaining = ctx.tools.schemas();
  assert.equal(remaining.length, 0);
  const disabled = await ctx.tools.execute({callId: 'p1-calculator-disabled', name: 'calculator',
    arguments: {expression: '1+1'}, signal: new AbortController().signal});
  receipt.operations.push({name: '停用后定义撤销与拒绝', state: pluginFiber.state, schemas: remaining, result: disabled});
  assert.equal(pluginFiber.state, 4);
  assert.equal(disabled.error.info.code, 'UNKNOWN_TOOL');
  receipt.passed = true;
} catch (error) {
  receipt.passed = false;
  receipt.error = {name: error.name, message: error.message, stack: error.stack};
  process.exitCode = 1;
} finally {
  await ctx.fiber.dispose();
  // 官方根 fiber 的 dispose 是 restart；以子插件释放和注册表清空核验收尾。
  receipt.cleanup = {rootState: ctx.fiber.state, pluginStates: loadedFibers.map(fiber => fiber.state),
    registrySize: ctx.registry.size, toolsPresent: ctx.tools !== undefined,
    systemPromptPresent: ctx.systemPrompt !== undefined};
  receipt.cleanupPassed = receipt.cleanup.pluginStates.every(state => state === 4)
    && receipt.cleanup.registrySize === 0 && !receipt.cleanup.toolsPresent && !receipt.cleanup.systemPromptPresent;
  if (!receipt.cleanupPassed) {receipt.passed = false; process.exitCode = 1;}
  receipt.finishedAt = new Date().toISOString();
  await writeFile(new URL('../browser-use-priority/dsh-p1-runtime-receipt.json', import.meta.url), JSON.stringify(receipt, null, 2));
  console.log(JSON.stringify(receipt, null, 2));
}
