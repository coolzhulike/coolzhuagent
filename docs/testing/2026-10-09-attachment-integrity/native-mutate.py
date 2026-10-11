"""仅改写本轮隔离上传对象，记录无模型派发的前后基线。"""
from pathlib import Path
import hashlib,json,sqlite3,urllib.request
p=Path(__file__).resolve().parent
original=(p/'visible-integrity-order.txt').read_bytes()
digest=hashlib.sha256(original).hexdigest()
store=p/'candidate-v2/attachments'
boundary='native-integrity-owned-file'
body=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="visible-integrity-order.txt"\r\nContent-Type: text/plain\r\n\r\n'.encode()+original+f'\r\n--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\ndocument\r\n--{boundary}--\r\n'.encode()
with urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:8768/api/attachments/upload',body,{'Content-Type':'multipart/form-data; boundary='+boundary})) as r:
 assert r.status==200
 uploaded=json.load(r)['attachment']
matches=list(store.glob('sha256-'+digest+'.*'))
assert len(matches)==1, matches
target=matches[0].resolve()
assert target.is_relative_to(store.resolve()) and target.read_bytes()==original
db=p/'candidate-v2/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db) as c:
 counts={t:c.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ('runtime_runs','devin_acp_attempts')}
changed=original+b'\nowned-negative-integrity-change'
target.write_bytes(changed)
(p/'native-negative.json').write_text(json.dumps({'original_sha256':digest,'changed_sha256':hashlib.sha256(changed).hexdigest(),'object':target.name,'counts_before':counts},ensure_ascii=False,indent=2),encoding='utf-8')
print('仅改写隔离上传对象，基线计数已保存')
