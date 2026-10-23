"""归档本轮验收证据，清理已核验的重复备份。"""
import collections
import datetime
import hashlib
import json
import pathlib
import shutil

root = pathlib.Path(r'D:\coolzhuagent').resolve()
stage = root / 'tmp/goal-32wbs-acceptance-20261013'
archive = root / 'docs/testing/2026-10-16-acceptance-evidence'
archive.mkdir(exist_ok=True)

def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()

def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

cleanup = {'state': 'retained', 'deleted_bytes': 0}
duplicate = stage / 'cold-start-20261016/bounded-backup.sqlite3'
rollback = stage / 'cold-start-20261016/session-before.sqlite3'
if duplicate.exists():
    checked = duplicate.resolve(strict=True)
    if not checked.is_relative_to(stage.resolve()) or duplicate.is_symlink() or duplicate.is_junction():
        raise RuntimeError('重复备份路径校验失败')
    same = digest(duplicate) == digest(rollback)
    cleanup.update(path=str(checked), rollback=str(rollback), identical_sha256=same)
    if same:
        cleanup.update(state='deleted_duplicate', deleted_bytes=duplicate.stat().st_size, sha256=digest(duplicate))
        duplicate.unlink()
save(stage / 'controller-cleanup-20261016.json', cleanup)

ledger_path = stage / 'acceptance-checklist.json'
ledger = json.loads(ledger_path.read_text(encoding='utf-8-sig'))
ledger['currentReview']['installedRuntime'] = ('2026-10-16诊断恢复：0.2.134，正式安装目录后台与Tauri可见；'
    'D工作区/原数据库11表逐行一致。PID是瞬时观察值，不作为跨轮身份。参见controller-cold-start-20261016.json。')
ledger['currentReview']['evidenceArchive'] = str(archive.relative_to(root)).replace('\\', '/')
ledger['currentReview']['bindingObserved'] = 'buttercup-pink（恢复启动前已存在）；本轮未改绑、未派发Devin。'
save(ledger_path, ledger)

files = [
    'acceptance-checklist.json', 'controller-35-item-review-20261016.json',
    'controller-current-full-20261016.json', 'controller-current-frontend-20261016.log',
    'controller-current-console-full-20261016.log', 'controller-reply-http-20261016.json',
    'controller-runtime-history-20261016.json', 'controller-cold-start-20261016.json',
    'controller-cleanup-20261016.json', 'controller-takeover-20261016.py',
    'controller-reply-http-20261016.py', 'controller-runtime-review-20261016.py',
    'controller-startup-20261016.py', 'controller-ledger-review-20261016.py',
    'controller-archive-20261016.py',
]
manifest = {'generated_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'state': 'completed', 'files': [], 'counts': dict(collections.Counter(x['status'] for x in ledger['items'])),
    'scope': '历史JSON保留35项；当前执行计划不含历史豁免项；局部通过不代表整项验收。'}
for name in files:
    src, dst = stage / name, archive / name
    shutil.copyfile(src, dst)
    source_hash, target_hash = digest(src), digest(dst)
    if source_hash != target_hash:
        raise RuntimeError('归档哈希不一致：' + name)
    manifest['files'].append({'path': name, 'sha256': target_hash, 'bytes': dst.stat().st_size})
save(archive / 'manifest.json', manifest)

plan = (root / 'docs/testing/2026-10-16-active-acceptance-plan.md').read_text(encoding='utf-8-sig')
checklist = (root / 'docs/testing/2026-10-13-goal-32wbs-acceptance-checklist.md').read_text(encoding='utf-8-sig')
assert '| 3.5 ' not in plan and 'Paint' not in plan
assert '| 3.5 ' not in checklist and '| EX-01 ' not in plan and '| 5.3 ' not in plan
print(json.dumps({'counts': manifest['counts'], 'archive_files': len(files), 'cleanup': cleanup,
    'plan_excludes_waived_and_accepted': True}, ensure_ascii=False))
