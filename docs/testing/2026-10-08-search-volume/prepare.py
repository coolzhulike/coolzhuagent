"""只为独立容量验收创建公开合成聊天资料；不复制任何用户记录或凭据。"""
import pathlib,json,time
folder=pathlib.Path(__file__).resolve().parent
workspace=folder/'workspace'
data=workspace/'.coolzhu'
data.mkdir(parents=True,exist_ok=True)
assert not (data/'web-sessions.sqlite3').exists(), '已有容量库，禁止覆盖'
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr = "127.0.0.1:64605"\n[pet]\nenabled = false\n[computer_use]\nenabled = false\n',encoding='utf-8')
now=int(time.time()*1000)
rooms=[{'id':f'volume-{n}','name':f'搜索容量验收 {n:,} 条','created_at':now,'updated_at':now,'messages':[]} for n in [10000,100000]]
state={'sessions':[],'active_session_id':None,'chat_rooms':rooms,'active_chat_room_id':'volume-100000'}
(data/'web-sessions.json').write_text(json.dumps(state,ensure_ascii=False),encoding='utf-8')
print('独立无凭据工作区准备完成；仅执行消息搜索，不调用模型')
