"use strict";
// 只检查启动生命周期与宿主边界；画面与动作必须由实际桌面截图验收。
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const bridge = require("../coolzhu-seven-letter-bridge.js");
const player = require("../scroll-startup-player.js");
const manifest = require("../coolzhu-seven-letter-manifest.js");

function eventTarget() {
  const handlers = new Map();
  return {
    addEventListener(type, fn) { handlers.set(type, fn); },
    removeEventListener(type) { handlers.delete(type); },
    dispatchEvent(event) { handlers.get(event.type)?.(event); return true; },
  };
}
function harness(options = {}) {
  const events=[], nativeEvents=[], timers=new Map(), frames=new Map(); let nextId=1;
  const documentRef=eventTarget(), storage=new Map();
  const windowRef={
    Image: class { set src(value) { this.url=value; } }, // 有意悬置，覆盖资源超时与加载前退出。
    CustomEvent: class {constructor(type, init) {this.type=type; this.detail=init.detail;}},
    setTimeout(fn, delay) { const id=nextId++; timers.set(id,{fn,delay}); return id; },
    clearTimeout(id) {timers.delete(id);},
    requestAnimationFrame(fn) {const id=nextId++; frames.set(id,fn); return id;},
    cancelAnimationFrame(id) {frames.delete(id);},
    localStorage:{getItem:key=>storage.get(key),setItem:(key,value)=>storage.set(key,value)},
    __TAURI__:{event:{emit(type, detail) {nativeEvents.push({type,detail}); return Promise.resolve();}}},
  };
  const dispatch=documentRef.dispatchEvent;
  documentRef.dispatchEvent=event=>{events.push(event); return dispatch(event);};
  const context=new Proxy({globalAlpha:1}, {get:(target,key)=>key in target?target[key]:()=>{}});
  const canvas={width:1680,height:900,style:{},getContext:()=>context,setAttribute(){}}, root={style:{}};
  const skipButton={...eventTarget(),style:{}};
  const controller=bridge.createBridge({documentRef,windowRef,canvas,presentationRoot:root,skipButton,playerApi:player,manifest,mode:"first",...options});
  return {controller,documentRef,windowRef,events,nativeEvents,canvas,root,skipButton,storage,timers,frames,
    expire(delay) { const hit=[...timers].find(([,timer])=>timer.delay===delay); assert.ok(hit,`缺少 ${delay}ms 定时器`); timers.delete(hit[0]); hit[1].fn(); },
    tick(time) {const callbacks=[...frames.values()]; frames.clear(); callbacks.forEach(fn=>fn(time));},
    completed() {return events.filter(event=>event.type===bridge.COMPLETE_EVENT);},
  };
}
function completedOnce(h,reason) {
  assert.equal(h.completed().length,1);
  assert.equal(h.completed()[0].detail.reason,reason);
  assert.equal(h.nativeEvents.length,1);
  assert.equal(h.nativeEvents[0].detail.consoleVisible,false);
  assert.equal(h.canvas.hidden,true);
  assert.equal(h.root.hidden,true);
  assert.equal(h.controller.isFinished(),true);
  assert.equal(h.timers.size,0);
  assert.equal(h.frames.size,0);
}
async function main() {
  const sources=[manifest.background.source.src,...manifest.actor.frames.map(frame=>frame.src),...manifest.letters.map(letter=>letter.glyphSource.src)];
  for (const source of new Set(sources)) assert.ok(fs.existsSync(path.join(__dirname,"..",source)),`缺少生产素材：${source}`);

  const skipped=harness(); skipped.controller.start();
  skipped.documentRef.dispatchEvent({type:"keydown",key:"Escape",preventDefault(){}});
  skipped.skipButton.dispatchEvent({type:"click"}); skipped.controller.finish("skipped");
  completedOnce(skipped,"skipped");

  const reduced=harness({reducedMotion:true}); reduced.controller.start();
  assert.equal([...reduced.timers.values()].some(timer=>timer.delay===2000),false,"减少动态效果不得等待图像");
  reduced.expire(120); completedOnce(reduced,"reduced-motion");

  const missing=harness(); missing.controller.start(); missing.expire(2000);
  completedOnce(missing,"resource-timeout");

  const restore=harness({mode:"restore"}); assert.equal(restore.controller.start(),null);
  completedOnce(restore,"restored");

  const full=harness({assets:{}}); full.controller.start(); await Promise.resolve();
  full.tick(100); full.tick(4700); completedOnce(full,"completed");
  assert.equal(full.storage.get("coolzhu.scroll-startup.seen.v1"),"1");

  const daily=harness({mode:"daily",assets:{}}); daily.controller.start(); await Promise.resolve();
  daily.tick(100); daily.tick(1080); completedOnce(daily,"completed");

  const broken=harness({playerApi:{createStartupPlayer(){throw new Error("创建失败");}}});
  broken.controller.start(); completedOnce(broken,"resource-error");
  console.log("启动生命周期：完整 / 日常 / Esc / 减少动态 / 资源超时 / 恢复 / 创建失败，全部通过。");
}
main().catch(error=>{console.error(error); process.exitCode=1;});
