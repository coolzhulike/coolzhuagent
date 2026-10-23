"""按实际Cargo构建记录汇总许可证原文，固定版本上游补件必须通过Git blob核验。"""
import argparse,base64,hashlib,json,zipfile
from pathlib import Path

def sha(data):return hashlib.sha256(data).hexdigest()
def read(path):return json.loads(Path(path).read_text('utf-8-sig'))
def repository(value):return (value or '').removesuffix('.git').rstrip('/').lower().replace('github.com/sfackler/rust-phf','github.com/rust-phf/rust-phf')

def payload_licenses(payload, texts, native_license_paths):
    payload=Path(payload).resolve(strict=True)
    inventory=read(payload/'payload-inventory.json');entries={f['path']:f for f in inventory['files']}
    def checked(name):
        entry=entries[name];path=(payload/name).resolve(strict=True);path.relative_to(payload)
        raw=path.read_bytes()
        if len(raw)!=entry['length'] or sha(raw)!=entry['sha256'].lower():raise ValueError('载荷文件已漂移：'+name)
        return raw
    def body(name):
        raw=checked(name);digest=sha(raw);texts[digest]=raw
        return {'path':f'texts/{digest}.txt','source_path':name,'source_scope':'actual-payload','sha256':digest}
    components=[];missing=[]
    native={}
    for proof_path in native_license_paths:
        proof=read(proof_path);base=Path(proof_path).resolve().parent
        package=(base/proof['package_path']).resolve(strict=True);package.relative_to(base)
        if sha(package.read_bytes())!=proof['package_sha256']:raise ValueError('原生SDK证据包摘要不符')
        with zipfile.ZipFile(package) as z:
            if z.getinfo(proof['license_member']).file_size>100000 or z.getinfo(proof['binary_member']).file_size>1000000:raise ValueError('原生SDK证据成员超限')
            raw=z.read(proof['license_member']);binary=z.read(proof['binary_member'])
        if sha(raw)!=proof['license_sha256'] or sha(binary)!=proof['binary_sha256']:raise ValueError('原生SDK成员摘要不符')
        digest=sha(raw);texts[digest]=raw
        native[sha(binary)]={'path':f'texts/{digest}.txt','sha256':digest,'source_path':proof['license_member'],'source_scope':'matching-native-sdk','source_url':proof['license_url'],'sdk_package_sha256':proof['package_sha256']}
    for name in sorted(entries):
        if name.endswith('/package.json'):
            package=json.loads(checked(name).decode('utf-8-sig'));parent=name.rsplit('/',1)[0]
            files=[body(f) for f in sorted(entries) if f.rsplit('/',1)[0]==parent and Path(f).name.lower() in ('license','license.md','license.txt','copying','notice','notice.txt')]
            if not files and '/node_modules/' not in name:files=[body('LICENSE')]
            item={'name':package.get('name'),'version':package.get('version'),'kind':'npm','manifest':name,'manifest_sha256':sha(checked(name)),'declared_license':package.get('license'),'files':files}
            components.append(item)
            if not files:missing.append({'name':item['name'],'version':item['version'],'reason':'实际载荷内缺许可证原文'})
    for name in sorted(entries):
        if name.endswith('/node.exe'):
            license_path=name.rsplit('/',1)[0]+'/LICENSE'
            components.append({'name':'Node.js runtime','kind':'native-runtime','binary':name,'binary_sha256':sha(checked(name)),'files':[body(license_path)] if license_path in entries else []})
            if license_path not in entries:missing.append({'name':'Node.js runtime','reason':'缺随附汇总许可证'})
        elif name.endswith('.dll'):
            digest=sha(checked(name));files=[native[digest]] if digest in native else []
            components.append({'name':Path(name).name,'kind':'native-library','binary':name,'binary_sha256':digest,'files':files})
            if not files:missing.append({'name':Path(name).name,'reason':'原生DLL的再分发许可需单独提供，不能用Rust包装crate许可证代替'})
    return {'inventory_sha256':sha((payload/'payload-inventory.json').read_bytes()),'payload_digest':inventory['payload_digest'],'components':components,'missing':missing}

def export(root,report_path,metadata_paths,evidence_paths,upstream_paths,output,payload=None,native_license_paths=(),supplement_paths=()):
    root=Path(root).resolve(strict=True);output=Path(output).absolute()
    if output.exists():raise FileExistsError('输出目录已存在，禁止覆盖既有证据')
    report=read(report_path);packages={};bodies={};observed={};verified=[]
    for path in metadata_paths:
        for p in read(path)['packages']:packages[sha(p['id'].encode())]=p
    for path in evidence_paths:
        for p in read(path)['components']:
            old=bodies.get(p['identity_sha256'])
            if old:
                # 同一组件可能分别由工作区继承导出和子工作区导出；合并来源，不能丢正文或掩盖同源差异。
                merged={(f.get('scope','component'),f['path']):f for f in old['files']}
                for f in p['files']:
                    source=(f.get('scope','component'),f['path'])
                    if source in merged and merged[source]!=f:
                        raise ValueError('同源组件许可证证据冲突：'+p['identity_sha256']+' '+str(source))
                    merged[source]=f
                old['files']=list(merged.values())
            else:bodies[p['identity_sha256']]=p
    for path in upstream_paths:
        for item in read(path):
            raw=item['content'].encode('utf-8')
            blob=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
            if blob!=item['sha']:raise ValueError('上游许可证字节与固定Git blob不符')
            if len(item['ref'])!=40:raise ValueError('上游来源必须为固定commit')
            verified.append(item)
    supplements={}
    for path in supplement_paths:
        for item in read(path)['components']:
            key=sha(item['id'].encode());package=packages.get(key)
            if not package or item['declared_license']!=package.get('license'):
                raise ValueError('补充许可与实际组件声明不符')
            manifest=Path(package['manifest_path'])
            if sha(manifest.read_bytes())!=item['manifest_sha256']:
                raise ValueError('补充许可对应的组件清单已变化')
            choices=package['license'].replace('/', ' OR ').split(' OR ')
            if item['selected_license'] not in [s.strip() for s in choices]:
                raise ValueError('补充许可不在原组件允许的选择中')
            files=[]
            for body in item['files']:
                raw=body['content'].encode('utf-8')
                if sha(raw)!=body['sha256']:raise ValueError('补充许可原文摘要不符')
                if body.get('git_blob') and hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()!=body['git_blob']:
                    raise ValueError('补充许可与上游Git blob不符')
                files.append(body)
            if not files:raise ValueError('补充许可缺正文')
            if key in supplements:raise ValueError('组件补充许可重复')
            supplements[key]={**item,'files':files}
    for entry in report['cargo_build_evidence']['entries']:
        path=(root/entry['path']).resolve(strict=True);path.relative_to(root)
        raw=path.read_bytes()
        if sha(raw)!=entry['sha256'].lower():raise ValueError('实际构建消息摘要不符')
        rows=[json.loads(line) for line in raw.decode('utf-8-sig').splitlines() if line.strip()]
        if rows[-1].get('reason')!='build-finished' or rows[-1].get('success') is not True:raise ValueError('构建没有成功终态')
        for row in rows:
            if row.get('reason') in ('compiler-artifact','build-script-executed'):
                identity=row['package_id'];key=sha(identity.encode())
                if key not in packages:raise ValueError('实际构建组件缺少对应metadata：'+identity)
                observed.setdefault(key,set()).add(entry['artifact_id'])
    components=[];texts={};missing=[]
    for key,artifacts in sorted(observed.items(),key=lambda pair:packages[pair[0]]['id']):
        package=packages[key];evidence=bodies.get(key)
        files=[]
        for item in (evidence or {}).get('files',[]):
            raw=item['content'].encode('utf-8') if item['encoding']=='utf-8' else base64.b64decode(item['content'],validate=True)
            if sha(raw)!=item['sha256'] or len(raw)!=item['bytes']:raise ValueError('本地许可证证据摘要不符')
            digest=sha(raw);texts[digest]=raw
            files.append({'path':f'texts/{digest}.txt','source_path':item['path'],'source_scope':item.get('scope','component'),'sha256':digest})
        if not files:
            vcs_path=Path(package['manifest_path']).parent/'.cargo_vcs_info.json'
            vcs=read(vcs_path) if vcs_path.is_file() else {}
            commit=vcs.get('git',{}).get('sha1')
            for item in verified:
                if item['ref']==commit and repository(item['repo'])==repository(package.get('repository')):
                    raw=item['content'].encode('utf-8');digest=sha(raw);texts[digest]=raw
                    files.append({'path':f'texts/{digest}.txt','source_scope':'upstream-fixed-commit','source_url':item['url'],'source_path':item['path'],'git_blob':item['sha'],'git_commit':commit,'sha256':digest})
        supplement=supplements.get(key) if not files else None
        if supplement:
            for item in supplement['files']:
                raw=item['content'].encode('utf-8');digest=sha(raw);texts[digest]=raw
                files.append({'path':f'texts/{digest}.txt','source_path':item['path'],'source_scope':item['source_scope'],'source_url':item['source_url'],'sha256':digest,**({'git_blob':item['git_blob']} if item.get('git_blob') else {})})
        component={'id':package['id'],'name':package['name'],'version':package['version'],'declared_license':package.get('license'),'repository':package.get('repository'),'observed_in':sorted(artifacts),'files':files}
        if supplement:component['supplemental_license']={k:supplement[k] for k in ['selected_license','basis','manifest_sha256']}
        components.append(component)
        if not files:missing.append({'name':package['name'],'version':package['version'],'declared_license':package.get('license'),'reason':'组件包与固定版本上游均缺许可证原文，不以其它版本文本冒充'})
    noncargo=payload_licenses(payload,texts,native_license_paths) if payload else None
    if noncargo:missing.extend(noncargo['missing'])
    result={'schema':1,'kind':'build-observed-license-bundle','scope':'实际Cargo构建观察的保守全集（含构建期和proc-macro），不等同精确运行时链接；非Cargo载荷单列；补附标准许可/同版本历史授权单独标明，不冒充原crate归档内文件','package_report_sha256':sha(Path(report_path).read_bytes()),'inputs':[{'path':str(p),'sha256':sha(Path(p).read_bytes())} for p in [*metadata_paths,*evidence_paths,*upstream_paths,*native_license_paths,*supplement_paths]],'component_count':len(components),'unique_text_count':len(texts),'missing':missing,'complete':not missing,'components':components,'non_cargo':noncargo}
    output.mkdir(parents=True);(output/'texts').mkdir()
    for digest,raw in texts.items():(output/'texts'/f'{digest}.txt').write_bytes(raw)
    (output/'components.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n','utf-8')
    lines=['# 第三方构建组件许可证索引','',result['scope'],'','缺原文组件：'+str(len(missing))+'；本索引不把缺失项记作完成。','']
    for component in components:
        lines+=['## '+component['name']+' '+component['version'],'','声明：'+str(component['declared_license']),'']
        if component.get('supplemental_license'):lines+=['补附依据：'+component['supplemental_license']['basis'],'选择：'+component['supplemental_license']['selected_license'],'']
        lines += ['- ['+item['source_path']+']('+item['path']+')；来源：'+item.get('source_url',item['source_scope']) for item in component['files']] or ['- 原文待补']
        lines+=['']
    if noncargo:
        lines+=['# 非Cargo载荷','']
        for component in noncargo['components']:
            lines+=['## '+component['name']+' '+str(component.get('version','')),'']
            lines+=['- ['+item['source_path']+']('+item['path']+')' for item in component['files']] or ['- 原文待补']
            lines+=['']
    (output/'THIRD-PARTY-NOTICES.md').write_text('\n'.join(lines),'utf-8')
    return result

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root',required=True);p.add_argument('--report',required=True);p.add_argument('--metadata',action='append',required=True);p.add_argument('--evidence',action='append',required=True);p.add_argument('--upstream',action='append',default=[]);p.add_argument('--output',required=True);p.add_argument('--payload');p.add_argument('--native-license',action='append',default=[]);p.add_argument('--supplement',action='append',default=[])
    a=p.parse_args();r=export(a.root,a.report,a.metadata,a.evidence,a.upstream,a.output,a.payload,a.native_license,a.supplement)
    print(json.dumps({k:r[k] for k in ['component_count','unique_text_count','missing','complete']},ensure_ascii=False))
