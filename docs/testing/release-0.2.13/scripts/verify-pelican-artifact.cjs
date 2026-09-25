// 成品验收：先枚举真实控件，再按可访问名称/标签发现暂停和速度，不假设模型生成的 ID/class。
// 所有外部网络被阻止；只打开本地 HTML 并在临时目录写证据。
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { browserRuntime, sha256, writeJson } = require('./browser-qa-common.cjs');
const controlSelector = 'button,input,select,[role="button"],[role="slider"],[role="switch"]';

async function inspectControls(page) {
  return page.locator(controlSelector).evaluateAll(nodes => nodes.map((node, index) => {
    const box = node.getBoundingClientRect(), style = getComputedStyle(node);
    return { index, tag: node.tagName.toLowerCase(), role: node.getAttribute('role'), type: node.type || '',
      text: (node.innerText || '').trim().slice(0, 160), ariaLabel: node.getAttribute('aria-label') || '',
      title: node.title || '', labels: [...(node.labels || [])].map(label => label.innerText.trim()).join(' '),
      value: node.value ?? null, min: node.min || '', max: node.max || '', step: node.step || '',
      options: node.options ? [...node.options].map(option => ({ value: option.value, text: option.text })) : [],
      visible: box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none',
      disabled: !!node.disabled };
  }));
}
const accessibleText = item => [item.text, item.ariaLabel, item.title, item.labels].join(' ');
async function animationProfile(page) {
  return page.evaluate(() => document.getAnimations().map(animation => ({
    playbackRate: animation.playbackRate, playState: animation.playState,
    duration: animation.effect?.getTiming().duration, currentTime: animation.currentTime,
  })));
}
async function svgFrame(page, file) {
  const svgs = page.locator('svg');
  const candidates = await svgs.evaluateAll(nodes => nodes.map((node, index) => {
    const box = node.getBoundingClientRect(); return { index, area: box.width * box.height };
  }));
  const largest = candidates.sort((a, b) => b.area - a.area)[0];
  if (!largest || largest.area <= 0) return null;
  const bytes = await svgs.nth(largest.index).screenshot({ path: file, animations: 'allow' });
  return { sha256: sha256(bytes), svgIndex: largest.index, area: largest.area };
}

(async () => {
  if (!process.argv[2]) throw new Error('用法：node verify-pelican-artifact.cjs <本地 HTML> [证据目录]');
  const input = path.resolve(process.argv[2]);
  if (!fs.statSync(input).isFile() || !/\.html?$/i.test(input)) throw new Error('输入必须是本地 HTML 文件');
  const output = path.resolve(process.argv[3] || path.join(__dirname, 'artifact-browser-qa'));
  fs.mkdirSync(output, { recursive: true });
  const report = { input, inputSha256: sha256(fs.readFileSync(input)), output, console: [], pageErrors: [], blockedRequests: [], checks: {}, passed: false };
  const browser = await browserRuntime().chromium.launch({ channel: 'msedge', headless: true });
  const page = await browser.newPage({ viewport: { width: 1200, height: 850 }, deviceScaleFactor: 1, reducedMotion: 'no-preference', serviceWorkers: 'block' });
  page.on('console', message => { if (['error', 'warning'].includes(message.type())) report.console.push({ type: message.type(), text: message.text() }); });
  page.on('pageerror', error => report.pageErrors.push(error.message));
  await page.route(/^https?:/, route => { report.blockedRequests.push({ method: route.request().method(), url: route.request().url() }); return route.abort('blockedbyclient'); });
  try {
    await page.goto(pathToFileURL(input).href, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(350);
    report.title = await page.title();
    report.controls = await inspectControls(page);
    report.svg = await page.locator('svg').evaluateAll(nodes => nodes.map(node => {
      const box = node.getBoundingClientRect(); return { viewBox: node.getAttribute('viewBox'), width: box.width, height: box.height, childElements: node.querySelectorAll('*').length, text: (node.textContent || '').trim().slice(0, 300) };
    }));
    report.checks.visibleSvg = report.svg.some(svg => svg.width > 100 && svg.height > 100 && svg.childElements > 0);
    await page.screenshot({ path: path.join(output, 'frame-01.png'), fullPage: true, animations: 'allow' });
    const first = await svgFrame(page, path.join(output, 'svg-frame-01.png'));
    report.animationBefore = await animationProfile(page);
    await page.waitForTimeout(780);
    await page.screenshot({ path: path.join(output, 'frame-02.png'), fullPage: true, animations: 'allow' });
    const second = await svgFrame(page, path.join(output, 'svg-frame-02.png'));
    report.frames = { first, second, sampleIntervalMs: 780 };
    report.checks.svgChangesOverTime = !!first && !!second && first.sha256 !== second.sha256;

    let controls = await inspectControls(page);
    const pause = controls.find(item => item.visible && !item.disabled && /暂停|pause|停止|stop/i.test(accessibleText(item)) && !/speed|速度/i.test(accessibleText(item)));
    report.pause = { control: pause || null, verified: false };
    if (pause) {
      await page.locator(controlSelector).nth(pause.index).click();
      await page.waitForTimeout(150);
      const pausedFirst = await svgFrame(page, path.join(output, 'paused-01.png'));
      await page.waitForTimeout(520);
      const pausedSecond = await svgFrame(page, path.join(output, 'paused-02.png'));
      report.pause = { control: pause, first: pausedFirst, second: pausedSecond,
        verified: !!pausedFirst && !!pausedSecond && pausedFirst.sha256 === pausedSecond.sha256,
        animations: await animationProfile(page) };
      controls = await inspectControls(page);
      const resume = controls.find(item => item.visible && !item.disabled && /继续|恢复|播放|resume|play|start|开始|暂停|pause/i.test(accessibleText(item)) && !/speed|速度/i.test(accessibleText(item)));
      if (resume) { await page.locator(controlSelector).nth(resume.index).click(); report.pause.resumedControl = resume; }
    }
    controls = await inspectControls(page);
    const speed = controls.find(item => item.visible && !item.disabled && /速度|倍速|速率|speed|rate/i.test(accessibleText(item)) && (item.tag === 'select' || ['range', 'number'].includes(item.type)));
    report.speed = { control: speed || null, valueChanges: false, behaviorVerified: false };
    if (speed) {
      const target = page.locator(controlSelector).nth(speed.index);
      const beforeProfile = await animationProfile(page);
      let nextValue;
      if (speed.tag === 'select') nextValue = speed.options.find(option => option.value !== speed.value)?.value;
      else {
        const minimum = speed.min === '' ? 0 : Number(speed.min), maximum = speed.max === '' ? Math.max(4, Number(speed.value) * 3) : Number(speed.max);
        const step = speed.step && speed.step !== 'any' ? Number(speed.step) : .1;
        const preferred = Number(speed.value) < (minimum + maximum) / 2 ? minimum + (maximum - minimum) * .75 : minimum + (maximum - minimum) * .25;
        nextValue = String(Math.max(minimum, Math.min(maximum, minimum + Math.round((preferred - minimum) / step) * step)));
      }
      if (nextValue != null) {
        if (speed.tag === 'select') await target.selectOption(nextValue);
        else await target.evaluate((node, value) => { node.value = value; node.dispatchEvent(new Event('input', { bubbles: true })); node.dispatchEvent(new Event('change', { bubbles: true })); }, nextValue);
        await page.waitForTimeout(250);
        const afterProfile = await animationProfile(page);
        report.speed = { control: speed, requestedValue: nextValue, actualValue: await target.inputValue(), beforeProfile, afterProfile,
          valueChanges: await target.inputValue() !== speed.value,
          behaviorVerified: JSON.stringify(beforeProfile.map(({ playbackRate, duration }) => ({ playbackRate, duration }))) !== JSON.stringify(afterProfile.map(({ playbackRate, duration }) => ({ playbackRate, duration }))) };
        if (!report.speed.behaviorVerified) report.speed.note = '值变化已验证；未发现可量化的 Web Animations 速度变化，若页面用 requestAnimationFrame 则需结合产物实现或人工观察确认速度效果。';
        await page.screenshot({ path: path.join(output, 'speed-changed.png'), fullPage: true, animations: 'allow' });
      }
    }
    report.checks.pauseFreezesSvg = report.pause.verified;
    report.checks.speedControlAcceptsChange = report.speed.valueChanges;
    report.checks.noPageErrors = report.pageErrors.length === 0;
    report.checks.noConsoleErrors = !report.console.some(message => message.type === 'error');
    report.checks.selfContained = report.blockedRequests.length === 0;
    report.passed = Object.values(report.checks).every(Boolean);
    report.visualReviewRequired = '截图必须人工确认鹈鹕、单车及美观；自动验收不把 SVG 元素存在等同于绘制内容正确。';
  } catch (error) { report.error = error.stack; }
  finally { writeJson(path.join(output, 'report.json'), report); await browser.close(); }
  console.log(JSON.stringify({ passed: report.passed, checks: report.checks, report: path.join(output, 'report.json') }, null, 2));
  if (!report.passed) process.exitCode = 1;
})().catch(error => { console.error(error); process.exitCode = 1; });
