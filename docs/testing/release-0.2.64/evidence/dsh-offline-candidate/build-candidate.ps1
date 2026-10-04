param([ValidateSet('Prepare', 'Build')][string]$Phase)
$ErrorActionPreference = 'Stop'
$repo = 'C:\Users\zhupu\Desktop\coolzhuagent'
Set-Location -LiteralPath $repo
$taskRoot = Join-Path $repo 'tmp/2026-10-02-dsh-msi'
$env:PATH = 'C:\Users\zhupu\.cargo\bin;C:\Python314;' + $env:PATH
$env:COOLZHU_RELEASE_VERSION = '0.2.64'
$env:COOLZHU_BUILD_DATE = '2026-10-02'
$env:COOLZHU_GIT_SHA = (& git rev-parse HEAD).Trim()
$env:COOLZHU_BUILD_TARGET = 'x86_64-pc-windows-msvc'
$manifest = 'config/package-manifest-0.2.64-candidate.json'
$packageRoot = 'tmp/candidate-064-package'
$started = [DateTime]::UtcNow
try {
    if ($Phase -eq 'Prepare') {
        $result = & scripts/package-all.ps1 -Manifest $manifest -Configuration release -PackageRoot $packageRoot -Prepare
        $pointer = @($result) | Select-Object -Last 1
        $pointer | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $taskRoot 'prepare-pointer.json') -Encoding utf8
    } else {
        $preparation = Get-Content -LiteralPath (Join-Path $taskRoot 'prepare-pointer.json') -Raw | ConvertFrom-Json
        if (-not $preparation.freeze_record) { throw '未取得准备冻结记录' }
        $result = & scripts/package-all.ps1 -Manifest $manifest -Configuration release -PackageRoot $packageRoot -FreezeRecordPath $preparation.freeze_record
        @($result) | Select-Object -Last 1 | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $taskRoot 'package-pointer.json') -Encoding utf8
        & scripts/build-msi.ps1 -Version '0.2.64' -Configuration release -PackageRoot $packageRoot -SkipPackageBuild
    }
    @{phase=$Phase; outcome='completed'; started_utc=$started.ToString('o'); ended_utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot ($Phase.ToLowerInvariant()+'-outcome.json')) -Encoding utf8
} catch {
    @{phase=$Phase; outcome='failed'; error=$_.ToString(); started_utc=$started.ToString('o'); ended_utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot ($Phase.ToLowerInvariant()+'-outcome.json')) -Encoding utf8
    throw
}
