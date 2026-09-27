// 不联网、不调用模型；验证交付证据哈希和关键结论。
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const json=rel=>JSON.parse(fs.readFileSync(path.join(root,rel),'utf8').replace(/^\uFEFF/,''));
const manifest=json('evidence-manifest.json');
for(const item of manifest.files){const file=path.resolve(root,item.path);assert(file.startsWith(root+path.sep));const bytes=fs.readFileSync(file);assert.equal(bytes.length,item.bytes,item.path);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),item.sha256,item.path)}
const install=json('evidence/install-result.json');assert.equal(install.exit_code,0);assert(install.userdata_unchanged);assert(install.binary_checks.every(x=>x.match));
const settings=json('evidence/installed-qwen-settings.json');assert.equal(settings.model,'qwen3.8-flash');assert(settings.base_url_unchanged&&settings.credential_unchanged);assert.equal(settings.image_input_strategy,'native');
for(const label of ['image-direct','image-via-agnes','installed-image-direct','installed-image-via-agnes']){const r=json(`evidence/${label}-summary.json`);assert(r.answer.includes('JADE-7382'));assert.equal(r.errors.length,0);assert.equal(r.run.state,'completed')}
const none=json('evidence/installed-none-check.json');assert(none.success);assert.equal(none.reasoning_events,0);
const svg=json('evidence/svg-metrics.json');assert.equal(svg.rounds.length,5);assert.equal(svg.total_tools,35);assert(svg.rounds.every(r=>r.run_state==='completed'));assert.equal(svg.total_usage[0].requests,36);
for(const label of ['paint-r1','paint-r2']){const events=json(`evidence/${label}-events.json`);const result=events.find(e=>e.data.kind==='tool-result').data.content;assert(result.includes('computer-use-task-controller'));assert(result.includes('invalid_plan'));assert(result.includes('"steps_completed":0'));assert(result.includes('"goal_achieved":false'))}
for(const rel of ['evidence/pelican-r5-browser-qa/report.json','evidence/pelican-r5-extended-qa/report.json','evidence/installed-settings-browser-qa/report.json']){const report=json(rel);assert(report.passed,rel)}
console.log(JSON.stringify({passed:true,verified_files:manifest.files.length,credentials_not_included:manifest.credential_scan_passed,svg:'PASS',paint:'EXPECTED_RECORDED_FAILURE',installed_version:'0.2.13'}));
