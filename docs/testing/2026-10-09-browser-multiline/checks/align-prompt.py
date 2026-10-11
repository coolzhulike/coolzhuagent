from pathlib import Path
p=Path(__file__).resolve().parents[2]/'modules/gui-web/packages/web-console/src/computer_use_planner.rs'
old='先核对该节点name确实包含本次evidence。'
new='先核对单节点name或按上述允许规则连接后的name确实包含本次evidence。'
raw=p.read_bytes();assert raw.count(old.encode())==1
p.write_bytes(raw.replace(old.encode(),new.encode()))
print('多节点提示与验证器契约一致')
