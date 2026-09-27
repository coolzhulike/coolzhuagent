$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $workspace 'config\package-manifest.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json

$artifactIds = @($manifest.artifacts | ForEach-Object { [string]$_.id })
$resourceIds = @($manifest.resources | ForEach-Object { [string]$_.id })

if ($artifactIds -notcontains 'gui-web.browser-native-host') {
    throw 'package manifest must include gui-web.browser-native-host artifact'
}

if ($artifactIds -notcontains 'gui-web.clawbot-sidecar') {
    throw 'package manifest must include gui-web.clawbot-sidecar artifact'
}

if ($artifactIds -notcontains 'gui-desktop.webview2-loader') {
    throw 'package manifest must include the Tauri WebView2 loader artifact'
}

$nativeHost = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-web.browser-native-host' })[0]
if ([string]$nativeHost.source -ne 'modules/gui-web/target/{profile}/coolzhu-browser-native-host.exe') {
    throw "unexpected native host source: $($nativeHost.source)"
}
if ([string]$nativeHost.target -ne 'bin/coolzhu-browser-native-host.exe') {
    throw "unexpected native host target: $($nativeHost.target)"
}

$clawbotSidecar = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-web.clawbot-sidecar' })[0]
if ([string]$clawbotSidecar.source -ne 'modules/gui-web/target/{profile}/coolzhu-clawbot-sidecar.exe') {
    throw "unexpected ClawBot sidecar source: $($clawbotSidecar.source)"
}
if ([string]$clawbotSidecar.target -ne 'bin/coolzhu-clawbot-sidecar.exe') {
    throw "unexpected ClawBot sidecar target: $($clawbotSidecar.target)"
}

$webView2Loader = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.webview2-loader' })[0]
# 声明源必须是**受控稳定导出路径**：不得直接指向 webview2-com-sys-* 这类动态哈希目录，
# 也不得指回 tauri-shell 的 profile 目录（tauri-build 只在 gnu 目标下才往那里写 Loader）。
if ([string]$webView2Loader.source -ne 'modules/gui-desktop/target/package-inputs/windows-x64/{profile}/WebView2Loader.dll') {
    throw "unexpected WebView2 loader source: $($webView2Loader.source)"
}
if ([string]$webView2Loader.source -match 'webview2-com-sys') {
    throw 'WebView2 loader source must not depend on a cargo hashed build directory'
}
if ([string]$webView2Loader.target -ne 'bin/WebView2Loader.dll') {
    throw "unexpected WebView2 loader target: $($webView2Loader.target)"
}
if (-not $webView2Loader.export) {
    throw 'WebView2 loader artifact must declare an export contract so its provenance is checkable'
}
if ([string]$webView2Loader.export.producer_package -ne 'webview2-com-sys') {
    throw "unexpected WebView2 loader export producer_package: $($webView2Loader.export.producer_package)"
}
if ([string]$webView2Loader.export.build_entry_artifact -ne 'gui-desktop.tauri-shell') {
    throw "unexpected WebView2 loader export build_entry_artifact: $($webView2Loader.export.build_entry_artifact)"
}
if ([string]$webView2Loader.export.architecture -ne 'x64') {
    throw "unexpected WebView2 loader export architecture: $($webView2Loader.export.architecture)"
}
$tauriShellArtifact = @($manifest.artifacts | Where-Object { $_.id -eq 'gui-desktop.tauri-shell' })[0]
if ([string]$tauriShellArtifact.build.capture -ne 'cargo-json-messages') {
    throw 'the tauri-shell build entry must capture cargo json messages so the loader producer can be located'
}
if (-not (@($tauriShellArtifact.build.args) | Where-Object { $_ -like '--message-format=*' })) {
    throw 'the tauri-shell build entry must set --message-format when capture is enabled'
}
if (-not (Test-Path -LiteralPath (Join-Path $workspace ([string]$webView2Loader.export.build_entry_manifest)) -PathType Leaf)) {
    throw 'WebView2 loader export build_entry_manifest must exist'
}
if (-not (Test-Path -LiteralPath (Join-Path $workspace ([string]$webView2Loader.export.lockfile)) -PathType Leaf)) {
    throw 'WebView2 loader export lockfile must exist'
}

if ($resourceIds -notcontains 'browser.extension') {
    throw 'package manifest must include browser.extension resource'
}

$extension = @($manifest.resources | Where-Object { $_.id -eq 'browser.extension' })[0]
if ([string]$extension.source -ne 'modules/browser-extension') {
    throw "unexpected browser extension source: $($extension.source)"
}
if ([string]$extension.target -ne 'modules/browser-extension') {
    throw "unexpected browser extension target: $($extension.target)"
}

if ($resourceIds -notcontains 'gui-web.stt-models') {
    throw 'package manifest must include gui-web.stt-models resource'
}

$sttModels = @($manifest.resources | Where-Object { $_.id -eq 'gui-web.stt-models' })[0]
if ([string]$sttModels.source -ne 'modules/gui-web/packages/web-console/models') {
    throw "unexpected STT models source: $($sttModels.source)"
}
if ([string]$sttModels.target -ne 'bin/models') {
    throw "unexpected STT models target: $($sttModels.target)"
}

if ($resourceIds -notcontains 'documentation.user-guide') {
    throw 'package manifest must include documentation.user-guide resource'
}

$userGuide = @($manifest.resources | Where-Object { $_.id -eq 'documentation.user-guide' })[0]
if ([string]$userGuide.source -ne 'docs/user-guide') {
    throw "unexpected user guide source: $($userGuide.source)"
}
if ([string]$userGuide.target -ne 'docs/user-guide') {
    throw "unexpected user guide target: $($userGuide.target)"
}

if ($resourceIds -notcontains 'documentation.command-line') {
    throw 'package manifest must include documentation.command-line resource'
}

$commandLineGuide = @($manifest.resources | Where-Object { $_.id -eq 'documentation.command-line' })[0]
if ([string]$commandLineGuide.source -ne 'docs/command-line.md') {
    throw "unexpected command-line guide source: $($commandLineGuide.source)"
}
if ([string]$commandLineGuide.target -ne 'docs/command-line.md') {
    throw "unexpected command-line guide target: $($commandLineGuide.target)"
}
if (-not (Test-Path -LiteralPath (Join-Path $workspace ([string]$commandLineGuide.source)) -PathType Leaf)) {
    throw 'command-line guide source file is missing'
}

# ---------------------------------------------------------------------------
# RD4-06：源码快照边界与构建输入声明（第五轮裁决 B-1）
#   没有这三条声明，package-all 就无法回答"本次用了哪份源码 / 配合什么构建输入"。
# ---------------------------------------------------------------------------
if (-not $manifest.PSObject.Properties['source_snapshot'] -or -not $manifest.source_snapshot) {
    throw 'package manifest must declare source_snapshot so the source snapshot boundary is auditable'
}
$snapshotRoots = @($manifest.source_snapshot.roots | Where-Object { $_ })
if ($snapshotRoots.Count -eq 0) {
    throw 'source_snapshot.roots must list the first-party source roots'
}

foreach ($requiredRoot in @(
        'Cargo.toml',
        'Cargo.lock',
        'scripts',
        'installer',
        'config',
        'modules',
        '.coolzhu/plugins',
        'docs/design-assets'
    )) {
    if ($snapshotRoots -notcontains $requiredRoot) {
        throw "source_snapshot.roots must include $requiredRoot (build scripts / installer definition / path dependencies / installer icon)"
    }
}

# 独立 Tauri 项目（独立 Cargo 项目）必须在源码快照范围内：其构建入口与锁文件不能只靠顶层 workspace 覆盖。
$tauriBuildEntryRoot = 'modules/gui-desktop/packages/tauri-shell/src-tauri'
$tauriCoveredByRoot = $false
foreach ($root in $snapshotRoots) {
    if ($root -eq $tauriBuildEntryRoot -or $tauriBuildEntryRoot.StartsWith($root + '/', [System.StringComparison]::Ordinal)) {
        $tauriCoveredByRoot = $true
        break
    }
}
if (-not $tauriCoveredByRoot) {
    throw "source_snapshot.roots must cover the standalone Tauri project: $tauriBuildEntryRoot"
}

if (-not $manifest.PSObject.Properties['build_inputs'] -or -not $manifest.build_inputs) {
    throw 'package manifest must declare build_inputs (lockfiles / build entry declarations / installer definition)'
}
$buildInputFiles = @($manifest.build_inputs.files | Where-Object { $_ })
foreach ($requiredBuildInput in @(
        'Cargo.lock',
        'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml',
        'modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.lock',
        'modules/gui-desktop/packages/tauri-shell/src-tauri/build.rs',
        'modules/gui-desktop/packages/tauri-shell/src-tauri/tauri.conf.json',
        'installer/Product.wxs'
    )) {
    if ($buildInputFiles -notcontains $requiredBuildInput) {
        throw "build_inputs.files must include $requiredBuildInput"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $workspace $requiredBuildInput) -PathType Leaf)) {
        throw "declared build input does not exist: $requiredBuildInput"
    }
}

# 外部构建依赖必须显式登记（不得靠无边界追随 junction 解决）。
if (-not $manifest.source_snapshot.PSObject.Properties['external_path_dependencies']) {
    throw 'source_snapshot.external_path_dependencies must exist (may be empty) so external build dependencies are registered explicitly'
}
foreach ($external in @($manifest.source_snapshot.external_path_dependencies)) {
    if (-not $external.path -or -not $external.reason) {
        throw 'each source_snapshot.external_path_dependencies entry needs both path and reason'
    }
}

# 排除规则必须真的拦住"不得混入"的类别（相对各声明 root 匹配）。
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$excludePatterns = @($script:SourceSnapshotDefaultExcludePatterns)
foreach ($forbidden in @(
        'target/debug/app.exe',
        'dist/x.msi',
        'node_modules/left-pad/index.js',
        'a/b/tmp/scratch.txt',
        '.coolzhu/web-sessions.json',
        'run/web-sessions.sqlite3',
        'credentials.json',
        'secrets.json',
        'token-cache.json',
        '.env',
        '.env.local',
        'model/coolzhu.toml',
        'keys/server.pem',
        'keys/signing.pfx',
        'models/local-vlm/weights.gguf',
        'models/uidetr/model.onnx',
        'model.safetensors',
        'gen/schemas/capabilities.json',
        'logs/app.log',
        'app.sqlite3-wal'
    )) {
    if (-not (Test-IdentityPatternMatch -RelativePath $forbidden -Patterns $excludePatterns)) {
        throw "source snapshot exclusion rules must block: $forbidden"
    }
}
# 反向：正常源码路径不得被排除规则误伤。
foreach ($mustStayIncluded in @(
        'crate/src/lib.rs',
        'src/main.rs',
        'tools/whisper_transcribe.py',
        'ui/assets/pet-theme.json',
        'assets/avatars/manifest.json',
        'installer/Product.wxs',
        'Cargo.lock',
        'build.rs',
        'web/app.js'
    )) {
    if (Test-IdentityPatternMatch -RelativePath $mustStayIncluded -Patterns $excludePatterns) {
        throw "source snapshot exclusion rules must not block first-party source: $mustStayIncluded"
    }
}


# ---------------------------------------------------------------------------
# RD4-06 准备/冻结分离（第七轮裁决 §3.1）：manifest 侧必须声明锁定语义与派生输出位置。
#   * 不设 Cargo.lock / tauri.conf.json 的构建中变更白名单；
#   * Cargo 正式入口用锁定语义（--locked 或等价 --frozen）；
#   * 生成式配置与派生输出位置显式声明，便于审查。
# ---------------------------------------------------------------------------
$preparation = $manifest.release_policy.preparation_phase
if (-not $preparation) {
    throw 'release_policy.preparation_phase must declare the RD4-06 prepare/freeze policy'
}
if (-not $preparation.cargo_lock_semantics -or -not [string]$preparation.cargo_lock_semantics.build) {
    throw 'preparation_phase must declare cargo_lock_semantics.build (locked semantics for the formal build)'
}
if ([string]$preparation.cargo_lock_semantics.no_whitelist -notmatch '白名单') {
    throw 'preparation_phase must state that there is no mid-build change whitelist'
}
foreach ($artifact in @($manifest.artifacts)) {
    $build = $artifact.build
    if (-not $build) { continue }
    if ([string]$build.command -ne 'cargo') { continue }
    $args = @($build.args | ForEach-Object { [string]$_ })
    if (-not (($args -contains '--locked') -or ($args -contains '--frozen'))) {
        throw ("cargo artifact {0} must use locked semantics (--locked or --frozen) in the formal build" -f [string]$artifact.id)
    }
    if (-not ($args -contains '--offline')) {
        throw ("cargo artifact {0} must keep the existing --offline requirement" -f [string]$artifact.id)
    }
}
if (@($preparation.derived_outputs).Count -lt 3) {
    throw 'preparation_phase.derived_outputs must declare the derived output locations (build intermediates, export slot, consumption staging, freeze records)'
}
foreach ($derived in @($preparation.derived_outputs)) {
    if (-not [string]$derived.path -or -not [string]$derived.generated_by) {
        throw ("derived output {0} must declare both path and generated_by" -f [string]$derived.path)
    }
}
if ([string]$preparation.generated_config_rule -notmatch '计入冻结输入') {
    throw 'preparation_phase.generated_config_rule must state that generated configs count as frozen inputs'
}
if ([string]$preparation.input_set_mutation_policy -notmatch '不得') {
    throw 'preparation_phase.input_set_mutation_policy must forbid mutating/shrinking the frozen input set'
}
foreach ($path in @('(^|/)tmp/package-freeze(/|$)', '(^|/)tmp/package-consume(/|$)')) {
    if (-not (@($manifest.release_policy.non_build_input_paths) | Where-Object { [string]$_.pattern -eq $path })) {
        throw ("release_policy.non_build_input_paths must declare {0} as a non build input" -f $path)
    }
}
Write-Host 'RD4-06 prepare/freeze policy: locked cargo semantics + declared derived outputs + no mid-build whitelist'

Write-Output 'PASS package-manifest'
