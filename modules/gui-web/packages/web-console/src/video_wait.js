/* 视频等待只投影后端任务状态；停止等待不代表供应商已取消生成。 */
window.CoolzhuVideoWait = (() => {
  const jobs = new Map();
  const terminal = new Set(["completed", "failed", "unknown"]);
  function removeControl(job) { job.api.article(job.id)?.querySelector(".video-wait-control")?.remove(); }
  function finish(job) {
    clearTimeout(job.timer); job.controller?.abort();
    if (jobs.get(job.key) !== job) return;
    removeControl(job); jobs.delete(job.key); job.api.clearTask(job.id);
  }
  function control(job, note = "") {
    if (jobs.get(job.key) !== job) return;
    const article = job.api.article(job.id); if (!article) return;
    let row = article.querySelector(".video-wait-control");
    if (!row) {
      row = document.createElement("div"); row.className = "video-wait-control";
      const button = document.createElement("button"); button.type = "button";
      button.textContent = "停止等待"; button.title = "停止本地提交和状态等待；远端任务可能仍在执行";
      button.addEventListener("click", () => void interrupt(job));
      const status = document.createElement("small"); status.setAttribute("role", "status");
      row.append(button, status); article.querySelector(".message-content")?.after(row);
    }
    const button = row.querySelector("button");
    button.disabled = job.stopping || job.stopRequested || !job.data?.scope?.session_id || !job.data?.scope?.chat_room_id || job.data?.can_interrupt === false;
    button.textContent = job.stopRequested ? "正在停止等待…" : "停止等待";
    row.querySelector("small").textContent = note || (job.stopRequested ? "已请求停止本地等待，等待后台确认；远端结果尚未确认。" : job.data?.can_interrupt === false ? "当前任务不能再接受停止请求，正在查询最终状态。" : "仅停止本地等待，不保证远端取消。");
  }
  async function interrupt(job) {
    if (jobs.get(job.key) !== job || !job.api.matches(job.scope) || job.stopping || job.stopRequested || !job.data?.scope?.session_id || !job.data?.scope?.chat_room_id) return;
    job.stopping = true; control(job);
    try {
      const response = await fetch(`/api/videos/${encodeURIComponent(job.id)}/interrupt`, {
        method: "POST", headers: {"Content-Type":"application/json"},
        body: JSON.stringify({session_id:job.data.scope.session_id,chat_room_id:job.data.scope.chat_room_id}),
      });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "停止等待请求失败");
      if (!job.api.matches(job.scope) || jobs.get(job.key) !== job) return;
      if (data.stop_requested === true || data.status === "stop_requested") {
        job.stopRequested = true; control(job);
      } else control(job, data.message || "已返回最新状态，等待后台确认任务结果。");
    } catch (error) { if (job.api.matches(job.scope)) control(job, `未能停止等待：${error.message}`); }
    finally {
      job.stopping = false;
      if (job.api.matches(job.scope) && jobs.get(job.key) === job) {
        control(job, job.api.article(job.id)?.querySelector(".video-wait-control small")?.textContent || "");
        // 成功或404都重新查询权威状态，不把停止请求本身当作任务已终结。
        if (!job.polling) { clearTimeout(job.timer); job.timer = setTimeout(() => void poll(job), 0); }
      }
    }
  }
  async function poll(job) {
    if (jobs.get(job.key) !== job) return;
    if (!job.api.matches(job.scope)) { finish(job); return; }
    if (job.attempts++ >= 260) {
      job.api.content(job.id, "本地状态查询已超时，远端结果未确认。刷新会话后可重新查询，不会自动重发生成请求。");
      removeControl(job); finish(job); return;
    }
    job.polling = true; job.controller = new AbortController();
    try {
      const response = await fetch(`/api/videos/${encodeURIComponent(job.id)}`, {signal:job.controller.signal});
      const data = await response.json();
      if (!response.ok) throw new Error(data.error || "视频任务状态读取失败");
      if (!job.api.matches(job.scope) || jobs.get(job.key) !== job) return;
      job.data = data; job.stopRequested ||= data.stop_requested === true;
      if (data.status === "completed" && data.url) {
        removeControl(job); job.api.completed(job.id, data.url); finish(job); return;
      }
      if (data.status === "failed" || data.status === "unknown") {
        job.api.content(job.id, data.status === "unknown"
          ? "视频任务状态不可用，远端结果未确认；未自动重新提交生成请求。"
          : `视频任务已结束：${data.error || "任务失败，未返回可播放结果"}`);
        removeControl(job); finish(job); return;
      }
      if (terminal.has(data.status)) {
        job.api.content(job.id, "视频任务报告完成但没有返回可播放地址，请检查轨迹；未自动重新提交生成请求。");
        removeControl(job); finish(job); return;
      }
      const elapsedStart = Number(data.elapsed_ms) > 0 ? Date.now() - Number(data.elapsed_ms) : job.startedAt;
      const progress = Math.max(0, Math.min(100, Number(data.progress) || 0));
      job.api.task(job.id, {id:`video-${job.id}`,owner_agent:"视频生成",executor_agent:"视频生成",status:"运行中",
        summary:job.stopRequested ? "正在停止本地视频等待" : `视频生成中 ${progress}%`,startedAt:elapsedStart,timeout_ms:0});
      control(job);
    } catch (error) { if (error.name !== "AbortError" && job.api.matches(job.scope)) control(job, `状态查询失败，将继续查询：${error.message}`); }
    finally {
      job.polling = false;
      if (jobs.get(job.key) === job) job.timer = setTimeout(() => void poll(job), 5000);
    }
  }
  function start(id, api) {
    if (!id) return;
    scopeChanged();
    const scope = api.scope(); const key = `${scope.workspace}\u0000${scope.room}\u0000${id}`;
    if (jobs.has(key)) return;
    const job = {id,key,scope,api,attempts:0,startedAt:Date.now(),stopping:false,stopRequested:false,polling:false};
    jobs.set(key,job); job.timer = setTimeout(() => void poll(job), 0);
  }
  function scopeChanged() { for (const job of jobs.values()) if (!job.api.matches(job.scope)) finish(job); }
  window.addEventListener("beforeunload", () => { for (const job of jobs.values()) finish(job); });
  return {start,scopeChanged};
})();
