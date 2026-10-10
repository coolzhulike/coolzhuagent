from pathlib import Path
import hashlib,json,sqlite3
p=Path(__file__).resolve().parent
facts=json.loads((p/'native-negative.json').read_text())
db=p/'candidate-v2/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db) as c:
 after={t:c.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ('runtime_runs','devin_acp_attempts')}
assert after==facts['counts_before']
target=p/'candidate-v2/attachments'/facts['object']
assert hashlib.sha256(target.read_bytes()).hexdigest()==facts['changed_sha256']
facts.update(counts_after=after,new_runtime_runs=0,new_acp_attempts=0,new_cloud_sessions=0,corrupted_object_unchanged=True,
 ui_error='上传失败：HTTP 500 Internal Server Error: 内容对象摘要不匹配，未覆盖损坏文件',
 ui_scope='原生上传冲突检查；发送前文本与图片读取校验由实际HTTP对照验证。')
(p/'native-negative.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2),encoding='utf-8')
print('实际界面负例：损坏对象保留，模型和运行计数均未增加')
