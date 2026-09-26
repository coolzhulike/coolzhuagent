// 小型协议响应有界读取，超时覆盖响应体而不只等到 HTTP headers。
export async function fetchJsonBounded(url, options = {}, timeoutMs = 7000, maxBytes = 4 * 1024 * 1024) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), Math.max(1, timeoutMs));
  try {
    const response = await fetch(url, { ...options, redirect: 'error', signal: controller.signal });
    const declared = Number(response.headers.get('content-length'));
    if (Number.isFinite(declared) && declared > maxBytes) throw new Error('iLink 响应超过大小限制');
    const reader = response.body?.getReader(); const chunks = []; let total = 0;
    if (reader) {
      try {
        for (;;) {
          const { value, done } = await reader.read(); if (done) break;
          total += value.byteLength;
          if (total > maxBytes) throw new Error('iLink 响应超过大小限制');
          chunks.push(Buffer.from(value));
        }
      } catch (error) { await reader.cancel().catch(() => {}); throw error; }
    }
    if (!response.ok) throw new Error(`iLink HTTP ${response.status}`);
    const text = Buffer.concat(chunks, total).toString('utf8');
    if (!text.trim()) return {};
    try { return JSON.parse(text); }
    catch { throw new Error(`iLink 返回无效 JSON，HTTP ${response.status}`); }
  } catch (error) {
    if (controller.signal.aborted) {
      const timeout = new Error(`iLink request timed out after ${timeoutMs}ms`);
      timeout.code = 'ETIMEDOUT'; throw timeout;
    }
    throw error;
  } finally { clearTimeout(timer); }
}
