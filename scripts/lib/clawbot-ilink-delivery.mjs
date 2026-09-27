import { mkdir, open, readFile, rename, stat, unlink } from 'node:fs/promises';
import { dirname } from 'node:path';
import { randomUUID } from 'node:crypto';

// 只持久化已确认游标和账号指纹，不落 token、图片或未授权聊天正文。
export class DeliveryCursor {
  constructor(path, identity) {
    this.path = path; this.identity = identity;
    this.record = { version: 1, identity, sequence: 0, cursor: '', last_acked_delivery_id: null };
    this.pending = null;
  }
  async load() {
    try {
      if ((await stat(this.path)).size > 128 * 1024) throw new Error('微信游标状态过大');
      const record = JSON.parse(await readFile(this.path, 'utf8'));
      if (record.version !== 1 || !Number.isSafeInteger(record.sequence) || record.sequence < 0
          || typeof record.cursor !== 'string' || record.cursor.length > 65536) throw new Error('微信游标状态损坏');
      if (record.identity === this.identity) this.record = record;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    return this;
  }
  async persist(record) {
    await mkdir(dirname(this.path), { recursive: true });
    const temporary = `${this.path}.${randomUUID()}.tmp`;
    const file = await open(temporary, 'wx', 0o600);
    try { await file.writeFile(JSON.stringify(record)); await file.sync(); }
    finally { await file.close(); }
    try { await rename(temporary, this.path); }
    catch (error) { await unlink(temporary).catch(() => {}); throw error; }
    this.record = record;
  }
  async stage(updates, cursor) {
    if (this.pending) return this.pending;
    if (typeof cursor !== 'string' || cursor.length > 65536) throw new Error('微信游标格式无效');
    if (!updates.length) {
      if (cursor !== this.record.cursor) await this.persist({ ...this.record, sequence: this.record.sequence + 1, cursor });
      return { updates: [], delivery_id: null };
    }
    this.pending = { updates, cursor, delivery_id: `${this.record.sequence + 1}:${randomUUID()}` };
    return this.pending;
  }
  async acknowledge(deliveryId) {
    if (typeof deliveryId !== 'string' || deliveryId.length > 128) throw new Error('微信批次确认身份无效');
    if (deliveryId === this.record.last_acked_delivery_id) return;
    if (!this.pending || deliveryId !== this.pending.delivery_id) throw new Error('微信批次确认已过期或不属于当前账号');
    await this.persist({ ...this.record, sequence: this.record.sequence + 1,
      cursor: this.pending.cursor, last_acked_delivery_id: deliveryId });
    this.pending = null;
  }
}
