"""真实当前聊天室的只读上下文预览；只输出压缩投影，不导出思考。"""
from pathlib import Path
import json,urllib.request
p=Path(__file__).resolve().parent
url='http://127.0.0.1:8765/api/sessions/session-1791131217833/context-preview?room_id=room-1791131523339&prompt=CONTINUATION-SUMMARY-AUDIT'
with urllib.request.urlopen(url,timeout=30) as r: a=json.load(r)
item=a.get('compaction_item') or {}
result={'truncated':a['truncated'],'history_message_count':a['history_message_count'],'summary':item.get('summary'),'message_count':item.get('message_count'),'lost_user_body':bool(item.get('summary') and '- 用户: [历史用户消息' in item['summary']),'selected_count':len(a['history_selection']['selected_ids']),'excluded_count':len(a['history_selection']['excluded_ids'])}
(p/'installed123-preview.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
