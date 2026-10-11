"""独立核对GitHub资产摘要、发布属性和实际标签。"""
from pathlib import Path
import hashlib,json,subprocess,sys
p=Path(__file__).resolve().parent;repo=p.parents[1]
def gh(*args):return json.loads(subprocess.check_output(['python','tmp/2026-10-05-devin-host/with-gh.py',*args]))
releases=gh('api','repos/coolzhulike/coolzhuagent/releases?per_page=20')
matches=[r for r in releases if r['tag_name']=='v0.2.122'];assert len(matches)==1
metadata=matches[0];assert metadata['prerelease'] and metadata['target_commitish']=='8edfa3ab87f411085e22975de14e007c6df52312'
expected=json.loads((p/'release-assets-local.json').read_text())
assert len(metadata['assets'])==4
for local in expected:
    remote=next(a for a in metadata['assets'] if a['name']==local['name'])
    assert remote['state']=='uploaded' and remote['size']==local['size'] and remote['digest']=='sha256:'+local['sha256']
if len(sys.argv)>1:
    assert not metadata['draft']
    tag=gh('api','repos/coolzhulike/coolzhuagent/git/ref/tags/v0.2.122')
    assert tag['object']['type']=='commit' and tag['object']['sha']=='8edfa3ab87f411085e22975de14e007c6df52312'
    for dest in [p,repo/'docs/testing/release-0.2.122/installed-validation']:
        (dest/'github-published-metadata.json').write_text(json.dumps(metadata,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        (dest/'github-tag.json').write_text(json.dumps(tag,indent=2)+'\n',encoding='utf-8')
(p/'release-id.json').write_text(json.dumps({'id':metadata['id'],'tag':metadata['tag_name'],'draft':metadata['draft']})+'\n')
print('四资产服务器长度/SHA一致；'+('已公开且标签源码一致' if len(sys.argv)>1 else '草稿核验通过'))
