// 对真实本地成品做独立只读测试：不写 HTML，不调用模型，结果只写证据目录。
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { browserRuntime, sha256, writeJson } = require('./browser-qa-common.cjs');
const controlsSelector = 'button,input,select,[role="button"],[role="switch"]';
const textOf = item => [item.text, item.label, item.title, item.aria].join(' ');
const near = (actual, expected, tolerance = .2) => Number.isFinite(actual) && Math.abs(actual - expected) <= expected * tolerance;

async function controls(page) {
  return page.locator(controlsSelector).evaluateAll(nodes => nodes.map((node, index) => {
    const box = node.getBoundingClientRect();
    return { index, tag: node.tagName.toLowerCase(), type: node.type || '', text: (node.innerText || '').trim(),
      label: [...(node.labels || [])].map(label => label.innerText).join(' '), aria: node.getAttribute('aria-label') || '', title: node.title || '',
      value: node.value ?? null, checked: typeof node.checked === 'boolean' ? node.checked : null,
      pressed: node.getAttribute('aria-pressed'), min: node.min || '', max: node.max || '',
      visible: box.width > 0 && box.height > 0, disabled: !!node.disabled };
  }));
}
async function mainSvg(page) {
  const svgs = await page.locator('svg').evaluateAll(nodes => nodes.map((node, index) => {
    const box = node.getBoundingClientRect(); return { index, area: box.width * box.height };
  }));
  const candidate = svgs.sort((a, b) => b.area - a.area)[0];
  if (!candidate?.area) throw new Error('未发现可见 SVG');
  return page.locator('svg').nth(candidate.index);
}
async function clockSample(page, durationMs = 1000) {
  return (await mainSvg(page)).evaluate(async (svg, duration) => {
    if (typeof svg.getCurrentTime !== 'function') return { supported: false };
    const wallStart = performance.now(), svgStart = svg.getCurrentTime();
    await new Promise(resolve => setTimeout(resolve, duration));
    const wallEnd = performance.now(), svgEnd = svg.getCurrentTime();
    const elapsedSeconds = (wallEnd - wallStart) / 1000;
    return { supported: true, elapsedSeconds, svgStart, svgEnd, svgAdvance: svgEnd - svgStart,
      actualRate: (svgEnd - svgStart) / elapsedSeconds };
  }, durationMs);
}
async function motionSample(page, prefix) {
  const svg = await mainSvg(page);
  const first = await svg.screenshot({ path: prefix + '-01.png', animations: 'allow' });
  const clock = await clockSample(page, 650);
  const second = await svg.screenshot({ path: prefix + '-02.png', animations: 'allow' });
  return { clock, pixelChanges: sha256(first) !== sha256(second), firstSha256: sha256(first), secondSha256: sha256(second) };
}
async function setSpeed(page, rate) {
  const list = await controls(page);
  const input = list.find(item => item.visible && !item.disabled && /速度|速率|倍速|speed/i.test(textOf(item)) && ['range', 'number'].includes(item.type));
  if (!input) return { found: false };
  if ((input.min && rate < Number(input.min)) || (input.max && rate > Number(input.max))) return { found: true, supported: false, input };
  const target = page.locator(controlsSelector).nth(input.index);
  await target.evaluate((node, value) => { node.value = String(value); node.dispatchEvent(new Event('input', { bubbles: true })); node.dispatchEvent(new Event('change', { bubbles: true })); }, rate);
  await page.waitForTimeout(100);
  return { found: true, supported: true, input, actualValue: Number(await target.inputValue()) };
}
async function layoutState(page) {
  return page.evaluate(selector => {
    const describe = node => {
      const box = node.getBoundingClientRect();
      let clip = { left: 0, top: 0, right: innerWidth, bottom: innerHeight };
      for (let parent = node.parentElement; parent; parent = parent.parentElement) {
        const style = getComputedStyle(parent), outer = parent.getBoundingClientRect();
        if (/hidden|clip|auto|scroll/.test(style.overflowX)) { clip.left = Math.max(clip.left, outer.left); clip.right = Math.min(clip.right, outer.right); }
        if (/hidden|clip|auto|scroll/.test(style.overflowY)) { clip.top = Math.max(clip.top, outer.top); clip.bottom = Math.min(clip.bottom, outer.bottom); }
      }
      return { tag: node.tagName.toLowerCase(), text: (node.innerText || node.getAttribute('aria-label') || '').trim().slice(0, 100),
        bounds: { x: box.x, y: box.y, width: box.width, height: box.height, right: box.right, bottom: box.bottom },
        visible: box.width > 0 && box.height > 0,
        fullyVisible: box.width > 0 && box.height > 0 && box.left >= clip.left - 1 && box.top >= clip.top - 1 && box.right <= clip.right + 1 && box.bottom <= clip.bottom + 1 };
    };
    const svg = [...document.querySelectorAll('svg')].sort((a, b) => b.getBoundingClientRect().width * b.getBoundingClientRect().height - a.getBoundingClientRect().width * a.getBoundingClientRect().height)[0];
    return { viewport: { width: innerWidth, height: innerHeight }, document: { width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight },
      svg: svg ? describe(svg) : null, controls: [...document.querySelectorAll(selector)].map(describe) };
  }, controlsSelector);
}

(async () => {
  if (!process.argv[2]) throw new Error('用法：node verify-pelican-extended.cjs <本地 HTML> [证据目录]');
  const input = path.resolve(process.argv[2]), output = path.resolve(process.argv[3] || path.join(__dirname, 'pelican-extended-qa'));
  if (!fs.statSync(input).isFile()) throw new Error('输入不是本地文件');
  fs.mkdirSync(output, { recursive: true });
  const inputHash = sha256(fs.readFileSync(input));
  const report = { input, inputSha256: inputHash, output, checks: {}, blockedRequests: [], pageErrors: [], consoleErrors: [], passed: false };
  const browser = await browserRuntime().chromium.launch({ channel: 'msedge', headless: true });
  async function newPage(reducedMotion = 'no-preference') {
    const page = await browser.newPage({ viewport: { width: 1200, height: 850 }, reducedMotion, serviceWorkers: 'block' });
    page.on('pageerror', error => report.pageErrors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') report.consoleErrors.push(message.text()); });
    await page.route(/^https?:/, route => { report.blockedRequests.push(route.request().url()); return route.abort('blockedbyclient'); });
    await page.goto(pathToFileURL(input).href, { waitUntil: 'load' });
    await page.waitForTimeout(180);
    return page;
  }
  const page = await newPage();
  try {
    report.initialControls = await controls(page);
    report.defaultMotion = await clockSample(page, 600);
    report.speed = {};
    for (const [label, rate] of [['half', .5], ['double', 2]]) {
      const setting = await setSpeed(page, rate);
      report.speed[label] = { setting, sample: setting.supported ? await clockSample(page, 1200) : null };
    }
    const slow = report.speed.half.sample?.actualRate, fast = report.speed.double.sample?.actualRate;
    report.speed.ratio = fast / slow;
    report.speed.expectedRatio = 4;
    report.speed.tolerance = '单档实际速度与预期差异不超过 20%，两档比值在 3.2–4.8 之间';
    report.checks.actualSpeed = !report.speed.half.setting.found ? 'missing' : near(slow, .5) && near(fast, 2) && near(report.speed.ratio, 4) ? 'passed' : 'failed';

    const list = await controls(page);
    const pause = list.find(item => item.visible && !item.disabled && /暂停|pause/i.test(textOf(item)));
    if (pause) await page.locator(controlsSelector).nth(pause.index).click();
    const reset = (await controls(page)).find(item => item.visible && !item.disabled && /恢复默认|重置|reset|default/i.test(textOf(item)));
    report.reset = { control: reset || null };
    if (reset) {
      await page.locator(controlsSelector).nth(reset.index).click();
      report.reset.immediateTime = await (await mainSvg(page)).evaluate(svg => svg.getCurrentTime());
      report.reset.controls = await controls(page);
      report.reset.motion = await clockSample(page, 700);
      const initialSpeed = report.initialControls.find(item => /速度|speed/i.test(textOf(item)) && ['range', 'number'].includes(item.type));
      const resetSpeed = report.reset.controls.find(item => /速度|speed/i.test(textOf(item)) && ['range', 'number'].includes(item.type));
      report.reset.speedRestored = !!initialSpeed && !!resetSpeed && resetSpeed.value === initialSpeed.value;
      report.reset.clockRestarted = report.reset.immediateTime < .3;
      report.reset.playingRestored = report.reset.motion.actualRate > .1;
      report.checks.resetDefaults = report.reset.speedRestored && report.reset.clockRestarted && report.reset.playingRestored ? 'passed' : 'failed';
    } else report.checks.resetDefaults = 'missing';

    report.layouts = [];
    for (const viewport of [{ width: 1024, height: 600 }, { width: 720, height: 480 }]) {
      await page.setViewportSize(viewport);
      await page.waitForTimeout(180);
      const state = await layoutState(page);
      state.noOverflow = state.document.width <= viewport.width + 1 && state.document.height <= viewport.height + 1;
      state.allVisible = !!state.svg?.fullyVisible && state.controls.every(control => !control.visible || control.fullyVisible);
      state.passed = state.noOverflow && state.allVisible;
      await page.screenshot({ path: path.join(output, `layout-${viewport.width}x${viewport.height}.png`), fullPage: false, animations: 'allow' });
      report.layouts.push(state);
    }
    report.checks.layout1024x600 = report.layouts[0].passed ? 'passed' : 'failed';
    report.checks.layout720x480 = report.layouts[1].passed ? 'passed' : 'failed';

    await page.setViewportSize({ width: 1200, height: 850 });
    const reduce = (await controls(page)).find(item => item.visible && item.type === 'checkbox' && /减少动态|减少动画|减少运动|reduc.*motion/i.test(textOf(item)));
    report.manualReducedMotion = { control: reduce || null };
    if (reduce) {
      const field = page.locator(controlsSelector).nth(reduce.index);
      await field.setChecked(false);
      const before = await motionSample(page, path.join(output, 'manual-normal'));
      await field.setChecked(true);
      const after = await motionSample(page, path.join(output, 'manual-reduced'));
      report.manualReducedMotion = { control: reduce, checked: await field.isChecked(), before, after,
        reduced: !after.pixelChanges || after.clock.actualRate < before.clock.actualRate * .75 };
      await field.setChecked(false);
      report.manualReducedMotion.restored = await motionSample(page, path.join(output, 'manual-restored'));
      report.checks.manualReducedMotion = report.manualReducedMotion.checked && report.manualReducedMotion.reduced ? 'passed' : 'failed';
      report.checks.manualMotionRestores = report.manualReducedMotion.restored.pixelChanges ? 'passed' : 'failed';
      // 减少动态与恢复默认的联动：避免恢复播放后复选框仍表示冻结。
      await field.setChecked(true);
      await setSpeed(page, 2);
      const reducedReset = (await controls(page)).find(item => item.visible && /恢复默认|重置|reset|default/i.test(textOf(item)));
      if (reducedReset) {
        await page.locator(controlsSelector).nth(reducedReset.index).click();
        const motion = await clockSample(page, 500);
        report.reducedReset = { checkboxCleared: !await field.isChecked(), motion };
        report.checks.resetClearsReducedMotion = report.reducedReset.checkboxCleared && near(motion.actualRate, 1) ? 'passed' : 'failed';
      } else report.checks.resetClearsReducedMotion = 'missing';
    } else { report.checks.manualReducedMotion = 'missing'; report.checks.manualMotionRestores = 'missing'; }

    const reducedPage = await newPage('reduce');
    try {
      const systemControls = await controls(reducedPage);
      const reducedCheckbox = systemControls.find(item => item.type === 'checkbox' && /减少动态|减少动画|减少运动|reduc.*motion/i.test(textOf(item)));
      const sample = await motionSample(reducedPage, path.join(output, 'system-reduced'));
      report.systemReducedMotion = { mediaMatches: await reducedPage.evaluate(() => matchMedia('(prefers-reduced-motion: reduce)').matches), checkbox: reducedCheckbox || null, sample,
        reduced: !sample.pixelChanges || sample.clock.actualRate < report.defaultMotion.actualRate * .75 };
      report.checks.prefersReducedMotion = report.systemReducedMotion.reduced ? 'passed' : !reduce ? 'missing' : 'failed';
      await reducedPage.emulateMedia({ reducedMotion: 'no-preference' });
      await reducedPage.waitForTimeout(100);
      const restoredClock = await clockSample(reducedPage, 500);
      await reducedPage.emulateMedia({ reducedMotion: 'reduce' });
      await reducedPage.waitForTimeout(100);
      const reducedAgainClock = await clockSample(reducedPage, 500);
      report.systemMotionChanges = { restoredClock, reducedAgainClock };
      report.checks.systemPreferenceChanges = restoredClock.actualRate > .1 && Math.abs(reducedAgainClock.actualRate) < .03 ? 'passed' : !reduce ? 'missing' : 'failed';
    } finally { await reducedPage.close(); }
    report.checks.noRuntimeErrors = !report.pageErrors.length && !report.consoleErrors.length ? 'passed' : 'failed';
    report.checks.noExternalRequests = !report.blockedRequests.length ? 'passed' : 'failed';
    report.checks.inputUnchanged = sha256(fs.readFileSync(input)) === inputHash ? 'passed' : 'failed';
    report.passed = Object.values(report.checks).every(status => status === 'passed');
    report.notes = ['missing 表示本轮产物尚未具备对应功能；不代替最终轮验收。', '减少动态允许冻结或明显减速；若模型仅禁用部分装饰动画而主 SVG 时钟不变，应结合截图与实现人工复核。'];
  } catch (error) { report.error = error.stack; }
  finally { writeJson(path.join(output, 'report.json'), report); await browser.close(); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks, speedRatio: report.speed?.ratio, report: path.join(output, 'report.json') }, null, 2));
  if (!report.passed) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
