# 控制台静态页面打包契约（D-INSTALLED-UI-VISIBLE-133 修复）。
# release 后端 static_resource_contract.rs 以 include_bytes! 内嵌页面字节执行
# “拒绝混版”预检：包根必须携带与本次构建输入逐字节一致的页面文件，
# 否则 MSI 覆盖安装后旧残留静态文件会让新后端拒载控制台。

function Get-StaticPageContractEntries {
  param([Parameter(Mandatory = $true)][string]$ContractSourcePath)
  $text = [System.IO.File]::ReadAllText($ContractSourcePath)
  return @([regex]::Matches($text, '\(\s*"([^"]+)"\s*,\s*include_bytes!') | ForEach-Object { $_.Groups[1].Value })
}

function Get-StaticPageFileHash {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
      $bytes = $sha.ComputeHash($stream)
      return ([System.BitConverter]::ToString($bytes)).Replace('-', '').ToLowerInvariant()
    } finally { $stream.Dispose() }
  } finally { $sha.Dispose() }
}

# 资源 include 过滤：$RelativePath 为源目录内相对路径（原始分隔符）。
# 排除模式作用于原始路径（与既有安全排除口径一致）；include 模式作用于
# “/”归一化路径。未声明 include 时除安全排除外全部包含。
function Test-PackageResourceIncluded {
  param(
    [Parameter(Mandatory = $true)][string]$RelativePath,
    [object[]]$IncludePatterns,
    [string]$ExcludePattern
  )
  if ($ExcludePattern -and ($RelativePath -match $ExcludePattern)) { return $false }
  if (-not $IncludePatterns -or @($IncludePatterns).Count -eq 0) { return $true }
  $normalized = $RelativePath -replace '\\', '/'
  foreach ($pattern in @($IncludePatterns)) {
    if ($normalized -match [string]$pattern) { return $true }
  }
  return $false
}

# 校验包根中的控制台页面与合约声明的构建输入逐字节一致（失败关闭：
# 契约源缺失/无条目、资源缺失、输入缺失、载荷缺失、字节不一致均记 error）。
function Test-StaticPageContract {
  param(
    [Parameter(Mandatory = $true)][object[]]$Resources,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$PackageRoot
  )
  $errors = [System.Collections.Generic.List[string]]::new()
  $verified = 0
  $resource = @($Resources | Where-Object { [string]$_.id -eq 'gui-web.console-pages' })
  if ($resource.Count -ne 1) {
    $errors.Add('manifest 必须且只能声明一个 gui-web.console-pages 资源')
  } else {
    $sourceRoot = Join-Path $RepoPath ([string]$resource[0].source)
    $targetRoot = Join-Path $PackageRoot ([string]$resource[0].target)
    $contractPath = Join-Path $sourceRoot 'src\static_resource_contract.rs'
    if (-not (Test-Path -LiteralPath $contractPath -PathType Leaf)) {
      $errors.Add("静态资源契约源缺失：$contractPath")
    } else {
      $entries = Get-StaticPageContractEntries -ContractSourcePath $contractPath
      if ($entries.Count -eq 0) {
        $errors.Add('静态资源契约未解析到任何 include_bytes 条目')
      }
      foreach ($entry in @($entries)) {
        $sourceFile = Join-Path $sourceRoot ($entry -replace '/', '\')
        $packageFile = Join-Path $targetRoot ($entry -replace '/', '\')
        $sourceHash = Get-StaticPageFileHash -Path $sourceFile
        $packageHash = Get-StaticPageFileHash -Path $packageFile
        if (-not $sourceHash) { $errors.Add("契约构建输入缺失：$entry"); continue }
        if (-not $packageHash) { $errors.Add("包根缺契约页面：$entry"); continue }
        if ($sourceHash -ne $packageHash) { $errors.Add("页面字节与构建输入不一致：$entry"); continue }
        $verified += 1
      }
    }
  }
  return [ordered]@{ errors = @($errors); verified = $verified }
}
