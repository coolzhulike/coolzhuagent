from pathlib import Path
import hashlib, json, subprocess, datetime
import jsonschema

root=Path.cwd()
out=root/'tmp/2026-10-09-package-file-sbom'
source=root/'docs/testing/release-0.2.125/evidence/build-identity/pkg-report-release-20261009-153059491-1bd127ea'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def read(path): return json.loads(path.read_text(encoding='utf-8-sig'))
report=read(source/'package-report.json')
inventory=read(source/'payload-inventory.json')
bom_path=out/'coolzhuagent-0.2.125.files.cdx.json'
bom=read(bom_path)
schema_path=root/'tmp/analysis-bom-1.6.schema.json'
schema=read(schema_path)
jsonschema.Draft7Validator.check_schema(schema)
errors=sorted(jsonschema.Draft7Validator(schema, format_checker=jsonschema.FormatChecker()).iter_errors(bom),key=lambda e:list(map(str,e.path)))
assert not errors, '\n'.join(str(e) for e in errors[:5])
component=bom['metadata']['component']
files=component['components']
assert component['version']==report['build_context']['release_version']=='0.2.125'
assert bom['compositions'][0]['aggregate']=='incomplete'
assert 'dependencies' not in bom
properties={p['name']:p['value'] for p in bom['metadata']['properties']}
assert properties['coolzhu:report:file_sha256']==sha(source/'package-report.json')
assert properties['coolzhu:inventory:file_sha256']==sha(source/'payload-inventory.json')
assert properties['coolzhu:build:source_commit']==report['build_context']['source_commit']
assert properties['coolzhu:build:target']==report['build_context']['build_target']
assert len(files)==inventory['file_count']==1159
actual={c['name']:(int(c['properties'][0]['value']),c['hashes'][0]['content']) for c in files}
expected={f['path']:(f['length'],f['sha256']) for f in inventory['files']}
assert actual==expected and len({c['bom-ref'] for c in files})==len(files)
assert component['bom-ref'] not in {c['bom-ref'] for c in files}
installed=Path('C:/Program Files/CoolzhuAgent')
checked=[]
for path,(length,digest) in expected.items():
    f=installed/path
    assert f.is_file(), f'已安装文件不存在：{path}'
    assert f.stat().st_size==length and sha(f)==digest, f'已安装字节不一致：{path}'
    checked.append({'path':path,'length':length,'sha256':digest,'matched':True})

# 仅修改自有清单副本；产品必须在落盘前拒绝它，不篡改正式归档报告。
bad=read(source/'payload-inventory.json')
bad['files'][0]['sha256']='0'*64
bad_path=out/'invalid-inventory.json'
bad_path.write_text(json.dumps(bad,ensure_ascii=False),encoding='utf-8')
negative_path=out/'must-not-be-produced.json'
assert not negative_path.exists()
args=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File','scripts/export-package-sbom.ps1',
      '-ReportPath',str(source/'package-report.json'),'-PayloadInventoryPath',str(bad_path),'-OutputPath',str(negative_path)]
negative=subprocess.run(args,capture_output=True,cwd=root)
def decode(data):
    for encoding in ['utf-8','gb18030']:
        try:return data.decode(encoding)
        except UnicodeDecodeError:pass
    return data.decode('utf-8',errors='replace')
(out/'invalid-export.stdout.log').write_bytes(negative.stdout)
(out/'invalid-export.stderr.log').write_bytes(negative.stderr)
assert negative.returncode!=0 and not negative_path.exists(), '错误清单不应产生SBOM'
assert '载荷文件集合' in decode(negative.stderr), decode(negative.stderr)

before=sha(bom_path)
overwrite=subprocess.run(args[:-2]+['-OutputPath',str(bom_path)],capture_output=True,cwd=root)
(out/'overwrite.stdout.log').write_bytes(overwrite.stdout)
(out/'overwrite.stderr.log').write_bytes(overwrite.stderr)
assert overwrite.returncode!=0 and sha(bom_path)==before
assert '输出已存在' in decode(overwrite.stderr), decode(overwrite.stderr)
receipt={
 'recorded_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'schema':{'url':'https://cyclonedx.org/schema/bom-1.6.schema.json','sha256':sha(schema_path),'validation':'passed'},
 'sbom_sha256':sha(bom_path),'source_report_sha256':sha(source/'package-report.json'),
 'source_inventory_sha256':sha(source/'payload-inventory.json'),
 'source_commit':report['build_context']['source_commit'],'file_count':len(files),
 'total_bytes':sum(v[0] for v in expected.values()),'installed_root':str(installed),
 'installed_matches':len(checked),'installed_files':checked,
 'tampered_inventory':{'exit_code':negative.returncode,'output_absent':True,'original_inputs_unchanged':True},
 'existing_output':{'exit_code':overwrite.returncode,'sha256_unchanged':True},
 'completeness':'incomplete','dependencies_claimed':False,'passed':True}
(out/'validation-result.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in receipt.items() if k!='installed_files'},ensure_ascii=False))
