param(
    [string]$Version = '0.2.0',
    [ValidateSet('debug', 'release')]
    [string]$Configuration = 'debug',
    [string]$PackageRoot = 'package',
    [switch]$SkipPackageBuild
)

$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$workspacePath = [System.IO.Path]::GetFullPath($workspace).TrimEnd('\', '/')
$workspacePrefix = $workspacePath + [System.IO.Path]::DirectorySeparatorChar

# 构建身份与报告治理（三身份分离 / 报告 ID+内容哈希 / 保留规则）。
. (Join-Path $workspacePath 'scripts/lib/build-identity.ps1')
$packageRootCandidate = if ([System.IO.Path]::IsPathRooted($PackageRoot)) {
    $PackageRoot
} else {
    Join-Path $workspace $PackageRoot
}
$packageRoot = [System.IO.Path]::GetFullPath($packageRootCandidate).TrimEnd('\', '/')
$productWxs = Join-Path $workspace 'installer\Product.wxs'
$applicationIcon = Join-Path $workspace 'docs\design-assets\coolzhu-icons-2026-08-27\final\app-icon-cz-moon-gate-lantern-v1.ico'
$distDir = Join-Path $workspace 'dist'
$localDotnetExe = Join-Path $workspace 'tmp\tools\dotnet\dotnet.exe'
$wixToolDir = Join-Path $workspace 'tmp\tools\wix'
$wixExe = Join-Path $wixToolDir 'wix.exe'
$wixDll = Join-Path $wixToolDir '.store\wix\5.0.2\wix\5.0.2\tools\net6.0\any\wix.dll'

if (Test-Path -LiteralPath $localDotnetExe -PathType Leaf) {
    $localDotnetRoot = Split-Path -Parent $localDotnetExe
    $env:DOTNET_ROOT = $localDotnetRoot
    $env:DOTNET_ROOT_X64 = $localDotnetRoot
}

function ConvertTo-WorkspaceRelativePath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not $fullPath.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "installer output must stay inside workspace: $fullPath"
    }
    return $fullPath.Substring($workspacePrefix.Length).Replace('\', '/')
}

function Get-ReleaseSourceCommit {
    $gitOutput = @(& git -C $workspace rev-parse --verify HEAD 2>&1)
    $gitExitCode = $LASTEXITCODE
    if ($gitExitCode -ne 0) {
        throw "git rev-parse HEAD failed with exit code ${gitExitCode}: $($gitOutput -join ' ')"
    }
    $commit = ([string]($gitOutput | Select-Object -First 1)).Trim()
    if ($commit -notmatch '^[0-9a-fA-F]{40}$') {
        throw "git rev-parse HEAD returned an invalid commit: $commit"
    }
    return $commit.ToLowerInvariant()
}

function Get-ReleaseBuildTarget {
    $configuredTarget = [Environment]::GetEnvironmentVariable('CARGO_BUILD_TARGET', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($configuredTarget)) {
        $target = $configuredTarget.Trim()
    } else {
        $cargoVersionOutput = @(& cargo -vV 2>&1)
        $cargoExitCode = $LASTEXITCODE
        if ($cargoExitCode -ne 0) {
            throw "cargo -vV failed with exit code ${cargoExitCode}: $($cargoVersionOutput -join ' ')"
        }
        $hostTargets = @(
            foreach ($line in $cargoVersionOutput) {
                $match = [regex]::Match([string]$line, '^host:\s*(\S+)\s*$')
                if ($match.Success) {
                    $match.Groups[1].Value
                }
            }
        )
        if ($hostTargets.Count -ne 1) {
            throw 'cargo -vV did not report exactly one host target'
        }
        $target = [string]$hostTargets[0]
    }
    if ($target -notmatch '^[A-Za-z0-9_.-]+$') {
        throw "invalid Cargo build target: $target"
    }
    return $target
}

function Assert-StagedCliVersion {
    param(
        [Parameter(Mandatory = $true)][string]$CliPath,
        [Parameter(Mandatory = $true)][string]$ExpectedVersion
    )

    if (-not (Test-Path -LiteralPath $CliPath -PathType Leaf)) {
        throw "staged CLI missing: $CliPath"
    }
    $versionOutput = @(& $CliPath --version 2>&1)
    $versionExitCode = $LASTEXITCODE
    if ($versionExitCode -ne 0) {
        throw "staged CLI --version failed with exit code $versionExitCode"
    }
    $reportedVersions = @(
        foreach ($line in $versionOutput) {
            $match = [regex]::Match([string]$line, '^\s*Version\s+(\d+\.\d+\.\d+)\s*$')
            if ($match.Success) {
                $match.Groups[1].Value
            }
        }
    )
    if ($reportedVersions.Count -ne 1) {
        throw 'staged CLI --version did not report exactly one three-part Version field'
    }
    $reportedVersion = [string]$reportedVersions[0]
    if (-not [string]::Equals($reportedVersion, $ExpectedVersion, [System.StringComparison]::Ordinal)) {
        throw "staged CLI version mismatch: MSI requests $ExpectedVersion but CLI reports $reportedVersion"
    }
    return $reportedVersion
}

function Invoke-WithReleaseBuildEnvironment {
    param(
        [Parameter(Mandatory = $true)]
        [System.Collections.IDictionary]$Environment,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    $environmentNames = @($Environment.Keys | ForEach-Object { [string]$_ })
    $previousEnvironment = @{}
    foreach ($name in $environmentNames) {
        $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    try {
        foreach ($entry in $Environment.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, [string]$entry.Value, 'Process')
        }
        & $Action
    } finally {
        foreach ($name in $environmentNames) {
            [Environment]::SetEnvironmentVariable(
                $name,
                $previousEnvironment[$name],
                'Process'
            )
        }
    }
}

function Assert-StagedExportedArtifacts {
    <#
      导出物来源链的最后一跳：staging 里的文件必须仍与导出记录一致。
      收据缺失 / 内容不符 / 架构不符 ⇒ 拒绝生成正式包（旧包目录、裸 DLL 都不能当输入）。
    #>
    param(
        [Parameter(Mandatory = $true)][string]$PackageRootPath,
        [Parameter(Mandatory = $true)][string]$ConfigurationName
    )

    . (Join-Path $workspace 'scripts/lib/webview2-loader.ps1')
    $manifestPath = Join-Path $workspace 'config/package-manifest.json'
    $manifestData = Get-Content -Raw -LiteralPath $manifestPath -Encoding UTF8 | ConvertFrom-IdentityJson
    $summaries = [System.Collections.Generic.List[object]]::new()
    foreach ($artifact in @($manifestData.artifacts)) {
        if (-not $artifact.export) { continue }
        $stagedPath = Join-Path $PackageRootPath ([string]$artifact.target).Replace('/', '\')
        $receiptPath = Join-Path $workspace (([string]$artifact.export.receipt).Replace('{profile}', $ConfigurationName))
        if (-not (Test-Path -LiteralPath $receiptPath -PathType Leaf)) {
            throw ("[RECEIPT-MISSING] artifact={0} target={1} profile={2}`ndetail: 缺少导出记录 {3}`nnext: 先执行正常构建打包（scripts/package-all.ps1）生成导出与记录；不要手工复制文件到 package" -f [string]$artifact.id, [string]$artifact.target, $ConfigurationName, $receiptPath)
        }
        $receipt = Get-Content -Raw -LiteralPath $receiptPath -Encoding UTF8 | ConvertFrom-LoaderJson
        if (-not (Test-Path -LiteralPath $stagedPath -PathType Leaf)) {
            throw ("[SOURCE-MISSING] artifact={0} target={1} profile={2}`ndetail: staging 缺少导出物 {3}`nnext: 重新执行打包" -f [string]$artifact.id, [string]$artifact.target, $ConfigurationName, $stagedPath)
        }
        $stagedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedPath).Hash.ToLowerInvariant()
        $stagedLength = (Get-Item -LiteralPath $stagedPath).Length
        if ($stagedHash -ne ([string]$receipt.file_identity.stable_export_sha256).ToLowerInvariant() -or $stagedLength -ne [long]$receipt.file_identity.stable_export_length) {
            throw ("[CONTENT-MISMATCH] artifact={0} target={1} profile={2}`ndetail: staging 文件与导出记录不一致（staged={3}/{4} receipt={5}/{6}）`nnext: 重新执行打包；不要手工替换 package 中的文件" -f [string]$artifact.id, [string]$artifact.target, $ConfigurationName, $stagedLength, $stagedHash, [long]$receipt.file_identity.stable_export_length, [string]$receipt.file_identity.stable_export_sha256)
        }
        $expectedMachine = Get-LoaderArchitectureMachine ([string]$artifact.export.architecture)
        $stagedMachine = Get-LoaderPeMachine $stagedPath
        if ($stagedMachine -ne $expectedMachine) {
            throw ("[ARCH-MISMATCH] artifact={0} target={1} profile={2}`ndetail: staging 文件架构不匹配（expected={3} actual={4}）`nnext: 用与目标架构一致的构建重新打包" -f [string]$artifact.id, [string]$artifact.target, $ConfigurationName, $expectedMachine, $stagedMachine)
        }
        $summaries.Add([pscustomobject]@{
                artifact_id = [string]$artifact.id
                target = [string]$artifact.target
                receipt = $receiptPath.Substring($workspacePath.Length + 1).Replace('\', '/')
                sha256 = $stagedHash
                length = [long]$stagedLength
                architecture = [string]$artifact.export.architecture
                machine = $stagedMachine
                producer_package_id = [string]$receipt.dependency_identity.package_id
                producer_out_dir = [string]$receipt.dependency_identity.producer_out_dir
                reuse_nature = [string]$receipt.reuse_nature
            }) | Out-Null
    }
    return @($summaries)
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "MSI Version must be three-part numeric SemVer, got: $Version"
}

if (-not $SkipPackageBuild) {
    $releaseBuildEnvironment = [ordered]@{
        COOLZHU_RELEASE_VERSION = $Version
        COOLZHU_BUILD_DATE = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
        COOLZHU_GIT_SHA = Get-ReleaseSourceCommit
        COOLZHU_BUILD_TARGET = Get-ReleaseBuildTarget
    }
    Invoke-WithReleaseBuildEnvironment -Environment $releaseBuildEnvironment -Action {
        # package-all.ps1 的最后一个输出对象是报告指针（report_id / 内容哈希 / 载荷清单）。
        $script:packageReportPointer = & (Join-Path $workspace 'scripts\package-all.ps1') `
            -Configuration $Configuration `
            -PackageRoot $packageRoot
    }
}

# ---------------------------------------------------------------------------
# 产物清单必须引用发布报告的**唯一 ID + 内容哈希**（第五轮裁决 B-2 第 ② 项）。
# 优先用本次 package-all 的返回值；没有时退回 latest 指针文件。
# 两者都没有 ⇒ fail-closed：不允许生成"无法回溯到构建身份"的正式包。
# ---------------------------------------------------------------------------
# 注意：这里是脚本顶层作用域，$packageReportPointer 与 $script:packageReportPointer 是**同一个**
# 变量；因此不能先写 "$packageReportPointer = $null"（那会把 Action 里刚捕获的值清掉）。
# 未捕获时它本来就是 $null，读之前先判存在即可。
# PR-PKG-01（第八轮裁决 · 产物身份安全缺陷，优先级高于来源缺口）：
# **PackageRoot 是唯一输入源**。禁止退回 latest 指针——那会让"报告来自 B 代、载荷来自 A 代"
# **静默通过**（比来源缺口更危险，因为它不拒绝）。绑定关系必须在**同一个包根**内自洽：
#
#     <PackageRoot>/payload-inventory.json
#          |-- payload_digest                      （本次载荷身份）
#          |-- report_ref{report_id, report_path, content_sha256}
#          |-- build_identity.source_snapshot_digest
#                       |
#                       +--> 必须与报告内的同名字段**逐一相等**
#
# 任何不一致 ⇒ 立即失败（throw ⇒ 非零退出），**不得** warning 后继续。
$inventoryPath = Join-Path $packageRoot 'payload-inventory.json'
if (-not (Test-Path -LiteralPath $inventoryPath -PathType Leaf)) {
    throw ("[PKG-ROOT-BINDING-MISSING] profile={0}`ndetail: 包根内没有 payload-inventory.json：{1}`nnext: 用 scripts/package-all.ps1 生成包根后再生成 MSI（不要手搓包根，也不要用别处的报告）" -f $Configuration, $inventoryPath)
}
$payloadInventory = Get-Content -Raw -LiteralPath $inventoryPath -Encoding UTF8 | ConvertFrom-IdentityJson
$packageRootIdentity = "{0}@{1}" -f ([string]$payloadInventory.report_ref.report_id), ([string]$payloadInventory.payload_digest)
Write-Host ("package root identity: {0}" -f $packageRootIdentity)
Write-Host ("package root: {0} (inventory payload_digest={1} files={2})" -f $payloadInventory.package_root, $payloadInventory.payload_digest, $payloadInventory.file_count)

$reportRefPath = [string]$payloadInventory.report_ref.report_path
if ([string]::IsNullOrWhiteSpace($reportRefPath)) {
    throw ("[PKG-ROOT-BINDING-MISSING] profile={0}`ndetail: 包根清单没有 report_ref.report_path（无法回溯构建身份）" -f $Configuration)
}
$referencedReportPath = Join-Path $workspacePath ($reportRefPath.Replace('/', '\'))
if (-not (Test-Path -LiteralPath $referencedReportPath -PathType Leaf)) {
    throw ("[REPORT-REF-MISSING] profile={0}`ndetail: 包根清单指向的报告不存在：{1}`nnext: 重新执行 scripts/package-all.ps1 生成包根与报告" -f $Configuration, $referencedReportPath)
}
$referencedReport = Get-Content -Raw -LiteralPath $referencedReportPath -Encoding UTF8 | ConvertFrom-IdentityJson
# 引用必须可核对：内容哈希重算通过才允许写进 installer report。
$verifiedReportIdentity = Assert-PackageReportContentHash -Report $referencedReport

# ---- 四项一致性断言：报告与包根必须是**同一代** ----
$expectedReportContent = ([string]$payloadInventory.report_ref.content_sha256).ToLowerInvariant()
if (-not [string]::IsNullOrWhiteSpace($expectedReportContent) -and $verifiedReportIdentity.content_sha256 -ne $expectedReportContent) {
    throw ("[PKG-ROOT-BINDING-MISMATCH] 报告内容哈希不符：包根清单记 {0}，实际重算 {1}" -f $expectedReportContent, $verifiedReportIdentity.content_sha256)
}
$inventoryReportId = [string]$payloadInventory.report_ref.report_id
$reportIdActual = [string]$referencedReport.report_identity.report_id
if (-not [string]::IsNullOrWhiteSpace($inventoryReportId) -and $reportIdActual -ne $inventoryReportId) {
    throw ("[PKG-ROOT-BINDING-MISMATCH] 报告 ID 不符：包根清单记 {0}，报告内是 {1}" -f $inventoryReportId, $reportIdActual)
}
$reportPayloadDigest = [string]$referencedReport.payload_inventory.payload_digest
if ($reportPayloadDigest -ne [string]$payloadInventory.payload_digest) {
    throw ("[PKG-ROOT-BINDING-MISMATCH] 报告的载荷摘要与包根不一致：报告 {0} / 包根 {1} —— 报告与载荷不是同一代，拒绝生成 MSI" -f $reportPayloadDigest, $payloadInventory.payload_digest)
}
$reportSnapshotDigest = [string]$referencedReport.build_identity.source_snapshot_digest
$inventorySnapshotDigest = [string]$payloadInventory.build_identity.source_snapshot_digest
if ($reportSnapshotDigest -ne $inventorySnapshotDigest) {
    throw ("[PKG-ROOT-BINDING-MISMATCH] 源码快照摘要不一致：报告 {0} / 包根 {1} —— 拒绝生成 MSI" -f $reportSnapshotDigest, $inventorySnapshotDigest)
}
# 候选第 2、9 条：**失败诊断**与 release_eligible=false 的报告不是可发布收据，
# 不得作为 MSI 的发布来源（否则等于给混合输入产物补一张可发布收据）。
[void](Assert-PackageReportReleaseEligible -Report $referencedReport -ReportPath $referencedReportPath -Purpose 'msi-release')
$reportFileHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $referencedReportPath).Hash.ToLowerInvariant()
Write-Host ("package report referenced: report_id={0} content_sha256={1}" -f $verifiedReportIdentity.report_id, $verifiedReportIdentity.content_sha256)
Write-Host ("package root binding verified: identity={0} snapshot={1}" -f $packageRootIdentity, $inventorySnapshotDigest)

$buildIdentityVcs = Get-VcsReferenceState -RepoPath $workspacePath

if (-not (Test-Path -LiteralPath (Join-Path $packageRoot 'COOLZHU-AGENT.exe'))) {
    throw "Package launcher missing: $(Join-Path $packageRoot 'COOLZHU-AGENT.exe')"
}
$stagedCliVersion = Assert-StagedCliVersion `
    -CliPath (Join-Path $packageRoot 'bin\coolzhu-cli.exe') `
    -ExpectedVersion $Version

# 导出物（WebView2Loader 等）：MSI 输入必须仍与导出记录一致，才允许继续生成正式包。
$stagedExportedArtifacts = Assert-StagedExportedArtifacts `
    -PackageRootPath $packageRoot `
    -ConfigurationName $Configuration
foreach ($exported in $stagedExportedArtifacts) {
    Write-Host ("staged export verified: {0} -> {1} sha256={2} ({3})" -f $exported.artifact_id, $exported.target, $exported.sha256, $exported.reuse_nature)
}

if (-not (Test-Path -LiteralPath $productWxs)) {
    throw "WiX product file missing: $productWxs"
}
if (-not (Test-Path -LiteralPath $applicationIcon -PathType Leaf)) {
    throw "Application icon missing: $applicationIcon"
}

$packageSafetyReport = Join-Path $distDir "CoolzhuAgent-$Version-package-safety.json"
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
& (Join-Path $workspace 'scripts\package-safety.ps1') -Root $packageRoot -ReportPath $packageSafetyReport | Out-Null

if (-not (Test-Path -LiteralPath $wixExe)) {
    $dotnetExe = if (Test-Path -LiteralPath $localDotnetExe) {
        $localDotnetExe
    } else {
        $cmd = Get-Command dotnet -ErrorAction SilentlyContinue
        if ($cmd) { $cmd.Source } else { $null }
    }
    $sdkList = if ($dotnetExe) { & $dotnetExe --list-sdks 2>$null } else { $null }
    if ($LASTEXITCODE -ne 0 -or -not $sdkList) {
        throw @"
.NET SDK is required to install WiX locally, but no SDK was found.
If download speed is slow, manually install .NET SDK 8 x64 into:
  $workspace\tmp\tools\dotnet
Download:
  https://dotnet.microsoft.com/download/dotnet/8.0
Then add that dotnet.exe to PATH for this shell and rerun scripts\build-msi.ps1.
"@
    }

    New-Item -ItemType Directory -Force -Path $wixToolDir | Out-Null
    & $dotnetExe tool install wix --tool-path $wixToolDir --version 5.0.2
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet tool install wix failed with exit code $LASTEXITCODE"
    }
}

$useLocalDotnetForWix = Test-Path -LiteralPath $localDotnetExe -PathType Leaf
if ($useLocalDotnetForWix -and -not (Test-Path -LiteralPath $wixDll -PathType Leaf)) {
    throw "WiX tool assembly missing: $wixDll"
}

$canonicalMsi = Join-Path $distDir "CoolzhuAgent-$Version.msi"
$outMsi = if (Test-Path -LiteralPath $canonicalMsi) {
    Join-Path $distDir "CoolzhuAgent-$Version-$(Get-Date -Format 'yyyyMMdd-HHmmss').msi"
} else {
    $canonicalMsi
}
$stagingMsi = Join-Path $distDir ".staging-CoolzhuAgent-$Version.msi"
if (Test-Path -LiteralPath $stagingMsi) {
    Remove-Item -LiteralPath $stagingMsi -Force
}

$wixBuildArgs = @(
    'build',
    $productWxs,
    '-arch', 'x64',
    '-d', "Version=$Version",
    '-d', "PackageRoot=$packageRoot",
    '-d', "ApplicationIcon=$applicationIcon",
    '-out', $stagingMsi
)
if ($useLocalDotnetForWix) {
    & $localDotnetExe $wixDll @wixBuildArgs
    $wixExitCode = $LASTEXITCODE
} else {
    & $wixExe @wixBuildArgs
    $wixExitCode = $LASTEXITCODE
}

if ($wixExitCode -ne 0) {
    throw "wix build failed with exit code $wixExitCode"
}

$msiHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $stagingMsi).Hash
Copy-Item -LiteralPath $stagingMsi -Destination $outMsi -Force
$publishedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $outMsi).Hash
if ($publishedHash -ne $msiHash) {
    throw "published MSI hash mismatch: $outMsi"
}
$wixVersion = if ($useLocalDotnetForWix) {
    & $localDotnetExe $wixDll --version | Select-Object -First 1
} else {
    & $wixExe --version | Select-Object -First 1
}
$publishedMsiRelative = ConvertTo-WorkspaceRelativePath $outMsi
$packageSafetyReportRelative = ConvertTo-WorkspaceRelativePath $packageSafetyReport
$installerReport = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    version = $Version
    cli_version = $stagedCliVersion
    configuration = $Configuration
    msi = $publishedMsiRelative
    sha256 = $msiHash
    package_safety_report = $packageSafetyReportRelative
    staged_exported_artifacts = @($stagedExportedArtifacts)
    wix_version = [string]$wixVersion
    signed = $false
    signing_status = 'unsigned'
    # 三个分开的身份 + 发布报告引用：MSI 作为最终产物必须能回溯到构建身份与报告。
    build_identity = [ordered]@{
        schema = [int]$script:BuildIdentitySchema
        source_snapshot_digest = [string]$referencedReport.build_identity.source_snapshot_digest
        build_input_digest = [string]$referencedReport.build_identity.build_input_digest
        payload_digest = [string]$referencedReport.build_identity.payload_digest
        msi_payload_sha256 = $msiHash.ToLowerInvariant()
        note = 'payload_digest 是 MSI 输入（暂存包）的文件清单摘要；msi_payload_sha256 是最终分发文件本身的摘要。两者一起回答"最终生成和分发了哪些二进制与资源"。'
    }
    # PR-PKG-01：引用必须来自**本次包根**（不再有 latest 指针），并带包根身份供一致性核对。
    package_report_ref = [ordered]@{
        report_id = [string]$verifiedReportIdentity.report_id
        content_sha256 = [string]$verifiedReportIdentity.content_sha256
        content_hash_scope = [string]$referencedReport.report_identity.content_hash_scope
        report_file_sha256 = $reportFileHash
        report_path = [string]$reportRefPath
        captured_from = 'package-root-inventory-report-ref'
        package_root_identity = $packageRootIdentity
        verified = $true
    }
    payload_inventory_ref = [ordered]@{
        path = $(if ($inventoryPath.StartsWith($workspacePath)) { $inventoryPath.Substring($workspacePath.Length) } else { $inventoryPath })
        file_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $inventoryPath).Hash.ToLowerInvariant()
        payload_digest = [string]$payloadInventory.payload_digest
        file_count = [int]$payloadInventory.file_count
        total_bytes = [long]$payloadInventory.total_bytes
        package_root_identity = $packageRootIdentity
    }
    vcs = [ordered]@{
        vcs_state = [string]$buildIdentityVcs.vcs_state
        vcs_reference_commit = [string]$buildIdentityVcs.vcs_reference_commit
        vcs_reference_commit_kind = [string]$buildIdentityVcs.vcs_reference_commit_kind
        source_commit = $buildIdentityVcs.source_commit
        source_commit_authority = [string]$buildIdentityVcs.source_commit_authority
        dirty_against_commit = $buildIdentityVcs.dirty_against_commit
        note = 'COOLZHU_GIT_SHA（编译期环境变量）取的就是 HEAD，在当前工作树只是**种子参考提交**，不是当前源码权威；源码权威是 package_report_ref 指向的报告里的 source_snapshot_digest。'
    }
}
$installerReportPath = Join-Path $distDir "CoolzhuAgent-$Version-installer-report.json"
$installerReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $installerReportPath -Encoding UTF8

# 保留规则：被正式发布引用的报告受保护，并连同发布证据归档到
# docs/testing/release-<version>/evidence/build-identity/（tmp 下原报告被清理后仍可核对）。
$retentionResult = & (Join-Path $workspace 'scripts\package-report-retention.ps1') `
    -Action Protect `
    -ReportPath $referencedReportPath `
    -ReleaseArtifact $publishedMsiRelative `
    -ReleaseVersion $Version `
    -Reason ('referenced by released MSI ' + $publishedMsiRelative) `
    -Archive `
    -ExtraEvidencePath $installerReportPath
$retentionResult = @($retentionResult) | Select-Object -Last 1
$installerReport | ConvertTo-Json -Compress
