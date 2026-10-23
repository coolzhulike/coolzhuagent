import json,sys,glob,os,re,collections
case=sys.argv[1]
ev=r"D:\coolzhuagent\docs\plans\harness-alignment\evidence\HA-11\20261021\runs"
rows=[]
for side in("baseline","candidate"):
    audit=[]
    for l in open(rf"D:\coolzhuagent\tmp\ha11-run\{side}\data\tool-audit.jsonl",encoding="utf-8",errors="replace"):
        l=l.strip()
        if not l.startswith("{"): continue
        try: audit.append(json.loads(l))
        except Exception:
            try: audit.append(json.JSONDecoder().raw_decode(l)[0])
            except Exception: pass
    runs=sorted(glob.glob(os.path.join(ev,f"{case}-{side}-[0-9]*")))
    metas=[json.load(open(os.path.join(r,"meta.json"),encoding="utf-8-sig")) for r in runs]
    starts=sorted(int(re.search(r"room-(\d+)",m["room_id"]).group(1))/1000 for m in metas)
    for r,m in zip(runs,metas):
        t0=int(re.search(r"room-(\d+)",m["room_id"]).group(1))/1000
        later=[s for s in starts if s>t0]
        m2p=os.path.join(r,"meta2.json")
        e2=json.load(open(m2p,encoding="utf-8-sig"))["elapsed_ms"] if os.path.exists(m2p) else 0
        t1=t0+(m["elapsed_ms"]+e2+20000)/1000
        if not m.get("first_turn_end_ms"): m["first_turn_end_ms"]=int((t0+m["elapsed_ms"]/1000+8)*1000) if e2 else None
        # window ends at the next run start of the SAME side (other side uses another data dir)
        fe=m.get("first_turn_end_ms")
        cut=fe/1000 if fe else None
        ev_all=[a for a in audit if t0<=float(a["ts"])<t1]
        ev_=[a for a in ev_all if cut is None or float(a["ts"])<=cut]
        fu_=[a for a in ev_all if cut is not None and float(a["ts"])>cut]
        c=collections.Counter(a["tool_name"] for a in ev_)
        st=collections.Counter(a["status"] for a in ev_)
        dec=collections.Counter(a["permission"]["decision"] for a in ev_ if a.get("permission"))
        resp=json.load(open(os.path.join(r,"response.json"),encoding="utf-8-sig"))
        ver=json.load(open(os.path.join(r,"verify.json"),encoding="utf-8-sig")) if os.path.exists(os.path.join(r,"verify.json")) else {}
        fu_c=collections.Counter(a['tool_name'] for a in fu_)
        shell_dry_run=sum(1 for a in ev_ if a['tool_name'] in('bash','PowerShell') and a['status']=='dry-run-only'); shell_failed=sum(1 for a in ev_ if a['tool_name'] in('bash','PowerShell') and a['status'] not in('ok','dry-run-only'))
        shell_ok=sum(1 for a in ev_ if a['tool_name'] in('bash','PowerShell') and a['status']=='ok'); shell_dry=sum(1 for a in ev_ if a['tool_name'] in('bash','PowerShell') and a['status']!='ok')
        rows.append(dict(shell_dry_run=shell_dry_run,shell_failed=shell_failed,shell_ok=shell_ok,shell_not_ok=shell_dry,followup_tool_calls=sum(fu_c.values()),followup_by_tool=dict(fu_c),side=side,round=m["round"],status=resp["status"],elapsed_ms=m["elapsed_ms"],tool_calls=sum(c.values()),by_tool=dict(c),by_status=dict(st),by_decision=dict(dec),
                         require_confirm=dec.get("require-confirm",0),indep_exit=ver.get("independent_cargo_test_exit"),final_lib=ver.get("final_lib")))
rows.sort(key=lambda x:(x["round"],x["side"]))
json.dump(rows,open(rf"D:\coolzhuagent\tmp\ha11-run\metrics-{case}.json","w",encoding="utf-8"),ensure_ascii=False,indent=1)
for x in rows: print(x["side"],x["round"],x["status"],x["elapsed_ms"],"tools=",x["tool_calls"],x["by_tool"],"statuses=",x["by_status"],"confirm=",x["require_confirm"],"shell_ok=",x["shell_ok"],"shell_not_ok=",x["shell_not_ok"],"followup_tools=",x["followup_tool_calls"],x["followup_by_tool"],"indep=",x["indep_exit"])


