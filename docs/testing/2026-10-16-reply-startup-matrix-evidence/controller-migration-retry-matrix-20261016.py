"""补齐历史v7-v10侧表后独立重验旧库，保留首次不完整夹具失败。"""
import importlib.util,json
from pathlib import Path
p=Path(__file__).with_name('controller-remaining-http-matrix-20261016.py')
spec=importlib.util.spec_from_file_location('matrix',p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.ROOT=m.ROOT/'migration-retry';m.ROOT.mkdir(exist_ok=False)
m.REPORT=p.with_name('controller-migration-retry-matrix-20261016.json')
m.result['boundary']='完整135发布二进制；仓库v12 Goal复合主键和v7-v10历史侧表DDL的合成旧库。非用户生产旧库/OS断电/安装器降级'
try:m.migration_matrix()
except Exception as e:m.result['harness_error']=repr(e)
m.result['passed']=sum(c.get('state')=='passed' for c in m.result['cases'])
m.result['failed']=sum(c.get('state')=='failed' for c in m.result['cases'])
m.result['state']='completed' if not m.result['failed'] and 'harness_error' not in m.result else 'failed';m.save()
print(json.dumps({k:m.result[k] for k in ['state','passed','failed']},ensure_ascii=False))
raise SystemExit(0 if m.result['state']=='completed' else 1)
