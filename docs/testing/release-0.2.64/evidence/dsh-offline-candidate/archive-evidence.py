"""归档候选包证据，并核对工作区、旧安装和资源未被替换。"""
from pathlib import Path
import datetime
import hashlib
import json
import shutil
import subprocess

repo = Path.cwd()
task = repo/'tmp/2026-10-02-dsh-msi'
target = repo/'docs/testing/release-0.2.64/evidence/dsh-offline-candidate'
target.mkdir(parents=True, exist_ok=True)
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))
def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
def git(*args):
    return subprocess.check_output(['git', *args], text=True, encoding='utf-8').strip()
artifacts = []
def archive(source, name, purpose):
    destination = target/name
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    assert sha(source) == sha(destination)
    artifacts.append({'file':destination.relative_to(target).as_posix(),
                      'source':source.relative_to(repo).as_posix(), 'size':destination.stat().st_size,
                      'sha256':sha(destination), 'purpose':purpose})
pointer = read(task/'package-pointer.json')
preparation = read(task/'prepare-pointer.json')
source_record = repo/pointer['source_snapshot_freeze_record']
snapshot = read(source_record)
report = read(repo/pointer['report_path'])
baseline = read(task/'baseline.json')
offline = read(task/'offline-msi-verification.json')
assert offline['verified'] and read(task/'build-outcome.json')['outcome'] == 'completed'
assert git('rev-parse','HEAD') == baseline['head_reference']
assert git('branch','--show-current') == baseline['branch']
changed = []
for entry in snapshot['entries']:
    path = repo/entry['path']
    if not path.is_file() or path.stat().st_size != entry['length'] or sha(path) != entry['sha256']:
        changed.append(entry['path'])
assert not changed, changed
prior_delivery = []
for relative, expected in baseline['prior_delivery'].items():
    path = repo/relative
    assert path.stat().st_size == expected['size'] and sha(path) == expected['sha256'], relative
    prior_delivery.append({'file':relative, **expected, 'unchanged':True})
phase3 = read(repo/'docs/testing/dsh-web-dispatch-2026-10-02/evidence-index.json')
for entry in phase3['sources']:
    assert sha(repo/entry['file']) == entry['sha256'], entry['file']
for entry in phase3['original_incident_references']:
    assert sha(repo/entry['file']) == entry['sha256'], entry['file']
baseline_lines = (repo/'tmp/2026-10-02-dsh-source/baseline-status.txt').read_text(encoding='utf-8-sig').splitlines()
original = [line[3:].strip('"') for line in baseline_lines if line.startswith('?? ')]
assert len(original) == 165 and not [p for p in original if not (repo/p).exists()]
document = repo/'docs/testing/pr69-browser-market/native-followup/release-0.2.29-handoff.md'
assert sha(document) == baseline['preserved_document_sha256'] == 'c61b4f5cf565c5da15b70b40a6c6bfde48507e7cb2cc4e2fd03c2e348f034079'
installed = read(task/'installed-readonly.json')
assert installed['build_version'] == '09dc04d0f812 · 2026-10-01'
assert installed['input_safety_recovery']['resource_state'] == 'isolated'
assert installed['input_safety_recovery']['accepts_new_input'] is False
assert installed['input_safety_recovery']['unacknowledged_open_blocks'] == 1
assert read(task/'incident-executor-readonly.json')['creation_time_filetime'] == 134353335372030593
prior_installed = read(repo/'docs/testing/dsh-web-dispatch-2026-10-02/installed-hashes-readonly.json')
current_installed = read(task/'installed-hashes-readonly.json')
assert isinstance(prior_installed, list) and isinstance(current_installed, list)
assert {p['Path'].replace('\\','/').lower():p['Hash'].lower() for p in prior_installed} == {p['path'].replace('\\','/').lower():p['sha256'] for p in current_installed}
roots = snapshot['scope']['roots']
source_patch = task/'working-tree-source.patch'
source_patch.write_bytes(subprocess.check_output(['git','diff','--binary','--',*roots]))
index_patch = task/'index-source.patch'
index_patch.write_bytes(subprocess.check_output(['git','diff','--cached','--binary','--',*roots]))
scoped_paths = {entry['path'] for entry in snapshot['entries']}
untracked_paths = sorted(set(git('ls-files','--others','--exclude-standard').splitlines()) & scoped_paths)
for relative in untracked_paths:
    archive(repo/relative, 'untracked-source/'+relative, '冻结快照中新增未跟踪源码/候选清单，配合源码patch及HEAD参考核对')
runtime_files = [{**f,'path':f['path'].removeprefix('bin/dsh-runtime/')} for f in offline['files'] if f['path'].startswith('bin/dsh-runtime/')]
write(task/'runtime-files.json', {'verification':offline['dsh_runtime'], 'files':runtime_files})
for name, purpose in {
    'prepare.log':'准备阶段解析与输入冻结',
    'build.log':'正式冻结输入release构建与MSI日志',
    'prepare-pointer.json':'准备冻结指针', 'prepare-outcome.json':'准备实际结果',
    'package-pointer.json':'本次独立包根绑定报告指针', 'build-outcome.json':'构建实际起止和结果',
    'baseline.json':'开始时Git/旧0.2.63文件身份', 'manifest-override.json':'独立候选清单改变范围',
    'source-runtime-verification.json':'复制前固定资源完整核验',
    'offline-msi-verification.json':'只读MSI/CAB全文件及安装布局核验',
    'offline-inspection-first.log':'首次离线检查短/长目录名解析失败，不替代最终结果',
    'archive-first.log':'首次归档旧摘要字段名不匹配，修正后最终保留核验通过',
    'offline-inspection-final.log':'最终离线完整核验',
    'extracted-runtime-verifier.txt':'实际MSI解包运行时完整资源再核验',
    'manifest-contract-final.log':'清单契约检查最终子进程输出',
    'manifest-contract-final-outcome.json':'清单契约检查子进程退出0',
    'final-checks.json':'最终diff检查退出0',
    'runtime-files.json':'MSI中的完整290固定资源文件大小与SHA256',
    'installed-readonly.json':'正式API只读版本与隔离状态',
    'incident-executor-readonly.json':'原事故执行者存活创建身份',
    'installed-hashes-readonly.json':'正式二进制未替换',
    'working-tree-source.patch':'本轮包实际未提交已跟踪源码差异',
    'index-source.patch':'索引源码差异；本轮为空',
    'prepare-candidate.py':'独立候选资源/清单建立脚本',
    'build-candidate.ps1':'本轮准备与构建入口/编译环境',
    'inspect-msi.py':'只读MSI/CAB核验实现',
    'archive-evidence.py':'证据归档与保留核验实现',
}.items():
    archive(task/name, name.replace('.log','.txt'), purpose)
for path in sorted(task.glob('expand-*.txt')):
    archive(path, path.name, 'Windows原生CAB只解压输出；未执行MSI安装')
archive(source_record, 'source-snapshot-freeze.json', '完整1280个源码文件及范围/逐文件SHA256')
archive(repo/preparation['freeze_record'], 'input-freeze.json', '正式构建消费的48项冻结输入')
archive(repo/preparation['prepare_report'], 'prepare-report.json', '非发布准备记录')
archive(repo/pointer['report_path'], 'package-report.json', '有内容身份的实际候选包报告')
archive(repo/'tmp/candidate-064-package/payload-inventory.json', 'payload-inventory.json', '完整1149文件载荷表，不含自引用载体自身')
archive(repo/'dist/CoolzhuAgent-0.2.64-installer-report.json', 'installer-report.json', 'MSI/包根/报告/快照绑定')
archive(repo/'dist/CoolzhuAgent-0.2.64-package-safety.json', 'package-safety.json', '候选包安全扫描1150文件、0发现')
archive(repo/'config/dsh-runtime-lock.json', 'dsh-runtime-lock.json', '受审查第三方完整文件表和官方来源锁')
archive(repo/'config/package-manifest-0.2.64-candidate.json', 'candidate-package-manifest.json', '实际候选清单，全部原发布门禁保留')
archive(repo/'docs/testing/release-0.2.64/change-report-and-targeted-test-plan.md', 'change-report-and-targeted-test-plan.md', '候选结果与未完成真实验收边界')
archive(repo/'docs/work-logs/2026-10-02-local-session-dsh-msi-candidate.md', 'work-log.md', '本轮中文工作记录')
preservation = {'head':baseline['head_reference'],'branch':baseline['branch'],
                'original_untracked_entries':165,'missing_original_entries':[],
                'original_document_sha256':sha(document), 'prior_delivery':prior_delivery,
                'phase3_sources_unchanged':len(phase3['sources']),
                'source_snapshot_files_reverified':len(snapshot['entries']),
                'formal_installed_binaries_unchanged':True,
                'incident_references_unchanged':phase3['original_incident_references']}
write(target/'evidence-index.json', {
    'generated_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'candidate_only':True,'version':'0.2.64','configuration':'release',
    'msi':{'file':'dist/CoolzhuAgent-0.2.64.msi','size':offline['msi_size'],'sha256':offline['msi_sha256'],'signed':False},
    'source_authority':'实际未提交工作区源码冻结快照；HEAD仅参考，不声明等同干净提交',
    'build_identity':report['build_identity'],'release_engineering_gates':report['release_eligibility']['gates'],
    'artifacts':artifacts,'preservation':preservation,
    'not_verified':['真实安装/升级/卸载','正式启用按钮与原生审批UI','真实Qwen父轮工具回包','Goal缺接纳身份','Browser竞态边界','完整Paint','动画/泛光/总体'],
})
assert all(sha(target/a['file']) == a['sha256'] for a in artifacts)
print(json.dumps({'artifacts':len(artifacts),'untracked_source_files':len(untracked_paths),'source_files_reverified':len(snapshot['entries']),'prior_delivery_unchanged':len(prior_delivery),'msi':offline['msi_sha256']},ensure_ascii=False))
