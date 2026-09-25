param(
    [string]$WorkspaceRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$DestinationRoot = (Join-Path ([Environment]::GetFolderPath('Desktop')) 'coolzhu-agent-project'),
    [string]$CurationMap = (Join-Path (Split-Path -Parent $PSScriptRoot) 'docs\obsidian-curation-map.json'),
    [string]$PublicDocsManifest = (Join-Path (Split-Path -Parent $PSScriptRoot) 'docs\github-public-docs.json'),
    [switch]$InventoryOnly,
    [string]$DateStamp = (Get-Date -Format 'yyyyMMdd')
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath $WorkspaceRoot).Path.TrimEnd('\', '/')
$destination = [System.IO.Path]::GetFullPath($DestinationRoot).TrimEnd('\', '/')
$sourceOutput = Join-Path $destination 'source'
$vaultRoot = Join-Path $destination 'obsidian-vault'
$reportsRoot = Join-Path $destination 'reports'
New-Item -ItemType Directory -Force -Path $sourceOutput | Out-Null

function Test-DeliveryExcludedPath {
    param([Parameter(Mandatory = $true)][string]$RelativePath)
    $path = $RelativePath.Replace('/', '\')
    $segments = @($path -split '\\')
    $isWorkspacePluginSource = $segments.Count -ge 2 -and [string]::Equals([string]$segments[0], '.coolzhu', [System.StringComparison]::OrdinalIgnoreCase) -and [string]::Equals([string]$segments[1], 'plugins', [System.StringComparison]::OrdinalIgnoreCase)
    $containsRuntimeStateDirectory = @($segments | Where-Object {
        [string]::Equals([string]$_, '.coolzhu', [System.StringComparison]::OrdinalIgnoreCase)
    }).Count -gt 0
    if ((-not $isWorkspacePluginSource) -and $containsRuntimeStateDirectory) {
        return $true
    }
    $blockedSegments = @(
        '.git', '.claude', '.superpowers', '.claw-agents', '__pycache__',
        'target', 'node_modules', 'tmp', 'dist', 'package', 'output',
        'test-results', 'models', 'logs', 'log', 'backup', 'backups',
        'session-attachments', 'audio-realtime'
    )
    if ($segments | Where-Object { $blockedSegments -contains $_.ToLowerInvariant() }) { return $true }
    if ($segments | Where-Object { $_ -match '(?i)^(backup-|.*-backup-)' }) { return $true }
    if ($segments | Where-Object { $_ -match '(?i)^generated-previews(?:-|$)' }) { return $true }
    if ($path -match '(?i)^modules\\gui-desktop\\packages\\tauri-shell\\src-tauri\\gen\\schemas\\') { return $true }
    if ($path -match '(?i)^modules\\[^\\]+\\docs\\') { return $true }
    if ($path -match '(?i)(web-sessions|(^|\\)sessions?(\\|$)|\.(sqlite3?|db)(-wal|-shm)?$|coolzhu\.toml$|(^|\\)\.env($|\.)|(^|\\)\.claw-todos\.json$|credentials|secrets|token-cache|(^|\\)[^\\]*-qrcode\.(png|jpe?g)$)') { return $true }
    if ($path -match '(?i)(\.bak|\.old|\.orig|\.rej|\.pyc|\.pyo|\.log|\.pem|\.key|\.p12|\.pfx|\.pt|\.ckpt|\.pth|\.gguf|\.onnx|\.safetensors|\.msi|\.zip|\.7z|\.exe|\.dll|\.pdb|\.rlib|\.rmeta|\.obj|\.lib)$') { return $true }
    return $false
}

function Get-WorkspaceRelativePath {
    param([Parameter(Mandatory = $true)][string]$FullName)
    return $FullName.Substring($workspace.Length).TrimStart('\', '/').Replace('/', '\')
}

function Get-RepositoryInfo {
    $roots = [System.Collections.Generic.List[string]]::new()
    if (Test-Path -LiteralPath (Join-Path $workspace '.git')) { $roots.Add($workspace) }
    $modules = Join-Path $workspace 'modules'
    if (Test-Path -LiteralPath $modules) {
        foreach ($module in Get-ChildItem -LiteralPath $modules -Directory -Force) {
            if (Test-Path -LiteralPath (Join-Path $module.FullName '.git')) { $roots.Add($module.FullName) }
        }
    }
    $items = @()
    foreach ($root in $roots) {
        $known = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        $status = @{}
        foreach ($line in @(& git -C $root ls-files --cached --others --exclude-standard 2>$null)) {
            if ($line) { [void]$known.Add(([string]$line).Replace('/', '\')) }
        }
        foreach ($line in @(& git -C $root status --porcelain=v1 --untracked-files=all 2>$null)) {
            if (-not $line -or $line.Length -lt 4) { continue }
            $code = $line.Substring(0, 2).Trim()
            $repoPath = $line.Substring(3).Trim('"').Replace('/', '\')
            if ($repoPath -match ' -> ') { $repoPath = ($repoPath -split ' -> ')[-1].Trim('"') }
            $status[$repoPath] = if ($code) { $code } else { 'modified' }
        }
        $items += [pscustomobject]@{ root = $root.TrimEnd('\', '/'); known = $known; status = $status }
    }
    return @($items | Sort-Object { $_.root.Length } -Descending)
}

$repositoryInfo = @(Get-RepositoryInfo)

function Get-FileRepositoryState {
    param([Parameter(Mandatory = $true)][string]$FullName)
    foreach ($repo in $repositoryInfo) {
        $prefix = $repo.root + [System.IO.Path]::DirectorySeparatorChar
        if ($FullName.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or $FullName.Equals($repo.root, [System.StringComparison]::OrdinalIgnoreCase)) {
            $repoPath = $FullName.Substring($repo.root.Length).TrimStart('\', '/').Replace('/', '\')
            $repoName = if ($repo.root -eq $workspace) { '.' } else { Get-WorkspaceRelativePath $repo.root }
            $gitStatus = if ($repo.status.ContainsKey($repoPath)) { [string]$repo.status[$repoPath] } elseif ($repo.known.Contains($repoPath)) { 'tracked-clean' } else { 'ignored-or-untracked' }
            return [pscustomobject]@{ repository = $repoName; git_status = $gitStatus }
        }
    }
    return [pscustomobject]@{ repository = 'unversioned'; git_status = 'unversioned' }
}

function Get-DeliveryInventory {
    $candidates = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($rootName in @('src', 'packages', 'modules', 'scripts', 'skills', 'installer', '.coolzhu\plugins')) {
        $rootPath = Join-Path $workspace $rootName
        if (-not (Test-Path -LiteralPath $rootPath)) { continue }
        foreach ($file in Get-ChildItem -LiteralPath $rootPath -File -Recurse -Force) {
            if ($seen.Add($file.FullName)) { $candidates.Add($file) }
        }
    }
    foreach ($name in @('README.md', 'LICENSE', 'Cargo.toml', 'Cargo.lock', 'package.json', 'package.ps1', 'AGENT.md', 'AGENTS.md', 'CLAUDE.md', '.gitignore', 'config\package-launcher.json', 'config\package-manifest.json')) {
        $path = Join-Path $workspace $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $file = Get-Item -LiteralPath $path
            if ($seen.Add($file.FullName)) { $candidates.Add($file) }
        }
    }

    if (Test-Path -LiteralPath $PublicDocsManifest -PathType Leaf) {
        $publicDocs = Get-Content -LiteralPath $PublicDocsManifest -Raw | ConvertFrom-Json
        foreach ($document in @($publicDocs.documents)) {
            $relativeDocument = ([string]$document).Replace('/', '\')
            if ([System.IO.Path]::IsPathRooted($relativeDocument) -or $relativeDocument -match '(^|\\)\.\.(\\|$)' -or -not $relativeDocument.StartsWith('docs\', [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "public docs entry must stay under docs/: $document"
            }
            $path = Join-Path $workspace $relativeDocument
            if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "public docs file missing: $document" }
            $file = Get-Item -LiteralPath $path
            if ($seen.Add($file.FullName)) { $candidates.Add($file) }
        }
        $manifestDocument = Get-Item -LiteralPath $PublicDocsManifest
        if ($seen.Add($manifestDocument.FullName)) { $candidates.Add($manifestDocument) }
    }

    $inventory = @()
    foreach ($file in $candidates) {
        $relative = Get-WorkspaceRelativePath $file.FullName
        if (Test-DeliveryExcludedPath $relative) { continue }
        if ($relative.StartsWith('config\', [System.StringComparison]::OrdinalIgnoreCase) -and $relative -notin @('config\package-launcher.json', 'config\package-manifest.json')) { continue }
        if ($file.Length -gt 50MB) { continue }
        $repoState = Get-FileRepositoryState $file.FullName
        $inventory += [pscustomobject][ordered]@{
            relative_path = $relative
            size = [long]$file.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash
            repository = $repoState.repository
            git_status = $repoState.git_status
        }
    }
    return @($inventory | Sort-Object relative_path)
}

$files = @(Get-DeliveryInventory)
$manifest = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    workspace = '.'
    file_count = $files.Count
    total_bytes = [long](($files | Measure-Object -Property size -Sum).Sum)
    files = $files
}
$manifestPath = Join-Path $sourceOutput 'SOURCE-MANIFEST.json'
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

if ($InventoryOnly) {
    Write-Output $manifestPath
    return
}

function Invoke-PublicSourceSafetyScan {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$ReportPath
    )

    $resolvedRoot = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\', '/')
    $rootPrefix = $resolvedRoot + [System.IO.Path]::DirectorySeparatorChar
    $textExtensions = @(
        '.txt', '.json', '.toml', '.yaml', '.yml', '.ini', '.conf', '.config',
        '.md', '.ps1', '.cmd', '.bat', '.sh', '.js', '.mjs', '.cjs', '.ts',
        '.tsx', '.jsx', '.html', '.css', '.scss', '.rs', '.py', '.c', '.h',
        '.cpp', '.hpp', '.java', '.kt', '.dart', '.sql', '.wxs', '.xml',
        '.svg', '.lock'
    )
    $credentialPattern = '(?im)^\s*["'']?(api[_-]?key|access[_-]?token|token|secret|password)["'']?\s*[:=]\s*["''](?!(test|dummy|example|sample|access-token|saved-access-token|expired-access-token|nope|not-a-real)\b)([A-Za-z0-9_\-\./+=]{16,})["'']'
    $bearerPattern = '(?im)^\s*(authorization\s*:\s*bearer\s+)[A-Za-z0-9_\-\./+=]{8,}'
    $currentUserPattern = if ($env:USERNAME) { '(?i)[A-Z]:[\\/]+Users[\\/]+' + [regex]::Escape($env:USERNAME) + '([\\/]|$)' } else { $null }
    $findings = [System.Collections.Generic.List[object]]::new()

    foreach ($file in Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse -Force) {
        $relativePath = $file.FullName.Substring($rootPrefix.Length).Replace('/', '\')
        if (Test-DeliveryExcludedPath $relativePath) {
            $findings.Add([pscustomobject]@{ path = $relativePath; reason = 'blocked-path' })
            continue
        }
        if ($textExtensions -notcontains $file.Extension.ToLowerInvariant()) { continue }

        # 大型源码（当前 main.rs 已超过 2 MB）也必须完整扫描。逐行读取既消除
        # “大文件跳过”的隐私盲区，也避免一次把几十 MB 文本全部载入内存。
        $credentialFound = $false
        $privatePathFound = $false
        $reader = [System.IO.StreamReader]::new($file.FullName, $true)
        try {
            while (-not $reader.EndOfStream) {
                $line = $reader.ReadLine()
                if (-not $credentialFound -and ($line -match $credentialPattern -or $line -match $bearerPattern)) {
                    $findings.Add([pscustomobject]@{ path = $relativePath; reason = 'credential-assignment' })
                    $credentialFound = $true
                }
                if (-not $privatePathFound -and $currentUserPattern -and $line -match $currentUserPattern) {
                    $findings.Add([pscustomobject]@{ path = $relativePath; reason = 'private-user-path' })
                    $privatePathFound = $true
                }
                if ($credentialFound -and ($privatePathFound -or -not $currentUserPattern)) {
                    break
                }
            }
        } finally {
            $reader.Dispose()
        }
    }

    $report = [ordered]@{
        scanned_at = (Get-Date).ToUniversalTime().ToString('o')
        root = '.'
        file_count = @(Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse -Force).Count
        safe = ($findings.Count -eq 0)
        findings = @($findings)
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
    if ($findings.Count -gt 0) {
        $lines = @($findings | ForEach-Object { "- $($_.path) [$($_.reason)]" })
        throw "Public source safety scan failed:`n$($lines -join "`n")"
    }
    return [pscustomobject]$report
}

if (-not (Test-Path -LiteralPath $CurationMap -PathType Leaf)) {
    throw "curation map not found: $CurationMap"
}
$parsedMap = Get-Content -LiteralPath $CurationMap -Raw | ConvertFrom-Json
$mapEntries = @($parsedMap | ForEach-Object { $_ })
$mappedDestinations = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
$sourceMap = @()
New-Item -ItemType Directory -Force -Path $vaultRoot, (Join-Path $vaultRoot '_meta'), (Join-Path $vaultRoot '00-首页'), $reportsRoot | Out-Null
foreach ($entry in $mapEntries) {
    $sourceRelative = ([string]$entry.source).Replace('/', '\')
    $destinationRelative = ([string]$entry.destination).Replace('/', '\')
    if ([System.IO.Path]::IsPathRooted($destinationRelative) -or $destinationRelative -match '(^|\\)\.\.(\\|$)') {
        throw "curation destination escapes vault: $destinationRelative"
    }
    if (-not $mappedDestinations.Add($destinationRelative)) { throw "duplicate curation destination: $destinationRelative" }
    $sourcePath = Join-Path $workspace $sourceRelative
    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) { throw "curation source missing: $sourceRelative" }
    $destinationPath = Join-Path $vaultRoot $destinationRelative
    $destinationFull = [System.IO.Path]::GetFullPath($destinationPath)
    if (-not $destinationFull.StartsWith($vaultRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw "curation destination escapes vault: $destinationRelative" }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destinationFull) | Out-Null
    $modified = (Get-Item -LiteralPath $sourcePath).LastWriteTime.ToString('yyyy-MM-dd')
    $frontmatter = "---`nsource_path: $(([string]$entry.source).Replace('\\','/'))`nsource_modified: $modified`ntopic: $($entry.topic)`nstatus: $($entry.status)`nmigrated: $(Get-Date -Format 'yyyy-MM-dd')`n---`n`n"
    $body = Get-Content -LiteralPath $sourcePath -Raw
    Set-Content -LiteralPath $destinationFull -Value ($frontmatter + $body) -Encoding UTF8
    $sourceMap += [pscustomobject]@{ source = ([string]$entry.source).Replace('\', '/'); destination = ([string]$entry.destination).Replace('\', '/'); topic = $entry.topic; status = $entry.status; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePath).Hash }
}
$sourceMap | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $vaultRoot '_meta\source-map.json') -Encoding UTF8
$mappedSources = @($sourceMap.source)
$excludedDocs = @()
$docsRoot = Join-Path $workspace 'docs'
if (Test-Path -LiteralPath $docsRoot) {
    $excludedDocs = @(Get-ChildItem -LiteralPath $docsRoot -File -Recurse -Filter '*.md' | ForEach-Object { (Get-WorkspaceRelativePath $_.FullName).Replace('\', '/') } | Where-Object { $mappedSources -notcontains $_ } | Sort-Object)
}
@("# 未迁移文档", "", "这些文档保留在原始 docs 目录，但未进入精选知识库。", "") + @($excludedDocs | ForEach-Object { "- ``$_``" }) | Set-Content -LiteralPath (Join-Path $vaultRoot '_meta\excluded-docs.md') -Encoding UTF8

$vaultHome = @('# COOLZHU Agent 项目知识库', '', '此知识库由当前工作树中的有效文档精选生成；原始 docs 文件保持不变。', '', '## 主题入口', '')
foreach ($entry in $sourceMap) {
    $link = ($entry.destination -replace '(?i)\.md$', '').Replace('\', '/')
    $title = [System.IO.Path]::GetFileNameWithoutExtension($entry.destination)
    $vaultHome += "- [[$link|$title]] — $($entry.topic) / $($entry.status)"
}
$vaultHome | Set-Content -LiteralPath (Join-Path $vaultRoot '00-首页\项目总览.md') -Encoding UTF8
@('# 阅读路径', '', '1. 从项目结构与开发标准开始。', '2. 再读实时语音、Computer Use 与安装交付专题。', '3. 最后查看验证报告和未完成项。', '', '返回 [[00-首页/项目总览|项目总览]]。') | Set-Content -LiteralPath (Join-Path $vaultRoot '00-首页\阅读路径.md') -Encoding UTF8

$stagingRoot = Join-Path $destination ".staging-project-delivery-$(Get-Date -Format 'yyyyMMdd-HHmmssfff')"
$sourceTree = Join-Path $stagingRoot 'source-tree'
$verifyRoot = Join-Path $stagingRoot 'verify'
New-Item -ItemType Directory -Force -Path $sourceTree | Out-Null
trap {
    $caughtError = $_
    if ($stagingRoot -and $destination) {
        $failedStaging = [System.IO.Path]::GetFullPath($stagingRoot)
        $expectedPrefix = $destination + [System.IO.Path]::DirectorySeparatorChar + '.staging-project-delivery-'
        if ((Test-Path -LiteralPath $failedStaging) -and $failedStaging.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $failedStaging -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    throw $caughtError
}
foreach ($entry in $files) {
    $sourcePath = Join-Path $workspace $entry.relative_path
    $targetPath = Join-Path $sourceTree $entry.relative_path
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $targetPath) | Out-Null
    Copy-Item -LiteralPath $sourcePath -Destination $targetPath -Force
}

# 本地工作区使用多个嵌套 Git 仓，因此根 .gitignore 会忽略 modules/ 和整个 .coolzhu/。
# GitHub-ready 交付物是单体仓库，必须生成独立规则以跟踪模块源码和 workspace 插件。
$githubIgnorePath = Join-Path $sourceTree '.gitignore'
$githubIgnoreLines = @(
    '# Rust / build outputs',
    '/target/',
    '**/target/',
    '/package/',
    '/dist/',
    '/output/',
    '/tmp/',
    '/backups/',
    '/backup/',
    'node_modules/',
    '**/node_modules/',
    'test-results/',
    'goal-artifacts/',
    '',
    '# Python caches',
    '__pycache__/',
    '**/__pycache__/',
    '*.py[cod]',
    '*$py.class',
    '',
    '# Model weights and binary artifacts',
    '/models/',
    '**/*.pth',
    '**/*.pt',
    '**/*.ckpt',
    '**/*.gguf',
    '**/*.onnx',
    '**/*.safetensors',
    '*.msi',
    '*.zip',
    '*.7z',
    '*.exe',
    '*.dll',
    '*.pdb',
    '',
    '# Private runtime state',
    '.coolzhu/*',
    '!.coolzhu/plugins/',
    '!.coolzhu/plugins/**',
    '.claw-agents/',
    '**/.claw-agents/',
    '.claude/',
    '**/.claude/',
    '.superpowers/',
    '**/.superpowers/',
    '**/web-sessions.*',
    '**/*.sqlite',
    '**/*.sqlite3',
    '**/*.sqlite3-wal',
    '**/*.sqlite3-shm',
    '**/*.db',
    '**/*.db-wal',
    '**/*.db-shm',
    '**/coolzhu.toml',
    '**/.env',
    '**/.env.*',
    '**/logs/',
    '**/*.log',
    '**/.claw-todos.json',
    '**/session-attachments/',
    '**/audio-realtime/',
    '**/credentials.json',
    '**/secrets.json',
    '**/token-cache.json',
    '**/*-qrcode.png',
    '**/*-qrcode.jpg',
    '**/*.pem',
    '**/*.key',
    '**/*.p12',
    '**/*.pfx',
    '',
    '# OS / editor',
    'Thumbs.db',
    '.DS_Store'
)
[System.IO.File]::WriteAllLines($githubIgnorePath, $githubIgnoreLines, [System.Text.UTF8Encoding]::new($false))
$gitIgnoreEntry = @($files | Where-Object { $_.relative_path -eq '.gitignore' } | Select-Object -First 1)
if ($gitIgnoreEntry.Count -eq 1) {
    $oldSize = [long]$gitIgnoreEntry[0].size
    $gitIgnoreEntry[0].size = [long](Get-Item -LiteralPath $githubIgnorePath).Length
    $gitIgnoreEntry[0].sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $githubIgnorePath).Hash
    $manifest.total_bytes = [long]$manifest.total_bytes - $oldSize + [long]$gitIgnoreEntry[0].size
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $sourceTree 'SOURCE-MANIFEST.json') -Encoding UTF8
@('# Source exclusions', '', 'Excluded: generated outputs, backups, model/session/runtime databases, secrets, logs, binary build artifacts, private acceptance records and non-whitelisted config files.', '', 'Whitelisted configuration:', '- config/package-launcher.json', '- config/package-manifest.json', '', 'Special source allowlist:', '- .coolzhu/plugins (Cargo workspace plugin crates only)', '- docs files listed by docs/github-public-docs.json') | Set-Content -LiteralPath (Join-Path $sourceTree 'SOURCE-EXCLUSIONS.md') -Encoding UTF8

$privacyReportPath = Join-Path $reportsRoot 'source-privacy-scan.json'
Invoke-PublicSourceSafetyScan -Root $sourceTree -ReportPath $privacyReportPath | Out-Null

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipName = "coolzhu-agent-source-$DateStamp.zip"
$zipStaging = Join-Path $stagingRoot $zipName
[System.IO.Compression.ZipFile]::CreateFromDirectory($sourceTree, $zipStaging, [System.IO.Compression.CompressionLevel]::Optimal, $false)
New-Item -ItemType Directory -Force -Path $verifyRoot | Out-Null
[System.IO.Compression.ZipFile]::ExtractToDirectory($zipStaging, $verifyRoot)
foreach ($entry in $files) {
    $verifiedPath = Join-Path $verifyRoot $entry.relative_path
    if (-not (Test-Path -LiteralPath $verifiedPath -PathType Leaf)) { throw "zip file missing: $($entry.relative_path)" }
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $verifiedPath).Hash
    if ($actualHash -ne $entry.sha256) { throw "zip hash mismatch: $($entry.relative_path)" }
    if (Test-DeliveryExcludedPath $entry.relative_path) { throw "excluded path entered zip: $($entry.relative_path)" }
}
$zipPath = Join-Path $sourceOutput $zipName
if (Test-Path -LiteralPath $zipPath) {
    $archiveDir = Join-Path $sourceOutput 'archive'
    New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null
    Copy-Item -LiteralPath $zipPath -Destination (Join-Path $archiveDir "$([System.IO.Path]::GetFileNameWithoutExtension($zipName))-$(Get-Date -Format 'yyyyMMdd-HHmmss').zip") -Force
}
Copy-Item -LiteralPath $zipStaging -Destination $zipPath -Force
$zipHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash
$githubTree = Join-Path $sourceOutput "github-ready-$DateStamp-$(Get-Date -Format 'HHmmss')"
Copy-Item -LiteralPath $sourceTree -Destination $githubTree -Recurse
Copy-Item -LiteralPath (Join-Path $sourceTree 'SOURCE-EXCLUSIONS.md') -Destination (Join-Path $sourceOutput 'SOURCE-EXCLUSIONS.md') -Force
"$zipHash  $zipName" | Set-Content -LiteralPath (Join-Path $sourceOutput 'SHA256SUMS.txt') -Encoding UTF8

$moduleCounts = @($files | Group-Object { ($_.relative_path -split '\\')[0] } | Sort-Object Name | ForEach-Object { "- $($_.Name): $($_.Count) files" })
@('# Project structure', '', "Generated: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')", '', "Source files: $($files.Count)", "Source bytes: $($manifest.total_bytes)", '', '## Top-level inventory', '') + $moduleCounts | Set-Content -LiteralPath (Join-Path $reportsRoot 'project-structure.md') -Encoding UTF8
@('# Docs migration', '', "Mapped documents: $($sourceMap.Count)", "Excluded but preserved documents: $($excludedDocs.Count)", '', 'Original docs were copied with frontmatter; no source document was deleted.') | Set-Content -LiteralPath (Join-Path $reportsRoot 'docs-migration.md') -Encoding UTF8
@('# Source package audit', '', "Archive: $zipName", "SHA-256: $zipHash", "GitHub-ready tree: $([System.IO.Path]::GetFileName($githubTree))", "Files verified: $($files.Count)", '', 'Result: PASS — round-trip expansion, manifest hash comparison and privacy scan succeeded.', '', 'Excluded classes: backups, generated output, model/session state, databases, secrets, private acceptance records, logs and build binaries.') | Set-Content -LiteralPath (Join-Path $reportsRoot 'source-package-audit.md') -Encoding UTF8

$resolvedStaging = (Resolve-Path -LiteralPath $stagingRoot).Path
if (-not $resolvedStaging.StartsWith($destination + [System.IO.Path]::DirectorySeparatorChar + '.staging-project-delivery-', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refuse to remove unexpected staging path: $resolvedStaging"
}
Remove-Item -LiteralPath $resolvedStaging -Recurse -Force

[pscustomobject]@{ destination = $destination; vault_documents = $sourceMap.Count; source_files = $files.Count; github_tree = $githubTree; zip = $zipPath; sha256 = $zipHash; privacy_report = $privacyReportPath }
