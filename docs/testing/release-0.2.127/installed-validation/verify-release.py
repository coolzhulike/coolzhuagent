"""发布前后独立校验五资产服务器摘要与完整源码标签。"""
from pathlib import Path
import json,subprocess,sys,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
def gh(*args):return json.loads(subprocess.check_output(['python','tmp/2026-10-05-devin-host/with-gh.py',*args],cwd=root))
expected=json.loads((p/'release-assets-local.json').read_text())
source=(p/'source-commit.txt').read_text().strip()
releases=gh('api','repos/coolzhulike/coolzhuagent/releases?per_page=20')
matches=[r for r in releases if r['tag_name']=='v0.2.127'];assert len(matches)==1
m=matches[0];assert m['prerelease'] and m['target_commitish']==source
assert len(m['assets'])==len(expected)==5
for local in expected:
    with Path(local['path']).open('rb') as s:assert hashlib.file_digest(s,'sha256').hexdigest()==local['sha256']
    a=next(x for x in m['assets'] if x['name']==local['name'])
    assert a['state']=='uploaded' and a['size']==local['size'] and a['digest']=='sha256:'+local['sha256'],a['name']
(p/'release-id.json').write_text(json.dumps({'id':m['id'],'tag':m['tag_name'],'draft':m['draft']})+'\n',encoding='utf-8')
if len(sys.argv)>1:
    assert not m['draft']
    tag=gh('api','repos/coolzhulike/coolzhuagent/git/ref/tags/v0.2.127')
    assert tag['object']['type']=='commit' and tag['object']['sha']==source
    out=root/'docs/testing/release-0.2.127/installed-validation'
    for d in (p,out):
        (d/'github-published-metadata.json').write_text(json.dumps(m,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        (d/'github-tag.json').write_text(json.dumps(tag,indent=2)+'\n',encoding='utf-8')
    (out/'verify-release.py').write_bytes(Path(__file__).read_bytes())
    (out/'release-notes.md').write_bytes((p/'release-notes.md').read_bytes())
    print(json.dumps({'published':True,'assets_verified':5,'source_tag':source,'url':m['html_url']}))
else:print(json.dumps({'draft_verified':True,'id':m['id'],'assets_verified':5}))
