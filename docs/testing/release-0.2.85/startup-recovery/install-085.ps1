$ErrorActionPreference='Stop'
$taskRoot='C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent'
$taskMsi=Join-Path $taskRoot 'dist/CoolzhuAgent-0.2.85.msi'
$taskLog=Join-Path $taskRoot 'tmp/2026-10-06-browser-oop/install-085.log'
$taskInstaller=Start-Process -FilePath 'C:/Windows/System32/msiexec.exe' -ArgumentList @('/i',('"'+$taskMsi+'"'),'/passive','/norestart','/L*v',('"'+$taskLog+'"')) -Verb RunAs -WindowStyle Hidden -PassThru
$taskInstaller.WaitForExit()
@{msi=$taskMsi;log=$taskLog;exit_code=$taskInstaller.ExitCode} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot 'tmp/2026-10-06-browser-oop/install-085-result.json') -Encoding utf8
Write-Output ('正常管理员安装返回：'+$taskInstaller.ExitCode)
if($taskInstaller.ExitCode -notin @(0,3010)){exit 1}
