// 安装版只读验收：允许指定本地服务的 GET，阻止所有写请求；不保存表单、不发送聊天。
const fs = require('node:fs');
const path = require('node:path');
const { browserRuntime, writeJson } = require('./browser-qa-common.cjs');
(async () => {
  if (!process.argv[2]) throw new Error('用法：node verify-installed-model-settings.cjs <本地服务URL> [sessionId] [证据目录]');
  const base = new URL(process.argv[2]);
  if (base.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(base.hostname)) throw new Error('只接受明确指定的本机 HTTP 服务');
  const sessionId = process.argv[3] || null;
  const output = path.resolve(process.argv[4] || path.join(__dirname, 'installed-settings-browser-qa'));
  fs.mkdirSync(output, { recursive: true });
  const report = { baseUrl: base.origin, requestedSessionId: sessionId, pageErrors: [], console: [], blockedRequests: [], checks: {}, passed: false, persistedChanges: false };
  const browser = await browserRuntime().chromium.launch({ channel: 'msedge', headless: true });
  const page = await browser.newPage({ viewport: { width: 1366, height: 900 }, deviceScaleFactor: 1, serviceWorkers: 'block' });
  page.on('pageerror', error => report.pageErrors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') report.console.push(message.text()); });
  await page.route('**/*', route => {
    const request = route.request(), url = new URL(request.url());
    if (url.origin === base.origin && ['GET', 'HEAD'].includes(request.method())) return route.continue();
    if (['data:', 'blob:'].includes(url.protocol)) return route.continue();
    report.blockedRequests.push({ method: request.method(), pathname: url.pathname, external: url.origin !== base.origin });
    return route.abort('blockedbyclient');
  });
  try {
    await page.goto(base.href, { waitUntil: 'domcontentloaded' });
    await page.getByRole('button', { name: '设置', exact: true }).click();
    const panel = page.locator('.model-settings');
    await panel.waitFor({ state: 'visible' });
    await panel.locator('[data-ms="session-select"]').waitFor();
    if (sessionId) await panel.locator('[data-ms="session-select"]').selectOption(sessionId);
    const field = panel.locator('[data-ms="supports-multimodal"]');
    await field.waitFor({ state: 'visible' });
    await page.waitForFunction(() => {
      const fields = document.querySelector('.model-settings [data-ms="fields"]');
      return fields && !fields.disabled;
    });
    const chosen = await panel.locator('[data-ms="session-select"]').inputValue();
    report.loaded = await page.evaluate(async id => {
      const response = await fetch(`/api/sessions/${encodeURIComponent(id)}/model-settings`);
      if (!response.ok) throw new Error(`读取配置失败 ${response.status}`);
      const data = await response.json();
      // 只把白名单信息带出页面；绝不打印完整 API 响应或密钥引用。
      return { sessionId: data.session?.id, model: data.session?.model, modelType: data.session?.model_type,
        apiKeyStatus: data.session?.api_key_status, supportsMultimodal: data.parameters?.supports_multimodal ?? null,
        effectiveSupportsMultimodal: data.effective_supports_multimodal, imageInputStrategy: data.image_input_strategy };
    }, chosen);
    const original = await field.inputValue();
    report.options = await field.locator('option').evaluateAll(nodes => nodes.map(node => ({ value: node.value, text: node.textContent })));
    report.checks.hasThreeStates = JSON.stringify(report.options.map(option => option.value)) === JSON.stringify(['', 'true', 'false']);
    report.checks.matchesSavedOverride = original === (report.loaded.supportsMultimodal == null ? '' : String(report.loaded.supportsMultimodal));
    report.checks.effectiveStrategyPresent = typeof report.loaded.effectiveSupportsMultimodal === 'boolean' && ['native', 'vision-description'].includes(report.loaded.imageInputStrategy);
    report.checks.passwordNeverEchoed = await panel.locator('[data-ms="api-key"]').inputValue() === '';
    report.originalHint = await panel.locator('[data-ms="multimodal-hint"]').innerText();
    await field.scrollIntoViewIfNeeded();
    await page.screenshot({ path: path.join(output, 'settings-saved-state.png'), fullPage: false });
    report.draftStates = [];
    const beforeModelType = await panel.locator('[data-ms="model-type"]').inputValue();
    for (const choice of ['true', 'false', '']) {
      await field.selectOption(choice);
      report.draftStates.push({ choice, actual: await field.inputValue(), hint: await panel.locator('[data-ms="multimodal-hint"]').innerText(),
        dirty: await panel.evaluate(node => node.__modelSettings?.dirty), modelType: await panel.locator('[data-ms="model-type"]').inputValue() });
    }
    report.checks.draftStatesWork = report.draftStates.every(state => state.choice === state.actual && state.dirty === true && state.hint.length > 0);
    report.checks.modelTypeUnchanged = report.draftStates.every(state => state.modelType === beforeModelType);
    await field.selectOption(original);
    await page.setViewportSize({ width: 1024, height: 640 });
    await field.scrollIntoViewIfNeeded();
    await page.screenshot({ path: path.join(output, 'settings-compact.png'), fullPage: false });
    const boxes = await field.evaluate(node => {
      const field = node.getBoundingClientRect(), panel = node.closest('.model-settings').getBoundingClientRect();
      return { field: { left: field.left, right: field.right, width: field.width }, panel: { left: panel.left, right: panel.right, width: panel.width }, viewportWidth: innerWidth };
    });
    report.compactLayout = boxes;
    report.checks.noHorizontalOverflow = boxes.field.left >= boxes.panel.left - 1 && boxes.field.right <= boxes.panel.right + 1 && boxes.field.right <= boxes.viewportWidth + 1;
    report.checks.noPageErrors = report.pageErrors.length === 0;
    report.checks.noChatRequests = !report.blockedRequests.some(request => /chat.*(?:stream|send)|\/api\/chat(?:\/|$)|completion/i.test(request.pathname));
    report.passed = Object.values(report.checks).every(Boolean);
    report.note = '三态仅修改独立浏览器中的未保存草稿；脚本未点击保存，拦截全部非 GET/HEAD 请求。整页启动产生的被阻止写请求单独记录，不能当作模型调用成功。';
  } catch (error) { report.error = error.stack; }
  finally { writeJson(path.join(output, 'report.json'), report); await browser.close(); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks, report: path.join(output, 'report.json') }, null, 2));
  if (!report.passed) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
