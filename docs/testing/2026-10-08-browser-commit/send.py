"""原SWE/原聊天室/唯一云端绑定的一次真实CU边界验收。"""
import pathlib,json,urllib.request,time
folder=pathlib.Path(__file__).resolve().parent
prompt='''BU-INSTALLED098-CROSS-ORIGIN-COMMIT-20261008：正式安装版按下期间跨来源HTTP提交专项，仅一次computer_use_perform(surface="browser",省略target,max_actions=1)，根据本轮新鲜节点点击“按下期间跨来源提交”一次。页面在真实pointerdown内发起从127.0.0.1到localhost的HTTP导航，并有2400毫秒按下处理；以独立网页时间戳验证实际提交时刻，不推断一定命中。导航后不得继续点击、输入或补发；新页面仅观察原动作是否释放。只报告真实宿主投递、释放与终态。允许read_request完整分页后respond、wait取最终结果。禁止重开、重试、脚本、shell、原生文件、其它工具或新云端会话；未知输入立即停止。旧任务均已结束。'''
payload={'expected_workspace_id':'ws-23f646a969206cb4','native_browser_panel':True,'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':prompt,'attachments':[]}
(folder/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf-8')
req=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=1000) as response,(folder/'stream-events.txt').open('w',encoding='utf-8') as output:
 for raw in response:
  line=raw.decode('utf-8');output.write(line);output.flush()
  if line.startswith('event:'):print(line.strip(),flush=True)
(folder/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'status':'EOF；另核实协议终态'}),encoding='utf-8')
