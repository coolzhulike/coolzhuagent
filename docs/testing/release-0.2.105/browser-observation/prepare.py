"""沿用普通实验网页；本轮只验已投递后的观察文案，不宣称严格按下窗口。"""
from pathlib import Path
import json
folder=Path(__file__).resolve().parent
root=folder.parents[1]
old=root/'tmp/2026-10-08-browser-native-replacement'
server=(old/'server.py').read_text(encoding='utf-8')
server=server.replace('NATIVE-REPLACEMENT-', 'OBSERVATION105-')
server=server.replace('替换窗口验收', '投递事实验收')
(folder/'server.py').write_text(server,encoding='utf-8')
status=(old/'status.py').read_text(encoding='utf-8').replace('BU-NATIVE-REPLACEMENT-101-20261008','BU-OBSERVATION-105-20261008')
(folder/'status.py').write_text(status,encoding='utf-8')
print('网页与只读状态采证脚本已准备；无模型调用。')
