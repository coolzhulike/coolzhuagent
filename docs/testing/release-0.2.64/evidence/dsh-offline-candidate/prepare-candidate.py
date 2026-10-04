"""建立独立候选清单和固定资源副本，不替换旧包或共享资源。"""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess

repo = Path.cwd()
root = repo / 'tmp/2026-10-02-dsh-msi'
root.mkdir(parents=True, exist_ok=True)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def git(*args):
    return subprocess.check_output(['git', *args], text=True, encoding='utf-8')
baseline = {
    'branch': git('branch', '--show-current').strip(),
    'head_reference': git('rev-parse', 'HEAD').strip(),
    'source_authority': '后续冻结的实际工作区源码快照；HEAD不是本轮未提交源码的权威',
    'status': git('status', '--porcelain'),
    'prior_delivery': {str(p.relative_to(repo)): {'size': p.stat().st_size, 'sha256': sha(p)}
                       for p in (repo/'dist').glob('CoolzhuAgent-0.2.63*') if p.is_file()},
    'preserved_document_sha256': sha(repo/'docs/testing/pr69-browser-market/native-followup/release-0.2.29-handoff.md'),
}
(root/'baseline.json').write_text(json.dumps(baseline, ensure_ascii=False, indent=2), encoding='utf-8')
(root/'baseline-diff.patch').write_bytes(subprocess.check_output(['git', 'diff', '--binary']))
source = repo/'tmp/2026-10-02-dsh-dispatch/bin/dsh-runtime'
destination = root/'fixed-runtime-source'
if destination.exists():
    raise RuntimeError('独立资源目录已存在，拒绝覆盖')
shutil.copytree(source, destination)
manifest_path = repo/'config/package-manifest-0.2.64-candidate.json'
if manifest_path.exists():
    raise RuntimeError('候选清单已存在，拒绝覆盖')
original = json.loads((repo/'config/package-manifest.json').read_text(encoding='utf-8'))
manifest = json.loads(json.dumps(original))
for resource in manifest['resources']:
    if resource['id'] == 'tooling.dsh-runtime':
        resource['source'] = 'tmp/2026-10-02-dsh-msi/fixed-runtime-source'
    if resource['id'] == 'package.manifest':
        resource['source'] = 'config/package-manifest-0.2.64-candidate.json'
for dependency in manifest['source_snapshot']['external_path_dependencies']:
    if dependency['path'] == 'tmp/dsh-runtime/windows-x64':
        dependency['path'] = 'tmp/2026-10-02-dsh-msi/fixed-runtime-source'
manifest['build_inputs']['files'].append('config/package-manifest-0.2.64-candidate.json')
manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
(root/'manifest-override.json').write_text(json.dumps({
    'base_manifest_sha256': sha(repo/'config/package-manifest.json'),
    'candidate_manifest_sha256': sha(manifest_path),
    'changes': ['独立固定资源source', '包内清单source指向实际候选清单', '外部固定资源声明对应source', '构建输入增加候选清单'],
    'unchanged': '全部artifact、安装target、源码roots、release gates和禁止规则',
}, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps({'candidate_version': '0.2.64', 'manifest': str(manifest_path), 'resource_source': str(destination)}, ensure_ascii=False))
