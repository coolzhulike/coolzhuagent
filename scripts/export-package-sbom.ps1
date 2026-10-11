<#
  从已归档的构建报告和载荷清单导出文件级 CycloneDX 1.6。
  本脚本只投影已有事实，不扫描工作区、不修改发布包、不声称完整库依赖或签名。
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$ReportPath,
  [Parameter(Mandatory = $true)][string]$PayloadInventoryPath,
  [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')

$reportFull = [IO.Path]::GetFullPath($ReportPath)
$inventoryFull = [IO.Path]::GetFullPath($PayloadInventoryPath)
$outputFull = [IO.Path]::GetFullPath($OutputPath)
if ($outputFull -eq $reportFull -or $outputFull -eq $inventoryFull) {
  throw 'SBOM 输出不能覆盖输入报告或载荷清单'
}
if (Test-Path -LiteralPath $outputFull) { throw 'SBOM 输出已存在；请选择新的输出路径' }

# 一次读取冻结字节；文件哈希与解析对象来自同一次读取。
$reportBytes = [IO.File]::ReadAllBytes($reportFull)
$inventoryBytes = [IO.File]::ReadAllBytes($inventoryFull)
$utf8 = [Text.UTF8Encoding]::new($false, $true)
$report = ConvertFrom-IdentityJson -Json ($utf8.GetString($reportBytes).TrimStart([char]0xfeff))
$inventory = ConvertFrom-IdentityJson -Json ($utf8.GetString($inventoryBytes).TrimStart([char]0xfeff))
$reportIdentity = Assert-PackageReportContentHash -Report $report
if ($inventory.schema -ne 1 -or $inventory.kind -ne 'package-payload-inventory') {
  throw '不支持的载荷清单格式'
}
if ($inventory.report_ref.report_id -ne $reportIdentity.report_id -or
    $inventory.report_ref.content_sha256 -ne $reportIdentity.content_sha256) {
  throw '载荷清单没有引用本次验证通过的报告身份'
}
foreach ($key in @('source_snapshot_digest', 'build_input_digest', 'payload_digest')) {
  $value = [string]$report.build_identity.$key
  if ($value -cnotmatch '^[a-f0-9]{64}$' -or $inventory.build_identity.$key -cne $value) {
    throw "报告与清单的构建身份不一致：$key"
  }
}
if ($inventory.self_carrier.path -ne 'payload-inventory.json' -or
    $inventory.self_carrier.excluded_from_payload_digest -ne $true) {
  throw '载荷清单的自引用排除口径不明确'
}

$components = [Collections.Generic.List[object]]::new()
$lines = [Collections.Generic.List[string]]::new()
$paths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
[long]$totalBytes = 0
foreach ($file in @($inventory.files)) {
  $path = [string]$file.path
  if ([string]::IsNullOrWhiteSpace($path) -or $path -match '[\\|\r\n]' -or
      $path.StartsWith('/') -or $path -match '(^|/)\.\.?(/|$)|:|//|/$' -or
      $path -eq $inventory.self_carrier.path -or -not $paths.Add($path)) {
    throw "载荷文件路径无效、重复或违反清单排除口径：$path"
  }
  if (($file.length -isnot [int] -and $file.length -isnot [long]) -or $file.length -lt 0 -or
      ([string]$file.sha256) -cnotmatch '^[a-f0-9]{64}$') {
    throw "载荷文件长度或 SHA256 无效：$path"
  }
  $totalBytes = [long]($totalBytes + [long]$file.length)
  $lines.Add(('file={0}|{1}|{2}' -f $path, $file.length, $file.sha256))
  $components.Add([ordered]@{
    type = 'file'
    'bom-ref' = "payload:file:$path"
    name = $path
    hashes = @([ordered]@{ alg = 'SHA-256'; content = [string]$file.sha256 })
    properties = @([ordered]@{ name = 'coolzhu:payload:length'; value = [string]$file.length })
  })
}
$payloadDigest = Get-IdentityHashFromLines -Lines $lines.ToArray()
if ($components.Count -eq 0 -or $components.Count -ne $inventory.file_count -or
    $components.Count -ne $report.payload_inventory.file_count -or
    $totalBytes -ne $inventory.total_bytes -or $totalBytes -ne $report.payload_inventory.total_bytes -or
    $payloadDigest -cne $inventory.payload_digest -or
    $payloadDigest -cne $report.payload_inventory.payload_digest -or
    $payloadDigest -cne $report.build_identity.payload_digest) {
  throw '载荷文件集合、总长度或规范摘要与报告不一致'
}
$version = [string]$report.build_context.release_version
if ([string]::IsNullOrWhiteSpace($version)) { throw '报告缺少发布版本；不能借用当前源码版本' }
$rootRef = "coolzhuagent:$($version):payload:$payloadDigest"
$properties = [Collections.Generic.List[object]]::new()
foreach ($key in @('source_snapshot_digest', 'build_input_digest', 'payload_digest')) {
  $properties.Add([ordered]@{ name = "coolzhu:build:$key"; value = [string]$report.build_identity.$key })
}
foreach ($pair in @(
    @('coolzhu:report:id', [string]$reportIdentity.report_id),
    @('coolzhu:report:content_sha256', [string]$reportIdentity.content_sha256),
    @('coolzhu:report:file_sha256', (Get-IdentityHashFromBytes -Bytes $reportBytes)),
    @('coolzhu:inventory:file_sha256', (Get-IdentityHashFromBytes -Bytes $inventoryBytes)),
    @('coolzhu:build:source_commit', [string]$report.build_context.source_commit),
    @('coolzhu:build:target', [string]$report.build_context.build_target),
    @('coolzhu:scope', '文件级载荷；排除自引用载体 payload-inventory.json；不包含完整库依赖、许可证、漏洞或签名信息'),
    @('coolzhu:identity:assurance', '自校验及报告关联一致；不证明来源签名、作者或跨机器可重现构建')
)) {
  $properties.Add([ordered]@{ name = $pair[0]; value = $pair[1] })
}

# 文件是应用的组成部分，不把它们误写为已解析的库依赖图。
$bom = [ordered]@{
  '$schema' = 'http://cyclonedx.org/schema/bom-1.6.schema.json'
  bomFormat = 'CycloneDX'
  specVersion = '1.6'
  serialNumber = 'urn:uuid:' + [Guid]::NewGuid().ToString()
  version = 1
  metadata = [ordered]@{
    timestamp = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    component = [ordered]@{
      type = 'application'
      'bom-ref' = $rootRef
      name = 'coolzhuagent'
      version = $version
      components = $components.ToArray()
    }
    properties = $properties.ToArray()
  }
  compositions = @([ordered]@{ aggregate = 'incomplete'; assemblies = @($rootRef) })
}

# 所有校验完成后才落盘；不覆盖既有成果，失败时不遗留半个输出文件。
$outputDirectory = [IO.Path]::GetDirectoryName($outputFull)
[IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$temporary = Join-Path $outputDirectory ('.sbom-' + [Guid]::NewGuid().ToString() + '.tmp')
try {
  [IO.File]::WriteAllText($temporary, (($bom | ConvertTo-Json -Depth 30) + "`n"), [Text.UTF8Encoding]::new($false))
  [IO.File]::Move($temporary, $outputFull)
} finally {
  if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
}
[pscustomobject]@{
  output_path = $outputFull
  sbom_sha256 = Get-IdentityFileHash -Path $outputFull
  report_id = $reportIdentity.report_id
  file_count = $components.Count
  total_bytes = $totalBytes
  payload_digest = $payloadDigest
  completeness = 'incomplete'
} | ConvertTo-Json
