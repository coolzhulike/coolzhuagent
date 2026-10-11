"""测正式HTTP搜索，不重写业务实现；合成资料不冒充模型回复。"""
import pathlib,json,sqlite3,urllib.request,urllib.parse,time,statistics,sys
folder=pathlib.Path(__file__).resolve().parent
db=folder/'workspace/.coolzhu/web-sessions.sqlite3'
base='http://127.0.0.1:64605'
for _ in range(30):
 try:
  with urllib.request.urlopen(base+'/api/chat/rooms',timeout=2) as r:rooms=json.load(r)
  break
 except OSError:time.sleep(.3)
else:raise RuntimeError('容量后端未就绪')
with sqlite3.connect(db) as c:
 count=c.execute('select count(*) from chat_room_messages').fetchone()[0]
 assert count in [0,110002], '资料规模异常，拒绝继续'
 seed=count==0
 base_time=1700000000000
 for n in ([10000,100000] if seed else []):
  rows=[]
  for i in range(1,n+1):
   # 同一毫秒的相邻ID检验稳定排序；可见内容与工具轨迹混合。
   text=f'竹林卷宗 {i:06d}：采购登记，玉石控件与工程说明。编号 RECORD-{i:06d}。'
   if i%1000==0:text+=' 稀有竹简 UNIQUE-BAMBOO-RECEIPT。'
   if i==n//2:text+=' 中文短词鹈鹕与大小写 MiXeDCaseProbe。'
   rows.append((f'volume-{n}',f'vol-{n}-{i:06d}','容量资料员','user','聊天',text,'user-message','[]',base_time+i//2))
  c.executemany('insert into chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) values (?,?,?,?,?,?,?,?,?)',rows)
  c.execute('insert into chat_room_messages values (?,?,?,?,?,?,?,?,?)',(f'volume-{n}',f'hidden-{n}','容量工具','tool','聊天','UNIQUE-BAMBOO-RECEIPT 不应出现在搜索','tool-result','[]',base_time+n))
 c.commit()
result={'binary_version':'0.2.97','dataset':'隔离合成资料；零模型生成；不改变日常库','cold_timing_note':'首轮已真实完成索引预热，但脚本误读响应字段，未保留有效冷耗时；下列仅统计已热索引。','rooms':{},'measurements':[]}
def fetch(n,**query):
 url=base+f'/api/chat/rooms/volume-{n}/search?'+urllib.parse.urlencode(query)
 start=time.perf_counter()
 with urllib.request.urlopen(url,timeout=180) as r:body=json.load(r)
 return body,round((time.perf_counter()-start)*1000,3)
for n in [10000,100000]:
 cold,elapsed=fetch(n,q='UNIQUE-BAMBOO-RECEIPT',limit=25)
 assert cold['message_count']==n and cold['total']==n//1000,cold
 expected=[f'vol-{n}-{i:06d}' for i in range(n,0,-1000)][:25]
 assert [x['id'] for x in cold['items']]==expected,cold
 assert [x['index'] for x in cold['items']]==list(range(n,0,-1000))[:25],cold
 result['measurements'].append({'room_size':n,'case':'首个计时查询（已热索引）','elapsed_ms':elapsed,'matched':cold['total']})
 warm=[]
 for _ in range(8):
  body,ms=fetch(n,q='UNIQUE-BAMBOO-RECEIPT',limit=25);warm.append(ms)
  assert body['total']==n//1000
 result['rooms'][str(n)]={'warm_ms':warm,'median_ms':statistics.median(warm),'max_ms':max(warm)}
 for needle in ['鹈鹕','mixedcaseprobe']:
  body,ms=fetch(n,q=needle)
  assert body['total']==1 and body['items'][0]['id']==f'vol-{n}-{n//2:06d}',body
  result['measurements'].append({'room_size':n,'case':needle,'elapsed_ms':ms,'matched':1})
 target=f'vol-{n}-{n//2:06d}'
 body,ms=fetch(n,around=target)
 assert body['found'] and body['position']==n//2 and len(body['messages'])==41,body
 assert body['messages'][20]['id']==target
 result['measurements'].append({'room_size':n,'case':'中间消息定位','elapsed_ms':ms,'position':body['position'],'window':41})
 page,ms=fetch(n,q='UNIQUE-BAMBOO-RECEIPT',offset=5,limit=3)
 assert [x['index'] for x in page['items']]==[n-5000,n-6000,n-7000]
 result['measurements'].append({'room_size':n,'case':'偏移分页','elapsed_ms':ms,'indices':[x['index'] for x in page['items']]})
(folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
