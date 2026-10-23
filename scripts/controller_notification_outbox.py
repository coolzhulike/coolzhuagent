"""持久化通知发件箱：结果未知时不重投，不把本地占位当作送达。

接收端没有幂等接口时只能保证本地单次发起；sending/unknown 必须用接收证据核销。
模块不创建任务、不启动监督，也不主动连接任何会话。
"""
import hashlib
from contextlib import contextmanager
import json
from pathlib import Path
import sqlite3
import time


class Outbox:
    def __init__(self, path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.connect() as db:
            db.execute("""CREATE TABLE IF NOT EXISTS notification_outbox (
                key TEXT PRIMARY KEY, message TEXT NOT NULL, message_sha256 TEXT NOT NULL,
                source TEXT NOT NULL, state TEXT NOT NULL,
                receipt_json TEXT, created_at REAL NOT NULL, updated_at REAL NOT NULL)""")

    @contextmanager
    def connect(self):
        db = sqlite3.connect(self.path, timeout=10)
        db.row_factory = sqlite3.Row
        try:
            db.execute("PRAGMA synchronous=FULL")
            with db:
                yield db
        finally:
            db.close()

    def prepare(self, key, message, source):
        if not key or len(key)>512 or not message or len(message.encode('utf-8'))>65536:
            raise ValueError('通知身份或消息大小无效')
        digest=hashlib.sha256(message.encode('utf-8')).hexdigest()
        with self.connect() as db:
            db.execute("INSERT OR IGNORE INTO notification_outbox VALUES(?,?,?,?, 'pending',NULL,?,?)",
                       (key,message,digest,source,time.time(),time.time()))
        return self.read(key)

    def read(self, key):
        with self.connect() as db:
            row=db.execute('SELECT * FROM notification_outbox WHERE key=?',(key,)).fetchone()
        if row is None: raise KeyError(key)
        result=dict(row)
        result['receipt']=json.loads(result.pop('receipt_json') or '{}')
        return result

    def deliver(self, key, sender):
        """sender 只由 pending 的唯一获胜者调用；投递前先完整持久化 sending。"""
        with self.connect() as db:
            updated=db.execute("UPDATE notification_outbox SET state='sending',updated_at=? WHERE key=? AND state='pending'",
                               (time.time(),key)).rowcount
        if updated!=1:
            result=self.read(key)
            return dict(result,replayed=False,needs_reconciliation=result['state'] in ('sending','unknown'))
        row=self.read(key)
        try:
            receipt=sender(row['message'])
            # 只能有已确认的接收端回执；非零退出、超时或连接失败可能发生在送达之后。
            state='queued' if receipt.get('confirmed') is True else 'unknown'
        except Exception as error:
            receipt={'error':type(error).__name__};state='unknown'
        encoded=json.dumps(receipt,ensure_ascii=False)
        if len(encoded.encode('utf-8'))>65536:
            encoded=json.dumps({'error':'receipt_too_large'});state='unknown'
        with self.connect() as db:
            db.execute("UPDATE notification_outbox SET state=?,receipt_json=?,updated_at=? WHERE key=? AND state='sending'",
                       (state,encoded,time.time(),key))
        return dict(self.read(key),replayed=False,needs_reconciliation=state=='unknown')

    def reconcile_received(self,key,message_sha256,evidence):
        """仅宿主核对真实接收记录后调用；不凭超时、PID退出或本地claim判断送达。"""
        if not isinstance(evidence,dict) or not evidence.get('receiver_record_id') or not evidence.get('receiver_source'):
            raise ValueError('缺少接收端记录身份及来源')
        row=self.read(key)
        if row['message_sha256']!=message_sha256:raise ValueError('核销内容不匹配')
        with self.connect() as db:
            db.execute("UPDATE notification_outbox SET state='queued',receipt_json=?,updated_at=? WHERE key=? AND state IN ('sending','unknown')",
                       (json.dumps({'confirmed':True,'reconciled':True,'evidence':evidence},ensure_ascii=False),time.time(),key))
        return self.read(key)
