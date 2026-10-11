$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskVerified=Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-10-release-127/package-127-verification.json') | ConvertFrom-Json
$taskMsi=[IO.Path]::GetFullPath((Join-Path $taskRoot $taskVerified.msi_relative_path))
$taskDist=[IO.Path]::GetFullPath((Join-Path $taskRoot 'dist'))
if([IO.Path]::GetDirectoryName($taskMsi) -ne $taskDist -or (Get-FileHash -LiteralPath $taskMsi).Hash -ne $taskVerified.msi_sha256){throw 'MSI路径或摘要未匹配正式核验'}
$taskLog=Join-Path $taskRoot 'tmp/2026-10-10-release-127/install-127.log'
$taskInstaller=Start-Process -FilePath 'C:/Windows/System32/msiexec.exe' -ArgumentList @('/i',('"'+$taskMsi+'"'),'/passive','/norestart','/L*v',('"'+$taskLog+'"')) -Verb RunAs -WindowStyle Hidden -Wait -PassThru
@{msi=$taskMsi;log=$taskLog;exit_code=$taskInstaller.ExitCode} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-10-release-127/install-127-result.json') -Encoding utf8
Write-Output ('正常管理员安装返回：'+$taskInstaller.ExitCode)
if($taskInstaller.ExitCode -notin @(0,3010)){exit 1}
