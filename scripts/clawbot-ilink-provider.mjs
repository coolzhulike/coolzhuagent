#!/usr/bin/env node

import http from 'node:http';
import { createCipheriv, createHash, randomBytes, randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';

import {
  normalizeIlinkGroupEvent,
  normalizeIlinkInboundIdentity,
  summarizeIlinkInboundPayload,
} from './lib/clawbot-ilink-message.mjs';

const bindHost = process.env.CLAWBot_ILINK_BIND_HOST || process.env.CLAWBOT_ILINK_BIND_HOST || '127.0.0.1';
const bindPort = Number(process.env.CLAWBot_ILINK_BIND_PORT || process.env.CLAWBOT_ILINK_BIND_PORT || '8790');
const ilinkBaseUrl = (process.env.CLAWBOT_ILINK_BASE_URL || 'https://ilinkai.weixin.qq.com').replace(/\/+$/, '');
const ilinkCdnBaseUrl = (process.env.CLAWBOT_ILINK_CDN_BASE_URL || 'https://novac2c.cdn.weixin.qq.com/c2c').replace(/\/+$/, '');
const channelVersion = process.env.CLAWBOT_ILINK_CHANNEL_VERSION || '2.4.6';
const ilinkAppId = process.env.CLAWBOT_ILINK_APP_ID || 'bot';
const botAgent = process.env.CLAWBOT_ILINK_BOT_AGENT || 'CoolzhuAgent/0.2.0';
const accountId = process.env.CLAWBOT_ILINK_ACCOUNT_ID || 'wx-ilink';
const providerToken = process.env.CLAWBOT_ILINK_PROVIDER_TOKEN || '';
const startupBotToken = process.env.CLAWBOT_ILINK_BOT_TOKEN || '';
const requestTimeoutMs = Math.max(250, Number(process.env.CLAWBOT_ILINK_REQUEST_TIMEOUT_MS || '7000') || 7000);
const updatesTimeoutMs = Math.max(1000, Number(process.env.CLAWBOT_ILINK_UPDATES_TIMEOUT_MS || '35000') || 35000);
const fileMaxBytes = Math.max(1, Number(process.env.CLAWBOT_ILINK_FILE_MAX_BYTES || String(20 * 1024 * 1024)) || 20 * 1024 * 1024);

function buildClientVersion(version) {
  const parts = String(version || '0.0.0').split('.').map((value) => Number.parseInt(value, 10) || 0);
  return ((parts[0] & 0xff) << 16) | ((parts[1] & 0xff) << 8) | (parts[2] & 0xff);
}

const ilinkAppClientVersion = Number(
  process.env.CLAWBOT_ILINK_APP_CLIENT_VERSION || buildClientVersion(channelVersion),
);

const state = {
  qrcode: null,
  qrcodeImage: null,
  qrcodeCreatedAtMs: 0,
  botToken: startupBotToken,
  botBaseUrl: ilinkBaseUrl,
  ilinkBotId: null,
  ilinkUserId: null,
  updatesCursor: '',
  longPollTimeoutMs: updatesTimeoutMs,
  sessionStarted: false,
  lastError: null,
  contextByPeer: new Map(),
};

function jsonResponse(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(body),
  });
  res.end(body);
}

function textResponse(res, status, body) {
  res.writeHead(status, {
    'content-type': 'text/plain; charset=utf-8',
    'content-length': Buffer.byteLength(body),
  });
  res.end(body);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8');
      if (!raw.trim()) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(raw));
      } catch (error) {
        reject(new Error(`invalid json body: ${error.message}`));
      }
    });
    req.on('error', reject);
  });
}

function redact(value) {
  let output = String(value || '');
  for (const secret of [providerToken, state.botToken]) {
    if (secret && secret.length >= 4) {
      output = output.split(secret).join('[redacted]');
    }
  }
  return output.replace(/(Authorization:\s*Bearer\s+)[^\s"']+/gi, '$1[redacted]');
}

function requireProviderAuth(req) {
  if (!providerToken) return true;
  const auth = req.headers.authorization || '';
  return auth === `Bearer ${providerToken}`;
}

function ilinkCommonHeaders() {
  return {
    'iLink-App-Id': ilinkAppId,
    'iLink-App-ClientVersion': String(ilinkAppClientVersion),
  };
}

function ilinkHeaders() {
  const headers = {
    'content-type': 'application/json',
    ...ilinkCommonHeaders(),
  };
  if (state.botToken) {
    headers.AuthorizationType = 'ilink_bot_token';
    headers.Authorization = `Bearer ${state.botToken}`;
    headers['X-WECHAT-UIN'] = Buffer.from(String(Math.floor(Math.random() * 4294967295))).toString('base64');
  }
  return headers;
}

function baseInfo() {
  return {
    channel_version: channelVersion,
    bot_agent: botAgent,
  };
}

async function notifySession(action) {
  if (!state.botToken) return;
  const payload = await fetchJson(`${state.botBaseUrl}/ilink/bot/msg/notify${action}`, {
    method: 'POST',
    headers: ilinkHeaders(),
    body: JSON.stringify({ base_info: baseInfo() }),
  });
  if ((payload.ret && payload.ret !== 0) || (payload.errcode && payload.errcode !== 0)) {
    throw new Error(`iLink notify${action} ret=${payload.ret || ''} errcode=${payload.errcode || ''} errmsg=${payload.errmsg || ''}`);
  }
}

async function ensureSessionStarted() {
  if (!state.botToken || state.sessionStarted) return;
  await notifySession('start');
  state.sessionStarted = true;
  state.lastError = null;
}

async function fetchJson(url, options = {}, timeoutMs = requestTimeoutMs) {
  const controller = timeoutMs > 0 ? new AbortController() : null;
  const timer = controller ? setTimeout(() => controller.abort(), timeoutMs) : null;
  let response;
  try {
    response = await fetch(url, {
      ...options,
      ...(controller ? { signal: controller.signal } : {}),
    });
  } catch (error) {
    if (error?.name === 'AbortError') {
      const timeoutError = new Error(`iLink request timed out after ${timeoutMs}ms`);
      timeoutError.code = 'ETIMEDOUT';
      throw timeoutError;
    }
    throw error;
  } finally {
    if (timer) clearTimeout(timer);
  }
  const text = await response.text();
  let payload = {};
  if (text.trim()) {
    try {
      payload = JSON.parse(text);
    } catch (error) {
      throw new Error(`iLink returned non-json ${response.status}: ${text.slice(0, 300)}`);
    }
  }
  if (!response.ok) {
    throw new Error(`iLink HTTP ${response.status}: ${JSON.stringify(payload).slice(0, 500)}`);
  }
  return payload;
}

async function uploadEncryptedFile(url, encrypted) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), requestTimeoutMs);
  let response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/octet-stream' },
      body: encrypted,
      signal: controller.signal,
    });
  } catch (error) {
    if (error?.name === 'AbortError') {
      throw new Error(`iLink CDN upload timed out after ${requestTimeoutMs}ms`);
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
  if (!response.ok) {
    const detail = response.headers.get('x-error-message') || (await response.text()).slice(0, 300);
    throw new Error(`iLink CDN upload HTTP ${response.status}: ${detail}`);
  }
  const downloadParam = response.headers.get('x-encrypted-param');
  if (!downloadParam) {
    throw new Error('iLink CDN upload response missing x-encrypted-param');
  }
  return downloadParam;
}

async function imageUrlToDataUrl(value) {
  if (!value) return null;
  if (String(value).startsWith('data:image/')) return value;
  if (/^https:\/\/liteapp\.weixin\.qq\.com\/q\//i.test(String(value))) return value;
  if (!/^https?:\/\//i.test(String(value))) return value;
  const response = await fetch(value);
  if (!response.ok) return value;
  const contentType = response.headers.get('content-type') || 'image/png';
  if (!contentType.toLowerCase().startsWith('image/')) {
    return value;
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  return `data:${contentType};base64,${bytes.toString('base64')}`;
}

async function refreshLogin(nowMs, generation) {
  if (state.botToken) {
    await ensureSessionStarted();
    state.lastError = null;
    return {
      generation,
      account_id: accountId,
      state: 'online',
      qr_code_data_url: null,
      expires_at_ms: null,
      last_error: null,
    };
  }
  let createdNewQrcode = false;
  if (!state.qrcode || nowMs - state.qrcodeCreatedAtMs > 110_000) {
    const payload = await fetchJson(`${ilinkBaseUrl}/ilink/bot/get_bot_qrcode?bot_type=3`, {
      method: 'GET',
      headers: ilinkCommonHeaders(),
    });
    state.qrcode = payload.qrcode || payload.token || payload.qr_code || null;
    state.qrcodeImage = await imageUrlToDataUrl(payload.qrcode_img_content || payload.qrcode_image || payload.url || null);
    state.qrcodeCreatedAtMs = nowMs;
    createdNewQrcode = true;
  }
  if (createdNewQrcode) {
    return {
      generation,
      account_id: state.ilinkBotId || null,
      state: 'awaiting_scan',
      qr_code_data_url: state.qrcodeImage,
      expires_at_ms: state.qrcodeCreatedAtMs + 120_000,
      last_error: null,
    };
  }
  if (state.qrcode) {
    let status;
    try {
      status = await fetchJson(`${ilinkBaseUrl}/ilink/bot/get_qrcode_status?qrcode=${encodeURIComponent(state.qrcode)}`, {
        method: 'GET',
        headers: ilinkCommonHeaders(),
      });
    } catch (error) {
      if (error?.code === 'ETIMEDOUT') {
        return {
          generation,
          account_id: state.ilinkBotId || null,
          state: 'awaiting_scan',
          qr_code_data_url: state.qrcodeImage,
          expires_at_ms: state.qrcodeCreatedAtMs + 120_000,
          last_error: null,
        };
      }
      throw error;
    }
    const loginStatus = String(status.status || status.state || '').toLowerCase();
    if (loginStatus === 'confirmed' || loginStatus === 'success' || status.bot_token) {
      state.botToken = status.bot_token || status.token || state.botToken;
      state.botBaseUrl = (status.baseurl || status.base_url || ilinkBaseUrl).replace(/\/+$/, '');
      state.ilinkBotId = status.ilink_bot_id || status.bot_id || state.ilinkBotId;
      state.ilinkUserId = status.ilink_user_id || status.user_id || state.ilinkUserId;
      state.sessionStarted = false;
      await ensureSessionStarted();
      state.lastError = null;
      return {
        generation,
        account_id: accountId,
        state: 'online',
        qr_code_data_url: null,
        expires_at_ms: null,
        last_error: null,
      };
    }
    if (loginStatus === 'expired') {
      state.qrcode = null;
      state.qrcodeImage = null;
      return {
        generation,
        account_id: null,
        state: 'expired',
        qr_code_data_url: null,
        expires_at_ms: null,
        last_error: '二维码已过期，请重新刷新',
      };
    }
  }
  return {
    generation,
    account_id: state.ilinkBotId || null,
    state: 'awaiting_scan',
    qr_code_data_url: state.qrcodeImage,
    expires_at_ms: state.qrcodeCreatedAtMs + 120_000,
    last_error: null,
  };
}

function extractText(message) {
  if (!message || typeof message !== 'object') return null;
  if (typeof message.text === 'string') return message.text;
  if (typeof message.content === 'string') return message.content;
  const items = message.item_list || message.itemList || [];
  for (const item of items) {
    const text = item?.text_item?.text || item?.textItem?.text || item?.text;
    if (typeof text === 'string' && text) return text;
  }
  return null;
}

function mapInbound(message, nowMs) {
  const identity = normalizeIlinkInboundIdentity(message, state.ilinkBotId || accountId);
  const peerId = identity.peerId;
  const contextToken = message.context_token || message.contextToken || null;
  if (contextToken) state.contextByPeer.set(peerId, contextToken);
  return {
    source: 'weixin_user',
    hop_count: 0,
    message: {
      account_id: accountId,
      peer_id: peerId,
      conversation_id: identity.conversationId,
      peer_name: identity.peerName,
      sender_id: identity.senderId,
      sender_name: identity.senderName,
      is_group: identity.isGroup,
      mentioned_bot: identity.mentionedBot,
      mentions: identity.mentions,
      group_event: normalizeIlinkGroupEvent(message, state.ilinkBotId || accountId),
      raw_payload_summary: summarizeIlinkInboundPayload(message),
      context_token: contextToken,
      external_msg_id: String(message.client_id || message.msg_id || message.msgid || message.id || randomUUID()),
      kind: 'text',
      text: extractText(message),
      media_refs: [],
      received_at_ms: Number(message.create_time_ms || message.createTimeMs || message.timestamp_ms || nowMs),
    },
  };
}

async function pollUpdates(nowMs) {
  if (!state.botToken) {
    return [];
  }
  await ensureSessionStarted();
  let payload;
  try {
    payload = await fetchJson(`${state.botBaseUrl}/ilink/bot/getupdates`, {
      method: 'POST',
      headers: ilinkHeaders(),
      body: JSON.stringify({
        get_updates_buf: state.updatesCursor || '',
        base_info: baseInfo(),
      }),
    }, state.longPollTimeoutMs);
  } catch (error) {
    if (error?.code === 'ETIMEDOUT') {
      return [];
    }
    throw error;
  }
  if (payload.ret && payload.ret !== 0) {
    throw new Error(`iLink getupdates ret=${payload.ret} errcode=${payload.errcode || ''} errmsg=${payload.errmsg || ''}`);
  }
  state.updatesCursor = payload.get_updates_buf || payload.getUpdatesBuf || state.updatesCursor || '';
  const suggestedTimeoutMs = Number(payload.longpolling_timeout_ms || payload.longPollingTimeoutMs || 0);
  if (suggestedTimeoutMs > 0) {
    state.longPollTimeoutMs = Math.min(60_000, Math.max(1_000, suggestedTimeoutMs));
  }
  const messages = payload.msgs || payload.messages || payload.updates || [];
  return messages.map((message) => mapInbound(message, nowMs));
}

async function sendText(payload) {
  if (!state.botToken) {
    throw new Error('尚未登录 iLink Bot');
  }
  const peerId = payload.peer_id || payload.peerId;
  const body = payload.body || payload.text || '';
  const contextToken = payload.context_token || payload.contextToken || state.contextByPeer.get(peerId);
  if (!contextToken) {
    throw new Error('send_text 需要 context_token；请先让目标微信用户向 Bot 发送一条消息');
  }
  const clientId = `coolzhu-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const response = await fetchJson(`${state.botBaseUrl}/ilink/bot/sendmessage`, {
    method: 'POST',
    headers: ilinkHeaders(),
    body: JSON.stringify({
      msg: {
        from_user_id: '',
        to_user_id: peerId,
        client_id: clientId,
        message_type: 2,
        message_state: 2,
        context_token: contextToken,
        item_list: [{ type: 1, text_item: { text: body } }],
      },
      base_info: baseInfo(),
    }),
  });
  if (response.ret && response.ret !== 0) {
    throw new Error(`iLink sendmessage ret=${response.ret} errcode=${response.errcode || ''} errmsg=${response.errmsg || ''}`);
  }
  return { provider_message_id: response.msg_id || response.message_id || clientId };
}

async function sendFile(payload) {
  if (!state.botToken) {
    throw new Error('尚未登录 iLink Bot');
  }
  await ensureSessionStarted();
  const peerId = payload.peer_id || payload.peerId;
  const contextToken = payload.context_token || payload.contextToken || state.contextByPeer.get(peerId);
  const localPath = String(payload.local_path || payload.localPath || '').trim();
  const displayName = String(payload.display_name || payload.displayName || '').trim();
  if (!peerId) {
    throw new Error('send_file 需要 peer_id');
  }
  if (!contextToken) {
    throw new Error('send_file 需要 context_token；请先让目标微信用户向 Bot 发送一条消息');
  }
  if (!localPath) {
    throw new Error('send_file 需要 local_path');
  }
  if (!displayName || /[\\/]/.test(displayName)) {
    throw new Error('send_file 的 display_name 必须是单个安全文件名');
  }

  const plaintext = await readFile(localPath);
  const declaredSize = Number(payload.size_bytes ?? payload.sizeBytes ?? plaintext.length);
  if (!Number.isSafeInteger(declaredSize) || declaredSize < 0 || declaredSize !== plaintext.length) {
    throw new Error(`send_file 文件大小已变化：声明 ${declaredSize} bytes，实际 ${plaintext.length} bytes`);
  }
  if (plaintext.length > fileMaxBytes) {
    throw new Error(`send_file 文件超过上限：${plaintext.length} bytes > ${fileMaxBytes} bytes`);
  }

  const rawfilemd5 = createHash('md5').update(plaintext).digest('hex');
  const filekey = randomBytes(16).toString('hex');
  const aesKey = randomBytes(16);
  const cipher = createCipheriv('aes-128-ecb', aesKey, null);
  const encrypted = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  const uploadResponse = await fetchJson(`${state.botBaseUrl}/ilink/bot/getuploadurl`, {
    method: 'POST',
    headers: ilinkHeaders(),
    body: JSON.stringify({
      filekey,
      media_type: 3,
      to_user_id: peerId,
      rawsize: plaintext.length,
      rawfilemd5,
      filesize: encrypted.length,
      no_need_thumb: true,
      aeskey: aesKey.toString('hex'),
      base_info: baseInfo(),
    }),
  });
  const uploadFullUrl = String(uploadResponse.upload_full_url || '').trim();
  const uploadParam = String(uploadResponse.upload_param || '').trim();
  if ((uploadResponse.ret && uploadResponse.ret !== 0) || (!uploadFullUrl && !uploadParam)) {
    throw new Error(`iLink getuploadurl failed response=${JSON.stringify(uploadResponse).slice(0, 500)}`);
  }

  const cdnUrl = uploadFullUrl
    ? new URL(uploadFullUrl)
    : new URL(`${ilinkCdnBaseUrl}/upload`);
  if (!uploadFullUrl) {
    cdnUrl.searchParams.set('encrypted_query_param', uploadParam);
    cdnUrl.searchParams.set('filekey', filekey);
  }
  const downloadParam = await uploadEncryptedFile(cdnUrl, encrypted);
  const clientId = `coolzhu-file-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const response = await fetchJson(`${state.botBaseUrl}/ilink/bot/sendmessage`, {
    method: 'POST',
    headers: ilinkHeaders(),
    body: JSON.stringify({
      msg: {
        from_user_id: '',
        to_user_id: peerId,
        client_id: clientId,
        message_type: 2,
        message_state: 2,
        context_token: contextToken,
        item_list: [{
          type: 4,
          file_item: {
            media: {
              encrypt_query_param: downloadParam,
              aes_key: Buffer.from(aesKey.toString('hex'), 'utf8').toString('base64'),
              encrypt_type: 1,
            },
            file_name: displayName,
            len: String(plaintext.length),
          },
        }],
      },
      base_info: baseInfo(),
    }),
  });
  if (response.ret && response.ret !== 0) {
    throw new Error(`iLink sendmessage file ret=${response.ret} errcode=${response.errcode || ''} errmsg=${response.errmsg || ''}`);
  }
  return { provider_message_id: response.msg_id || response.message_id || clientId };
}

async function handle(req, res) {
  try {
    if (!requireProviderAuth(req)) {
      jsonResponse(res, 401, { error: 'unauthorized' });
      return;
    }
    const url = new URL(req.url, `http://${req.headers.host || `${bindHost}:${bindPort}`}`);
    if (req.method === 'GET' && url.pathname === '/health') {
      jsonResponse(res, 200, {
        provider: 'coolzhu-ilink-provider',
        provider_version: '0.2.0',
        account_id: accountId,
        online: Boolean(state.botToken && state.sessionStarted),
        last_error: state.lastError,
      });
      return;
    }
    if (req.method === 'POST' && url.pathname === '/login/refresh') {
      const body = await readBody(req);
      const report = await refreshLogin(Number(body.now_ms || Date.now()), Number(body.generation || 1));
      jsonResponse(res, 200, report);
      return;
    }
    if (req.method === 'POST' && url.pathname === '/login/logout') {
      if (state.botToken && state.sessionStarted) {
        await notifySession('stop');
      }
      state.botToken = '';
      state.ilinkBotId = null;
      state.ilinkUserId = null;
      state.updatesCursor = '';
      state.longPollTimeoutMs = updatesTimeoutMs;
      state.sessionStarted = false;
      state.contextByPeer.clear();
      jsonResponse(res, 200, { ok: true });
      return;
    }
    if (req.method === 'GET' && url.pathname === '/updates') {
      const updates = await pollUpdates(Number(url.searchParams.get('since_ms') || Date.now()));
      jsonResponse(res, 200, { updates });
      return;
    }
    if (req.method === 'POST' && url.pathname === '/send_text') {
      const body = await readBody(req);
      jsonResponse(res, 200, await sendText(body));
      return;
    }
    if (req.method === 'POST' && url.pathname === '/send_file') {
      const body = await readBody(req);
      jsonResponse(res, 200, await sendFile(body));
      return;
    }
    textResponse(res, 404, 'not found');
  } catch (error) {
    state.lastError = redact(error?.stack || error?.message || error);
    jsonResponse(res, 500, { error: state.lastError });
  }
}

const server = http.createServer((req, res) => {
  handle(req, res);
});

server.listen(bindPort, bindHost, () => {
  console.log(`coolzhu iLink provider listening on http://${bindHost}:${bindPort}`);
  console.log(`iLink upstream: ${ilinkBaseUrl}`);
  console.log(`provider token: ${providerToken ? `SET(len=${providerToken.length})` : '<unset>'}`);
  if (state.botToken) {
    ensureSessionStarted().catch((error) => {
      state.lastError = redact(error?.stack || error?.message || error);
      console.error(`iLink notify start failed: ${state.lastError}`);
    });
  }
});
