/* 卷轴山水启动合成：复用固定字形与静坐人物素材，人物只做呼吸起伏，不做骨骼动画。 */
(function (root, factory) {
  const api = factory(root);
  if (typeof module === "object" && module.exports) module.exports = api;
  else root.CoolzhuScrollStartupPlayer = api;
})(typeof globalThis === "object" ? globalThis : this, function (root) {
  "use strict";
  const clamp = value => Math.max(0, Math.min(1, value));
  const ease = value => { const t = clamp(value); return t * t * (3 - 2 * t); };
  const between = (time, start, end) => ease((time - start) / (end - start));
  function createStartupPlayer(options = {}) {
    const manifest = options.manifest;
    const canvas = options.canvas;
    if (!manifest || !canvas?.getContext) throw new Error("启动卷轴缺少画布或素材清单");
    const context = canvas.getContext("2d");
    if (!context) throw new Error("启动卷轴无法创建画布");
    const windowRef = options.windowRef || root;
    const documentRef = options.documentRef || root.document;
    const daily = options.mode === "daily";
    // 日常启动也播放完整动作；两种模式只压缩展开和收势，不省略人物。
    const duration = daily ? 6500 : 7800;
    const sceneDuration = 7800;
    const width=1680, height=900;
    let assets = options.assets || null;
    let active = false, finished = false, frameId=0, timerId=0, loadTimer=0, startedAt=null;
    let elapsed = 0, lastScene = null, frameCount = 0;
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
    function getState() { const sceneTime=elapsed*sceneDuration/duration; return {elapsedMs:elapsed,totalDurationMs:duration,frameCount,completed:elapsed>=duration,mode:daily?"daily":"first",phase:elapsed>=duration?"done":sceneTime<1400?"unfold":sceneTime<5700?"logo-and-meditation":sceneTime<6700?"hold":"handoff"}; }
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
      // 原字形不变，低对比竹叶作为次层构图，避免视觉元素挤入字内。
      context.globalAlpha*=0.45;
      context.fillStyle="#B8D9C6";
      [[552,524,-0.55],[564,543,0.45],[1112,523,0.55]].forEach(([leafX,leafY,angle])=>{
        context.save(); context.translate(leafX,leafY); context.rotate(angle);
        context.beginPath(); context.ellipse(0,0,20,4,0,0,Math.PI*2); context.fill(); context.restore();
      });
      context.restore();
    }
    function actor(time,alpha,still) {
      const frame=actorFrames[0], image=assets?.[frame.src];
      const rect=frame.sourceRect, anchor=frame.anchor;
      // 呼吸：4.6 秒一循环的轻微缩放与下缘起伏；减少动态或静态调用时冻结在呼气中点。
      const breath=still?0:Math.sin(time/4600*Math.PI*2);
      const scale=200/rect.height*(1+0.02*breath);
      const x=840, y=745+2.4*breath;
      if (!image || alpha<=0) return {x,y,scale,height:rect.height*scale};
      context.save(); context.globalAlpha*=alpha;
      context.translate(x,y);
      context.drawImage(image,rect.x,rect.y,rect.width,rect.height,-anchor.x*scale,-anchor.y*scale,rect.width*scale,rect.height*scale);
      context.restore();
      return {x,y,scale,height:rect.height*scale};
    }
    function aura(time,alpha,still) {
      if (alpha<=0) return;
      // 低对比气息光晕：跟随同一呼吸相位，克制在人物身后，不遮字标。
      const breath=still?0:Math.sin(time/4600*Math.PI*2);
      context.save();
      context.globalAlpha*=alpha*(0.10+0.05*breath);
      const gradient=context.createRadialGradient?.(840,650,8,840,650,160);
      if (gradient) {
        gradient.addColorStop(0,"rgba(231,214,170,0.85)");
        gradient.addColorStop(1,"rgba(231,214,170,0)");
        context.fillStyle=gradient;
      } else context.fillStyle="rgba(231,214,170,0.5)";
      context.beginPath(); context.ellipse(840,650,168,132,0,0,Math.PI*2); context.fill();
      context.restore();
      if (still) return;
      // 稀疏气息微粒缓慢上浮，亮度压在不干扰字标的范围。
      context.save(); context.globalAlpha*=alpha*0.16; context.fillStyle="#D9C896";
      for(let index=0;index<9;index++) {
        const px=776+((index*47)%132), py=730-((time*0.018+index*73)%170);
        context.beginPath(); context.arc(px,py,1.7,0,Math.PI*2); context.fill();
      }
      context.restore();
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
      const presentationTime=elapsed*sceneDuration/duration;
      const progress=reduced?1:between(presentationTime,280,1400);
      const fade=reduced?1:1-between(presentationTime,6700,7800);
      context.save(); context.globalAlpha=fade;
      background(progress);
      const logoAlpha=reduced?1:between(presentationTime,1800,3500);
      logo(logoAlpha); ambience(presentationTime,progress);
      let motion=null;
      // 静坐人物随字标淡入，定势段保持；减少动态时若资产已在则静态呈现。
      const figureAlpha=reduced?1:between(presentationTime,1900,2700);
      if (figureAlpha>0) {
        if (assets?.[actorFrames[0].src]) aura(presentationTime,figureAlpha,reduced);
        motion=actor(presentationTime,figureAlpha,reduced);
      }
      context.restore();
      lastScene={state:getState(),scrollProgress:progress,logoAlpha,actor:motion,figure:"meditation"};
      return lastScene;
    }
    function tick(timestamp) {
      if (!active || finished) return;
      try {
        if (startedAt === null) startedAt=timestamp;
        renderAt(timestamp-startedAt);
        frameCount++;
        if (frameCount === 1) options.onFirstFrame?.();
        if (elapsed>=duration) finish("completed");
        else frameId=windowRef.requestAnimationFrame(tick);
      } catch (error) {
        // 帧绘制失败立即收尾，不等待演出超时后才退出。
        finish("resource-error",error);
      }
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
        options.onAssetsReady?.();
        frameId=windowRef.requestAnimationFrame(tick);
      }).catch(error=>finish("resource-error",error));
      return true;
    }
    return Object.freeze({manifest,start,stop:finish,finish,renderAt,getState,getLastScene:()=>lastScene,isActive:()=>active,isFinished:()=>finished});
  }
  return Object.freeze({createStartupPlayer});
});
