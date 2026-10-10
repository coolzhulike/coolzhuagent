$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/cargo-build-evidence.ps1')
$root = Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/cargo-stderr-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$path = Join-Path $root 'stderr.log'
$utf8 = [Text.UTF8Encoding]::new($false)
$cases = @(
  @{ text = ''; expected = $false },
  @{ text = '普通编译错误'; expected = $false },
  @{ text = 'error: no matching package named fixture'; expected = $true },
  @{ text = "普通前缀`r`nfailed to download fixture`r`n尾部"; expected = $true },
  @{ text = 'no such file or directory: registry/cache'; expected = $true }
)
foreach ($case in $cases) {
  [IO.File]::WriteAllText($path, $case.text, $utf8)
  if ((Test-CargoDependencyUnavailableLog -Path $path) -ne $case.expected) { throw 'stderr 分类不一致' }
}
# 命中后仍读取尾部，不能把错误尾部隐藏为正常分类结果。
foreach ($invalidBytes in @(
  [byte[]]($utf8.GetBytes("failed to download fixture`n") + [byte[]]@(0xFF)),
  [byte[]]($utf8.GetBytes("failed to download fixture`n" + ('x' * (4 * 1024 * 1024 + 1))))
)) {
  [IO.File]::WriteAllBytes($path, $invalidBytes)
  $rejected = $false
  try { $null = Test-CargoDependencyUnavailableLog -Path $path } catch { $rejected = $true }
  if (-not $rejected) { throw '损坏或超长尾部未拒绝' }
}
# 大日志逐条生成，依赖错误在最后一行；不依靠小夹具推断长输出路径。
$writer = [IO.StreamWriter]::new($path, $false, $utf8, 4096)
try {
  for ($index = 0; $index -lt 12000; $index++) { $writer.WriteLine('普通构建诊断 ' + ('x' * 1024)) }
  $writer.WriteLine('attempting to make an HTTP request but --offline was specified')
} finally { $writer.Dispose() }
if (-not (Test-CargoDependencyUnavailableLog -Path $path)) { throw '大日志末尾依赖错误遗漏' }
$missingRejected = $false
try { $null = Test-CargoDependencyUnavailableLog -Path (Join-Path $root 'missing.log') } catch { $missingRejected = $true }
if (-not $missingRejected) { throw '缺失日志不能当作普通失败分类' }
Write-Output 'PASS stderr 逐条分类：5 个分类、2 个损坏尾部拒绝、大日志末尾命中、缺失日志拒绝。'
