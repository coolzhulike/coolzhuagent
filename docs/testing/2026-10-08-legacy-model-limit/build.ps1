$ErrorActionPreference='Stop'
& cargo build -p coolzhu-web-console --offline *> 'tmp/2026-10-08-legacy-model-limit/build.log'
$taskBuildExit=$LASTEXITCODE
@{exit_code=$taskBuildExit;command='cargo build -p coolzhu-web-console --offline'} | ConvertTo-Json | Set-Content -Encoding utf8 'tmp/2026-10-08-legacy-model-limit/build-result.json'
Get-Content 'tmp/2026-10-08-legacy-model-limit/build.log' -Tail 14
exit $taskBuildExit
