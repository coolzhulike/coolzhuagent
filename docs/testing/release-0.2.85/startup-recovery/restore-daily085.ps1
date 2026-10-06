$ErrorActionPreference='Stop'
$taskDir=Split-Path -Parent $PSCommandPath
if(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue){throw '日常端口被占用，保留其它进程'}
# 仅移除当前启动脚本的测试覆盖；不改用户配置或输入安全库。
Get-ChildItem Env: | Where-Object {$_.Name -like 'COOLZHU_*'} | ForEach-Object {Remove-Item -LiteralPath ('Env:'+$_.Name)}
Remove-Item -LiteralPath 'Env:WEBVIEW2_USER_DATA_FOLDER','Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' -ErrorAction SilentlyContinue
$taskExe='C:/Program Files/CoolzhuAgent/COOLZHU-AGENT.exe'
$taskDaily=Start-Process -FilePath $taskExe -WorkingDirectory 'C:/Users/zhupu/coolzhuagent' -WindowStyle Hidden -PassThru
@{stage='正常桌面启动入口';daily_launcher=@{pid=$taskDaily.Id;path=$taskExe;created=$taskDaily.StartTime.ToUniversalTime().ToString('o')};observed_utc=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskDir 'restored-daily-085-process.json') -Encoding utf8
Write-Output '原日常桌面入口已启动，等待自检与实际进程核验。'
