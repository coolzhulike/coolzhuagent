param(
    [string]$RuntimeDirectory = $PSScriptRoot,
    [int]$Port = 18775,
    [int]$MockPort = 18776,
    [string]$InstalledExe = 'C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe',
    [string]$StaticRoot = 'C:\Program Files\CoolzhuAgent\modules\gui-web\packages\web-console'
)
$ErrorActionPreference = 'Stop'
if ($Port -eq 8765 -or $MockPort -eq 8765 -or $Port -eq $MockPort -or $Port -lt 1024 -or $MockPort -lt 1024 -or $Port -gt 65535 -or $MockPort -gt 65535) { throw '必须使用两个独立的本地验收端口，禁止8765' }
if (-not (Test-Path -LiteralPath $InstalledExe -PathType Leaf) -or -not (Test-Path -LiteralPath $StaticRoot -PathType Container)) { throw '安装程序或安装静态目录不存在' }
$nodeExecutable = (Get-Command node -ErrorAction Stop).Source
foreach ($testPort in @($Port, $MockPort)) {
    if (Get-NetTCPConnection -LocalPort $testPort -State Listen -ErrorAction SilentlyContinue) { throw "验收端口 $testPort 已占用；本脚本不会停止已有服务" }
}
$acceptanceRoot = [System.IO.Path]::GetFullPath($RuntimeDirectory)
New-Item -ItemType Directory -Path $acceptanceRoot -Force | Out-Null
$fixtureConfig = Join-Path $acceptanceRoot 'coolzhu.toml'
$fixtureMarker = Join-Path $acceptanceRoot '.release-acceptance-fixture'
if (-not (Test-Path -LiteralPath $fixtureMarker) -and ((Test-Path -LiteralPath $fixtureConfig) -or (Test-Path -LiteralPath (Join-Path $acceptanceRoot '.coolzhu')))) { throw '该目录已有非夹具配置或会话数据，拒绝启动；请选择全新隔离目录' }
'Coolzhu 0.2.12 合成验收数据；允许本目录的隔离测试。' | Set-Content -LiteralPath $fixtureMarker -Encoding UTF8
if (-not (Test-Path -LiteralPath $fixtureConfig)) {
    "[web]`nbind_addr = `"127.0.0.1:$Port`"`n[model]`nenable_real_llm = true`nenable_llm_tools = true`nllm_tool_exposure = `"all`"`n[tool]`ndev_open_permissions = true`n[computer_use]`nenabled = false`n[pet]`nenabled = false`n" | Set-Content -LiteralPath $fixtureConfig -Encoding UTF8
} elseif (-not ((Get-Content -LiteralPath $fixtureConfig -Raw).Contains("127.0.0.1:$Port"))) { throw '已有夹具配置端口不符，请使用新的隔离目录' }
$env:COOLZHU_RUNTIME_DIR = $acceptanceRoot
$env:COOLZHU_WEB_STATIC_ROOT = $StaticRoot
$env:COOLZHU_WEB_SESSION_STORE = Join-Path $acceptanceRoot '.coolzhu/web-sessions.json'
$env:COOLZHU_WEB_SESSION_DB = Join-Path $acceptanceRoot '.coolzhu/web-sessions.sqlite3'
$env:COOLZHU_WEB_ATTACHMENT_STORE = Join-Path $acceptanceRoot '.coolzhu/attachments'
$env:RELEASE_ACCEPTANCE_DIR = $acceptanceRoot
$env:RELEASE_MOCK_PORT = "$MockPort"
$env:RELEASE_BASE_URL = "http://127.0.0.1:$Port"
$env:RELEASE_MOCK_URL = "http://127.0.0.1:$MockPort"
$server = Start-Process -FilePath $InstalledExe -WorkingDirectory $acceptanceRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $acceptanceRoot 'server-out.log') -RedirectStandardError (Join-Path $acceptanceRoot 'server-err.log') -PassThru
$server.Id | Set-Content -LiteralPath (Join-Path $acceptanceRoot 'server.pid')
$mockFixture = Join-Path $PSScriptRoot 'mock-openai.cjs'
$mock = Start-Process -FilePath $nodeExecutable -ArgumentList @('"' + $mockFixture + '"') -WorkingDirectory $acceptanceRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $acceptanceRoot 'mock-out.log') -RedirectStandardError (Join-Path $acceptanceRoot 'mock-err.log') -PassThru
$mock.Id | Set-Content -LiteralPath (Join-Path $acceptanceRoot 'mock.pid')
[pscustomobject]@{ server_pid=$server.Id; mock_pid=$mock.Id; url="http://127.0.0.1:$Port/"; mock_url="http://127.0.0.1:$MockPort"; runtime_dir=$acceptanceRoot; executable=$InstalledExe; static_root=$StaticRoot } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $acceptanceRoot 'launch.json')
# 等两个服务实际可用，避免后续测试与初始化抢跑；不重复创建实例。
$ready = $false
for ($attempt = 0; $attempt -lt 50; $attempt++) {
    try {
        $null = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/api/workspace" -TimeoutSec 1
        $mockHealth = Invoke-RestMethod -Uri "http://127.0.0.1:$MockPort/health" -TimeoutSec 1
        if ($mockHealth.pid -eq $mock.Id) { $ready = $true; break }
    } catch { }
    Start-Sleep -Milliseconds 200
}
if (-not $ready) { throw '隔离服务未就绪；检查本目录日志及launch.json，不要直接重复启动。' }
Get-Content -LiteralPath (Join-Path $acceptanceRoot 'launch.json')
