/* 卷轴山水启动合成：复用固定字形和Q版整人姿态，不把整图位移称为骨骼动画。 */
(function (root, factory) {
  const api = factory(root);
  if (typeof module === "object" && module.exports) module.exports = api;
  else root.CoolzhuScrollStartupPlayer = api;
})(typeof globalThis === "object" ? globalThis : this, function (root) {
  "use strict";
  const clamp = value => Math.max(0, Math.min(1, value));
  const ease = value => { const t = clamp(value); return t * t * (3 - 2 * t); };
  const between = (time, start, end) => ease((time - start) / (end - start));
  const lerp = (left, right, progress) => left + (right - left) * progress;
  // 每个姿态是真实整人帧，锚点校正到鞋底；轨迹明确经历沉身、前移、抬升、落脚与回稳。
  const poses = [
    {time:1050, frame:0, x:328, y:696, angle:-0.04, scale:1},
    {time:1280, frame:0, x:330, y:712, angle:-0.10, scale:0.96},
    {time:1480, frame:1, x:347, y:695, angle:0.045, scale:1.02},
    {time:1700, frame:2, x:380, y:670, angle:0.08, scale:1.03},
    {time:1900, frame:4, x:409, y:691, angle:0.025, scale:1},
    {time:2100, frame:3, x:415, y:711, angle:-0.03, scale:0.96},
    {time:2320, frame:1, x:419, y:698, angle:0, scale:1},
  ];
  const feet = [[320,560],[320,576],[320,573],[320,483],[320,476]];
  function motionAt(time) {
    if (time <= poses[0].time) return {...poses[0],nextFrame:poses[0].frame,mix:0};
    for (let index=1; index<poses.length; index++) {
      const right=poses[index], left=poses[index-1];
      if (time <= right.time) {
        const progress=between(time,left.time,right.time);
        return {x:lerp(left.x,right.x,progress),y:lerp(left.y,right.y,progress),angle:lerp(left.angle,right.angle,progress),scale:lerp(left.scale,right.scale,progress),frame:left.frame,nextFrame:right.frame,mix:clamp((time-right.time+90)/90)};
      }
    }
    return {...poses.at(-1),nextFrame:poses.at(-1).frame,mix:0};
  }
  function createStartupPlayer(options = {}) {
    const manifest = options.manifest;
    const canvas = options.canvas;
    if (!manifest || !canvas?.getContext) throw new Error("启动卷轴缺少画布或素材清单");
    const context = canvas.getContext("2d");
    if (!context) throw new Error("启动卷轴无法创建画布");
    const windowRef = options.windowRef || root;
    const documentRef = options.documentRef || root.document;
    const daily = options.mode === "daily";
    const duration = daily ? 980 : 4600;
    const width=1680, height=900;
    let assets = options.assets || null;
    let active = false, finished = false, frameId=0, timerId=0, loadTimer=0, startedAt=null;
    let elapsed = 0, lastScene = null;
    const actorFrames=manifest.actor.frames;
    const sources=[...new Set([manifest.background.source.src,...manifest.letters.map(letter=>letter.glyphSource.src),...actorFrames.map(frame=>frame.src)])];
    const cancel = () => {
      if (frameId) windowRef.cancelAnimationFrame?.(frameId);
      windowRef.clearTimeout?.(timerId); windowRef.clearTimeout?.(loadTimer);
      frameId=0; timerId=0; loadTimer=0;
    };
    function finish(reason,error=null) {
      if (finished) return false;
      finished=true; active=false; cancel();
      if (error) options.onError?.(error);
      if (reason === "completed") options.onComplete?.(getState(),lastScene);
      const EventClass=windowRef.CustomEvent || root.CustomEvent;
      if (EventClass) documentRef?.dispatchEvent?.(new EventClass(manifest.completeEvent || "coolzhu-seven-letter-startup-internal",{detail:{reason,error,state:getState()}}));
      return true;
    }
    function getState() { return {elapsedMs:elapsed,totalDurationMs:duration,completed:elapsed>=duration,mode:daily?"daily":"first",phase:elapsed>=duration?"done":daily?"daily":elapsed<1250?"unfold":elapsed<2500?"sword-and-logo":elapsed<3350?"hold":"handoff"}; }
    function background(progress) {
      const image=assets?.[manifest.background.source.src];
      context.fillStyle="#071512"; context.fillRect(0,0,width,height);
      if (!image) return;
      const sw=image.naturalWidth || 1672, sh=image.naturalHeight || 941;
      const scale=Math.min(width/sw,height/sh), dw=sw*scale, dh=sh*scale;
      const x=(width-dw)/2, y=(height-dh)/2, roller=114*scale;
      const inner=dw-roller*2, reveal=inner*progress, center=width/2;
      // 卷面按中心裁显，两根玉轴使用同一个展开进度，保持原山水比例。
      context.save(); context.beginPath(); context.rect(center-reveal/2,y,reveal,dh); context.clip();
      context.drawImage(image,x,y,dw,dh); context.restore();
      context.drawImage(image,0,0,114,sh,center-reveal/2-roller,y,roller,dh);
      context.drawImage(image,sw-114,0,114,sh,center+reveal/2,y,roller,dh);
    }
    function logo(alpha) {
      if (alpha<=0) return;
      const h=154, gap=16;
      const glyphWidths=manifest.letters.map(letter=>h*letter.glyphRect.width/letter.glyphRect.height);
      const total=glyphWidths.reduce((sum,value)=>sum+value,0)+gap*6;
      const fit=970/total;
      let x=(width-970)/2;
      context.save(); context.globalAlpha*=alpha;
      manifest.letters.forEach((letter,index)=>{
        const image=assets?.[letter.glyphSource.src], rect=letter.glyphRect;
        const w=glyphWidths[index]*fit, glyphHeight=h*fit;
        if (image) context.drawImage(image,rect.x,rect.y,rect.width,rect.height,x,height*0.48-glyphHeight/2,w,glyphHeight);
        x+=w+gap*fit;
      });
      // 原字形不变，低对比剑脊和竹叶作为次层构图，避免视觉元素挤入字内。
      context.globalAlpha*=0.45;
      context.strokeStyle="#C2A46D"; context.lineWidth=2;
      context.beginPath(); context.moveTo(592,535); context.quadraticCurveTo(840,561,1102,535); context.stroke();
      context.beginPath(); context.moveTo(1057,526); context.lineTo(1066,550); context.stroke();
      context.fillStyle="#B8D9C6";
      [[552,524,-0.55],[564,543,0.45],[1112,523,0.55]].forEach(([leafX,leafY,angle])=>{
        context.save(); context.translate(leafX,leafY); context.rotate(angle);
        context.beginPath(); context.ellipse(0,0,20,4,0,0,Math.PI*2); context.fill(); context.restore();
      });
      context.restore();
    }
    function actor(time,alpha) {
      const motion=motionAt(time);
      const draw=(index,weight)=>{
        const image=assets?.[actorFrames[index].src]; if (!image || weight<=0) return;
        const scale=0.25*motion.scale, foot=feet[index];
        context.save(); context.globalAlpha*=alpha*weight;
        context.translate(motion.x,motion.y); context.rotate(motion.angle);
        context.drawImage(image,-foot[0]*scale,-foot[1]*scale,640*scale,640*scale);
        context.restore();
      };
      draw(motion.frame,1-motion.mix); draw(motion.nextFrame,motion.mix);
      return motion;
    }
    function seal(alpha) {
      if (alpha<=0) return;
      context.save(); context.globalAlpha*=alpha; context.translate(1166,552); context.rotate(-0.045);
      context.fillStyle="#8B3026"; context.fillRect(0,0,126,144);
      context.strokeStyle="#E5C09A"; context.lineWidth=3; context.strokeRect(7,7,112,130);
      context.fillStyle="#FFF0D6"; context.textAlign="center"; context.textBaseline="middle";
      context.font='700 52px "STKaiti", "KaiTi", "Microsoft YaHei", serif';
      context.fillText("酷",63,44); context.fillText("朱",63,102); context.restore();
    }
    function ambience(time,alpha) {
      if (alpha<=0) return;
      context.save(); context.globalAlpha*=alpha*0.13; context.strokeStyle="#ADBBAF"; context.lineWidth=1;
      for(let index=0;index<18;index++) {
        const x=index<9?130+index*15:1410+(index-9)*15;
        const y=200+(index*73+time*0.04)%500;
        context.beginPath(); context.moveTo(x,y); context.lineTo(x-3,y+14); context.stroke();
      }
      context.restore();
    }
    function renderAt(time) {
      elapsed=Math.max(0,time);
      if (canvas.width!==width || canvas.height!==height) { canvas.width=width; canvas.height=height; }
      context.clearRect(0,0,width,height);
      const reduced=options.reducedMotion === true;
      const presentationTime=daily?2600:elapsed;
      const progress=daily||reduced?1:between(elapsed,280,1250);
      const fade=reduced?1:daily?1-between(elapsed,800,980):1-between(elapsed,3350,4600);
      context.save(); context.globalAlpha=fade;
      background(progress);
      const logoAlpha=daily?between(elapsed,30,330):reduced?1:between(elapsed,1550,2500);
      logo(logoAlpha); ambience(presentationTime,progress);
      let motion=null;
      if (!reduced && !daily && elapsed>=1050) motion=actor(elapsed,between(elapsed,1050,1190));
      seal(daily?between(elapsed,130,480):reduced?1:between(elapsed,2350,2700));
      context.restore();
      lastScene={state:getState(),scrollProgress:progress,logoAlpha,actor:motion,sealText:"酷朱"};
      return lastScene;
    }
    function tick(timestamp) {
      if (!active || finished) return;
      if (startedAt === null) startedAt=timestamp;
      renderAt(timestamp-startedAt);
      if (elapsed>=duration) finish("completed");
      else frameId=windowRef.requestAnimationFrame(tick);
    }
    function load() {
      if (assets) return Promise.resolve(assets);
      return Promise.all(sources.map(src=>new Promise((resolve,reject)=>{
        const image=new windowRef.Image(); image.decoding="async";
        image.onload=()=>resolve([src,image]); image.onerror=()=>reject(new Error(`启动素材不可用：${src}`)); image.src=src;
      }))).then(entries=>Object.fromEntries(entries));
    }
    function start() {
      if (active || finished) return false; active=true;
      if (options.reducedMotion === true) {
        // 减少动态效果不等待图片下载，120ms内交还宿主；服务ready仍由宿主独立确认。
        renderAt(duration); timerId=windowRef.setTimeout(()=>finish("reduced-motion"),120); return true;
      }
      loadTimer=windowRef.setTimeout(()=>finish("resource-timeout"),2000);
      load().then(loaded=>{
        if (!active || finished) return;
        windowRef.clearTimeout(loadTimer); loadTimer=0; assets=loaded;
        frameId=windowRef.requestAnimationFrame(tick);
      }).catch(error=>finish("resource-error",error));
      return true;
    }
    return Object.freeze({manifest,start,stop:finish,finish,renderAt,getState,getLastScene:()=>lastScene,isActive:()=>active,isFinished:()=>finished});
  }
  return Object.freeze({createStartupPlayer,motionAt});
});
