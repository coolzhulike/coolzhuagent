"""两个真实EXE读取同一真实SQLite快照；不发送模型，不写原运行库。"""
from pathlib import Path
import copy,hashlib,json,os,sqlite3,subprocess,time,tomllib,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[1]
original=root/'tmp/2026-10-04-devin-models/workspace'
assert not (p/'snapshot.sqlite3').exists(), '禁止覆盖旧证据'
with sqlite3.connect((original/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as source,sqlite3.connect(p/'snapshot.sqlite3') as target:source.backup(target)
cfg=tomllib.loads((original/'coolzhu.toml').read_text(encoding='utf-8-sig'))
base={k:copy.deepcopy(cfg[k]) for k in ('configuration_revision','paths','session','model','web','pet','session_model_limits')}
base['model']['enable_real_llm']=False;base['model']['enable_semantic_memory']=False
base['web']['bind_addr']='127.0.0.1:8768';base['pet']['enabled']=False
base['scheduled_tasks']={'tasks':[]}
def encode(v):
 if isinstance(v,bool):return 'true' if v else 'false'
 if isinstance(v,(int,float)):return str(v)
 if isinstance(v,str):return json.dumps(v,ensure_ascii=False)
 if isinstance(v,list):return '['+','.join(encode(x) for x in v)+']'
 raise TypeError(type(v).__name__)
def dump(table,path=()):
 lines=[] if not path else ['['+'.'.join(json.dumps(x,ensure_ascii=False) for x in path)+']']
 lines += [json.dumps(k,ensure_ascii=False)+' = '+encode(v) for k,v in table.items() if not isinstance(v,dict)]
 for k,v in table.items():
  if isinstance(v,dict):lines+=['',dump(v,(*path,k))]
 return '\n'.join(lines)
config=dump(base);assert tomllib.loads(config)==base
address=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';saved=address.read_bytes() if address.exists() else None
facts={}
try:
 for kind,binary in [('before',Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')),('candidate',root/'target/debug/coolzhu-web-console.exe')]:
  f=p/kind;workspace=f/'workspace';(workspace/'.coolzhu').mkdir(parents=True)
  with sqlite3.connect(p/'snapshot.sqlite3') as source,sqlite3.connect(workspace/'.coolzhu/web-sessions.sqlite3') as target:source.backup(target)
  (workspace/'coolzhu.toml').write_text(config,encoding='utf-8')
  env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(f/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(f/'isolated-input-safety'))
  for key in ('COOLZHU_WEB_STATIC_ROOT','COOLZHU_GUI_WEB_URL'):env.pop(key,None)
  def attempt_count():
   with sqlite3.connect(workspace/'.coolzhu/web-sessions.sqlite3') as c:return c.execute('select count(*) from devin_acp_attempts').fetchone()[0]
  attempts=attempt_count()
  with (f/'stdout.log').open('wb') as out,(f/'stderr.log').open('wb') as err:
   proc=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
   try:
    url='http://127.0.0.1:8768/api/sessions/session-1791131217833/context-preview?room_id=room-1791131523339&prompt=CONTINUATION-SUMMARY-AUDIT'
    for _ in range(150):
     assert proc.poll() is None
     try:
      with urllib.request.urlopen(url,timeout=15) as response:a=json.load(response)
      break
     except OSError:time.sleep(.1)
    else:raise RuntimeError('服务未就绪')
    summary=a['compaction_item']['summary']
    facts[kind]={'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'summary':summary,'compacted_message_count':a['compaction_item']['message_count'],'history_message_count':a['history_message_count'],'selected_ids':a['history_selection']['selected_ids'],'excluded_ids':a['history_selection']['excluded_ids'],'token_budget':a['token_budget'],'lost_user_body_count':summary.count('- 用户: [历史用户消息'),'new_acp_attempts':attempt_count()-attempts}
    (f/'preview-projection.json').write_text(json.dumps(facts[kind],ensure_ascii=False,indent=2),encoding='utf-8')
   finally:
    proc.terminate();proc.wait(timeout=15)
  facts[kind]['process_reaped']=proc.poll() is not None
 before,after=facts['before'],facts['candidate']
 assert before['binary_sha256']=='9b2ef060a6cafaab1c6ca3898c85352983f4f297103f125bfe8c4df54d1d7407'
 assert before['lost_user_body_count']>0 and after['lost_user_body_count']==0
 assert before['selected_ids']==after['selected_ids'] and before['excluded_ids']==after['excluded_ids']
 assert before['compacted_message_count']==after['compacted_message_count']
 assert after['token_budget']['total']<=after['token_budget']['budget']
 with sqlite3.connect((p/'snapshot.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
  users=[row[0] for row in c.execute("select content from chat_room_messages where role='user'")]
 recovered=[]
 for line in after['summary'].splitlines():
  if line.startswith('- 用户: '):
   body=line[len('- 用户: '):].removesuffix('…')
   assert any(value.strip().startswith(body) for value in users), '摘要正文必须对应真实用户原文'
   recovered.append(body)
 assert len(recovered)==before['lost_user_body_count']
 assert all(x['new_acp_attempts']==0 for x in facts.values())
 result={'passed':True,'recovered_user_requests':len(recovered),'compacted_message_count':after['compacted_message_count'],'selected_history_unchanged':True,'excluded_history_unchanged':True,'budget_preserved':True,'new_model_attempts':0,'new_cloud_sessions':0,'original_database_writes':0,'candidate_only':True}
 (p/'comparison-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(result),flush=True)
finally:
 if saved is not None:address.write_bytes(saved)
 elif address.exists():address.unlink()
