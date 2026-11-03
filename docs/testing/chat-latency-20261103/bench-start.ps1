param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Tag, [string]$Ws = 'D:\coolzhuagent\tmp\real-todo-ws')
$ws = $Ws
Get-Process coolzhu-web-console -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$env:COOLZHU_RUNTIME_DIR = $ws
$out = "D:\coolzhuagent\tmp\bench-$Tag-stdout.log"
$err = "D:\coolzhuagent\tmp\bench-$Tag-stderr.log"
$p = Start-Process -FilePath $Exe -WorkingDirectory $ws -RedirectStandardOutput $out -RedirectStandardError $err -PassThru -WindowStyle Hidden
$sw = [Diagnostics.Stopwatch]::StartNew()
$listening = $null
while ($sw.Elapsed.TotalSeconds -lt 900) {
    $text = ((Get-Content $out -Raw -ErrorAction SilentlyContinue) + (Get-Content $err -Raw -ErrorAction SilentlyContinue))
    if (-not $listening -and $text -match '已启动: http') { $listening = $sw.Elapsed.TotalSeconds }
    if ($text -match '启动后台初始化完成') { break }
    if ($p.HasExited) { Write-Output "进程已退出 code=$($p.ExitCode)"; exit 1 }
    Start-Sleep -Milliseconds 500
}
Write-Output ("pid={0} listening_s={1:N1} init_ready_s={2:N1}" -f $p.Id, $listening, $sw.Elapsed.TotalSeconds)
