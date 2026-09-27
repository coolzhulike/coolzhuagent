const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

function browserRuntime() {
  const candidates = [process.env.PLAYWRIGHT_MODULE_PATH, 'playwright-core', 'playwright',
    path.join(process.env.USERPROFILE || '', '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')].filter(Boolean);
  for (const candidate of candidates) {
    try { return require(candidate); } catch (error) { if (error.code !== 'MODULE_NOT_FOUND') throw error; }
  }
  throw new Error('找不到 Playwright；请设置 PLAYWRIGHT_MODULE_PATH 或 NODE_PATH。');
}
const sha256 = value => crypto.createHash('sha256').update(value).digest('hex');
const writeJson = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n', 'utf8');
module.exports = { browserRuntime, sha256, writeJson };
