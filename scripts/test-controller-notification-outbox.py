"""隔离接收端+实际子进程崩溃；不投递Codex消息。"""
import hashlib,json,os
from contextlib import closing
from pathlib import Path
import sqlite3,subprocess,sys,tempfile,unittest
from controller_notification_outbox import Outbox


def receive(root,message):
    # 故意不在接收端加唯一约束；重复发送会成为第二行，不能由夹具替产品去重。
    with closing(sqlite3.connect(root/'receiver.sqlite3')) as db:
        with db:
            db.execute('CREATE TABLE IF NOT EXISTS inbox(id INTEGER PRIMARY KEY,message TEXT,sha TEXT)')
            db.execute('INSERT INTO inbox(message,sha) VALUES(?,?)',(message,hashlib.sha256(message.encode()).hexdigest()))
    return {'confirmed':True}


class CrashRecovery(unittest.TestCase):
    def test_real_crash_after_receiver_commit_does_not_resend(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TEMP')) as directory:
            root=Path(directory);key='controller/round-unique';message='隔离通知验收'
            outbox=Outbox(root/'outbox.sqlite3')
            outbox.prepare(key,message,'executor')
            # 模拟准备后重启：pending 是尚未尝试的状态，可安全继续。
            child=subprocess.run([sys.executable,__file__,'--crash-child',str(root),key],capture_output=True)
            self.assertEqual(child.returncode,86)
            resumed=Outbox(root/'outbox.sqlite3')
            resumed.prepare(key,'另一通道的同轮消息','controller')
            result=resumed.deliver(key,lambda text:receive(root,text))
            self.assertEqual(result['state'],'sending');self.assertTrue(result['needs_reconciliation'])
            with closing(sqlite3.connect(root/'receiver.sqlite3')) as db:rows=db.execute('SELECT id,message,sha FROM inbox').fetchall()
            self.assertEqual(len(rows),1);self.assertEqual(rows[0][1],message)
            proof={'receiver_record_id':str(rows[0][0]),'receiver_source':str(root/'receiver.sqlite3')}
            result=resumed.reconcile_received(key,rows[0][2],proof)
            self.assertEqual(result['state'],'queued')
            resumed.deliver(key,lambda text:receive(root,text))
            with closing(sqlite3.connect(root/'receiver.sqlite3')) as db:self.assertEqual(db.execute('SELECT count(*) FROM inbox').fetchone()[0],1)
            evidence={'passed':True,'sender_exit':86,'receiver_deliveries':1,'pre_reconcile_state':'sending','pre_reconcile_claimed_success':False,'recovered_state':result['state'],'receiver_has_dedupe':False,'real_codex_messages_sent':0}
            output=os.environ.get('NOTIFICATION_KEY_EVIDENCE')
            if output:Path(output).write_text(json.dumps(evidence,ensure_ascii=False,indent=2),encoding='utf-8')

    def test_transport_unknown_is_retained_without_retry(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TEMP')) as directory:
            box=Outbox(Path(directory)/'outbox.sqlite3');box.prepare('round','message','executor')
            calls=[]
            def sender(message):calls.append(message);raise TimeoutError()
            self.assertEqual(box.deliver('round',sender)['state'],'unknown')
            self.assertTrue(box.deliver('round',sender)['needs_reconciliation'])
            self.assertEqual(calls,['message'])


if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='--crash-child':
        root=Path(sys.argv[2]);box=Outbox(root/'outbox.sqlite3')
        def crash(message):receive(root,message);os._exit(86)
        box.deliver(sys.argv[3],crash)
    else:unittest.main()
