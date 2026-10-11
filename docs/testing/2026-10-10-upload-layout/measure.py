"""仅分析原始实拍像素，不裁剪或重绘截图；窗口控件范围来自实际观察。"""
from pathlib import Path
from PIL import Image
import json
p=Path(__file__).resolve().parent
out={}
for name in ['formal-before.jpg','candidate-after-settled.jpg']:
 image=Image.open(p/name).convert('RGB')
 pixels=[]
 for y in range(823,846):
  for x in range(124,150):
   r,g,b=image.getpixel((x,y))
   if r+g+b>=520:pixels.append((x,y))
 assert pixels
 bounds=[min(x for x,y in pixels),min(y for x,y in pixels),max(x for x,y in pixels),max(y for x,y in pixels)]
 out[name]={'image_size':list(image.size),'glyph_bright_bounds':bounds,'glyph_center_x':(bounds[0]+bounds[2])/2,'note':'金色主笔画像素近似测量，JPEG抗锯齿会带来亚像素误差'}
out['shift_right_px']=out['candidate-after-settled.jpg']['glyph_center_x']-out['formal-before.jpg']['glyph_center_x']
(p/'pixel-measurement.json').write_text(json.dumps(out,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(out,ensure_ascii=False))
