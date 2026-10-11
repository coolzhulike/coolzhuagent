from pathlib import Path
root=Path(__file__).resolve().parents[3]
p=root/'modules/gui-web/packages/web-console/src/computer_use_planner.rs'
old='句子被拆成相邻节点时，可按连续数组索引顺序原样连接这些name，不添加空格或其它文字。也可只选StaticText节点按数组顺序原样连接'
new='句子被拆成相邻节点时，可按连续数组索引顺序逐字连接这些name；节点边界只允许不加分隔、单个ASCII空格或单个换行LF，节点内部字符不得删除或修改。也可只选StaticText节点按数组顺序以同样三种分隔之一连接'
raw=p.read_bytes();assert raw.count(old.encode())==1
p.write_bytes(raw.replace(old.encode(),new.encode()))
print('已依据读过的确切上下文同步引用提示，其它字节保持')
