"""真实终态后仅撤回本轮自有记忆记录，不还原整个会话或删除历史。"""
from pathlib import Path
import json,sqlite3,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[1];base='http://127.0.0.1:8765';sid='session-1791131217833'
receipt=json.loads((p/'owned-memory.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
def get():
    with urllib.request.urlopen(base+'/api/sessions/'+sid+'/beads',timeout=30) as response:return json.load(response)['beads']
current=get()
for owned in receipt['owned']:
    match=next((x for x in current if x['id']==owned['id']),None)
    assert match and match['source']==receipt['source'] and match['summary']==owned['summary'],'记录已被修改，保留待核对'
    request=urllib.request.Request(base+'/api/sessions/'+sid+'/beads/'+owned['id'],method='DELETE')
    with urllib.request.urlopen(request,timeout=30) as response:json.load(response)
after=get();after_ids={x['id'] for x in after}
assert set(receipt['before_ids'])<=after_ids
assert not any(x['source']==receipt['source'] for x in after)
(p/'memory-cleanup.json').write_text(json.dumps({'passed':True,'removed_owned_ids':[x['id'] for x in receipt['owned']],'original_ids_preserved':True,'remaining_count':len(after)},ensure_ascii=False,indent=2),encoding='utf-8')
print('仅本轮自有三项记忆已撤回，原记忆ID均保留')
