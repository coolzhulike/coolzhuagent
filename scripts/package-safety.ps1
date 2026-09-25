param(
    [Parameter(Mandatory = $true)]
    [string]$Root,
    [string]$ReportPath
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
    throw "package root not found: $Root"
}

$resolvedRoot = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\', '/')
$rootPrefix = $resolvedRoot + [System.IO.Path]::DirectorySeparatorChar
$blockedPathPattern = '(?i)(^|[/\\])(\.coolzhu|\.git|\.claude|\.superpowers|__pycache__|backups?(?:-[^/\\]+)?|tmp|logs?|sessions?)([/\\]|$)|web-sessions|coolzhu\.toml$|package-report\.json$|(^|[/\\])\.env($|\.)|\.(sqlite3?|db)(-(wal|shm|journal))?$|\.(pyc|pyo|bak|old|orig|rej|pem|key|pfx|p12)$|(^|[/\\])(credentials?|secrets?|token-cache|credential-cache|access-token|refresh-token|session-token|auth-token)(\.[^/\\]+)?$'
$textExtensions = @(
    '.txt', '.json', '.toml', '.yaml', '.yml', '.ini', '.conf', '.config',
    '.md', '.ps1', '.cmd', '.bat', '.sh', '.js', '.mjs', '.cjs', '.ts',
    '.tsx', '.jsx', '.html', '.css', '.scss', '.rs', '.py', '.c', '.h',
    '.cpp', '.hpp', '.java', '.kt', '.dart', '.sql', '.wxs', '.xml',
    '.svg', '.lock'
)
$credentialPattern = '(?im)^\s*["'']?(api[_-]?key|access[_-]?token|refresh[_-]?token|session[_-]?token|client[_-]?secret|private[_-]?key|cookie|credential|token|secret|password)["'']?\s*[:=]\s*["''](?!(test|dummy|example|sample|access-token|saved-access-token|expired-access-token)\b)([A-Za-z0-9_\-\./+=]{16,})["'']'
$bearerPattern = '(?im)\bauthorization\s*[:=]\s*["'']?bearer\s+(?!(test|dummy|example|sample)\b)[A-Za-z0-9_\-\./+=]{8,}'
$urlCredentialPattern = '(?i)[?&](api[_-]?key|access[_-]?token|refresh[_-]?token|session[_-]?token|token|secret|key)=(?!(test|dummy|example|sample)\b)[A-Za-z0-9_\-\./+=]{12,}'
$privateKeyPattern = '(?i)-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----'
$findings = [System.Collections.Generic.List[object]]::new()

foreach ($file in Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse -Force) {
    $relativePath = $file.FullName.Substring($rootPrefix.Length).Replace('/', '\')
    if ($relativePath -match $blockedPathPattern) {
        $findings.Add([pscustomobject]@{ path = $relativePath; reason = 'blocked-path' })
        continue
    }
    if ($textExtensions -notcontains $file.Extension.ToLowerInvariant()) {
        continue
    }

    # 逐行扫描而不是跳过大文件，避免模型配置、内联前端资源等超过 2 MB 后
    # 成为凭据泄漏盲区；同时不把整份巨型文本一次性载入内存。
    $reader = [System.IO.StreamReader]::new($file.FullName, $true)
    try {
        while (($line = $reader.ReadLine()) -ne $null) {
            if (
                $line -match $credentialPattern -or
                $line -match $bearerPattern -or
                $line -match $urlCredentialPattern -or
                $line -match $privateKeyPattern
            ) {
                $findings.Add([pscustomobject]@{ path = $relativePath; reason = 'credential-assignment' })
                break
            }
        }
    } finally {
        $reader.Dispose()
    }
}

$report = [ordered]@{
    scanned_at = (Get-Date).ToUniversalTime().ToString('o')
    root = $resolvedRoot
    file_count = @(Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse -Force).Count
    safe = ($findings.Count -eq 0)
    findings = @($findings)
}

if ($ReportPath) {
    $reportParent = Split-Path -Parent $ReportPath
    if ($reportParent) { New-Item -ItemType Directory -Force -Path $reportParent | Out-Null }
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
}

if ($findings.Count -gt 0) {
    $lines = @($findings | ForEach-Object { "- $($_.path) [$($_.reason)]" })
    throw "Package safety scan failed:`n$($lines -join "`n")"
}

[pscustomobject]$report
