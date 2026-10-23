import json,os,glob,shutil,re
E=r"D:\coolzhuagent\docs\plans\harness-alignment\evidence\HA-11\20261021"
T=r"D:\coolzhuagent\tmp\ha11-run"
os.makedirs(E+r"\harness",exist_ok=True)
for f in("run-case.ps1","run-pairs.ps1","analyze.py","summ.ps1","series2.ps1","b03-baseline.txt","b03-candidate.txt","b04-baseline.txt","b04-candidate.txt","b04-followup.txt","b05-baseline.txt","b05-candidate.txt"):
    shutil.copy(os.path.join(T,f),os.path.join(E,"harness",f))
for c in("b03","b04","b05"):
    shutil.copy(os.path.join(T,f"metrics-{c}.json"),os.path.join(E,f"metrics-{c}.json"))
for side in("baseline","candidate"):
    lines=[l for l in open(rf"{T}\{side}\ws\err.log",encoding="utf-8",errors="replace") if "SHELL-GATE" in l]
    open(os.path.join(E,f"shell-gate-not-exempt-{side}.log"),"w",encoding="utf-8").writelines(lines)
def med(x):
    x=sorted(x);n=len(x);return x[n//2] if n%2 else (x[n//2-1]+x[n//2])/2
out=["# HA-11 B03–B05：基线/候选各 3 组配对（系列 3，最终）","",
"- 模型：`qwen3.8-flash`（provider `custom`，effort `auto`），session `session-1792519612697-22ffea3a2956ec99`，每轮新建空 room，基线/候选交替。",
"- 隔离：`tmp/ha11-run/{baseline,candidate}/ws` 为各自可写工作区副本，端口 8766/8767，独立 data_dir；未触碰真实仓库。",
"- 基线：`tmp/ha11-baseline` 重建版（补齐未跟踪依赖，非原样快照），**并打了与候选相同的 shell 开发命令放行补丁**（见下）。",
"- 系列 1（`runs-series1-before-output-suffix-fix/`）、系列 2（`runs-series2-before-chdir-echo-widening/`）是白名单更窄时的同场景结果，保留但不作为本表依据；`runs-series1*/b03-baseline-attempt0-noshell` 是默认档位看不到 shell 的最早尝试。",
"",
"## 前置缺陷与修复（候选与基线同补丁）",
"",
"默认房间档位 workspace-write 不暴露 bash/PowerShell（tool-registry 标 DangerFullAccess），B03–B05 因此无法执行 `cargo test`。",
"修复：workspace-write 暴露 bash/PowerShell；工作区内单条开发命令（cargo build/test/check/clippy/fmt…、只读 git、npm test/run build|test|lint、`--version`，可带尾部 `2>&1`、`| Out-String`、`| Select-Object -Last N`、末尾退出码回显，可带开头的工作区内 `Set-Location/cd`；另放行工作区内的 `dir/ls/Get-ChildItem/tree` 只读列目录；拒绝 `--manifest-path/--target-dir`）以“以工作区为目标的 WorkspaceWrite”进入原闸门自动放行，其余 shell 调用仍 require-confirm。",
"文件：`workspace_dev_command.rs`（新）、`main.rs`（`workspace_dev_gate_inputs`、allowlist、2 处闸门调用）、`host_child_agent.rs`（`permits_tool`）。单测：`workspace_dev_command` 3 项、`llm_tool_definitions` 6 项通过；`llm_tool_definitions_whitelist_follows_room_permission_scope` 旧断言已更新。",
"",
]
for c,title,note in(("b03","B03 单文件修复并 cargo test",""),("b04","B04 三文件修复 + 进度追问（追问要求不再运行命令）",""),("b05","B05 可控长等待（测试 sleep 75s，要求只运行一次）","")):
    rows=json.load(open(os.path.join(E,f"metrics-{c}.json"),encoding="utf-8"))
    out+=[f"## {title}","","| 轮次 | 侧 | 状态 | 耗时 ms | 工具调用 | shell 成功 | shell dry-run(require-confirm) | shell 失败 | 追问期工具调用 | 我方独立 cargo test 退出码 | 合同 |","|---|---|---|---|---|---|---|---|---|---|---|"]
    for x in rows:
        ok = (x["shell_ok"]+x["shell_failed"])>=1 and x["indep_exit"]==0 and x["followup_tool_calls"]==0
        out.append(f"| {x['round']} | {x['side']} | {x['status']} | {x['elapsed_ms']} | {x['tool_calls']} | {x['shell_ok']} | {x['shell_dry_run']} | {x['shell_failed']} | {x['followup_tool_calls']} | {x['indep_exit']} | {'满足' if ok else '未满足（模型未真正运行测试）'} |")
    for s in("baseline","candidate"):
        el=[x["elapsed_ms"] for x in rows if x["side"]==s]; tc=[x["tool_calls"] for x in rows if x["side"]==s]
        out.append(f"\n- {s}：耗时中位 {med(el)/1000:.1f}s（{', '.join(f'{v/1000:.1f}' for v in el)}），工具调用中位 {med(tc)}")
    out.append("")
open(os.path.join(E,"b03-b05.md"),"w",encoding="utf-8").write("\n".join(out))
print("\n".join(out)[-3800:])
