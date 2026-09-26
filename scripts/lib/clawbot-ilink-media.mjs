// iLink 图片接收只负责协议解码；视觉理解仍由控制台的统一附件入口处理。
import { createDecipheriv, createHash } from 'node:crypto';

export const MAX_IMAGE_BYTES = 20 * 1024 * 1024;
export const MAX_TOTAL_IMAGE_BYTES = 32 * 1024 * 1024;

function imageType(bytes) {
  if (bytes.subarray(0, 8).equals(Buffer.from('89504e470d0a1a0a', 'hex'))) return ['image/png', 'png'];
  if (bytes.subarray(0, 3).equals(Buffer.from('ffd8ff', 'hex'))) return ['image/jpeg', 'jpg'];
  if (['GIF87a', 'GIF89a'].includes(bytes.subarray(0, 6).toString('ascii'))) return ['image/gif', 'gif'];
  if (bytes.subarray(0, 4).toString('ascii') === 'RIFF' && bytes.subarray(8, 12).toString('ascii') === 'WEBP') return ['image/webp', 'webp'];
  throw new Error('图片格式不受支持或内容已损坏');
}

function mediaKey(image) {
  if (image.aeskey) {
    if (!/^[a-f\d]{32}$/i.test(image.aeskey)) throw new Error('图片 AES 密钥格式无效');
    return Buffer.from(image.aeskey, 'hex');
  }
  if (!image.media?.aes_key) return null;
  const encoded = image.media.aes_key;
  if (!/^[A-Za-z\d+/]+={0,2}$/.test(encoded)) throw new Error('图片 AES 密钥编码无效');
  const decoded = Buffer.from(encoded, 'base64');
  if (decoded.length === 16) return decoded;
  if (decoded.length === 32 && /^[a-f\d]{32}$/i.test(decoded.toString('ascii'))) return Buffer.from(decoded.toString('ascii'), 'hex');
  throw new Error('图片 AES 密钥长度无效');
}

function downloadUrl(image, cdnBaseUrl) {
  const base = new URL(cdnBaseUrl);
  const media = image.media;
  if (!media || (!media.full_url && !media.encrypt_query_param)) throw new Error('图片缺少 CDN 引用');
  const url = media.full_url ? new URL(media.full_url) : new URL(`${base.href.replace(/\/$/, '')}/download`);
  if (!media.full_url) url.searchParams.set('encrypted_query_param', media.encrypt_query_param);
  // 服务端媒体不能把本机请求引向任意地址；自定义 CDN 只允许用户配置的同一 origin。
  const official = url.protocol === 'https:' && url.hostname.endsWith('.cdn.weixin.qq.com') && !url.port;
  if (url.username || url.password || (!official && url.origin !== base.origin)
      || !['http:', 'https:'].includes(url.protocol)) throw new Error('图片 CDN 地址不在允许范围');
  return url;
}

export async function receiveIlinkImages(message, { cdnBaseUrl, timeoutMs = 15000, maxTotalBytes = MAX_TOTAL_IMAGE_BYTES, fetchImpl = fetch } = {}) {
  const images = (message.item_list || message.itemList || []).filter((item) => item.type === 2 || item.image_item);
  if (images.length > 8) return [{ media_id: 'image-limit', error: '每条微信消息最多接收 8 张图片' }];
  let total = 0;
  const refs = [];
  for (const [index, item] of images.entries()) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      if (total >= maxTotalBytes) throw new Error('本批次图片超过累计大小限制');
      const image = item.image_item;
      if (!image) throw new Error('图片结构缺失');
      const key = mediaKey(image);
      const response = await fetchImpl(downloadUrl(image, cdnBaseUrl), { signal: controller.signal, redirect: 'error' });
      if (!response.ok) throw new Error(`图片下载失败（HTTP ${response.status}）`);
      const limit = Math.min(MAX_IMAGE_BYTES, maxTotalBytes - total) + 16;
      if (Number(response.headers.get('content-length') || 0) > limit) throw new Error('图片超过大小限制');
      const chunks = []; let count = 0;
      for await (const chunk of response.body) {
        count += chunk.length;
        if (count > limit) { controller.abort(); throw new Error('图片超过大小限制'); }
        chunks.push(Buffer.from(chunk));
      }
      let bytes = Buffer.concat(chunks);
      if (key) {
        const decipher = createDecipheriv('aes-128-ecb', key, null);
        bytes = Buffer.concat([decipher.update(bytes), decipher.final()]);
      }
      if (!bytes.length || bytes.length > MAX_IMAGE_BYTES || total + bytes.length > maxTotalBytes) throw new Error('图片超过大小限制');
      const [mime, extension] = imageType(bytes);
      total += bytes.length;
      const digest = createHash('sha256').update(bytes).digest('hex');
      refs.push({ media_id: `sha256:${digest}`, file_name: `wechat-image-${index + 1}.${extension}`,
        mime_type: mime, size_bytes: bytes.length, content_base64: bytes.toString('base64') });
    } catch (error) {
      // 不回传 CDN query、AES key 或 fetch 底层 URL；失败会阻止模型假装识图。
      const detail = error?.name === 'AbortError' ? '图片下载超时' : '图片下载、解密或格式校验失败';
      refs.push({ media_id: `failed-image-${index + 1}`, error: detail });
    } finally { clearTimeout(timer); }
  }
  return refs;
}
