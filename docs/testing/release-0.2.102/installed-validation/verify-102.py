"""核对正常发布产物与实际安装文件，不伪造构建收据。"""
import hashlib,json,pathlib,shutil,subprocess,sys
repo=pathlib.Path(__file__).resolve().parents[2]
out=pathlib.Path(__file__).resolve().parent
version='0.2.102'
package=repo/'package'
expected='95e01409fe4736421a0d4a576a64ab8722928061'
def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
inventory=json.loads((package/'payload-inventory.json').read_text(encoding='utf-8-sig'))
producer=json.loads((repo/inventory['report_ref']['report_path']).read_text(encoding='utf-8-sig'))
assert producer['release_eligible'] is True
gates=producer['release_eligibility']['gates']
assert len(gates)==6 and all(g['result']=='pass' for g in gates)
assert producer['release_eligibility']['source_snapshot_digest']==inventory['build_identity']['source_snapshot_digest']
installed=len(sys.argv)>1 and sys.argv[1]=='installed'
target=pathlib.Path('C:/Program Files/CoolzhuAgent') if installed else package
for item in inventory['files']:
    file=target/item['path']
    assert file.stat().st_size==item['length'] and sha(file)==item['sha256'],item['path']
report=json.loads((repo/f'dist/CoolzhuAgent-{version}-installer-report.json').read_text(encoding='utf-8-sig'))
msi=repo/f'dist/CoolzhuAgent-{version}.msi'
assert report['version']==version and report['configuration']=='release'
assert sha(msi)==report['sha256'].lower()
assert inventory['vcs']['source_commit']==expected
safety=json.loads((repo/f'dist/CoolzhuAgent-{version}-package-safety.json').read_text(encoding='utf-8-sig'))
assert safety['safe'] is True
cli=subprocess.check_output([str(target/'bin/coolzhu-cli.exe'),'--version']).decode('utf-8')
assert 'Version          '+version in cli
facts={'stage':'正式安装文件摘要通过，模型功能另验' if installed else '正式发布链出包，尚未安装',
    'version':version,'source_commit':inventory['vcs']['source_commit'],
    'source_snapshot_digest':inventory['build_identity']['source_snapshot_digest'],
    'verified_file_count':len(inventory['files']),'msi_sha256':sha(msi),'msi_bytes':msi.stat().st_size,
    'web_sha256':sha(target/'bin/coolzhu-web-console.exe'),
    'shell_sha256':sha(target/'bin/coolzhu-tauri-shell.exe'),'cli_output':cli,'package_safe':True,
    'release_gates':[{'gate':g['gate'],'result':g['result']} for g in gates],
    'producer_report_ref':inventory['report_ref']}
(out/('installed-102-verification.json' if installed else 'package-102-verification.json')).write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(facts,ensure_ascii=False))
