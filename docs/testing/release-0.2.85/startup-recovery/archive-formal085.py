"""只接受正常安装版真实SWE的四项证据；分别核对投递、释放及页面事件。"""
import hashlib,json,pathlib,re,shutil
folder=pathlib.Path(__file__).resolve().parent;repo=folder.parents[1]
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
installed=read(folder/'installed-085-verification.json');process=read(folder/'installed-085-standard-processes.json')
assert installed['package_safe'] and installed['version']=='0.2.85'
assert process['web_sha256']==installed['web_sha256'] and process['shell_sha256']==installed['shell_sha256']
paths=[folder/n for n in ['installed-085-verification.json','package-085-verification.json','installed-085-standard-processes.json','install-085-result.json','server.py','server-process.json','server-boundaries.py','boundary-server-process.json','events.jsonl','boundary-events.jsonl']]
cases=[]
for suffix,label in [('CLICK','click'),('SCALE','scale'),('COVER','cover'),('NAV4','nav4')]:
    prefix='BU-INSTALLED-085-OOP-'+suffix+'-20261006';data=read(folder/(prefix+'-facts.json'));context=data['context_and_tools'];cu=data['computer_use'][0];out=cu['terminal_result_json']
    assert len(data['computer_use'])==1 and all(a['state']=='terminal' and a['protocol_stop']=='end_turn' and a['process_drained']==1 and json.loads(a['model_json'])['effective']=='swe-2-medium' for a in context['acp'])
    assert next(b for b in context['bindings'] if b['agent_id']=='session-1791131217833')['remote_session_id']=='veiled-anise'
    assert out['attempts']==1 and len(cu['steps'])==1
    step=cu['steps'][0]
    if suffix in ['CLICK','SCALE']:
        assert context['run']['state']=='completed' and out['goal_achieved'] is True and out['status']=='succeeded' and out['steps_completed']==1
        assert step['action_type']=='click' and step['input_delivery']=='sent' and step['native_dispatch_state']=='released'
        assert json.loads(step['native_binding_json'])['executor']['process']['pid']==process['shell_pid']
        summary=json.loads(out['summary']);assert summary['criteria_met']==2 and summary['ungrounded_positive_count']==0
        assert {n['name'] for n in summary['observed_page']['nodes']}=={'父点击计数：0','子点击计数：1'}
        events=[e for e in data['web_events'] if e.get('kind')=='child-click' and step['started_at_ms']<=e['observed_ms']<=step['completed_at_ms']]
        assert len(events)==1 and events[0]['trusted'] is True and events[0]['count']==1
    else:
        assert context['run']['state']=='failed' and out['status']=='blocked' and out['steps_completed']==0 and out['goal_achieved'] is False
        assert out['error']['receipt']['input_delivery']=='not_sent' and step['input_delivery']=='not_sent' and step['native_dispatch_state'] is None
        assert out['error']['code']==('native_browser_target_hit_mismatch' if suffix=='COVER' else 'native_browser_document_changed')
        assert not [e for e in data['web_events'] if e.get('kind')=='child-click' and cu['created_at_ms']<=e['observed_ms']<=cu['updated_at_ms']]
        if suffix=='NAV4':
            planner=next(p for p in cu['planner'] if p['request_kind']=='computer_use_planning')
            setups=[e for e in data['web_events'] if e.get('kind')=='setup-switch' and planner['started_at_ms']<=e['observed_ms']<=planner['completed_at_ms']]
            assert len(setups)==1 and setups[0]['trusted'] is True
    cases.append({'prefix':prefix,'outcome':out,'context':context,'steps':cu['steps'],'planner':cu['planner']})
    paths += [folder/n for n in [prefix+'-facts.json',prefix+'-context.log',prefix+'-browser.log',prefix+'-events.json',prefix+'-request.txt','installed-'+label+'-before.jpg','installed-'+label+'-before-ax.txt','installed-'+label+'-after.jpg','installed-'+label+'-after-ax.txt']]
    # facts.json已包含真实上下文、CU/许可原字段；两个log保存原读取输出，不重复同名归档。
paths += [folder/'installed-nav4-switch.jpg',folder/'installed-nav4-switch-ax.txt']
diagnostics=[]
for suffix,label,claim in [('NAV','nav','未发生setup-switch，不计规划期间导航负例通过'),('NAV2','nav2','setup-switch晚于动作释放及页面验收，不计规划期间导航负例通过'),('NAV3','nav3','setup-switch在规划返回后约124毫秒、输入预检期间；零投递拒绝通过，但不计规划期间切换')]:
    prefix='BU-INSTALLED-085-OOP-'+suffix+'-20261006';data=read(folder/(prefix+'-facts.json'))
    diagnostics.append({'prefix':prefix,'claim':claim,'actual':data})
    paths += [folder/n for n in [prefix+'-facts.json',prefix+'-context.log',prefix+'-browser.log',prefix+'-events.json',prefix+'-request.txt','installed-'+label+'-before.jpg','installed-'+label+'-before-ax.txt','installed-'+label+'-after.jpg','installed-'+label+'-after-ax.txt']]
    if suffix!='NAV':paths += [folder/('installed-'+label+'-switch.jpg'),folder/('installed-'+label+'-switch-ax.txt')]
dest=repo/'docs/testing/release-0.2.85/installed-oop';assert not dest.exists();dest.mkdir(parents=True);files=[]
for path in sorted(set(paths)):
    raw=path.read_bytes()
    if path.suffix not in ['.jpg','.png']:assert not re.search(rb'\bsk-[A-Za-z0-9_-]{24,}|Bearer\s+[A-Za-z0-9_-]{24,}',raw),path.name
    target=dest/path.name;assert not target.exists();shutil.copyfile(path,target)
    files.append({'file':path.name,'length':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(dest/'manifest.json').write_text(json.dumps({'stage':'installed','installed':installed,'process':process,'cases':cases,'diagnostics':diagnostics,'files':files,'limits':['仅OOP按钮及矩形正向轴缩放，不外推编辑/键盘/滚动','覆盖和规划期间子导航为预期零输入；按住期间变化仍开放','Opus暂停；主会话独立实操；Paint基础已有正式通过，微信不改不测']},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('正式安装版四项原证据归档',len(files))
