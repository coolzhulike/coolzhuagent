$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath '.').Path
if($taskRoot -ne 'C:\Users\zhupu\.codex\worktrees\input-recovery-20261004\coolzhuagent'){throw '工作树不一致'}
$taskCommit=(git rev-parse HEAD).Trim()
$taskFolder=Join-Path $taskRoot 'tmp/2026-10-08-release-106'
$taskProcess=Start-Process -FilePath 'C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe' -ArgumentList @('-NoProfile','-File','tmp/2026-10-08-release-106/child-build.ps1') -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $taskFolder 'build-stdout.log') -RedirectStandardError (Join-Path $taskFolder 'build-stderr.log') -Wait -PassThru
$taskExit=$taskProcess.ExitCode
if($null -eq $taskExit){throw '未捕获构建子进程退出码，不标成功'}
@{version='0.2.106';source_commit=$taskCommit;exit_code=$taskExit;stdout='build-stdout.log';stderr='build-stderr.log'}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $taskFolder 'build-result.json') -Encoding UTF8
Write-Output "正式构建子进程退出码：$taskExit"
exit $taskExit
