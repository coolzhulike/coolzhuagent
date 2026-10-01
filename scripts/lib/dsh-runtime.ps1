# 固定第三方运行资源单独核验，不改变源码/构建输入/载荷三个身份的口径。
function Invoke-DshRuntimeVerification {
  param(
    [Parameter(Mandatory = $true)][string]$Repo,
    [Parameter(Mandatory = $true)][string]$Root
  )
  $python = Get-Command python -CommandType Application -ErrorAction Stop | Select-Object -First 1
  $verifier = Join-Path $Repo 'scripts/prepare-dsh-runtime.py'
  $output = & $python.Source $verifier --verify $Root 2>&1
  if ($LASTEXITCODE -ne 0) {
    throw ('固定DSH运行资源核验失败；先单独准备已锁定资源，不允许回退全局Node/SDK：{0}' -f ($output -join "`n"))
  }
  $result = ($output -join "`n") | ConvertFrom-Json
  if ($result.verified -ne $true -or [int]$result.file_count -le 0 -or [string]$result.lock_sha256 -notmatch '^[0-9a-f]{64}$') {
    throw '固定DSH运行资源未返回有效核验结果'
  }
  return $result
}
