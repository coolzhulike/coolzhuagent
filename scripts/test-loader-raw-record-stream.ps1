$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/webview2-loader.ps1')
$root = Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/loader-raw-stream-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$record = '{"reason":"build-script-executed","package_id":"registry+fixture#webview2-com-sys@0.38.2","out_dir":"fixture/out","linked_paths":["fixture/lib"]}'
$expected = @(Get-LoaderBuildMessageRecords -BuildMessages @($record) -ProducerPackage 'webview2-com-sys') | ConvertTo-Json -Depth 10 -Compress
foreach ($bom in @($false, $true)) {
  $path = Join-Path $root ('valid-' + $bom + '.jsonl')
  [IO.File]::WriteAllText($path, ($record + "`r`n" + '{"reason":"build-finished","success":true}'), [Text.UTF8Encoding]::new($bom))
  $actual = @(Get-LoaderBuildMessageRecords -RawRecordPath $path -ProducerPackage 'webview2-com-sys') | ConvertTo-Json -Depth 10 -Compress
  if ($actual -cne $expected) { throw "原始回执生产者不一致：BOM=$bom" }
  Write-Output "PASS 生产者一致 BOM=$bom"
}
$invalid = Join-Path $root 'invalid-utf8.jsonl'
[IO.File]::WriteAllBytes($invalid, [byte[]]@(0xFF, 0xFE, 0xFF))
$oversized = Join-Path $root 'oversized.jsonl'
[IO.File]::WriteAllText($oversized, ('x' * (4 * 1024 * 1024 + 1)), [Text.UTF8Encoding]::new($false))
foreach ($path in @((Join-Path $root 'missing.jsonl'), $invalid, $oversized)) {
  $rejected = $false
  try {
    # 同时提供有效旧数组，证明原始文件出错时不会静默回退。
    $null = Get-LoaderBuildMessageRecords -RawRecordPath $path -BuildMessages @($record) -ProducerPackage 'webview2-com-sys'
  } catch { $rejected = $true }
  if (-not $rejected) { throw "损坏或缺失回执没有拒绝：$path" }
  Write-Output ('PASS 拒绝且不回退：' + [IO.Path]::GetFileName($path))
}
Write-Output 'Loader 原始回执流式契约：5 项通过；夹具保留于 tmp。'
