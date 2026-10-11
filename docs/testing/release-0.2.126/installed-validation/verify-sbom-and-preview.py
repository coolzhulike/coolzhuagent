"""正式126已安装字节、文件级SBOM与有界预览的独立核对。"""
from pathlib import Path
import hashlib,json,urllib.request,urllib.parse,jsonschema,datetime
root=Path(__file__).resolve().parents[2];p=Path(__file__).resolve().parent
def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def sha(data):return hashlib.sha256(data).hexdigest()
installed=read(p/'installed-126-verification.json')
assert installed['version']=='0.2.126' and installed['package_safe']
inventory=read(root/'package/payload-inventory.json')
report_path=root/inventory['report_ref']['report_path'];report=read(report_path)
schema_path=root/'tmp/analysis-bom-1.6.schema.json'
schema=read(schema_path);jsonschema.Draft7Validator.check_schema(schema)
bom_path=p/'coolzhuagent-0.2.126.files.cdx.json';bom=read(bom_path)
jsonschema.Draft7Validator(schema,format_checker=jsonschema.FormatChecker()).validate(bom)
assert bom['metadata']['component']['version']=='0.2.126'
assert bom['compositions'][0]['aggregate']=='incomplete' and 'dependencies' not in bom
props={x['name']:x['value'] for x in bom['metadata']['properties']}
assert props['coolzhu:report:file_sha256']==sha(report_path.read_bytes())
assert props['coolzhu:inventory:file_sha256']==sha((root/'package/payload-inventory.json').read_bytes())
assert props['coolzhu:build:source_commit']==installed['source_commit']
expected={f['path']:(f['length'],f['sha256']) for f in inventory['files']}
actual={f['name']:(int(f['properties'][0]['value']),f['hashes'][0]['content']) for f in bom['metadata']['component']['components']}
assert actual==expected
for name,(length,digest) in expected.items():
    f=Path('C:/Program Files/CoolzhuAgent')/name;raw=f.read_bytes()
    assert len(raw)==length and sha(raw)==digest,name
workspace=root/'tmp/2026-10-04-devin-models/workspace'
name='sbom-0.2.126-files.cdx.json';copy=workspace/name
assert not copy.exists()
raw=bom_path.read_bytes();copy.write_bytes(raw)
api='http://127.0.0.1:8765';evidence=p/'preview-api';evidence.mkdir(exist_ok=True)
def get(label,path):
    with urllib.request.urlopen(api+path,timeout=30) as r:data=r.read()
    (evidence/(label+'.json')).write_bytes(data)
    return json.loads(data)
current=get('workspace','/api/workspace');assert Path(current['workspace']).resolve()==workspace.resolve()
query='path='+urllib.parse.quote(name)
meta=get('meta','/api/project/file/meta?'+query)
assert meta['previewable'] and not meta['editable'] and not meta['binary']
assert meta['file_size']==len(raw) and meta['revision']==sha(raw)
offset=0;joined=b'';pages=[]
while True:
    page=get('byte-page-'+str(len(pages)+1),'/api/project/file?'+query+f'&offset={offset}&limit=262144')
    data=page['content'].encode();assert len(data)==page['bytes_read']<=262144
    assert page['offset']==offset and page['meta']['revision']==sha(raw)
    pages.append({'offset':offset,'bytes':len(data),'next_offset':page['next_offset']});joined+=data
    if page['next_offset'] is None:break
    assert page['next_offset']>offset;offset=page['next_offset']
assert joined==raw
lines=raw.decode().splitlines()
tail=get('tail-lines','/api/project/file?'+query+f'&start_line={len(lines)-400}&end_line={len(lines)+400}')
assert tail['total_lines']==len(lines) and tail['end_line']==len(lines)
assert tail['content']=='\n'.join(lines[tail['start_line']-1:tail['end_line']])
facts={'recorded_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'passed':True,
       'stage':'正式126安装字节与API验证；原生GUI另验','version':'0.2.126','source_commit':installed['source_commit'],
       'schema_sha256':sha(schema_path.read_bytes()),'sbom_sha256':sha(raw),'sbom_bytes':len(raw),
       'installed_files_matched':len(actual),'total_bytes':sum(x[0] for x in actual.values()),
       'file_sbom_completeness':'incomplete','compiled_dependency_graph_claimed':False,
       'preview_file':name,'total_lines':len(lines),'byte_pages':pages,'bytes_reconstructed':True,
       'tail_range':[tail['start_line'],tail['end_line']],'gui_passed':False}
(p/'sbom-and-preview-verification.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(facts,ensure_ascii=False))
