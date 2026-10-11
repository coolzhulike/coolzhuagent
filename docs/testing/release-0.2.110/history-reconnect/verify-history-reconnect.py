from pathlib import Path
import json
p=Path(__file__).resolve().parent/'history-reconnect'
before=json.loads((p/'before-dom.json').read_text(encoding='utf-8'))
after=json.loads((p/'after-dom.json').read_text(encoding='utf-8'))
expected=json.loads((p/'expected-ui.json').read_text(encoding='utf-8'))
assert not before['hidden'] and not after['hidden']
assert len(before['rows'])==80 and before['rows'][0]['id']=='old-0271'
assert len(after['rows'])==expected['count']==527
assert [x['id'] for x in after['rows']]==[x[0] for x in expected['rows']]
assert len({x['id'] for x in after['rows']})==527
for actual,(identifier,content) in zip(after['rows'],expected['rows']):assert content in actual['text'],identifier
assert not {'old-0271','old-0300','old-0350'} & {x['id'] for x in after['rows']}
result={'version':'0.2.110','initial_visible_window':80,'offline_inserted':450,'deleted_loaded_rows':3,'updated_loaded_rows':1,'reconnected_ui_rows':527,'ids_order_and_content_match_sqlite':True,'duplicates':0,'reload_or_room_reselect_after_disconnect':False,'test_messages_role':'user','model_requests':0,'new_cloud_sessions':0,'original_database_writes':0,'boundary':'只验证真实后台断开/重连与跨页新增删除更新，不替代模型流式、跨工作区竞争或任意网络故障矩阵'}
(p/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
