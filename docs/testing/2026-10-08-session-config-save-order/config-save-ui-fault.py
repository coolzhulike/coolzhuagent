from pathlib import Path
import json, sqlite3, sys, urllib.request
p=Path(__file__).resolve().parent/'config-save-live'
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
sid=facts['session_id'];dbpath=p/'workspace/.coolzhu/web-sessions.sqlite3'
def current():
    with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions/'+sid+'/model-settings') as r:return json.load(r)
if sys.argv[1]=='arm':
    before=current()
    with sqlite3.connect(dbpath) as db:db.execute("CREATE TRIGGER config_save_ui_fault BEFORE INSERT ON sessions WHEN NEW.id = '"+sid+"' BEGIN SELECT RAISE(ABORT,'配置保存故障验收'); END")
    (p/'ui-before.json').write_text(json.dumps(before,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print({'fault_armed':True})
else:
    after=current();before=json.loads((p/'ui-before.json').read_text(encoding='utf-8'))
    assert after['session']['name']==before['session']['name'] and after['parameters']==before['parameters']
    assert after['configuration_revision']==before['configuration_revision']+2
    with sqlite3.connect(dbpath) as db:
        assert db.execute('SELECT name FROM sessions WHERE id=?',(sid,)).fetchone()[0]==before['session']['name']
        db.execute('DROP TRIGGER config_save_ui_fault')
    (p/'ui-fault-result.json').write_text(json.dumps({'state_restored':True,'revision_before':before['configuration_revision'],'revision_after':after['configuration_revision'],'fault_removed':True},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print({'restored':True,'fault_removed':True})
