from pathlib import Path
import sqlite3,json
p=Path(__file__).resolve().parent;root=p.parents[2]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 row=c.execute("SELECT attachments_json FROM chat_room_messages WHERE content LIKE 'ATTACHMENT-BROWSER-INSTALLED124-20261009%' ORDER BY created_at DESC LIMIT 1").fetchone()
 record=[{'name':x['name'],'snapshot_metadata':{k:v for k,v in x.get('text_snapshot',{}).items() if k!='text'}} for x in json.loads(row[0])]
(p/'snapshot-metadata.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(record,ensure_ascii=False))
