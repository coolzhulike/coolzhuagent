$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$scanner = Join-Path $PSScriptRoot 'package-safety.ps1'
$sandbox = Join-Path $workspace 'tmp\tests\package-safety'

if (Test-Path -LiteralPath $sandbox) {
    Remove-Item -LiteralPath $sandbox -Recurse -Force
}
New-Item -ItemType Directory -Force -Path (Join-Path $sandbox 'safe\config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $sandbox 'safe\models'), (Join-Path $sandbox 'safe\src') | Out-Null
New-Item -ItemType Directory -Force -Path `
    (Join-Path $sandbox 'resource-fixture\public'), `
    (Join-Path $sandbox 'resource-fixture\src\__pycache__'), `
    (Join-Path $sandbox 'resource-fixture\sessions') | Out-Null
Set-Content -LiteralPath (Join-Path $sandbox 'safe\app.exe') -Value 'binary-placeholder'
Set-Content -LiteralPath (Join-Path $sandbox 'safe\config\package-launcher.json') -Value '{"health_url":"http://127.0.0.1"}'
Set-Content -LiteralPath (Join-Path $sandbox 'safe\models\tokens.txt') -Value 'normal-model-token-vocabulary'
Set-Content -LiteralPath (Join-Path $sandbox 'safe\src\owner-token.js') -Value 'const owner_token = request.owner_token;'
Set-Content -LiteralPath (Join-Path $sandbox 'safe\src\example-config.js') -Value 'const api_key = "example-placeholder-token-1234567890";'
Set-Content -LiteralPath (Join-Path $sandbox 'resource-fixture\public\guide.txt') -Value 'public-resource'
Set-Content -LiteralPath (Join-Path $sandbox 'resource-fixture\src\__pycache__\cached.pyc') -Value 'compiled-cache'
Set-Content -LiteralPath (Join-Path $sandbox 'resource-fixture\sessions\web-sessions.sqlite3') -Value 'private-session'

& $scanner -Root (Join-Path $sandbox 'safe') | Out-Null

$resourceRoot = Join-Path $sandbox 'resource-package'
$resourceManifest = Join-Path $sandbox 'resource-manifest.json'
$resourceReport = Join-Path $sandbox 'reports\resource-package-report.json'
[ordered]@{
    package_root = $resourceRoot
    backup_keep = 1
    artifacts = @(
        [ordered]@{
            id = 'safe-artifact'
            source = 'tmp/tests/package-safety/safe/app.exe'
            target = 'bin/safe-app.exe'
        }
    )
    resources = @(
        [ordered]@{ id = 'launcher'; source = 'config/package-launcher.json'; target = 'config/package-launcher.json' },
        [ordered]@{ id = 'manifest'; source = 'config/package-manifest.json'; target = 'config/package-manifest.json' },
        [ordered]@{ id = 'documentation.command-line'; source = 'docs/command-line.md'; target = 'docs/command-line.md' },
        [ordered]@{ id = 'filtered-resource'; source = 'tmp/tests/package-safety/resource-fixture'; target = 'resources' }
    )
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resourceManifest -Encoding UTF8
New-Item -ItemType Directory -Force -Path (Join-Path $resourceRoot 'backup'), (Join-Path $resourceRoot 'tmp\logs') | Out-Null
Set-Content -LiteralPath (Join-Path $resourceRoot 'backup\old.exe') -Value 'stale-backup'
Set-Content -LiteralPath (Join-Path $resourceRoot 'tmp\logs\runtime.log') -Value 'stale-log'
New-Item -ItemType Directory -Force -Path (Join-Path $resourceRoot 'bin') | Out-Null
Set-Content -LiteralPath (Join-Path $resourceRoot 'bin\coolzhu-desktop-console.exe') -Value 'removed-artifact'
Set-Content -LiteralPath (Join-Path $resourceRoot 'README.md') -Value 'stale-readme'
Set-Content -LiteralPath (Join-Path $resourceRoot 'package-report.json') -Value '{"developer_path":"C:\\Users\\private"}'
& (Join-Path $PSScriptRoot 'package-all.ps1') -Manifest $resourceManifest -SkipBuild -ReportPath $resourceReport | Out-Null
$stalePackageState = @(
    'backup', 'tmp', 'logs', 'sessions', 'bin\coolzhu-desktop-console.exe', 'README.md', 'package-report.json' |
        ForEach-Object { Join-Path $resourceRoot $_ } |
        Where-Object { Test-Path -LiteralPath $_ }
)
if ($stalePackageState.Count -ne 0) {
    throw "stale blocked package state was not removed: $($stalePackageState.FullName -join ', ')"
}
$packagedConfigs = @(Get-ChildItem -LiteralPath (Join-Path $resourceRoot 'config') -File | Select-Object -ExpandProperty Name | Sort-Object)
if (($packagedConfigs -join ',') -ne 'package-launcher.json,package-manifest.json') {
    throw "unexpected packaged config files: $($packagedConfigs -join ',')"
}
if (-not (Test-Path -LiteralPath (Join-Path $resourceRoot 'bin\safe-app.exe') -PathType Leaf)) {
    throw 'safe manifest artifact was not packaged'
}
if (-not (Test-Path -LiteralPath (Join-Path $resourceRoot 'docs\command-line.md') -PathType Leaf)) {
    throw 'command-line guide was not packaged at docs/command-line.md'
}
if (-not (Test-Path -LiteralPath (Join-Path $resourceRoot 'resources\public\guide.txt') -PathType Leaf)) {
    throw 'safe nested resource was not packaged'
}
if (
    (Test-Path -LiteralPath (Join-Path $resourceRoot 'resources\src\__pycache__\cached.pyc')) -or
    (Test-Path -LiteralPath (Join-Path $resourceRoot 'resources\sessions\web-sessions.sqlite3'))
) {
    throw 'blocked nested resource leaked into PackageRoot'
}
if (-not (Test-Path -LiteralPath $resourceReport -PathType Leaf)) {
    throw "package report missing outside PackageRoot: $resourceReport"
}
$resourceReportText = Get-Content -Raw -LiteralPath $resourceReport
if ($resourceReportText.IndexOf($workspace, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or $resourceReportText -match '(?i)[A-Z]:\\\\Users\\\\') {
    throw 'package report leaked an absolute developer path'
}
if (Test-Path -LiteralPath (Join-Path $resourceRoot 'package-report.json')) {
    throw 'package report must not be written into PackageRoot'
}

$preflightSentinel = Join-Path $resourceRoot 'preflight-sentinel.keep'
Set-Content -LiteralPath $preflightSentinel -Value 'must-survive-rejected-preflight'
$invalidInputManifest = Join-Path $sandbox 'invalid-input-manifest.json'
[ordered]@{
    package_root = $resourceRoot
    artifacts = @()
    resources = @(
        [ordered]@{ id = 'outside-input'; source = '../outside-workspace'; target = 'outside.txt' }
    )
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $invalidInputManifest -Encoding UTF8
$invalidInputMessage = $null
try {
    & (Join-Path $PSScriptRoot 'package-all.ps1') -Manifest $invalidInputManifest -SkipBuild -ReportPath $resourceReport | Out-Null
} catch {
    $invalidInputMessage = $_.Exception.Message
}
if ($invalidInputMessage -notlike '*release input must stay inside the workspace*') {
    throw "outside release input was not rejected: $invalidInputMessage"
}
if (-not (Test-Path -LiteralPath $preflightSentinel -PathType Leaf)) {
    throw 'PackageRoot was cleared before outside release input preflight completed'
}

$invalidTargetManifest = Join-Path $sandbox 'invalid-target-manifest.json'
[ordered]@{
    package_root = $resourceRoot
    artifacts = @()
    resources = @(
        [ordered]@{ id = 'outside-target'; source = 'config/package-launcher.json'; target = '../escaped.txt' }
    )
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $invalidTargetManifest -Encoding UTF8
$invalidTargetMessage = $null
try {
    & (Join-Path $PSScriptRoot 'package-all.ps1') -Manifest $invalidTargetManifest -SkipBuild -ReportPath $resourceReport | Out-Null
} catch {
    $invalidTargetMessage = $_.Exception.Message
}
if ($invalidTargetMessage -notlike '*release output must stay inside PackageRoot*') {
    throw "outside release output was not rejected: $invalidTargetMessage"
}
if (-not (Test-Path -LiteralPath $preflightSentinel -PathType Leaf)) {
    throw 'PackageRoot was cleared before outside release output preflight completed'
}

$unsafeRootMessage = $null
try {
    & (Join-Path $PSScriptRoot 'package-all.ps1') `
        -Manifest $resourceManifest `
        -PackageRoot (Join-Path $workspace 'scripts\unsafe-package-root') `
        -SkipBuild `
        -ReportPath $resourceReport | Out-Null
} catch {
    $unsafeRootMessage = $_.Exception.Message
}
if ($unsafeRootMessage -notlike '*workspace\package or a child of workspace\tmp*') {
    throw "unsafe PackageRoot was not rejected before cleanup: $unsafeRootMessage"
}

$unsafe = Join-Path $sandbox 'unsafe'
New-Item -ItemType Directory -Force -Path `
    (Join-Path $unsafe '.coolzhu'), `
    (Join-Path $unsafe '.claude'), `
    (Join-Path $unsafe '.superpowers'), `
    (Join-Path $unsafe 'src\__pycache__'), `
    (Join-Path $unsafe 'backup'), `
    (Join-Path $unsafe 'backup-20260728'), `
    (Join-Path $unsafe 'config') | Out-Null
Set-Content -LiteralPath (Join-Path $unsafe '.coolzhu\web-sessions.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $unsafe '.coolzhu\clawbot-gateway.sqlite3') -Value 'sqlite-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe '.claude\launch.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $unsafe '.superpowers\draft.md') -Value '# private draft'
Set-Content -LiteralPath (Join-Path $unsafe 'src\__pycache__\cached.pyc') -Value 'compiled-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'coolzhu.toml') -Value '[models]'
Set-Content -LiteralPath (Join-Path $unsafe '.env') -Value 'SAFE_TEST=true'
Set-Content -LiteralPath (Join-Path $unsafe 'chat.sqlite3') -Value 'sqlite-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'chat.sqlite3-wal') -Value 'sqlite-wal-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'chat.sqlite3-shm') -Value 'sqlite-shm-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'chat.sqlite3-journal') -Value 'sqlite-journal-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'cache.db-wal') -Value 'db-wal-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'backup\old.exe') -Value 'backup-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'backup-20260728\old.exe') -Value 'timestamped-backup-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'settings.bak') -Value 'backup-extension-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'settings.old') -Value 'old-extension-placeholder'
Set-Content -LiteralPath (Join-Path $unsafe 'private.pem') -Value '-----BEGIN PRIVATE KEY-----'
Set-Content -LiteralPath (Join-Path $unsafe 'config\credential.txt') -Value 'api_key="sk-package-safety-fixture-1234567890"'
Set-Content -LiteralPath (Join-Path $unsafe 'config\wechat-cookie.txt') -Value 'token="wx-cookie-fixture-1234567890"'
Set-Content -LiteralPath (Join-Path $unsafe 'config\refresh.txt') -Value 'refresh_token="refresh-fixture-value-1234567890"'
Set-Content -LiteralPath (Join-Path $unsafe 'config\request-url.txt') -Value 'https://example.invalid/?access_token=url-fixture-value-1234567890'
Set-Content -LiteralPath (Join-Path $unsafe 'config\credentials.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $unsafe 'config\token-cache.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $unsafe 'config\refresh-token.json') -Value '{}'
$largeCredential = Join-Path $unsafe 'config\large-credential.txt'
$largeWriter = [System.IO.StreamWriter]::new($largeCredential, $false)
try {
    for ($index = 0; $index -lt 35000; $index += 1) {
        $largeWriter.WriteLine(('safe-padding-{0:D5}-{1}' -f $index, ('x' * 64)))
    }
    $largeWriter.WriteLine('access_token="large-file-fixture-token-1234567890"')
} finally {
    $largeWriter.Dispose()
}

$unsafeMessage = $null
try {
    & $scanner -Root $unsafe | Out-Null
} catch {
    $unsafeMessage = $_.Exception.Message
}
if (-not $unsafeMessage) { throw 'unsafe package unexpectedly passed' }

foreach ($expected in @(
    '.coolzhu\web-sessions.json',
    '.coolzhu\clawbot-gateway.sqlite3',
    '.claude\launch.json',
    '.superpowers\draft.md',
    'src\__pycache__\cached.pyc',
    'coolzhu.toml',
    '.env',
    'chat.sqlite3',
    'chat.sqlite3-wal',
    'chat.sqlite3-shm',
    'chat.sqlite3-journal',
    'cache.db-wal',
    'backup\old.exe',
    'backup-20260728\old.exe',
    'settings.bak',
    'settings.old',
    'private.pem',
    'config\credential.txt',
    'config\wechat-cookie.txt',
    'config\refresh.txt',
    'config\request-url.txt',
    'config\credentials.json',
    'config\token-cache.json',
    'config\refresh-token.json',
    'config\large-credential.txt'
)) {
    if ($unsafeMessage -notlike "*$expected*") {
        throw "unsafe finding missing from report: $expected"
    }
}

$productWxs = Get-Content -Raw -LiteralPath (Join-Path $workspace 'installer\Product.wxs')
foreach ($requiredExclude in @(
    '**.sqlite-*',
    '**.sqlite3-*',
    '**.db-*',
    '**.bak',
    '**.old',
    '**package-report.json',
    '**token-cache.*',
    '**refresh-token.*'
)) {
    if ($productWxs.IndexOf($requiredExclude, [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
        throw "Product.wxs missing release exclusion: $requiredExclude"
    }
}

# Windows 小尺寸/高 DPI 图标契约：最终母版、七份 final PNG、最终 ICO、
# Tauri ICO/PNG 与 desktop-console PNG 必须真实存在并保留 32-bit Alpha。
$iconFinalRoot = Join-Path $workspace 'docs\design-assets\coolzhu-icons-2026-08-27\final'
$iconManifestPath = Join-Path $iconFinalRoot 'SOURCE-MANIFEST.json'
if (-not (Test-Path -LiteralPath $iconManifestPath -PathType Leaf)) {
    throw "P5-F icon manifest missing: $iconManifestPath"
}
$iconManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $iconManifestPath | ConvertFrom-Json
if ($iconManifest.asset_id -ne 'app-icon-cz-moon-gate-lantern-v1') {
    throw 'P5-F icon manifest asset_id mismatch'
}
$applicationIcon = Join-Path $iconFinalRoot 'app-icon-cz-moon-gate-lantern-v1.ico'
$applicationPng = Join-Path $iconFinalRoot 'app-icon-cz-moon-gate-lantern-v1.png'
$tauriIconRoot = Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\icons'
$tauriPng = Join-Path $tauriIconRoot 'app-icon-cz-moon-gate-lantern-v1.png'
$tauriIco = Join-Path $tauriIconRoot 'app-icon-cz-moon-gate-lantern-v1.ico'
$desktopConsolePng = Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\assets\app-icon-cz-moon-gate-lantern-v1.png'

Add-Type -AssemblyName System.Drawing
function Assert-P5fPng {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$ExpectedSize
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "P5-F PNG asset missing: $Path"
    }
    $bitmap = $null
    try {
        $bitmap = [System.Drawing.Bitmap]::new($Path)
        if ($bitmap.Width -ne $ExpectedSize -or $bitmap.Height -ne $ExpectedSize) {
            throw "P5-F PNG size mismatch: $Path -> $($bitmap.Width)x$($bitmap.Height)"
        }
        if ($bitmap.PixelFormat.ToString() -ne 'Format32bppArgb') {
            throw "P5-F PNG must be Format32bppArgb: $Path -> $($bitmap.PixelFormat)"
        }
        $last = $ExpectedSize - 1
        $cornerAlphas = @(
            $bitmap.GetPixel(0, 0).A,
            $bitmap.GetPixel($last, 0).A,
            $bitmap.GetPixel(0, $last).A,
            $bitmap.GetPixel($last, $last).A
        )
        if (@($cornerAlphas | Where-Object { $_ -gt 1 }).Count -gt 0) {
            throw "P5-F PNG visible corner detected: $Path -> $($cornerAlphas -join ',')"
        }
    } finally {
        if ($null -ne $bitmap) {
            $bitmap.Dispose()
        }
    }
}

Assert-P5fPng -Path $applicationPng -ExpectedSize 1254
foreach ($expectedSize in @(16, 24, 32, 48, 64, 128, 256)) {
    Assert-P5fPng -Path (Join-Path $iconFinalRoot "app-icon-cz-moon-gate-lantern-v1-${expectedSize}.png") -ExpectedSize $expectedSize
}
Assert-P5fPng -Path $tauriPng -ExpectedSize 256
Assert-P5fPng -Path $desktopConsolePng -ExpectedSize 256

foreach ($iconPath in @($applicationIcon, $tauriIco)) {
    if (-not (Test-Path -LiteralPath $iconPath -PathType Leaf)) {
        throw "P5-F ICO asset missing: $iconPath"
    }
    $iconBytes = [System.IO.File]::ReadAllBytes($iconPath)
    if ($iconBytes.Length -lt 22 -or $iconBytes[0] -ne 0 -or $iconBytes[1] -ne 0 -or $iconBytes[2] -ne 1 -or $iconBytes[3] -ne 0) {
        throw "invalid ICO header: $iconPath"
    }
    $iconCount = [int]$iconBytes[4] + (256 * [int]$iconBytes[5])
    if ($iconCount -ne 7 -or $iconBytes.Length -lt (6 + (16 * $iconCount))) {
        throw "ICO does not contain exactly seven multi-size entries: $iconPath"
    }
    $sizes = @(
        for ($index = 0; $index -lt $iconCount; $index += 1) {
            $offset = 6 + (16 * $index)
            $width = if ($iconBytes[$offset] -eq 0) { 256 } else { [int]$iconBytes[$offset] }
            $height = if ($iconBytes[$offset + 1] -eq 0) { 256 } else { [int]$iconBytes[$offset + 1] }
            if ($width -ne $height) { throw "non-square ICO entry at index ${index}: $iconPath" }
            $width
        }
    )
    foreach ($expectedSize in @(16, 24, 32, 48, 64, 128, 256)) {
        if ($sizes -notcontains $expectedSize) {
            throw "ICO missing ${expectedSize}px entry: $iconPath"
        }
    }
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $tauriIco).Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $applicationIcon).Hash) {
    throw 'Tauri ICO SHA256 differs from final ICO'
}
$finalRuntimePngHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $iconFinalRoot 'app-icon-cz-moon-gate-lantern-v1-256.png')).Hash
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $tauriPng).Hash -ne $finalRuntimePngHash) {
    throw 'Tauri PNG SHA256 differs from final 256px PNG'
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $desktopConsolePng).Hash -ne $finalRuntimePngHash) {
    throw 'desktop-console PNG SHA256 differs from final 256px PNG'
}
$retiredRuntimeAssets = @(
    (Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\icons\icon.ico'),
    (Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\icons\icon.png'),
    (Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\assets\coolzhu-agent-icon.png')
)
foreach ($retiredAsset in $retiredRuntimeAssets) {
    if (Test-Path -LiteralPath $retiredAsset) {
        throw "retired runtime icon still exists: $retiredAsset"
    }
}
# 由稳定组件拥有安装目录，避免收集得到的文件组件在卸载后遗留空目录。
# 让 XML 解析器直接加载文件，并遵循 XML 声明中的默认 UTF-8 编码。
$productWxsXml = New-Object System.Xml.XmlDocument
$productWxsXml.Load((Join-Path $workspace 'installer\Product.wxs'))
$wixNamespace = [System.Xml.XmlNamespaceManager]::new($productWxsXml.NameTable)
$wixNamespace.AddNamespace('wix', 'http://wixtoolset.org/schemas/v4/wxs')
$launcherComponent = $productWxsXml.SelectSingleNode(
    '//wix:Component[@Id="LauncherComponent"]',
    $wixNamespace
)
if (-not $launcherComponent) {
    throw 'Product.wxs missing LauncherComponent directory owner'
}

$installDirectoryContracts = @(
    @{
        Name = 'INSTALLDIR CreateFolder ownership'
        XPath = 'wix:CreateFolder[@Directory="INSTALLDIR" and not(@Subdirectory)]'
    },
    @{
        Name = 'bin CreateFolder ownership'
        XPath = 'wix:CreateFolder[@Directory="INSTALLDIR" and @Subdirectory="bin"]'
    },
    @{
        Name = 'bin uninstall RemoveFolder cleanup'
        XPath = 'wix:RemoveFolder[@Id="RemoveInstallBinDir" and @Directory="INSTALLDIR" and @Subdirectory="bin" and @On="uninstall"]'
    },
    @{
        Name = 'INSTALLDIR uninstall RemoveFolder cleanup'
        XPath = 'wix:RemoveFolder[@Id="RemoveInstallRootDir" and @Directory="INSTALLDIR" and not(@Subdirectory) and @On="uninstall"]'
    }
)
foreach ($contract in $installDirectoryContracts) {
    if (-not $launcherComponent.SelectSingleNode($contract.XPath, $wixNamespace)) {
        throw "Product.wxs missing installer directory contract: $($contract.Name)"
    }
}
$applicationIconNode = $productWxsXml.SelectSingleNode(
    '//wix:Icon[@Id="CoolzhuApplicationIcon" and @SourceFile="$(ApplicationIcon)"]',
    $wixNamespace
)
if (-not $applicationIconNode) {
    throw 'Product.wxs missing CoolzhuApplicationIcon source contract'
}
$arpProductIconNode = $productWxsXml.SelectSingleNode(
    '//wix:Property[@Id="ARPPRODUCTICON" and @Value="CoolzhuApplicationIcon"]',
    $wixNamespace
)
if (-not $arpProductIconNode) {
    throw 'Product.wxs ARPPRODUCTICON must use CoolzhuApplicationIcon'
}
foreach ($shortcutId in @('StartMenuShortcut', 'DesktopShortcut')) {
    $shortcut = $launcherComponent.SelectSingleNode(
        "wix:Shortcut[@Id='$shortcutId' and @Icon='CoolzhuApplicationIcon']",
        $wixNamespace
    )
    if (-not $shortcut) {
        throw "Product.wxs shortcut missing application icon contract: $shortcutId"
    }
}

# 桌面快捷方式最终执行的是 package launcher。除了 WiX Shortcut/Icon 表，
# 启动器 PE 自身也必须嵌入同一份应用图标，避免 Windows 图标缓存失效或
# 快捷方式被复制后回退成白色默认文件图标。
$launcherCargoManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (
    Join-Path $workspace 'packages\app-launcher\Cargo.toml'
)
if ($launcherCargoManifest -notmatch '(?m)^build\s*=\s*["'']build\.rs["'']\s*$') {
    throw 'app-launcher Cargo.toml missing Windows icon build script contract'
}
$launcherBuildScript = Get-Content -Raw -Encoding UTF8 -LiteralPath (
    Join-Path $workspace 'packages\app-launcher\build.rs'
)
foreach ($requiredLauncherIconContract in @(
    'COOLZHU_APPLICATION_ICON',
    'find_resource_compiler',
    'cargo:rustc-link-arg-bin=COOLZHU-AGENT=',
    'app-icon-cz-moon-gate-lantern-v1.ico'
)) {
    if ($launcherBuildScript.IndexOf($requiredLauncherIconContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "app-launcher build.rs missing icon embedding contract: $requiredLauncherIconContract"
    }
}
$desktopConsoleBuildPath = Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\build.rs'
if (-not (Test-Path -LiteralPath $desktopConsoleBuildPath -PathType Leaf)) {
    throw "desktop-console build.rs missing: $desktopConsoleBuildPath"
}
$desktopConsoleBuildScript = Get-Content -Raw -Encoding UTF8 -LiteralPath $desktopConsoleBuildPath
foreach ($requiredDesktopConsoleIconContract in @(
    'CARGO_CFG_WINDOWS',
    'cargo:rerun-if-changed={APPLICATION_ICON}',
    'cargo:rerun-if-env-changed=RC',
    'cargo:rerun-if-env-changed=WINDOWS_RC',
    'find_resource_compiler',
    'app-icon-cz-moon-gate-lantern-v1.ico',
    'cargo:rustc-link-arg-bin={BIN_NAME}=',
    'coolzhu-desktop-console'
)) {
    if ($desktopConsoleBuildScript.IndexOf($requiredDesktopConsoleIconContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "desktop-console build.rs missing PE icon contract: $requiredDesktopConsoleIconContract"
    }
}

$buildMsiScript = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $workspace 'scripts\build-msi.ps1')
$buildMsiIconContracts = @(
    '$applicationIcon = Join-Path',
    'app-icon-cz-moon-gate-lantern-v1.ico',
    'ApplicationIcon=$applicationIcon',
    'Application icon missing'
)
foreach ($requiredIconContract in $buildMsiIconContracts) {
    if ($buildMsiScript.IndexOf($requiredIconContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "build-msi.ps1 missing application icon contract: $requiredIconContract"
    }
}
$tauriBuildScript = Get-Content -Raw -Encoding UTF8 -LiteralPath (
    Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\build.rs'
)
foreach ($requiredTauriIconContract in @(
    'rerun-if-changed=icons/app-icon-cz-moon-gate-lantern-v1.ico',
    'rerun-if-changed=icons/app-icon-cz-moon-gate-lantern-v1.png'
)) {
    if ($tauriBuildScript.IndexOf($requiredTauriIconContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "Tauri build.rs missing icon rebuild contract: $requiredTauriIconContract"
    }
}
$tauriConfigPath = Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\tauri.conf.json'
$tauriConfig = Get-Content -Raw -Encoding UTF8 -LiteralPath $tauriConfigPath | ConvertFrom-Json
$tauriAppPropertyNames = @($tauriConfig.app.PSObject.Properties | Select-Object -ExpandProperty Name)
if ($tauriAppPropertyNames -contains 'trayIcon') {
    throw 'tauri.conf.json must not define a duplicate automatic tray icon'
}
$expectedTauriBundleIcons = @(
    'icons/app-icon-cz-moon-gate-lantern-v1.ico',
    'icons/app-icon-cz-moon-gate-lantern-v1.png'
)
$actualTauriBundleIcons = @($tauriConfig.bundle.icon | ForEach-Object { [string]$_ })
if (
    $actualTauriBundleIcons.Count -ne $expectedTauriBundleIcons.Count -or
    (($actualTauriBundleIcons -join "`n") -cne ($expectedTauriBundleIcons -join "`n"))
) {
    throw "tauri.conf.json bundle.icon must exactly equal: $($expectedTauriBundleIcons -join ', ')"
}
$tauriMain = Get-Content -Raw -Encoding UTF8 -LiteralPath (
    Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\src\main.rs'
)
$buildTrayStart = $tauriMain.IndexOf('fn build_tray', [System.StringComparison]::Ordinal)
$buildConsoleStart = if ($buildTrayStart -ge 0) {
    $tauriMain.IndexOf('fn build_console_window', $buildTrayStart + 1, [System.StringComparison]::Ordinal)
} else {
    -1
}
if ($buildTrayStart -lt 0 -or $buildConsoleStart -le $buildTrayStart) {
    throw 'Tauri build_tray function range is missing or malformed'
}
$buildTraySource = $tauriMain.Substring($buildTrayStart, $buildConsoleStart - $buildTrayStart)
if ([regex]::Matches($buildTraySource, 'TrayIconBuilder::').Count -ne 1) {
    throw 'Tauri build_tray must contain exactly one TrayIconBuilder'
}
foreach ($requiredTrayIconContract in @(
    'TrayIconBuilder::with_id("main")',
    '.icon(',
    '.menu(&menu)',
    '.show_menu_on_left_click(false)',
    '.on_menu_event(',
    '.on_tray_icon_event(',
    'default_window_icon',
    'TrayIconEvent::Click',
    'MouseButton::Left',
    'MouseButtonState::Up'
)) {
    if ($buildTraySource.IndexOf($requiredTrayIconContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "Tauri build_tray missing manual tray contract: $requiredTrayIconContract"
    }
}
if ($buildTraySource.IndexOf('.show_menu_on_left_click(true)', [System.StringComparison]::Ordinal) -ge 0) {
    throw 'Tauri build_tray must not open the menu on left click when left click toggles the console'
}
$trayEventStart = $buildTraySource.IndexOf('.on_tray_icon_event(', [System.StringComparison]::Ordinal)
if ($trayEventStart -lt 0) {
    throw 'Tauri build_tray missing tray event handler range'
}
$trayEventSource = $buildTraySource.Substring($trayEventStart)
if ([regex]::Matches($trayEventSource, 'do_toggle_console').Count -ne 1) {
    throw 'Tauri tray event handler must toggle the console exactly once'
}
if ($trayEventSource -match 'TrayIconEvent::(?:Enter|Move|Leave|DoubleClick)') {
    throw 'Tauri tray hover/double-click events must not toggle the console'
}
if ($trayEventSource -notmatch 'TrayIconEvent::Click\s*\{[\s\S]*button:\s*MouseButton::Left[\s\S]*button_state:\s*MouseButtonState::Up') {
    throw 'Tauri tray toggle must be restricted to left-button release Click'
}

$activeIconContractFiles = @(
    (Join-Path $workspace 'installer\Product.wxs'),
    (Join-Path $workspace 'scripts\build-msi.ps1'),
    (Join-Path $workspace 'packages\app-launcher\build.rs'),
    (Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\build.rs'),
    (Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\src\main.rs'),
    (Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\build.rs'),
    $tauriConfigPath,
    (Join-Path $workspace 'modules\gui-desktop\packages\tauri-shell\src-tauri\src\main.rs')
)
$forbiddenActiveIconTokens = @(
    'coolzhu-icons-2026-08-12',
    'coolzhu-application-icon',
    'coolzhu-installer-icon',
    'icons/icon.ico',
    'icons/icon.png',
    'coolzhu-agent-icon.png',
    'InstallerIcon'
)
foreach ($activeIconContractFile in $activeIconContractFiles) {
    $activeIconSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $activeIconContractFile
    foreach ($forbiddenActiveIconToken in $forbiddenActiveIconTokens) {
        if ($activeIconSource.IndexOf($forbiddenActiveIconToken, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw "active icon chain still references retired icon token '$forbiddenActiveIconToken': $activeIconContractFile"
        }
    }
}
$desktopConsoleMain = Get-Content -Raw -Encoding UTF8 -LiteralPath (
    Join-Path $workspace 'modules\gui-desktop\packages\desktop-console\src\main.rs'
)
if ($desktopConsoleMain.IndexOf('app-icon-cz-moon-gate-lantern-v1.png', [System.StringComparison]::Ordinal) -lt 0) {
    throw 'desktop-console missing P5-F window icon contract'
}
$buildMsiTokens = $null
$buildMsiParseErrors = $null
$buildMsiAst = [System.Management.Automation.Language.Parser]::ParseInput(
    $buildMsiScript,
    [ref]$buildMsiTokens,
    [ref]$buildMsiParseErrors
)
if ($buildMsiParseErrors.Count -gt 0) {
    throw "build-msi.ps1 parse failed: $($buildMsiParseErrors[0].Message)"
}
foreach ($requiredLocalRuntimeContract in @(
    '$env:DOTNET_ROOT = $localDotnetRoot',
    '$env:DOTNET_ROOT_X64 = $localDotnetRoot',
    '$useLocalDotnetForWix = Test-Path',
    '$wixBuildArgs = @(',
    '& $localDotnetExe $wixDll @wixBuildArgs',
    '$wixExitCode = $LASTEXITCODE'
)) {
    if ($buildMsiScript.IndexOf($requiredLocalRuntimeContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "build-msi.ps1 missing local .NET runtime contract: $requiredLocalRuntimeContract"
    }
}

foreach ($requiredReleaseContract in @(
    'COOLZHU_RELEASE_VERSION = $Version',
    'COOLZHU_BUILD_DATE = (Get-Date).ToUniversalTime()',
    'COOLZHU_GIT_SHA = Get-ReleaseSourceCommit',
    'COOLZHU_BUILD_TARGET = Get-ReleaseBuildTarget',
    'Invoke-WithReleaseBuildEnvironment -Environment $releaseBuildEnvironment -Action',
    '$stagedCliVersion = Assert-StagedCliVersion',
    '& $CliPath --version',
    '[string]::Equals($reportedVersion, $ExpectedVersion, [System.StringComparison]::Ordinal)',
    'cli_version = $stagedCliVersion'
)) {
    if ($buildMsiScript.IndexOf($requiredReleaseContract, [System.StringComparison]::Ordinal) -lt 0) {
        throw "build-msi.ps1 missing release version contract: $requiredReleaseContract"
    }
}

$releaseEnvironmentNames = @(
    'COOLZHU_RELEASE_VERSION',
    'COOLZHU_BUILD_DATE',
    'COOLZHU_GIT_SHA',
    'COOLZHU_BUILD_TARGET'
)
$originalReleaseEnvironment = @{}
foreach ($name in $releaseEnvironmentNames) {
    $originalReleaseEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
try {
    foreach ($name in $releaseEnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name, "package-safety-sentinel-$name", 'Process')
    }
    $releaseEnvironmentHelper = $buildMsiAst.Find(
        {
            param($node)
            $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
                $node.Name -eq 'Invoke-WithReleaseBuildEnvironment'
        },
        $true
    )
    if (-not $releaseEnvironmentHelper) {
        throw 'build-msi.ps1 missing Invoke-WithReleaseBuildEnvironment helper'
    }
    Invoke-Expression $releaseEnvironmentHelper.Extent.Text

    $fixtureReleaseEnvironment = [ordered]@{
        COOLZHU_RELEASE_VERSION = '9.8.7'
        COOLZHU_BUILD_DATE = '2099-12-31'
        COOLZHU_GIT_SHA = '1111111111111111111111111111111111111111'
        COOLZHU_BUILD_TARGET = 'release-contract-fixture'
    }
    $actionFailure = $null
    try {
        Invoke-WithReleaseBuildEnvironment -Environment $fixtureReleaseEnvironment -Action {
            if ($env:COOLZHU_RELEASE_VERSION -ne '9.8.7') {
                throw "action observed wrong release version: $env:COOLZHU_RELEASE_VERSION"
            }
            foreach ($name in @('COOLZHU_BUILD_DATE', 'COOLZHU_GIT_SHA', 'COOLZHU_BUILD_TARGET')) {
                $observedValue = [Environment]::GetEnvironmentVariable($name, 'Process')
                if ([string]::IsNullOrWhiteSpace($observedValue) -or $observedValue -like 'package-safety-sentinel-*') {
                    throw "action did not observe injected release metadata ${name}: $observedValue"
                }
            }
            throw 'release-environment-action-sentinel'
        }
    } catch {
        $actionFailure = $_.Exception.Message
    }
    if ($actionFailure -ne 'release-environment-action-sentinel') {
        throw "release environment action did not throw its expected sentinel: $actionFailure"
    }
    foreach ($name in $releaseEnvironmentNames) {
        $restoredValue = [Environment]::GetEnvironmentVariable($name, 'Process')
        if ($restoredValue -ne "package-safety-sentinel-$name") {
            throw "build-msi did not restore process environment variable ${name}: $restoredValue"
        }
    }
} finally {
    foreach ($name in $releaseEnvironmentNames) {
        [Environment]::SetEnvironmentVariable(
            $name,
            $originalReleaseEnvironment[$name],
            'Process'
        )
    }
}

Write-Output 'PASS package-safety'
