"""仅移除独立容量库可重建的FTS派生表；保留全部消息与用户数据库。"""
import pathlib,sqlite3,json,urllib.request,urllib.parse,time,concurrent.futures
folder=pathlib.Path(__file__).resolve().parent
db=folder/'workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db) as c:
 before=c.execute('select count(*) from chat_room_messages').fetchone()[0]
 assert before==110002
 c.execute('drop table chat_search_fts_v1')
 c.commit()
base='http://127.0.0.1:64605'
def read(path):
 start=time.perf_counter()
 with urllib.request.urlopen(base+path,timeout=120) as r:body=json.load(r)
 return body,(time.perf_counter()-start)*1000
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
 pending=pool.submit(read,'/api/chat/rooms/volume-100000/search?q=UNIQUE-BAMBOO-RECEIPT&limit=25')
 reads=[]
 while not pending.done():
  body,ms=read('/api/chat/rooms/volume-10000/messages?limit=20')
  reads.append(round(ms,3))
  time.sleep(.05)
 cold,cold_ms=pending.result()
assert cold['message_count']==100000 and cold['total']==100
with sqlite3.connect(db) as c:
 after=c.execute('select count(*) from chat_room_messages').fetchone()[0]
 dirty=c.execute('select count(*) from chat_search_dirty_v1').fetchone()[0]
 indexed=c.execute('select count(*) from chat_search_fts_v1').fetchone()[0]
 assert after==before and dirty==0 and indexed==110000
result={'stage':'0.2.97正式后端；110000可见资料冷重建；保留全部消息','message_rows_before':before,'message_rows_after':after,'indexed_visible_rows':indexed,'dirty_remaining':dirty,'cold_rebuild_search_ms':round(cold_ms,3),'matched':cold['total'],'concurrent_read_ms':reads,'concurrent_read_max_ms':max(reads,default=None),'scope':'仅HTTP搜索与并发历史读取；不代表模型并发写入或磁盘极限容量'}
(folder/'cold-rebuild-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
