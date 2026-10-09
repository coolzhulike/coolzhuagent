from pathlib import Path
p=Path(__file__).resolve().parent;root=p.parents[1]
text=(root/'tmp/2026-10-09-session-config-scope/restart.py').read_text(encoding='utf-8')
text=text.replace("f=p/'save-scope'","f=p/'candidate2'").replace('8768','50528')
text=text.replace("saved=facts['saved_after_release']", "saved=json.loads((f/'native-saved-settings.json').read_text(encoding='utf-8'))")
text=text.replace("print('同候选EXE重启恢复工程A名称/温度/容量/revision；等待原生界面',flush=True)","print('同候选EXE重启恢复原生保存0.45/revision12/工程A容量8192；等待原生实拍',flush=True)")
(p/'restart.py').write_text(text,encoding='utf-8')
