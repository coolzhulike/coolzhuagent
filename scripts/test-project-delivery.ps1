$ErrorActionPreference = 'Stop'

& (Join-Path $PSScriptRoot 'test-powershell-script-compat.ps1')

$workspace = Split-Path -Parent $PSScriptRoot
$script = Join-Path $PSScriptRoot 'project-delivery.ps1'
$sandbox = Join-Path $workspace 'tmp\tests\project-delivery'
$source = Join-Path $sandbox 'workspace'
$destination = Join-Path $sandbox 'delivery'
$mapPath = Join-Path $sandbox 'curation-map.json'
$publicDocsPath = Join-Path $source 'docs\github-public-docs.json'

if (Test-Path -LiteralPath $sandbox) {
    Remove-Item -LiteralPath $sandbox -Recurse -Force
}
New-Item -ItemType Directory -Force -Path `
    (Join-Path $source 'src'), `
    (Join-Path $source 'backup'), `
    (Join-Path $source '.coolzhu'), `
    (Join-Path $source '.coolzhu\plugins\example-plugin\src'), `
    (Join-Path $source '.claude'), `
    (Join-Path $source '.superpowers'), `
    (Join-Path $source 'modules\tooling\.claw-agents'), `
    (Join-Path $source 'modules\gui-web\src'), `
    (Join-Path $source 'modules\gui-web\src\__pycache__'), `
    (Join-Path $source 'modules\gui-web\session-attachments'), `
    (Join-Path $source 'modules\gui-web\audio-realtime'), `
    (Join-Path $source 'modules\gui-web\.coolzhu\audio-realtime\chunks'), `
    (Join-Path $source 'modules\gui-web\docs'), `
    (Join-Path $source 'modules\gui-desktop\packages\tauri-shell\src-tauri\gen\schemas'), `
    (Join-Path $source 'modules\gui-desktop\assets\generated-previews-test'), `
    (Join-Path $source 'target'), `
    (Join-Path $source 'config'), `
    (Join-Path $source 'docs') | Out-Null
Set-Content -LiteralPath (Join-Path $source 'src\main.rs') -Value 'fn main() {}'
Set-Content -LiteralPath (Join-Path $source 'backup\main.rs') -Value 'backup'
Set-Content -LiteralPath (Join-Path $source '.coolzhu\web-sessions.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $source '.coolzhu\plugins\example-plugin\Cargo.toml') -Value '[package]'
Set-Content -LiteralPath (Join-Path $source '.coolzhu\plugins\example-plugin\src\lib.rs') -Value 'pub fn plugin() {}'
Set-Content -LiteralPath (Join-Path $source '.claude\launch.json') -Value '{"program":"C:\\Users\\private\\app.exe"}'
Set-Content -LiteralPath (Join-Path $source '.superpowers\draft.md') -Value '# Private draft'
Set-Content -LiteralPath (Join-Path $source 'modules\tooling\.claw-agents\agent.json') -Value '{"workspace":"private"}'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\main.rs') -Value 'pub fn serve() {}'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\__pycache__\cached.pyc') -Value 'compiled private path'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\session-attachments\private.bin') -Value 'private attachment'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\audio-realtime\private.webm') -Value 'private audio'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\private-model.pt') -Value 'private model'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\login-qrcode.png') -Value 'private qrcode'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\provider-secret.pem') -Value 'private key'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\app.log') -Value 'private log'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\chat.db-wal') -Value 'private database sidecar'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\src\.claw-todos.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\.coolzhu\audio-realtime\chunks\private.webm') -Value 'private audio'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-web\docs\private.md') -Value '# Private acceptance log'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-desktop\packages\tauri-shell\src-tauri\gen\schemas\windows-schema.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $source 'modules\gui-desktop\assets\generated-previews-test\manifest.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $source 'target\app.exe') -Value 'generated'
Set-Content -LiteralPath (Join-Path $source 'config\package-launcher.json') -Value '{}'
Set-Content -LiteralPath (Join-Path $source 'config\private-model.json') -Value '{"api_key":"NOPE"}'
Set-Content -LiteralPath (Join-Path $source 'coolzhu.toml') -Value '[models]'
Set-Content -LiteralPath (Join-Path $source 'docs\current.md') -Value '# Current'
@(
    [ordered]@{ source = 'docs/current.md'; destination = '01-项目/当前.md'; topic = 'architecture'; status = 'current' }
) | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $mapPath -Encoding UTF8
@{
    version = 1
    documents = @('docs/current.md')
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $publicDocsPath -Encoding UTF8

& $script -WorkspaceRoot $source -DestinationRoot $destination -CurationMap $mapPath -PublicDocsManifest $publicDocsPath -InventoryOnly | Out-Null
$manifestPath = Join-Path $destination 'source\SOURCE-MANIFEST.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$paths = @($manifest.files.relative_path | Sort-Object)
if (($paths -join ',') -ne '.coolzhu\plugins\example-plugin\Cargo.toml,.coolzhu\plugins\example-plugin\src\lib.rs,config\package-launcher.json,docs\current.md,docs\github-public-docs.json,modules\gui-web\src\main.rs,src\main.rs') {
    throw "unexpected inventory: $($paths -join ',')"
}
foreach ($field in @('relative_path', 'size', 'sha256', 'repository', 'git_status')) {
    if ($null -eq $manifest.files[0].$field) { throw "manifest field missing: $field" }
}

$deliveryResult = & $script -WorkspaceRoot $source -DestinationRoot $destination -CurationMap $mapPath -PublicDocsManifest $publicDocsPath -DateStamp '20260628'
$vaultFile = Join-Path $destination 'obsidian-vault\01-项目\当前.md'
if (-not (Test-Path -LiteralPath $vaultFile)) { throw 'curated vault file missing' }
$vaultText = Get-Content -LiteralPath $vaultFile -Raw
if ($vaultText -notmatch 'source_path: docs/current.md' -or $vaultText -notmatch '# Current') { throw 'vault content invalid' }
$vaultHome = Get-Content -LiteralPath (Join-Path $destination 'obsidian-vault\00-首页\项目总览.md') -Raw
if ($vaultHome -notmatch '\[\[01-项目/当前\|当前\]\]' -or $vaultHome -match '\[\[[^\]]+\.\|') {
    throw 'generated vault wikilink is invalid'
}

$zipPath = Join-Path $destination 'source\coolzhu-agent-source-20260628.zip'
$expanded = Join-Path $sandbox 'expanded'
Expand-Archive -LiteralPath $zipPath -DestinationPath $expanded -Force
$zipManifest = Get-Content -LiteralPath (Join-Path $expanded 'SOURCE-MANIFEST.json') -Raw | ConvertFrom-Json
foreach ($entry in $zipManifest.files) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $expanded $entry.relative_path)).Hash
    if ($actual -ne $entry.sha256) { throw "zip hash mismatch: $($entry.relative_path)" }
}
if (Get-ChildItem -LiteralPath $expanded -Recurse -Force | Where-Object FullName -Match '(?i)(backup|web-sessions|coolzhu\.toml|private-model|private\.webm|private\.bin|login-qrcode|provider-secret|app\.log|chat\.db-wal|\.claw-todos|windows-schema\.json)') {
    throw 'excluded content found in source zip'
}
if (-not (Test-Path -LiteralPath $deliveryResult.github_tree -PathType Container)) { throw 'GitHub-ready source tree missing' }
$privacyReport = Get-Content -LiteralPath $deliveryResult.privacy_report -Raw | ConvertFrom-Json
if (-not $privacyReport.safe) { throw 'privacy report is not safe' }
if (-not (Test-Path -LiteralPath (Join-Path $deliveryResult.github_tree '.coolzhu\plugins\example-plugin\src\lib.rs'))) {
    throw 'workspace plugin source missing from GitHub-ready tree'
}
# 在交付树内创建隔离仓库，避免 git check-ignore 继承外层工作区对 tmp/ 的忽略规则。
& git -C $deliveryResult.github_tree init --quiet
if ($LASTEXITCODE -ne 0) { throw 'failed to initialize isolated GitHub-ready test repository' }
& git -C $deliveryResult.github_tree check-ignore --no-index '.coolzhu/plugins/example-plugin/src/lib.rs' 2>$null | Out-Null
if ($LASTEXITCODE -eq 0) { throw 'GitHub-ready .gitignore hides workspace plugin source' }
& git -C $deliveryResult.github_tree check-ignore --no-index 'modules/gui-web/src/main.rs' 2>$null | Out-Null
if ($LASTEXITCODE -eq 0) { throw 'GitHub-ready .gitignore hides module source' }
& git -C $deliveryResult.github_tree check-ignore --no-index 'target/debug/app.exe' 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'GitHub-ready .gitignore does not block build outputs' }
foreach ($privatePath in @(
    '.claude/launch.json',
    '.superpowers/draft.md',
    'modules/gui-web/src/__pycache__/cached.pyc',
    'modules/gui-web/src/cached.pyc',
    'modules/vision/resources/private-model.pt',
    'runtime/web-sessions.backup',
    'runtime/session-attachments/private.bin',
    'runtime/login-qrcode.png',
    'runtime/provider-secret.pem',
    'runtime/app.log'
)) {
    & git -C $deliveryResult.github_tree check-ignore --no-index $privatePath 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "GitHub-ready .gitignore does not block private/generated path: $privatePath" }
}

# 回归：隐私扫描不得再跳过大于 2 MB 的源码。将当前用户名路径放到
# 2 MB 之后，旧实现会漏过，新实现必须拒绝整个交付。
$largePrivatePath = Join-Path $source 'src\large-private.rs'
$largePrefix = 'x' * (2MB + 1024)
$privateSuffix = "`n// C:\Users\$env:USERNAME\private-project"
[System.IO.File]::WriteAllText(
    $largePrivatePath,
    $largePrefix + $privateSuffix,
    [System.Text.UTF8Encoding]::new($false)
)
$unsafeDestination = Join-Path $sandbox 'unsafe-delivery'
$unsafeRejected = $false
try {
    & $script -WorkspaceRoot $source -DestinationRoot $unsafeDestination -CurationMap $mapPath -PublicDocsManifest $publicDocsPath -DateStamp '20260628' | Out-Null
} catch {
    $unsafeRejected = $_.Exception.Message -match 'private-user-path'
}
if (-not $unsafeRejected) {
    throw 'large text privacy fixture was not rejected'
}

Write-Output 'PASS project-delivery'
