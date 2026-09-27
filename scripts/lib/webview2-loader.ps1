# ============================================================================
# WebView2Loader 确定性产物链（PKG-01 / PKG-02）
#
# 设计约束（来自第四轮裁决，本文件是唯一实现点）：
#   1. 任何选源都必须有**可核对的来源记录**：本次 cargo 构建消息中的
#      `build-script-executed`（含 package_id 与 out_dir），或显式的稳定导出收据。
#      本文件**不提供**任何"目录扫描 / 按时间挑选"的兜底分支。
#   2. `Test-Path` 不是有效性检查。声明源存在也必须验证来源与内容身份。
#   3. 正式打包只消费稳定导出路径，不直接消费 cargo 的哈希目录
#      `webview2-com-sys-*`（该目录名随 features/profile/版本变化）。
#   4. 失败一律 fail-closed，错误文本必须含 artifact id、目标、候选来源、失败类别。
#   5. 同一个**实际导出目标**只允许一个发布者（PKG-L07c 裁决）。锁范围按实际共享
#      输出槽位（规范目标根 + 目标文件 + package target + profile）确定，不能只按
#      build id 加锁：两个不同 build id 仍会覆盖同一个文件。争用者可返回明确 Busy，
#      且不修改赢家的锁 / 暂存 / 已发布结果。
#   6. 产物与收据是**连续替换两个文件**，不是跨文件原子提交；收据是"本代次完成"的
#      唯一标记且内嵌产物内容身份，进程在两者之间退出时消费端必须拒绝该不完整代次。
#   7. 发布权必须覆盖**完整发布临界区**（PKG-L07c P0 补强，裁决 §3.2）：产物最终核对 +
#      收据生成与提交 + **当前有效代次的发布**（槽位代次指针）全都在同一把排他权内完成，
#      且在每一次共享槽位写入之前重新自检发布权。失败竞争者的 catch/finally 只写自己的
#      诊断，绝不向共享有效槽位发布自己的收据。
#   8. 代次是**不可变的发布标识**（不是 mtime）：每次成功发布产生一个新 generation，
#      其收据另在槽位代次档案里留一份不可变副本；"当前有效代次"由槽位指针唯一指定。
#      因此消费端消费过 G1 之后槽位合法更新到 G2，G1 的报告仍然可核对（依据是保存下来的
#      G1 收据 + 本次独立 staging 的字节），不要求指针停在 G1。
#   9. 消费端固定流程：取得读取资格 → 读取**明确 generation**（来自指针）→ 核对收据与产物
#      → 复制到本次打包的**独立 staging** → 核对复制内容 → 固定本次消费收据 → 释放读取权；
#      报告**只**从这份固定收据生成，不得在报告生成时重新读取共享槽位的"最新收据"。
# ============================================================================

# 收据 schema 3：在 schema 2 的 publication 块上补齐固定代次语义
# （slot_key / generation / producer_run_id / artifact_digest / receipt_digest /
#   build_input_digest）并把"当前有效代次"从"收据文件自身"改为**显式槽位指针** +
# 不可变代次档案。schema 2 是单发布者协议第一版（无代次档案、无指针），
# schema 1 更早；两者都必须重新构建导出，不能继续作为发布凭据。
$script:LoaderReceiptSchema = 3
# 槽位内的持久结构：代次档案目录（每个成功代次一份不可变收据）+ 当前有效代次指针。
$script:LoaderGenerationDirPrefix = '.loader-generations-'
$script:LoaderCurrentPointerPrefix = '.loader-current-'
$script:LoaderGenerationRecordSchema = 1
$script:LoaderSlotPointerSchema = 1

function New-LoaderFailureText {
  param(
    [Parameter(Mandatory = $true)][string]$Category,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)',
    [string[]]$Candidates = @(),
    [Parameter(Mandatory = $true)][string]$Detail,
    [string]$Remediation
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("[$Category] artifact=$ArtifactId target=$Target profile=$Profile")
  $lines.Add("detail: $Detail")
  if (@($Candidates).Count -gt 0) {
    $lines.Add('candidates:')
    foreach ($candidate in @($Candidates)) {
      $lines.Add("  - $candidate")
    }
  } else {
    $lines.Add('candidates: (none)')
  }
  if ($Remediation) {
    $lines.Add("next: $Remediation")
  }
  return ($lines -join "`n")
}

function New-LoaderFailureException {
  param(
    [Parameter(Mandatory = $true)][string]$Category,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)',
    [string[]]$Candidates = @(),
    [Parameter(Mandatory = $true)][string]$Detail,
    [string]$Remediation
  )

  $text = New-LoaderFailureText `
    -Category $Category `
    -ArtifactId $ArtifactId `
    -Target $Target `
    -Profile $Profile `
    -Candidates $Candidates `
    -Detail $Detail `
    -Remediation $Remediation
  return [System.InvalidOperationException]::new($text)
}

function Get-LoaderFileIdentity {
  param([string]$Path)

  # 身份读取必须"要么给出真实身份、要么给 $null"：Test-Path 与 Get-Item 之间有窗口，
  # 让它抛 "Cannot find path ... does not exist" 会把并发症状伪装成无关错误。
  try {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if (-not $item) { return $null }
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName -ErrorAction Stop).Hash
    return [pscustomobject]@{
      path = $item.FullName
      length = [long]$item.Length
      sha256 = $hash.ToLowerInvariant()
    }
  } catch {
    return $null
  }
}

<#
  规范内容摘要（本库自用的最小实现，不依赖 build-identity.ps1 的同类函数：
  本文件可以被独立 dot-source）。

  口径：把任意 JSON 结构展开成"path=value"叶子行（字符串/布尔/数字/空值），
  按 Ordinal 排序后取 SHA256。排除路径（如 publication.receipt_digest）在展开时跳过，
  这样"收据自身的内容摘要"不会自引用。
#>
function Get-LoaderDigestLeafLines {
  param(
    [AllowNull()]$Value,
    [string]$Prefix = '',
    [AllowEmptyCollection()][string[]]$ExcludePaths = @()
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  if ($ExcludePaths -contains $Prefix -and $Prefix) { return $lines }
  if ($null -eq $Value) {
    $lines.Add("$Prefix=<null>")
    return $lines
  }
  if ($Value -is [string]) {
    $lines.Add("$Prefix=$Value")
    return $lines
  }
  if ($Value -is [bool]) {
    $lines.Add(("$Prefix=" + $(if ($Value) { 'true' } else { 'false' })))
    return $lines
  }
  if ($Value -is [int] -or $Value -is [long] -or $Value -is [int64] -or $Value -is [int32] -or $Value -is [double] -or $Value -is [decimal]) {
    $lines.Add(("$Prefix=" + ([string]$Value)))
    return $lines
  }
  if ($Value -is [System.Collections.IDictionary]) {
    foreach ($key in @($Value.Keys)) {
      $childPrefix = $(if ($Prefix) { "$Prefix.$key" } else { [string]$key })
      foreach ($line in @(Get-LoaderDigestLeafLines -Value $Value[$key] -Prefix $childPrefix -ExcludePaths $ExcludePaths)) {
        $lines.Add($line)
      }
    }
    return $lines
  }
  if ($Value -is [System.Collections.IEnumerable]) {
    $index = 0
    foreach ($item in $Value) {
      $childPrefix = "$Prefix[$index]"
      foreach ($line in @(Get-LoaderDigestLeafLines -Value $item -Prefix $childPrefix -ExcludePaths $ExcludePaths)) {
        $lines.Add($line)
      }
      $index++
    }
    return $lines
  }
  # PSObject（ConvertFrom-Json 的产物）与其它对象：按属性名展开。
  foreach ($property in @($Value.PSObject.Properties)) {
    if ($property.Name -in @('PSPath', 'PSParentPath', 'PSChildName', 'PSDrive', 'PSProvider', 'PSIsContainer')) { continue }
    $childPrefix = $(if ($Prefix) { "$Prefix.$($property.Name)" } else { [string]$property.Name })
    foreach ($line in @(Get-LoaderDigestLeafLines -Value $property.Value -Prefix $childPrefix -ExcludePaths $ExcludePaths)) {
      $lines.Add($line)
    }
  }
  return $lines
}

function Get-LoaderCanonicalDigest {
  param(
    [AllowNull()]$Value,
    [AllowEmptyCollection()][string[]]$ExcludePaths = @()
  )

  $lines = @(Get-LoaderDigestLeafLines -Value $Value -ExcludePaths $ExcludePaths)
  $array = @($lines)
  if ($array.Count -gt 1) {
    [System.Array]::Sort($array, [System.StringComparer]::Ordinal)
  }
  $joined = ($array -join "`n")
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($joined)))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

# 收据自身的内容摘要：口径固定为"除 publication.receipt_digest 之外的叶子行"，
# 因此任何读到该收据的一方（写侧、消费侧、事后复核）都能重算出同一个值。
function Get-LoaderReceiptDigest {
  param([Parameter(Mandatory = $true)]$Receipt)

  return (Get-LoaderCanonicalDigest -Value $Receipt -ExcludePaths @('publication.receipt_digest'))
}

<#
  构建输入身份的摘要：回答"这一代产物是由哪一组构建输入身份产出的"。
  它只登记**身份摘要**（不复制文件内容），因此可以随收据外发而不泄漏源码。
#>
function Get-LoaderBuildInputDigest {
  param(
    [string]$ArtifactContractSha256,
    [string]$SourceIdentitySha256,
    [int]$SourceIdentityFileCount = 0,
    [string]$LockfileSha256,
    [string]$BuildEntryManifestSha256,
    [string]$BuildTarget,
    [string]$CargoTargetDir,
    [string]$Profile,
    [string]$ReleaseVersion,
    [string]$ProducerPackage
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("artifact_contract_sha256=$ArtifactContractSha256")
  $lines.Add("source_identity_sha256=$SourceIdentitySha256")
  $lines.Add("source_identity_file_count=$SourceIdentityFileCount")
  $lines.Add("lockfile_sha256=$LockfileSha256")
  $lines.Add("build_entry_manifest_sha256=$BuildEntryManifestSha256")
  $lines.Add("build_target=$BuildTarget")
  $lines.Add("cargo_target_dir=$CargoTargetDir")
  $lines.Add("profile=$Profile")
  $lines.Add("release_version=$ReleaseVersion")
  $lines.Add("producer_package=$ProducerPackage")
  return (Get-LoaderCanonicalDigest -Value @($lines.ToArray()))
}

function Get-LoaderPeMachine {
  param([string]$Path)

  $stream = $null
  try {
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    if ($stream.Length -lt 0x40) { return 'TRUNCATED' }
    $reader = [System.IO.BinaryReader]::new($stream)
    $stream.Position = 0
    if ($reader.ReadUInt16() -ne 0x5A4D) { return 'NOT-PE' }
    $stream.Position = 0x3C
    $peOffset = $reader.ReadInt32()
    if ($peOffset -le 0 -or ($peOffset + 6) -gt $stream.Length) { return 'TRUNCATED' }
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550) { return 'NOT-PE' }
    $machine = $reader.ReadUInt16()
    switch ($machine) {
      0x8664 { return 'AMD64' }
      0x014C { return 'I386' }
      0xAA64 { return 'ARM64' }
      default { return ('UNKNOWN-0x{0:X4}' -f $machine) }
    }
  } catch {
    return 'UNREADABLE'
  } finally {
    if ($stream) { $stream.Dispose() }
  }
}

function Get-LoaderArchitectureMachine {
  param([string]$Architecture)

  switch ($Architecture) {
    'x64' { return 'AMD64' }
    'x86' { return 'I386' }
    'arm64' { return 'ARM64' }
    default { return $Architecture }
  }
}

<#
  解析真实路径：逐段展开 Windows 重解析点（junction / symlink）。

  必要性：本仓库历史共享 target 目录存在 junction 记录
  （tmp/2026-09-19-agent-fixes/pr-checkout/modules/gui-desktop/target -> modules/gui-desktop/target）。
  cargo 记录下来的 out_dir 可能是另一个拼写（走 junction 的路径），
  只用字符串前缀比较会得到错误的"越界"或错误的"在界内"。这里按实际允许根校验。
#>
function Resolve-LoaderRealPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $full = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  $root = [System.IO.Path]::GetPathRoot($full)
  if (-not $root) { return $full }

  $current = $root.TrimEnd('\', '/')
  if (-not $current) { $current = $root }
  $segments = $full.Substring($root.Length).Split([char]'\', [char]'/')
  foreach ($segment in $segments) {
    if (-not $segment) { continue }
    $current = Join-Path $current $segment
    if (-not (Test-Path -LiteralPath $current)) { continue }
    $item = Get-Item -LiteralPath $current -Force
    if (-not ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) { continue }
    $target = $item.Target
    if ($target -is [array]) { $target = $target[0] }
    if (-not $target) { continue }
    if (-not [System.IO.Path]::IsPathRooted($target)) {
      $target = Join-Path (Split-Path -Parent $current) $target
    }
    $current = [System.IO.Path]::GetFullPath([string]$target).TrimEnd('\', '/')
  }
  return $current
}

function Test-LoaderPathInside {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Root
  )

  $candidate = $Path.TrimEnd('\', '/')
  $allowed = $Root.TrimEnd('\', '/')
  if ($candidate.Equals($allowed, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
  return (
    $candidate.StartsWith($allowed + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
    $candidate.StartsWith($allowed + '/', [System.StringComparison]::OrdinalIgnoreCase)
  )
}

# ============================================================================
# 单一发布者协议（PKG-L07c）
#
# 为什么需要：本仓库实测（tmp/l07c-repro，两个真实进程竞争同一导出目标）
#   * 目标不存在时两侧同时走 File.Move ⇒ ERROR_ALREADY_EXISTS
#     （"Cannot create a file when that file already exists."）；
#   * 目标已存在时并发 File.Replace ⇒ ERROR_UNABLE_TO_REMOVE_REPLACED /
#     ERROR_UNABLE_TO_MOVE_REPLACEMENT / ERROR_SHARING_VIOLATION；
#   * 失败方还可能观察到目标"暂时不存在"（进程之间没有任何互斥）。
#   因此并发发布既不是可靠成功，也不能只靠"事后核对"来当发布机制。
#
# 协议：取得目标发布权 → 在本次运行独立暂存区生成与核对 → 发布匹配的产物与收据
#       → 校验完成（两个文件互相自洽）→ 释放发布权。
#
# 明确不声称的事情：产物与收据是**连续替换两个文件**，不是跨文件原子提交。
# 唯一可依赖的不变量是：收据内嵌产物内容身份，因而任何"产物已换、收据未更新"
# 或"收据已换、产物不符"的状态都会被消费端内容核对拒绝。
# ============================================================================

$script:LoaderPublishLockPrefix = '.loader-publish-'
$script:LoaderStagePrefix = '.loader-stage-'
$script:LoaderSlotWaitSecondsDefault = 300
# "持有者 pid 还活着"时仍允许回收的年龄阈值（分钟）。发布窗口是秒级操作，
# 超过该阈值只能来自释放失败/异常暂停，不回收会把打包卡到超时。
$script:LoaderStaleLockMinutes = 10
if ($env:COOLZHU_LOADER_STALE_LOCK_MINUTES) {
  try { $script:LoaderStaleLockMinutes = [int]$env:COOLZHU_LOADER_STALE_LOCK_MINUTES } catch { $script:LoaderStaleLockMinutes = 10 }
}
# 同进程内已持有的发布权（key -> lock），避免自嵌套自锁（自死锁）。
$script:LoaderHeldSlots = @{}

<#
  计算"实际共享输出槽位"。key 只由**共享输出槽位**决定：
  规范槽位目录（含 profile 路径）+ 目标文件名 + package target + profile。

  故意**不含** build id / 运行 ID / 产物哈希：不同 build id 可能覆盖同一个文件，
  按 build id 加锁等于没加锁（裁决明确要求）。也不含仓库根：槽位目录是绝对路径，
  已经唯一确定这个共享输出位置。
#>
function Get-LoaderPublishSlot {
  param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [string]$RepoPath,
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)'
  )

  $destinationFull = [System.IO.Path]::GetFullPath($Destination)
  $slotDir = [System.IO.Path]::GetDirectoryName($destinationFull)
  if (-not $slotDir) {
    throw [System.InvalidOperationException]::new("destination has no parent directory: $Destination")
  }
  # 消费端也要能建立互斥边界（否则无法保证读一致）；槽位目录是派生输出目录。
  New-Item -ItemType Directory -Force -Path $slotDir | Out-Null
  $slotDirResolved = Resolve-LoaderRealPath $slotDir
  $fileName = [System.IO.Path]::GetFileName($destinationFull)
  $packageTarget = ([string]$Target).ToLowerInvariant()
  $profileName = ([string]$Profile).ToLowerInvariant()
  $identity = @(
    "slot_dir=$($slotDirResolved.ToLowerInvariant())",
    "file=$($fileName.ToLowerInvariant())",
    "package_target=$packageTarget",
    "profile=$profileName"
  ) -join '|'
  $repoResolved = $null
  if ($RepoPath) { $repoResolved = Resolve-LoaderRealPath $RepoPath }

  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $key = ([System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($identity)))).Replace('-', '').ToLowerInvariant().Substring(0, 16)
  } finally {
    $sha.Dispose()
  }

  return [pscustomobject]@{
    key = $key
    identity = $identity
    slot_dir = $slotDirResolved
    destination = $destinationFull
    destination_file_name = $fileName
    package_target = [string]$Target
    profile = [string]$Profile
    repo = $repoResolved
    lock_path = (Join-Path $slotDirResolved ($script:LoaderPublishLockPrefix + $key + '.lock'))
    # 代次档案与"当前有效代次"指针：同一槽位身份内的持久结构（目录惰性创建）。
    generation_dir = (Join-Path $slotDirResolved ($script:LoaderGenerationDirPrefix + $key))
    pointer_path = (Join-Path $slotDirResolved ($script:LoaderCurrentPointerPrefix + $key + '.json'))
  }
}

<#
  解析槽位内的代次档案路径（generation -> 该代次的不可变收据副本）。
  代次 ID 由本库生成（32 位十六进制），这里仍做一次字符白名单校验，
  避免任何外部字符串把路径带出槽位目录。
#>
function Resolve-LoaderGenerationRecordPath {
  param(
    [Parameter(Mandatory = $true)][object]$Slot,
    [Parameter(Mandatory = $true)][string]$Generation
  )

  if ($Generation -notmatch '^[0-9a-f]{8,64}$') {
    throw [System.InvalidOperationException]::new("unexpected generation id rejected: '$Generation'")
  }
  return (Join-Path $Slot.generation_dir ($Generation + '.json'))
}

<#
  故障注入 / 受控同步接缝（**仅供契约测试**，默认完全惰性）。

  必要性（裁决第 6 条）：并发时序必须用屏障或可控同步触发，不能靠 sleep 碰运气。
  这里在发布路径的固定点提供接缝：
    COOLZHU_LOADER_TEST_SEAM=<point>     触发点，未设置时本函数立即返回
    COOLZHU_LOADER_TEST_SEAM_ACTION      park（默认，屏障等待）| throw | exit
    COOLZHU_LOADER_TEST_READY_EVENT      本进程就绪事件名（park 用）
    COOLZHU_LOADER_TEST_GO_EVENT         放行事件名（park 用）
    COOLZHU_LOADER_TEST_TIMEOUT_SECONDS  屏障等待上限（默认 120）

  触发点：before-lock / after-lock / before-file-publish / between-file-and-receipt
          / before-generation-commit / between-generation-and-pointer / after-receipt
  生产运行不得设置这些变量；一旦触发会 Write-Warning 并出现在构建日志里。
#>
function Invoke-LoaderTestSeam {
  param(
    [Parameter(Mandatory = $true)][string]$Point,
    [string]$Detail = ''
  )

  $armedPoint = [string]$env:COOLZHU_LOADER_TEST_SEAM
  if (-not $armedPoint) { return }
  if ($armedPoint -ne $Point) { return }
  $action = [string]$env:COOLZHU_LOADER_TEST_SEAM_ACTION
  if (-not $action) { $action = 'park' }
  Write-Warning ("[test-seam] point=$Point action=$action pid=$PID detail=$Detail")
  switch ($action) {
    'park' {
      $readyName = [string]$env:COOLZHU_LOADER_TEST_READY_EVENT
      $goName = [string]$env:COOLZHU_LOADER_TEST_GO_EVENT
      if (-not $readyName -or -not $goName) {
        throw [System.InvalidOperationException]::new('[TEST-SEAM] park requires COOLZHU_LOADER_TEST_READY_EVENT and COOLZHU_LOADER_TEST_GO_EVENT')
      }
      $timeoutSeconds = 120
      if ($env:COOLZHU_LOADER_TEST_TIMEOUT_SECONDS) { $timeoutSeconds = [int]$env:COOLZHU_LOADER_TEST_TIMEOUT_SECONDS }
      $ready = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $readyName)
      $go = [System.Threading.EventWaitHandle]::OpenExisting($goName)
      try {
        # 注意：接缝必须**不产生任何管道输出**，否则会把调用方的返回值污染成数组。
        [void]$ready.Set()
        if (-not $go.WaitOne($timeoutSeconds * 1000)) {
          throw [System.InvalidOperationException]::new("[TEST-SEAM] barrier timeout at $Point")
        }
      } finally {
        $ready.Dispose()
        $go.Dispose()
      }
    }
    'throw' {
      throw [System.InvalidOperationException]::new("[TEST-SEAM-ABORT] injected failure at $Point ($Detail)")
    }
    'exit' {
      # 模拟"进程在两次文件替换之间退出"：不展开 finally、不写收据、不释放锁文件，
      # 只留 OS 关闭句柄。后续发布者必须绕过 pid 存活判定的老旧锁。
      [System.Environment]::Exit(97)
    }
    default {
      throw [System.InvalidOperationException]::new("[TEST-SEAM] unknown action: $action")
    }
  }
}

function Remove-LoaderFileIfExists {
  param([Parameter(Mandatory = $true)][string]$Path)

  try {
    if ([System.IO.File]::Exists($Path)) { [System.IO.File]::Delete($Path) }
  } catch {
    # 清理失败不影响判定：留着的是可被后续 pid 存活判定回收的残片。
  }
}

<#
  释放发布权时删除锁文件：必须重试到成功。

  为什么不能"删不掉就算了"：竞争者此刻可能正以只读方式打开锁文件（共享冲突会让
  Delete 失败几百微秒）。如果就这样留下一个"持有者 pid 还活着"的锁文件，后续发布者
  会一直等到超时（默认 300 秒）才拿到明确 Busy —— fail-closed 但会把打包卡死几分钟。
#>
function Release-LoaderPublishLock {
  param(
    [Parameter(Mandatory = $true)][object]$Lock,
    [int]$Attempts = 40
  )

  for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
    try {
      if (-not [System.IO.File]::Exists($Lock.lock_path)) { return $true }
      [System.IO.File]::Delete($Lock.lock_path)
      return $true
    } catch {
      Start-Sleep -Milliseconds 25
    }
  }
  Write-Warning ("无法删除自己的发布权锁文件（竞争者可能持续只读持有，或权限异常）：{0}；后续发布者会等到超时并返回 EXPORT-SLOT-BUSY（不会误发布）" -f $Lock.lock_path)
  return $false
}

function Read-LoaderPublishHolder {
  param([Parameter(Mandatory = $true)][string]$LockPath)

  $stream = $null
  try {
    if (-not [System.IO.File]::Exists($LockPath)) { return $null }
    # 持有者用 FileShare::Read 持有锁文件（自己可读，别人不可写/改名/删除）。
    # Windows 共享规则要求**读取方的共享模式必须包含持有者的访问权限**，
    # 因此这里必须显式用 FileShare::ReadWrite 打开；用 File.ReadAllText
    # （共享模式 Read）会被直接挡下——那正是"锁生效"的表现，不是记录不可读。
    $stream = [System.IO.File]::Open($LockPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::UTF8)
    try {
      $raw = $reader.ReadToEnd()
    } finally {
      $reader.Dispose()
    }
    if (-not $raw) { return $null }
    return ($raw | ConvertFrom-Json)
  } catch {
    return $null
  } finally {
    if ($stream) { $stream.Dispose() }
  }
}

function New-LoaderPublishBusyException {
  param(
    [Parameter(Mandatory = $true)][object]$Slot,
    [Parameter(Mandatory = $true)][string]$Purpose,
    [string]$ArtifactId = '(unknown)',
    [string]$Target = '(unknown)',
    [string]$ProfileName = '',
    [object]$Holder = $null,
    [int]$WaitSeconds = 0
  )

  $holderText = 'holder record unreadable (lock file is held open by another process)'
  if ($Holder) {
    $holderText = ("pid={0} host={1} purpose={2} artifact={3} acquired={4} token={5}" -f $Holder.owner_pid, $Holder.owner_host, $Holder.purpose, $Holder.artifact_id, $Holder.acquired_utc, $Holder.owner_token)
  }
  $candidates = @(
    "slot_key=$($Slot.key)",
    "slot_dir=$($Slot.slot_dir)",
    "lock_file=$($Slot.lock_path)",
    $holderText
  )
  return (New-LoaderFailureException `
    -Category 'EXPORT-SLOT-BUSY' `
    -ArtifactId $ArtifactId `
    -Target $Target `
    -Profile ([string]$ProfileName) `
    -Candidates $candidates `
    -Detail ("busy=true retryable=true: 该导出槽位已有发布者（purpose=$Purpose 等待 $WaitSeconds 秒仍未取得发布权）") `
    -Remediation "同一实际导出目标只允许一个发布者：等待其完成（该窗口只覆盖产物与收据的连续替换，秒级）后重试；或为本次运行指定独立的输出目录（--target-dir / 独立 profile）。不要删除他人的锁文件、不要改动他人正在发布的产物")
}

<#
  取得槽位发布权。返回的 lock 对象持有独占句柄：在自己释放前，别的进程既不能删除
  也不能改名这个锁文件（跨会话有效）。

  争用判定不看"锁文件是否存在"就下结论，而是核对持有者记录：pid 不存在或进程启动
  时刻不匹配（pid 复用）才允许回收；否则等待到超时并抛出明确 Busy。
#>
function Enter-LoaderPublishSlot {
  param(
    [Parameter(Mandatory = $true)][object]$Slot,
    [string]$Purpose = 'export',
    [int]$WaitSeconds = -1,
    [string]$ArtifactId = '(unknown)',
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)'
  )

  # 同进程重入：返回已持有的发布权（引用计数），不重复取锁、不自死锁。
  if ($script:LoaderHeldSlots.ContainsKey($Slot.key)) {
    $held = $script:LoaderHeldSlots[$Slot.key]
    $held.refcount = $held.refcount + 1
    return $held
  }

  $wait = $script:LoaderSlotWaitSecondsDefault
  if ($env:COOLZHU_LOADER_SLOT_WAIT_SECONDS) {
    try { $wait = [int]$env:COOLZHU_LOADER_SLOT_WAIT_SECONDS } catch { $wait = $script:LoaderSlotWaitSecondsDefault }
  }
  if ($WaitSeconds -ge 0) { $wait = $WaitSeconds }

  Invoke-LoaderTestSeam -Point 'before-lock' -Detail "slot=$($Slot.key) purpose=$Purpose"

  $deadline = (Get-Date).AddSeconds($wait)
  $ownerToken = [guid]::NewGuid().ToString('N')
  $hostId = 'unknown-host'
  try { $hostId = [System.Net.Dns]::GetHostName() } catch { $hostId = 'unknown-host' }
  if ($env:COOLZHU_LOADER_SLOT_HOST_ID) { $hostId = [string]$env:COOLZHU_LOADER_SLOT_HOST_ID }
  $processStartedUtc = $null
  try { $processStartedUtc = (Get-Process -Id $PID).StartTime.ToUniversalTime().ToString('o') } catch { $processStartedUtc = $null }

  while ($true) {
    $stream = $null
    try {
      # CreateNew：已存在就失败（不覆盖任何人的锁文件）。FileShare::Read 允许
      # 竞争者只读地看到持有者记录，但删除/改名需要 Delete 权限，会被共享模式挡住。
      $stream = [System.IO.File]::Open($Slot.lock_path, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::Read)
      $record = [ordered]@{
        protocol = 'webview2-loader-single-publisher'
        protocol_version = 1
        slot_key = $Slot.key
        slot_identity = $Slot.identity
        slot_dir = $Slot.slot_dir
        destination = $Slot.destination
        package_target = $Slot.package_target
        profile = $Slot.profile
        purpose = $Purpose
        artifact_id = $ArtifactId
        owner_pid = $PID
        owner_token = $ownerToken
        owner_host = $hostId
        owner_process_started_utc = $processStartedUtc
        acquired_utc = (Get-Date).ToUniversalTime().ToString('o')
      }
      $payload = [System.Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Depth 6))
      $stream.Write($payload, 0, $payload.Length)
      $stream.Flush()

      $lock = [pscustomobject]@{
        slot = $Slot
        key = $Slot.key
        purpose = $Purpose
        owner_token = $ownerToken
        lock_path = $Slot.lock_path
        record = $record
        refcount = 1
        stream = $stream
        reclaimed_stale = $false
      }
      $script:LoaderHeldSlots[$Slot.key] = $lock
      Write-Host ("publish right acquired: slot={0} purpose={1} lock={2}" -f $Slot.key, $Purpose, $Slot.lock_path)
      Invoke-LoaderTestSeam -Point 'after-lock' -Detail "slot=$($Slot.key) purpose=$Purpose"
      return $lock
    } catch [System.IO.IOException] {
      if ($stream) { $stream.Dispose(); $stream = $null }
      if ($_.Exception -is [System.IO.DirectoryNotFoundException]) { throw }
      $holder = Read-LoaderPublishHolder -LockPath $Slot.lock_path
      $stale = $false
      $staleReason = ''
      if (-not $holder) {
        # 记录读不出来有两种原因：① 持有者被杀留下的空/半写文件；② 持有者刚好在
        # CreateNew 之后写记录（微秒级窗口）。只有"文件也明显久置"才判定为死锁，
        # 否则一律按"有人持有"等待——宁可 Busy，也不误删活锁。
        $lockItem = Get-Item -LiteralPath $Slot.lock_path -Force -ErrorAction SilentlyContinue
        $lockAgeSeconds = 0
        if ($lockItem) { $lockAgeSeconds = ((Get-Date).ToUniversalTime() - $lockItem.LastWriteTimeUtc).TotalSeconds }
        if ($lockAgeSeconds -gt 5) {
          $stale = $true
          $staleReason = ("lock record unreadable and lock file is {0:N1}s old" -f $lockAgeSeconds)
        }
      } else {
        $pidAlive = $false
        $processStartMatches = $null
        try {
          $ownerProcess = Get-Process -Id ([int]$holder.owner_pid) -ErrorAction Stop
          $pidAlive = $true
          try {
            $processStartMatches = ($ownerProcess.StartTime.ToUniversalTime().ToString('o') -eq [string]$holder.owner_process_started_utc)
          } catch {
            $processStartMatches = $null
          }
        } catch {
          $pidAlive = $false
        }
        if (-not $pidAlive) {
          $stale = $true
          $staleReason = "owner pid $($holder.owner_pid) 不存在（进程已退出或被杀）"
        } elseif ($processStartMatches -eq $false) {
          $stale = $true
          $staleReason = "owner pid $($holder.owner_pid) 已被复用（进程启动时刻与记录不匹配）"
        } else {
          # 兜底自愈：发布窗口只覆盖"采样身份 + 复制产物 + 写收据 + 完成校验"，
          # 是秒级操作。一个"持有者还活着但已持有远超阈值"的锁只可能来自释放失败
          # （release 时被只读句柄挡住）或异常暂停；不回收会让打包长期被卡到超时。
          # 回收是安全的：老持有者在写产物前会做发布权自检（PUBLISH-RIGHT-LOST），
          # 因此不会出现两个发布者同时写同一个槽位。
          $acquiredUtc = $null
          try { $acquiredUtc = [datetime]::Parse([string]$holder.acquired_utc) } catch { $acquiredUtc = $null }
          if ($acquiredUtc) {
            $ageMinutes = ((Get-Date).ToUniversalTime() - $acquiredUtc.ToUniversalTime()).TotalMinutes
            if ($ageMinutes -gt $script:LoaderStaleLockMinutes) {
              $stale = $true
              $staleReason = ("lock 已持有 {0:N1} 分钟（阈值 {1} 分钟：发布窗口是秒级，超过即不可能是活的发布窗口）" -f $ageMinutes, $script:LoaderStaleLockMinutes)
            }
          }
        }
      }

      if ($stale) {
        # 只有"持有者确实已死"时文件才可删（持有者句柄随进程消失）。删除是幂等的，
        # 谁先 CreateNew 成功谁成为新持有者，因此不存在"两个发布者都以为自己赢了"。
        # 回收失败（例如判定有误、句柄仍在）不得变成无限循环：仍要受 deadline 约束。
        Write-Warning ("reclaim stale publish lock: slot={0} reason={1}; 持有者已死，锁不构成互斥" -f $Slot.key, $staleReason)
        Remove-LoaderFileIfExists -Path $Slot.lock_path
      }

      # 超时判定放在"尝试过并失败"之后：WaitSeconds=0 表示立即返回明确 Busy，
      # 但绝不跳过第一次尝试（首次尝试成功即取得发布权）。
      if ((Get-Date) -ge $deadline) {
        throw (New-LoaderPublishBusyException -Slot $Slot -Purpose $Purpose -ArtifactId $ArtifactId -Target $Target -ProfileName $Profile -Holder $holder -WaitSeconds $wait)
      }
      # 等待是互斥本身的语义（不是并发触发手段）：抢锁由测试的屏障同步触发。
      Start-Sleep -Milliseconds 25
    } catch {
      if ($stream) { $stream.Dispose() }
      throw
    }
  }
}

<#
  发布前的发布权自检：锁文件可能被第三方删除或夺走（例如人工清理 target）。
  这种情况下**必须中止发布**，而不是继续写产物。
#>
function Assert-LoaderPublishRight {
  param([Parameter(Mandatory = $true)][object]$Lock)

  $holder = Read-LoaderPublishHolder -LockPath $Lock.lock_path
  if (-not $holder -or [string]$holder.owner_token -ne [string]$Lock.owner_token) {
    throw (New-LoaderFailureException `
      -Category 'PUBLISH-RIGHT-LOST' `
      -ArtifactId ([string]$Lock.record.artifact_id) `
      -Target ([string]$Lock.slot.package_target) `
      -Profile ([string]$Lock.slot.profile) `
      -Candidates @("lock_file=$($Lock.lock_path)") `
      -Detail ("发布权已失效：锁文件缺失或被他人接管（本进程 token=$($Lock.owner_token)，当前=$($holder.owner_token))。本进程不继续写产物与收据") `
      -Remediation "排查是谁清理/接管了共享输出槽位；重试本次发布。不要手工删除正在使用的锁文件")
  }
  return $holder
}

function Exit-LoaderPublishSlot {
  param([Parameter(Mandatory = $true)][object]$Lock)

  if ($Lock.refcount -gt 1) {
    $Lock.refcount = $Lock.refcount - 1
    return
  }
  if ($script:LoaderHeldSlots.ContainsKey($Lock.key)) {
    $script:LoaderHeldSlots.Remove($Lock.key) | Out-Null
  }
  try { if ($Lock.stream) { $Lock.stream.Dispose() } } catch { }

  # 只删除**本进程持有的那个**锁文件：token 不匹配就什么都不做。
  # 一个竞争者失败/取消绝不删除赢家的锁、暂存或已发布结果。
  $holder = Read-LoaderPublishHolder -LockPath $Lock.lock_path
  if ($holder -and [string]$holder.owner_token -eq [string]$Lock.owner_token) {
    [void](Release-LoaderPublishLock -Lock $Lock)
  }
  Write-Host ("publish right released: slot={0} purpose={1}" -f $Lock.key, $Lock.purpose)
}

function New-LoaderStagedFilePath {
  param(
    [Parameter(Mandatory = $true)][object]$Slot,
    [string]$Purpose = 'artifact'
  )

  return (Join-Path $Slot.slot_dir ($script:LoaderStagePrefix + $Purpose + '-' + [guid]::NewGuid().ToString('N')))
}

# ============================================================================
# 代次档案与"当前有效代次"指针（PKG-L07c P0 补强）
#
#   代次档案：<slot_dir>/.loader-generations-<key>/<generation>.json
#             = 该代次收据的**不可变副本**。写入用独占创建（已存在即不覆盖），
#               因此"同一 generation 对应不同收据内容"是可判定的冲突，而不是静默覆盖。
#   槽位指针：<slot_dir>/.loader-current-<key>.json
#             = **唯一**指定"当前有效代次"的文档（显式 generation + 收据摘要 + 产物摘要）。
#               写入在发布权内、且在产物核对与代次档案之后；消费端先读它取得明确 generation。
#
#   为什么必须分开：仅靠"收据文件是最新收据"无法在事后回答"某次打包消费的是哪一代"，
#   槽位合法更新到 G2 之后也无法核对 G1。指针 + 不可变档案把"当前"与"历史"分开。
# ============================================================================

<#
  独占写 JSON：目标已存在时**不覆盖**，返回 $false（由调用方判定"幂等"还是"冲突"）。
  写入走临时文件 + 非覆盖 Move，因此观察到的是"要么完整新内容、要么保持原内容"。
#>
function Write-LoaderJsonFileExclusive {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Path
  )

  $parent = Split-Path -Parent $Path
  if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
  if ([System.IO.File]::Exists($Path)) { return $false }
  $json = $Value | ConvertTo-Json -Depth 20
  $temp = Join-Path $parent ('.tmp-' + [System.IO.Path]::GetFileName($Path) + '-' + [guid]::NewGuid().ToString('N'))
  [System.IO.File]::WriteAllText($temp, $json, [System.Text.UTF8Encoding]::new($false))
  try {
    [System.IO.File]::Move($temp, $Path)
    return $true
  } catch [System.IO.IOException] {
    if ([System.IO.File]::Exists($Path)) {
      Remove-LoaderFileIfExists -Path $temp
      return $false
    }
    Remove-LoaderFileIfExists -Path $temp
    throw
  }
}

function Read-LoaderSlotPointer {
  param([Parameter(Mandatory = $true)][object]$Slot)

  if (-not (Test-Path -LiteralPath $Slot.pointer_path -PathType Leaf)) { return $null }
  try {
    return (Get-Content -Raw -LiteralPath $Slot.pointer_path -Encoding UTF8 | ConvertFrom-Json)
  } catch {
    throw (New-LoaderFailureException `
      -Category 'POINTER-INVALID' `
      -ArtifactId ([string]$Slot.package_target) `
      -Target ([string]$Slot.package_target) `
      -Profile ([string]$Slot.profile) `
      -Candidates @([string]$Slot.pointer_path) `
      -Detail ("槽位代次指针无法解析：{0}" -f $_.Exception.Message) `
      -Remediation '不要手工编辑槽位指针；用正常构建重新发布该槽位')
  }
}

<#
  发布"当前有效代次"（**这是发布临界区的最后一步**）。

  调用方必须已持有该槽位的发布权；本函数在每一次共享槽位写入之前重新自检发布权，
  因此"写前复核"与"写入"处于同一排他保护之下（不存在"复核后被替换"的窗口）。
#>
function Publish-LoaderGeneration {
  param(
    [Parameter(Mandatory = $true)][object]$Lock,
    [Parameter(Mandatory = $true)]$Receipt,
    [Parameter(Mandatory = $true)][string]$RepoPath
  )

  $slot = $Lock.slot
  $artifactId = [string]$Lock.record.artifact_id
  $target = [string]$slot.package_target
  $profile = [string]$slot.profile
  $generation = [string]$Receipt.publication.generation
  if (-not $generation) {
    throw [System.InvalidOperationException]::new('receipt.publication.generation missing before publish')
  }
  $generationRecordPath = Resolve-LoaderGenerationRecordPath -Slot $slot -Generation $generation
  $receiptDigest = [string]$Receipt.publication.receipt_digest
  if (-not $receiptDigest) { $receiptDigest = Get-LoaderReceiptDigest -Receipt $Receipt }

  # ---- 1) 代次档案：不可变，独占创建 ----
  [void](Assert-LoaderPublishRight -Lock $Lock)
  $written = Write-LoaderJsonFileExclusive -Value $Receipt -Path $generationRecordPath
  $archiveReused = (-not $written)
  if ($archiveReused) {
    # 已存在同代次档案：内容摘要必须相同（幂等重放）；否则是"同一 generation 对应不同
    # 收据内容"的冲突，必须拒绝而不是覆盖。
    $existing = $null
    try { $existing = Get-Content -Raw -LiteralPath $generationRecordPath -Encoding UTF8 | ConvertFrom-Json } catch { $existing = $null }
    $existingDigest = $(if ($existing) { Get-LoaderReceiptDigest -Receipt $existing } else { $null })
    if ([string]$existingDigest -ne [string]$receiptDigest) {
      throw (New-LoaderFailureException `
        -Category 'GENERATION-CONFLICT' `
        -ArtifactId $artifactId `
        -Target $target `
        -Profile $profile `
        -Candidates @(
          "generation_record=$generationRecordPath (receipt_digest=$existingDigest)",
          "incoming_receipt_digest=$receiptDigest",
          "generation=$generation"
        ) `
        -Detail '同一 generation 在代次档案里已经有一份**不同内容**的收据：代次是不可变发布标识，不允许被改写' `
        -Remediation '这是"同代次不同收据"冲突（并发发布或人工替换）。不要覆盖代次档案；排查是谁改写了槽位后重新发布一个新代次')
    }
  }

  # ---- 2) 声明收据路径（manifest 声明的公开凭据）：必须与代次档案同内容 ----
  Invoke-LoaderTestSeam -Point 'before-generation-commit' -Detail "slot=$($slot.key) generation=$generation"
  [void](Assert-LoaderPublishRight -Lock $Lock)
  $declaredReceiptPath = [string]$Receipt.publication.declared_receipt_path
  $declaredParent = Split-Path -Parent $declaredReceiptPath
  if ($declaredParent) { New-Item -ItemType Directory -Force -Path $declaredParent | Out-Null }
  $declaredTemp = Join-Path $declaredParent ('.tmp-declared-receipt-' + [guid]::NewGuid().ToString('N'))
  [System.IO.File]::WriteAllText($declaredTemp, ($Receipt | ConvertTo-Json -Depth 20), [System.Text.UTF8Encoding]::new($false))
  try {
    Move-LoaderFileIntoPlace -Source $declaredTemp -Destination $declaredReceiptPath
  } finally {
    Remove-LoaderFileIfExists -Path $declaredTemp
  }
  # 声明收据落地后立即核对（同一排他保护内）：它必须就是本代次收据。
  $declaredOnDisk = Get-Content -Raw -LiteralPath $Receipt.publication.declared_receipt_path -Encoding UTF8 | ConvertFrom-Json
  if ([string]$declaredOnDisk.publication.generation -ne $generation -or
      [string](Get-LoaderReceiptDigest -Receipt $declaredOnDisk) -ne [string]$receiptDigest) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-COMMIT-INCONSISTENT' `
      -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @([string]$Receipt.publication.declared_receipt_path, "generation=$generation") `
      -Detail '声明收据路径落地后的内容不是本代次收据（写前复核与写入之间被替换，或写入失败）' `
      -Remediation '排查共享输出槽位是否被外部写入；用独立输出目录重试本次发布')
  }

  # ---- 3) 当前有效代次指针（最后一次共享槽位写入） ----
  Invoke-LoaderTestSeam -Point 'between-generation-and-pointer' -Detail "slot=$($slot.key) generation=$generation"
  [void](Assert-LoaderPublishRight -Lock $Lock)
  $previousPointer = $null
  try { $previousPointer = Read-LoaderSlotPointer -Slot $slot } catch { $previousPointer = $null }
  $pointer = [ordered]@{
    schema = $script:LoaderSlotPointerSchema
    kind = 'webview2-loader-slot-generation-pointer'
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    slot_key = $slot.key
    slot_identity = $slot.identity
    slot_dir = $slot.slot_dir
    destination = $slot.destination
    package_target = $slot.package_target
    profile = $slot.profile
    artifact_id = $artifactId
    generation = $generation
    producer_run_id = [string]$Receipt.publication.producer_run_id
    artifact_digest = [string]$Receipt.publication.artifact_digest
    artifact_length = [long]$Receipt.publication.artifact_length
    receipt_digest = $receiptDigest
    build_input_digest = [string]$Receipt.publication.build_input_digest
    generation_record = (ConvertTo-LoaderSlotRelativePath -SlotDir $slot.slot_dir -Path $generationRecordPath)
    generation_record_absolute = $generationRecordPath
    declared_receipt_path = [string]$Receipt.publication.declared_receipt_path
    supersedes_generation = $(if ($previousPointer) { [string]$previousPointer.generation } else { $null })
    committed_utc = (Get-Date).ToUniversalTime().ToString('o')
    publish_right_owner_pid = $PID
    publish_right_owner_token = $Lock.owner_token
    pointer_rule = '当前有效代次由本指针唯一指定：消费端必须按本指针的 generation 读取代次档案中的收据，再与产物内容身份核对。'
    generation_retention_rule = '每个成功发布的代次在代次档案中保留各自的不可变收据，因此槽位更新到新代次**不会**让已按旧代次消费的报告失效（旧报告依据保存的旧代次收据与它自己的 staging）。'
    incomplete_generation_rule = '若声明收据路径或代次档案与指针不一致（例如进程在两次写入之间退出），该状态属于不完整代次：消费端必须拒绝，不得当作上一成功版本。'
  }
  Write-LoaderJsonFile -Value $pointer -Path $slot.pointer_path | Out-Null
  return [pscustomobject]@{
    pointer = $pointer
    pointer_path = $slot.pointer_path
    generation_record_path = $generationRecordPath
    generation_record_reused = $archiveReused
    receipt_digest = $receiptDigest
    supersedes_generation = $(if ($previousPointer) { [string]$previousPointer.generation } else { $null })
  }
}

<#
  槽位内相对路径（登记在指针/收据里的可核对位置：不写绝对路径，跨机器可复核）。
#>
function ConvertTo-LoaderSlotRelativePath {
  param(
    [Parameter(Mandatory = $true)][string]$SlotDir,
    [Parameter(Mandatory = $true)][string]$Path
  )

  $prefix = $SlotDir.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
  if ($Path.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    return ($Path.Substring($prefix.Length).Replace('\', '/'))
  }
  return $Path
}

<#
  清理槽位里**久置**的残片（上次中断留下的暂存文件/备份/临时文件）。

  只在持有发布权时调用：该槽位此刻只有一个发布者，因此不存在"删掉别人正在写的
  中转文件"的问题（原实现在无锁状态下只能靠 30 分钟阈值猜测）。
  绝不清理锁文件：锁只能由持有者释放，或由 pid 存活判定回收。
#>
function Clear-LoaderStaleStaging {
  param(
    [Parameter(Mandatory = $true)][object]$Lock,
    [int]$StaleMinutes = 30
  )

  $cutoff = (Get-Date).ToUniversalTime().AddMinutes(-$StaleMinutes)
  $prefixes = @($script:LoaderStagePrefix, '.tmp-', '.bak-')
  foreach ($item in @(Get-ChildItem -LiteralPath $Lock.slot.slot_dir -Force -File -ErrorAction SilentlyContinue)) {
    $matched = $false
    foreach ($prefix in $prefixes) {
      if ($item.Name.StartsWith($prefix, [System.StringComparison]::Ordinal)) { $matched = $true; break }
    }
    if (-not $matched) { continue }
    if ($item.LastWriteTimeUtc -lt $cutoff) {
      Remove-LoaderFileIfExists -Path $item.FullName
    }
  }
}

<#
  从 cargo 的 `--message-format=json-render-diagnostics` 输出中，取出目标依赖包
  的 `build-script-executed` 记录。

  注意（裁决第 9 条）：`build-script-executed` 对**更新过的构建脚本**会返回缓存输出，
  因此它只能用于"定位本次构建实际采用的产物目录"，**不得**当成"本轮重新执行了脚本"。
  是否重跑由 build 目录的 invoked.timestamp 与本次调用时刻比较后单独记录。
#>
function Get-LoaderBuildMessageRecords {
  param(
    [string[]]$BuildMessages,
    [Parameter(Mandatory = $true)][string]$ProducerPackage
  )

  $records = @()
  if (-not $BuildMessages) { return $records }
  $needle = '#' + $ProducerPackage + '@'
  foreach ($line in $BuildMessages) {
    if (-not $line) { continue }
    $trimmed = $line.Trim()
    if (-not $trimmed.StartsWith('{')) { continue }
    if ($trimmed -notmatch '"reason":"build-script-executed"') { continue }
    if (-not $trimmed.Contains($needle)) { continue }
    $message = $null
    try {
      $message = $trimmed | ConvertFrom-Json
    } catch {
      continue
    }
    if ($message.reason -ne 'build-script-executed') { continue }
    $outDir = [string]$message.out_dir
    if (-not $outDir) { continue }
    $records += [pscustomobject]@{
      package_id = [string]$message.package_id
      out_dir = $outDir
      linked_paths = @($message.linked_paths)
    }
  }
  return $records
}

<#
  按 package 身份 + 目标 + 构建上下文定位生产者，**不按目录最新时间挑选**。
  同一 target 里存在旧版本残留不必然失败：本次构建能唯一确定生产者时忽略无关旧目录。
  只有仍无法唯一确定本次有效来源时才报歧义。
#>
function Resolve-LoaderProducerRecord {
  param(
    [object[]]$Records = @(),
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)',
    [string]$ProducerPackage = 'webview2-com-sys',
    [string]$Architecture = 'x64'
  )

  $byOutDir = [ordered]@{}
  foreach ($record in @($Records)) {
    if (-not $record -or -not $record.out_dir) { continue }
    $normalized = ([string]$record.out_dir).TrimEnd('\', '/')
    if (-not $byOutDir.Contains($normalized)) {
      $byOutDir[$normalized] = $record
    }
  }

  $distinct = @($byOutDir.Values)
  if ($distinct.Count -eq 0) {
    throw (New-LoaderFailureException `
      -Category 'PRODUCER-NOT-FOUND' `
      -ArtifactId $ArtifactId `
      -Target $Target `
      -Profile $Profile `
      -Detail "本次 cargo 构建消息中没有 $ProducerPackage 的 build-script-executed 记录，无法确定 Loader 生产者目录" `
      -Remediation "使用独立、受控的构建目录重新执行该构建入口（例如为 tauri-shell 指定单独的 --target-dir），不要依赖共享 target 中已有的残留目录；也不要用全局 cargo clean 作为常规修复前置")
  }

  $candidates = @($distinct | ForEach-Object { "{0} (package_id={1})" -f $_.out_dir, $_.package_id })
  if ($distinct.Count -gt 1) {
    throw (New-LoaderFailureException `
      -Category 'AMBIGUOUS-PRODUCER' `
      -ArtifactId $ArtifactId `
      -Target $Target `
      -Profile $Profile `
      -Candidates $candidates `
      -Detail "本次构建消息出现 $($distinct.Count) 个不同的 $ProducerPackage 产物目录，无法唯一确定本次有效来源（本流程不按目录时间排序挑选）" `
      -Remediation "使用独立、受控的构建目录重建后再打包；排查是否有多个构建入口共用同一个 target 目录")
  }

  $producer = $distinct[0]
  $linkedNative = @()
  foreach ($entry in @($producer.linked_paths)) {
    $text = [string]$entry
    if ($text.StartsWith('native=', [System.StringComparison]::OrdinalIgnoreCase)) {
      $linkedNative += $text.Substring(7)
    }
  }
  $expectedNative = Resolve-LoaderRealPath (Join-Path $producer.out_dir.TrimEnd('\', '/') $Architecture)
  $evidence = 'out_dir-only'
  if ($linkedNative.Count -gt 0) {
    $matched = $false
    foreach ($native in $linkedNative) {
      # cargo 混用 '/' 与 '\'；统一规范化后再比较，避免把拼写差异当成来源不一致。
      $resolvedNative = Resolve-LoaderRealPath ($native.TrimEnd('\', '/'))
      if ($resolvedNative.Equals($expectedNative, [System.StringComparison]::OrdinalIgnoreCase)) {
        $matched = $true
        break
      }
    }
    if (-not $matched) {
      throw (New-LoaderFailureException `
        -Category 'PRODUCER-EVIDENCE-INCONSISTENT' `
        -ArtifactId $ArtifactId `
        -Target $Target `
        -Profile $Profile `
        -Candidates $candidates `
        -Detail "构建记录里的 rustc-link-search 与 out_dir 不一致：out_dir=$expectedNative 但 linked_paths=$($linkedNative -join '; ')" `
        -Remediation "使用独立、受控的构建目录重建后再打包")
    }
    $evidence = 'out_dir+linked_paths'
  }

  return [pscustomobject]@{
    out_dir = $producer.out_dir.TrimEnd('\', '/')
    package_id = $producer.package_id
    candidate_count = $distinct.Count
    candidates = $candidates
    evidence = $evidence
  }
}

function Get-LoaderArtifactContract {
  param([Parameter(Mandatory = $true)][object]$Artifact)

  $contract = [ordered]@{
    id = $Artifact.id
    source = $Artifact.source
    target = $Artifact.target
    build = $Artifact.build
    export = $Artifact.export
  }
  return ($contract | ConvertTo-Json -Depth 10 -Compress)
}

function Get-LoaderArtifactContractHash {
  param([Parameter(Mandatory = $true)][object]$Artifact)

  $text = Get-LoaderArtifactContract -Artifact $Artifact
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

<#
  构建入口的源码/配置身份。**不使用整个工作树的 git status**：
  tauri-build 每次都会重写 src-tauri/gen/schemas/capabilities.json，
  若把工作树状态算进身份，收据会在每次构建后立刻自我失效。
  这里只用 manifest 显式声明的输入文件 + 目录集合。
#>
function Get-LoaderSourceIdentity {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][object]$Spec,
    [string]$Profile = 'release'
  )

  function Expand-LoaderToken {
    param([string]$Value)
    return ([string]$Value).Replace('{profile}', $Profile).Replace('{configuration}', $Profile)
  }

  $entries = [System.Collections.Generic.List[object]]::new()
  foreach ($file in @($Spec.files)) {
    $relative = Expand-LoaderToken ([string]$file)
    if (-not $relative) { continue }
    $full = [System.IO.Path]::GetFullPath((Join-Path $RepoPath $relative))
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
      throw [System.InvalidOperationException]::new("build entry source identity file is missing: $relative")
    }
    $identity = Get-LoaderFileIdentity $full
    if (-not $identity) {
      throw [System.InvalidOperationException]::new("build entry source identity file disappeared or became unreadable while hashing: $relative")
    }
    $entries.Add([pscustomobject]@{
      path = $relative.Replace('\', '/')
      length = $identity.length
      sha256 = $identity.sha256
    })
  }
  $excludePatterns = @($Spec.exclude_patterns)
  foreach ($directory in @($Spec.directories)) {
    $relativeDir = Expand-LoaderToken ([string]$directory)
    if (-not $relativeDir) { continue }
    $fullDir = [System.IO.Path]::GetFullPath((Join-Path $RepoPath $relativeDir))
    if (-not (Test-Path -LiteralPath $fullDir -PathType Container)) { continue }
    $prefix = $fullDir.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    foreach ($item in @(Get-ChildItem -LiteralPath $fullDir -File -Recurse -Force | Sort-Object FullName)) {
      $relativePath = ($relativeDir.TrimEnd('\', '/') + '/' + $item.FullName.Substring($prefix.Length)).Replace('\', '/')
      $skip = $false
      foreach ($pattern in $excludePatterns) {
        if ($relativePath -match $pattern) { $skip = $true; break }
      }
      if ($skip) { continue }
      $identity = Get-LoaderFileIdentity $item.FullName
      if (-not $identity) {
        throw [System.InvalidOperationException]::new("build entry source identity file disappeared or became unreadable while hashing: $relativePath")
      }
      $entries.Add([pscustomobject]@{
        path = $relativePath
        length = $identity.length
        sha256 = $identity.sha256
      })
    }
  }

  $sorted = @($entries | Sort-Object path)
  $builder = [System.Text.StringBuilder]::new()
  foreach ($entry in $sorted) {
    [void]$builder.AppendLine("$($entry.path)|$($entry.length)|$($entry.sha256)")
  }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $treeHash = ([System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($builder.ToString())))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }

  return [pscustomobject]@{
    kind = 'declared-file-hash-set'
    tree_sha256 = $treeHash
    file_count = $sorted.Count
    files = $sorted
  }
}

<#
  VCS 参考状态（第五轮裁决 B-1，口径与 scripts/lib/build-identity.ps1 的
  Get-VcsReferenceState 一致：这里只做只读探测，不做 init/add/commit/tag）。

  * HEAD 只登记为 vcs_reference_commit（种子参考），**不是** source_commit 权威；
  * `git ls-files` 列举的是**索引中的已跟踪文件**，为 0 只说明"当前源码没有被该提交
    有效覆盖"，不能推断"没有 .git"；
  * 因此索引为空时 source_commit 留空、dirty_against_commit = not_evaluable——
    绝不把"没有被跟踪的变更"写成 dirty = false。
#>
function Get-LoaderRepoHead {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  $result = [ordered]@{
    source_commit = $null
    source_commit_available = $false
    source_commit_authority = 'not-authoritative'
    tracked_build_entry_files = 0
    tracked_index_file_count = 0
    vcs_state = 'no_vcs'
    vcs_reference_commit = $null
    vcs_reference_commit_kind = 'seed-reference-only'
    dirty_against_commit = 'not_evaluable'
    vcs_evidence = $null
  }
  try {
    $output = @(& git -C $RepoPath rev-parse --verify HEAD 2>$null)
    if ($LASTEXITCODE -eq 0 -and $output.Count -gt 0) {
      $candidate = ([string]($output | Select-Object -First 1)).Trim()
      if ($candidate -match '^[0-9a-fA-F]{40}$') {
        $result.vcs_reference_commit = $candidate.ToLowerInvariant()
      }
    }
    $tracked = @(& git -C $RepoPath ls-files -- 'modules/gui-desktop/packages/tauri-shell/src-tauri' 2>$null)
    if ($LASTEXITCODE -eq 0) {
      $result.tracked_build_entry_files = @($tracked | Where-Object { $_ }).Count
    }
    $trackedAll = @(& git -C $RepoPath ls-files 2>$null)
    if ($LASTEXITCODE -eq 0) {
      $result.tracked_index_file_count = @($trackedAll | Where-Object { $_ }).Count
    }
    if ($result.tracked_index_file_count -eq 0) {
      $result.vcs_state = 'untracked_snapshot'
      $result.vcs_evidence = ('git rev-parse HEAD = {0}（种子参考提交）；git ls-files 索引已跟踪文件数 = 0 ⇒ 当前源码没有被该提交有效覆盖' -f $result.vcs_reference_commit)
    } else {
      $result.vcs_state = 'tracked_worktree'
      $result.source_commit = $result.vcs_reference_commit
      $result.source_commit_available = ($null -ne $result.vcs_reference_commit)
      $result.source_commit_authority = 'tracked-commit'
      $status = @(& git -C $RepoPath status --porcelain 2>$null)
      if ($LASTEXITCODE -eq 0) {
        $result.dirty_against_commit = (@($status | Where-Object { $_ }).Count -gt 0)
      }
    }
  } catch {
    # git 不可用时保持显式未知；身份以文件哈希集合为准（见 source_identity_kind）。
  }
  return [pscustomobject]$result
}

function Write-LoaderJsonFile {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Path
  )

  $parent = Split-Path -Parent $Path
  if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
  $json = $Value | ConvertTo-Json -Depth 20
  # 写盘走临时文件 + 原子替换：收据与导出物不允许出现"半成品"。
  $temp = Join-Path $parent ('.tmp-' + [System.IO.Path]::GetFileName($Path) + '-' + [guid]::NewGuid().ToString('N'))
  [System.IO.File]::WriteAllText($temp, $json, [System.Text.UTF8Encoding]::new($false))
  Move-LoaderFileIntoPlace -Source $temp -Destination $Path
  return $Path
}

<#
  原子落地：同一卷内用 ReplaceFile 语义替换目标，避免"先删后写"留下空窗。
  .NET Framework 的 File.Move 没有 overwrite 重载，File.Replace 的备份参数也不能传 $null
  （PowerShell 会把它绑成空字符串并抛 "path is not of a legal form"），因此显式给备份路径再删除。
#>
<#
  原子落地：同一卷内用 ReplaceFile 语义替换目标，避免"先删后写"留下空窗。
  .NET Framework 的 File.Move 没有 overwrite 重载，File.Replace 的备份参数也不能传 $null
  （PowerShell 会把它绑成空字符串并抛 "path is not of a legal form"），因此显式给备份路径再删除。

  调用方必须已持有该输出槽位的发布权（见 Enter-LoaderPublishSlot）：
  并发 ReplaceFile 同一目标会以 ERROR_UNABLE_TO_REMOVE_REPLACED /
  ERROR_UNABLE_TO_MOVE_REPLACEMENT / ERROR_SHARING_VIOLATION 失败。

  目标在 Test-Path 与落地之间被创建时不再抛"文件已存在"这种没有诊断价值的失败，
  而是走 ReplaceFile 语义（该分支留作无锁直接调用时的兜底）。目标真实存在时，
  使用非覆盖 Move 失败后同样回退到 Replace。
#>
function Move-LoaderFileIntoPlace {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Destination
  )

  if (Test-Path -LiteralPath $Destination) {
    $backup = Join-Path (Split-Path -Parent $Destination) ('.bak-' + [guid]::NewGuid().ToString('N'))
    try {
      [System.IO.File]::Replace($Source, $Destination, $backup, $true)
    } finally {
      Remove-LoaderFileIfExists -Path $backup
    }
    return
  }
  try {
    [System.IO.File]::Move($Source, $Destination)
  } catch [System.IO.IOException] {
    if (-not (Test-Path -LiteralPath $Destination)) { throw }
    $backup = Join-Path (Split-Path -Parent $Destination) ('.bak-' + [guid]::NewGuid().ToString('N'))
    try {
      [System.IO.File]::Replace($Source, $Destination, $backup, $true)
    } finally {
      Remove-LoaderFileIfExists -Path $backup
    }
  }
}

<#
  把已校验的源文件发布到稳定导出路径：本次运行独立暂存 → 复核暂存内容身份 →
  复核发布权仍在 → 原子替换 → 复核目标内容身份。任何一步不符都不发布半成品。

  SlotLock：调用方已持有的发布权（导出器等需要跨"产物 + 收据"两次发布时传入）。
  未传时**本函数自己**取得该输出槽位的发布权并在结束时释放——因此无锁的直接调用
  （例如契约测试里的并发复制）也遵守"一个槽位一个发布者"。
#>
function Copy-LoaderVerifiedFile {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Destination,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256,
    [Parameter(Mandatory = $true)][long]$ExpectedLength,
    [object]$SlotLock,
    [string]$ArtifactId = '(unknown)',
    [string]$Target = '(unknown)',
    [string]$Profile = '(unknown)'
  )

  $slot = Get-LoaderPublishSlot -Destination $Destination -Target $Target -Profile $Profile
  $ownsLock = $false
  $lock = $SlotLock
  if (-not $lock) {
    $lock = Enter-LoaderPublishSlot -Slot $slot -Purpose 'export' -ArtifactId $ArtifactId -Target $Target -Profile $Profile
    $ownsLock = $true
  } else {
    $lock.refcount = $lock.refcount + 1
  }

  try {
    Clear-LoaderStaleStaging -Lock $lock
    $staged = New-LoaderStagedFilePath -Slot $lock.slot -Purpose 'artifact'
    try {
      Copy-Item -LiteralPath $Source -Destination $staged -Force
      $stagedIdentity = Get-LoaderFileIdentity $staged
      if (-not $stagedIdentity) {
        throw [System.InvalidOperationException]::new("staged copy disappeared: $staged")
      }
      if ($stagedIdentity.sha256 -ne $ExpectedSha256 -or $stagedIdentity.length -ne $ExpectedLength) {
        throw [System.InvalidOperationException]::new(
          "content identity changed while copying: expected $ExpectedLength/$ExpectedSha256, staged $($stagedIdentity.length)/$($stagedIdentity.sha256)")
      }
      [void](Assert-LoaderPublishRight -Lock $lock)
      Move-LoaderFileIntoPlace -Source $staged -Destination $Destination
    } finally {
      Remove-LoaderFileIfExists -Path $staged
    }

    $finalIdentity = Get-LoaderFileIdentity $Destination
    if (-not $finalIdentity -or $finalIdentity.sha256 -ne $ExpectedSha256 -or $finalIdentity.length -ne $ExpectedLength) {
      throw (New-LoaderFailureException `
        -Category 'PUBLISH-INCONSISTENT' `
        -ArtifactId $ArtifactId `
        -Target $Target `
        -Profile $Profile `
        -Candidates @("destination=$Destination", "expected=$ExpectedLength/$ExpectedSha256") `
        -Detail ("发布后的产物内容与校验身份不一致：$Destination。在持有发布权的情况下出现该状态说明有第三方改动了共享输出槽位") `
        -Remediation "排查共享输出槽位是否被外部写入（不要手工替换稳定导出文件）；用独立输出目录重试本次发布")
    }
    return $finalIdentity
  } finally {
    if ($ownsLock) { Exit-LoaderPublishSlot -Lock $lock } else { $lock.refcount = $lock.refcount - 1 }
  }
}

<#
  正常构建模式：从本次 cargo 构建消息定位实际生产者 → 校验架构与内容 → 导出到稳定路径 → 写收据。
  返回收据对象。
#>
function Export-LoaderArtifact {
  param([Parameter(Mandatory = $true)][hashtable]$Context)

  $artifact = $Context.Artifact
  $export = $Context.Export
  $artifactId = [string]$artifact.id
  $target = [string]$artifact.target
  $profile = [string]$Context.Profile
  $repoPath = $Context.RepoPath
  $stablePath = [string]$Context.StablePath
  $receiptPath = [string]$Context.ReceiptPath

  $records = @(Get-LoaderBuildMessageRecords -BuildMessages $Context.BuildMessages -ProducerPackage ([string]$export.producer_package))
  $producer = Resolve-LoaderProducerRecord `
    -Records $records `
    -ArtifactId $artifactId `
    -Target $target `
    -Profile $profile `
    -ProducerPackage ([string]$export.producer_package) `
    -Architecture ([string]$export.architecture)

  $buildRootText = Join-Path $repoPath (([string]$export.build_root).Replace('{profile}', $profile))
  $buildRoot = Resolve-LoaderRealPath $buildRootText
  $producerOutDir = Resolve-LoaderRealPath $producer.out_dir
  if (-not (Test-LoaderPathInside -Path $producerOutDir -Root $buildRoot)) {
    throw (New-LoaderFailureException `
      -Category 'OUTSIDE-ALLOWED-ROOT' `
      -ArtifactId $artifactId `
      -Target $target `
      -Profile $profile `
      -Candidates $producer.candidates `
      -Detail "本次构建报告的生产者目录不在允许根内：resolved=$producerOutDir allowed=$buildRoot" `
      -Remediation "不要从共享 target 或工作区外的目录取发布输入；使用独立、受控的构建目录重建后再打包")
  }

  $rawDirectory = Join-Path $producerOutDir ([string]$export.producer_arch_directory)
  $rawPath = Join-Path $rawDirectory ([string]$export.producer_file_name)
  if (-not (Test-Path -LiteralPath $rawPath -PathType Leaf)) {
    throw (New-LoaderFailureException `
      -Category 'SOURCE-MISSING' `
      -ArtifactId $artifactId `
      -Target $target `
      -Profile $profile `
      -Candidates $producer.candidates `
      -Detail "构建记录指向的生产者目录内没有 Loader 文件：$rawPath" `
      -Remediation "确认构建入口确实启用了 WebView2 loader 依赖；使用独立、受控的构建目录重建后再打包")
  }

  $rawIdentity = Get-LoaderFileIdentity $rawPath
  if (-not $rawIdentity) {
    throw (New-LoaderFailureException `
      -Category 'SOURCE-MISSING' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates $producer.candidates -Detail "生产者输出的 Loader 无法读取身份（读取消歧：文件消失或被独占）：$rawPath" `
      -Remediation "确认构建仍在进行／没有第三方在改写构建目录；使用独立、受控的构建目录重建后再打包")
  }
  if ($rawIdentity.length -le 0) {
    throw (New-LoaderFailureException `
      -Category 'SOURCE-INVALID' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates $producer.candidates -Detail "生产者输出的 Loader 为空文件：$rawPath" `
      -Remediation "使用独立、受控的构建目录重建后再打包")
  }
  $expectedMachine = Get-LoaderArchitectureMachine ([string]$export.architecture)
  $rawMachine = Get-LoaderPeMachine $rawPath
  if ($rawMachine -ne $expectedMachine) {
    throw (New-LoaderFailureException `
      -Category 'ARCH-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates $producer.candidates `
      -Detail "生产者输出架构不匹配：expected=$expectedMachine actual=$rawMachine path=$rawPath" `
      -Remediation "确认构建目标架构与 manifest 声明一致；不要把 x86/arm64 的 Loader 装进 x64 包")
  }

  # ---------------------------------------------------------------------------
  # 发布临界区（PKG-L07c P0 补强，裁决 §3.2）：**同一把排他发布权**覆盖
  #   ① 产物最终核对与发布（staged 内容身份 + 发布后复核）
  #   ② 收据生成与提交（不可变代次档案 + 声明收据路径，写前均自检发布权）
  #   ③ **当前有效代次的发布**（槽位代次指针，最后一次共享槽位写入）
  #   → 完成校验 → 释放。
  # 产物与收据仍是"连续替换两个文件"，不是跨文件原子提交；但"哪一代是当前有效代次"
  # 只由指针宣布，所以任何中断都落在"指针仍指旧代次 + 产物/收据不匹配"这种可判定状态。
  # 失败路径（catch/finally）**只写自己的诊断**：本函数失败时不会写声明收据、不会写指针，
  # 因此失败竞争者无法向共享有效槽位发布自己的收据。
  # ---------------------------------------------------------------------------
  $slot = Get-LoaderPublishSlot -Destination $stablePath -RepoPath $repoPath -Target $target -Profile $profile
  $lock = Enter-LoaderPublishSlot -Slot $slot -Purpose 'export' -ArtifactId $artifactId -Target $target -Profile $profile
  try {
    # 声明输入的源码/配置身份必须在**发布产物之前**取样，并在收据落地前再取样核对：
    # 导出窗口内输入发生变化 ⇒ 本次发布不获准（不写收据），不依赖任何 quiescent 放行开关。
    $sourceIdentityBeforePublish = Get-LoaderSourceIdentity -RepoPath $repoPath -Spec $export.source_identity -Profile $profile
    Invoke-LoaderTestSeam -Point 'before-file-publish' -Detail "slot=$($slot.key)"
    $generation = [guid]::NewGuid().ToString('N')
    # 产出这一代次的运行 ID：调用方（打包运行）提供则用之，否则由导出器生成并如实标注来源。
    $producerRunId = [string]$Context.ProducerRunId
    if (-not $producerRunId) { $producerRunId = [string]$Context.RunId }
    $producerRunIdSource = 'provided-by-caller'
    if (-not $producerRunId) {
      $producerRunId = ('loader-export-run-{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), [guid]::NewGuid().ToString('N').Substring(0, 8))
      $producerRunIdSource = 'generated-by-exporter'
    }

  $stableIdentity = Copy-LoaderVerifiedFile `
    -Source $rawPath `
    -Destination $stablePath `
    -ExpectedSha256 $rawIdentity.sha256 `
    -ExpectedLength $rawIdentity.length `
    -SlotLock $lock `
    -ArtifactId $artifactId -Target $target -Profile $profile
  $filePublishedUtc = (Get-Date).ToUniversalTime().ToString('o')
  Invoke-LoaderTestSeam -Point 'between-file-and-receipt' -Detail "slot=$($slot.key) generation=$generation"

  $sourceIdentity = Get-LoaderSourceIdentity -RepoPath $repoPath -Spec $export.source_identity -Profile $profile
  if ($sourceIdentity.tree_sha256 -ne $sourceIdentityBeforePublish.tree_sha256) {
    throw (New-LoaderFailureException `
      -Category 'SOURCE-INPUT-CHANGED' `
      -ArtifactId $artifactId `
      -Target $target `
      -Profile $profile `
      -Candidates @(
        "before=$($sourceIdentityBeforePublish.tree_sha256)",
        "after=$($sourceIdentity.tree_sha256)",
        "files_before=$($sourceIdentityBeforePublish.file_count) files_after=$($sourceIdentity.file_count)"
      ) `
      -Detail "导出窗口内构建入口声明的源码/配置输入发生变化：本次发布不获准（不写收据，产物不构成任何已核对的代次）" `
      -Remediation "让源码/配置停止变化后重新打包；不要用 quiescent=false 之类的开关给混合输入补一张可发布收据。已产出的临时产物属于不可发布分区")
  }
  $head = Get-LoaderRepoHead -RepoPath $repoPath
  $producerBuildDir = Split-Path -Parent $producerOutDir
  $invokedTimestamp = Join-Path $producerBuildDir 'invoked.timestamp'
  $invokedTimestampUtc = $null
  $rerunInThisInvocation = $null
  if (Test-Path -LiteralPath $invokedTimestamp -PathType Leaf) {
    $invokedTimestampUtc = (Get-Item -LiteralPath $invokedTimestamp -Force).LastWriteTimeUtc.ToString('o')
    if ($Context.BuildStartedUtc) {
      $rerunInThisInvocation = ([datetime]::Parse($invokedTimestampUtc) -ge [datetime]::Parse($Context.BuildStartedUtc))
    }
  }

  $rawRecordRelative = $null
  $rawRecordSha256 = $null
  if ($Context.RawRecordPath -and (Test-Path -LiteralPath $Context.RawRecordPath -PathType Leaf)) {
    $rawRecordIdentity = Get-LoaderFileIdentity $Context.RawRecordPath
    $rawRecordSha256 = $rawRecordIdentity.sha256
    if (Test-LoaderPathInside -Path $rawRecordIdentity.path -Root $repoPath) {
      $rawRecordRelative = $rawRecordIdentity.path.Substring($repoPath.TrimEnd('\', '/').Length + 1).Replace('\', '/')
    }
  }

  # 构建输入身份 + artifact 声明摘要：两者都进收据，且 build_input_digest 允许消费侧
  # 在报告里独立核对"这一代产物对应哪一组构建输入"。
  $artifactContractHash = Get-LoaderArtifactContractHash -Artifact $artifact
  $buildInputDigest = Get-LoaderBuildInputDigest `
    -ArtifactContractSha256 $artifactContractHash `
    -SourceIdentitySha256 $sourceIdentity.tree_sha256 `
    -SourceIdentityFileCount ([int]$sourceIdentity.file_count) `
    -LockfileSha256 ([string]$Context.LockfileSha256) `
    -BuildEntryManifestSha256 ([string]$Context.BuildEntryManifestSha256) `
    -BuildTarget ([string]$Context.BuildTarget) `
    -CargoTargetDir ([string]$Context.CargoTargetDir) `
    -Profile $profile `
    -ReleaseVersion ([string]$Context.ReleaseVersion) `
    -ProducerPackage ([string]$export.producer_package)

  $receipt = [ordered]@{
    schema = $script:LoaderReceiptSchema
    kind = 'webview2-loader-export'
    artifact_id = $artifactId
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    mode = 'build'
    artifact_contract_sha256 = $artifactContractHash
    build_identity = [ordered]@{
      build_input_digest = $buildInputDigest
      build_input_digest_rule = 'SHA256（artifact 声明摘要 + 构建入口源码/配置身份 + 构建入口 manifest/lockfile + target/profile/cargo target-dir/release version）'
      source_identity_kind = $sourceIdentity.kind
      source_identity_sha256 = $sourceIdentity.tree_sha256
      source_identity_file_count = $sourceIdentity.file_count
      source_identity_files = $sourceIdentity.files
      # 身份不冒充（第五轮裁决 B-1）：上面的 source_identity_sha256 只是**构建入口声明
      # 文件 + 导入目录**的子范围哈希集合，不是全量源码快照。全量源码身份是 package
      # report 的 source_snapshot_digest（本收据由 package_association 指向包报告）。
      source_identity_is_full_source_snapshot = $false
      # 取样口径：在发布产物之前与收据落地之前各取样一次，两次必须一致
      # （不一致即 SOURCE-INPUT-CHANGED，本次发布不获准）。这样收据不会给"发布窗口内
      # 已变化的输入"背书。
      source_identity_sampling = 'sampled-before-publish-and-before-receipt-commit; pre-must-equal-post'
      source_identity_before_publish_sha256 = $sourceIdentityBeforePublish.tree_sha256
      source_snapshot_digest = $null
      source_snapshot_digest_available = $false
      source_snapshot_note = '全量源码/构建资源快照身份（source_snapshot_digest）由 scripts/package-all.ps1 计算并写入 package report 的 build_identity；构建输入身份是 build_input_digest，载荷身份是 payload_digest，三者互不冒充'
      source_commit = $head.source_commit
      source_commit_available = $head.source_commit_available
      source_commit_authority = $head.source_commit_authority
      vcs_state = $head.vcs_state
      vcs_reference_commit = $head.vcs_reference_commit
      vcs_reference_commit_kind = $head.vcs_reference_commit_kind
      dirty_against_commit = $head.dirty_against_commit
      vcs_evidence = $head.vcs_evidence
      tracked_build_entry_files = $head.tracked_build_entry_files
      tracked_index_file_count = $head.tracked_index_file_count
      build_entry_artifact = [string]$export.build_entry_artifact
      build_entry_manifest = [string]$Context.BuildEntryManifest
      build_entry_manifest_sha256 = $Context.BuildEntryManifestSha256
      lockfile = [string]$Context.LockfileRelative
      lockfile_sha256 = $Context.LockfileSha256
      cargo_target_dir = [string]$Context.CargoTargetDir
      profile = $profile
      build_target = [string]$Context.BuildTarget
      host_target = [string]$Context.HostTarget
      cargo_version = [string]$Context.CargoVersion
      release_version = [string]$Context.ReleaseVersion
    }
    build_invocation = [ordered]@{
      command = [string]$Context.BuildCommand
      args = @($Context.BuildArgs)
      working_dir = [string]$Context.BuildWorkingDir
      message_format = 'json-render-diagnostics'
      invoked_at = [string]$Context.BuildStartedUtc
      raw_message_record = $rawRecordRelative
      raw_message_record_sha256 = $rawRecordSha256
      # 裁决第 9 条：build-script-executed 可能是缓存输出，
      # 这里显式记录"是否本次真正重跑"，不得把它当成"本次重新生成"。
      producer_build_script_invoked_timestamp_utc = $invokedTimestampUtc
      producer_build_script_rerun_in_this_invocation = $rerunInThisInvocation
    }
    dependency_identity = [ordered]@{
      producer_package = [string]$export.producer_package
      package_id = $producer.package_id
      producer_out_dir = $producer.out_dir
      producer_out_dir_resolved = $producerOutDir
      producer_out_dir_repo_relative = $(if (Test-LoaderPathInside -Path $producerOutDir -Root $repoPath) {
          $producerOutDir.Substring($repoPath.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        } else { $null })
      producer_candidate_count = $producer.candidate_count
      producer_candidates = @($producer.candidates)
      producer_selection_evidence = $producer.evidence
    }
    file_identity = [ordered]@{
      raw_source = $rawIdentity.path
      raw_source_length = $rawIdentity.length
      raw_source_sha256 = $rawIdentity.sha256
      raw_source_machine = $rawMachine
      stable_export = ($stableIdentity.path.Substring($repoPath.TrimEnd('\', '/').Length + 1).Replace('\', '/'))
      stable_export_absolute = $stableIdentity.path
      stable_export_length = $stableIdentity.length
      stable_export_sha256 = $stableIdentity.sha256
      package_target = $target
      architecture = [string]$export.architecture
      machine = $expectedMachine
    }
    reuse_nature = $(if ($rerunInThisInvocation -eq $true) {
        'producer-output-regenerated-in-this-invocation'
      } else { 'producer-output-reused-from-existing-cargo-build-directory' })
    package_association = [ordered]@{
      package_target = $target
      profile = $profile
      consumed_by = 'scripts/package-all.ps1'
      msi_input = 'staged package root (see package report / installer report)'
    }
    # 裁决第 8 条：Loader 来源与 WebView2 Runtime 前置分开登记。
    runtime_dependency = [ordered]@{
      webview2_runtime_distribution = 'not-managed-by-this-pipeline'
      webview2_runtime_asserted = $false
      note = '本收据只证明 Loader 文件来源；目标机器是否具备可用的 WebView2 Runtime 不由本流程断言。'
    }
    # 发布代次与发布权（PKG-L07c / schema 3）：固定代次语义。
    #   generation 是一次成功发布的**不可变代次 ID**（不是 mtime：相同字节的两次发布
    #   也会得到不同 generation）；artifact_digest 是产物最终核对后的内容身份；
    #   receipt_digest 是本收据自身的规范内容摘要（排除自身字段，可由任何一方重算）；
    #   build_input_digest 是"该产物由哪一组构建输入身份产出"的摘要。
    publication = [ordered]@{
      protocol = 'webview2-loader-single-publisher'
      protocol_version = 2
      cross_file_atomicity = 'none: continuous-replace-of-two-files'
      slot_key = $slot.key
      slot_identity = $slot.identity
      slot_dir = $slot.slot_dir
      destination = $slot.destination
      generation = $generation
      state = 'complete'
      producer_run_id = $producerRunId
      producer_run_id_source = $producerRunIdSource
      artifact_digest = $stableIdentity.sha256
      artifact_length = $stableIdentity.length
      receipt_digest = $null
      receipt_digest_rule = 'SHA256（本收据除 publication.receipt_digest 之外的叶子行按 Ordinal 排序后连接）'
      build_input_digest = $buildInputDigest
      build_input_digest_rule = 'SHA256（artifact 声明摘要 + 构建入口源码/配置身份 + 构建入口 manifest/lockfile + target/profile/cargo target-dir/release version）'
      declared_receipt_path = [System.IO.Path]::GetFullPath($receiptPath)
      declared_receipt = (ConvertTo-LoaderSlotRelativePath -SlotDir $repoPath -Path ([System.IO.Path]::GetFullPath($receiptPath)))
      generation_record_path = (Resolve-LoaderGenerationRecordPath -Slot $slot -Generation $generation)
      generation_record = (ConvertTo-LoaderSlotRelativePath -SlotDir $slot.slot_dir -Path (Resolve-LoaderGenerationRecordPath -Slot $slot -Generation $generation))
      slot_pointer_path = $slot.pointer_path
      file_published_utc = $filePublishedUtc
      receipt_committed_utc = (Get-Date).ToUniversalTime().ToString('o')
      publish_right = [ordered]@{
        mechanism = 'exclusive-create + hold-open lock file in the output slot'
        covers = 'artifact final verification + receipt generation and commit + current generation publication (slot pointer)'
        lock_file = $lock.lock_path
        owner_pid = $PID
        owner_token = $lock.owner_token
        owner_host = $lock.record.owner_host
        acquired_utc = $lock.record.acquired_utc
        reclaimed_stale_lock = $lock.reclaimed_stale
      }
      incomplete_generation_rule = '产物已替换但收据未落地（或收据与指针不一致）的代次属于不完整代次：消费端按"指针 + 代次档案 + 声明收据 + 产物内容身份"四方核对必然拒绝，不得把它当作上一成功版本'
      consumer_rule = '读侧必须持有同一槽位的读取权，先读槽位指针取得**明确 generation**，再按该代次代次档案与声明收据核对产物；不得只凭收据存在就采用产物，也不得在报告生成阶段重新读取"最新收据"'
      generation_retention_rule = '每个成功发布的代次在槽位代次档案中保留各自的不可变收据；槽位更新到新一代次不会让已按旧代次消费的报告失效'
    }
  }

  # 收据内容摘要：在收据对象最终定型之后计算（排除 publication.receipt_digest 自身，
  # 避免自引用）。任何读到该收据的一方都能重算出同一个值。
  $receipt.publication.receipt_digest = Get-LoaderReceiptDigest -Receipt $receipt

  # 发布代次（**完整发布临界区**，全部在同一把发布权内）：
  #   代次档案（不可变）→ 声明收据路径 → 当前有效代次指针。
  # 每一步写入之前都重新自检发布权，因此不存在"复核之后被替换"的窗口。
  $publicationResult = Publish-LoaderGeneration -Lock $lock -Receipt $receipt -RepoPath $repoPath
  Invoke-LoaderTestSeam -Point 'after-receipt' -Detail "slot=$($slot.key) generation=$generation"

  # 完成校验（发布权仍在手）：盘上的"产物 + 指针 + 代次档案 + 声明收据"必须自洽且属于本代次。
  # 直接复用消费端入口，保证导出端与打包端用的是同一套判定规则。
  $verified = Assert-LoaderExportReceipt -Context $Context -Slot $slot
  if ([string]$verified.receipt.publication.generation -ne $generation) {
    throw (New-LoaderFailureException `
      -Category 'PUBLISH-INCONSISTENT' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (generation=$($verified.receipt.publication.generation))", "expected=$generation") `
      -Detail '完成校验读到的是别的代次收据：共享输出槽位被并发写入' `
      -Remediation "排查是否有第二个打包者在写同一个输出槽位；给并发的打包运行指定独立输出目录")
  }
  # 发布权仍在手时完成**本次消费**（复制到本次打包的独立 staging + 固定消费收据），
  # 使"收据提交"与"本次消费的字节"处于同一排他保护之下（裁决 §3.2 的消费端固定流程）。
  if ($Context.ConsumeStagingDirectory) {
    $Context.Consumption = Use-LoaderExportGeneration `
      -Context $Context `
      -ConsumerRunId ([string]$Context.ConsumerRunId) `
      -ConsumeStagingDirectory ([string]$Context.ConsumeStagingDirectory) `
      -Slot $slot
  }

  Write-Host ("export {0}: {1} -> {2} (sha256={3}, length={4}, generation={5})" -f $artifactId, $rawPath, $stableIdentity.path, $stableIdentity.sha256, $stableIdentity.length, $generation)
  return $receipt
  } finally {
    Exit-LoaderPublishSlot -Lock $lock
  }
}

<#
  发布资格校验**入口**（消费端一致性边界）。

  读侧与写侧遵守同一条边界：只有先取得该输出槽位的读取权，才去核对"产物 + 收据"。
  发布者正在连续替换两个文件（产物先、收据后）时，消费端不会被插入读取；如果发布者
  持有发布权，本调用等到超时并给出明确的 EXPORT-SLOT-BUSY（fail-closed），而不是
  读到一个"看起来存在"的中间状态。

  Slot：导出端已在同一槽位持有发布权时传入（保证后续读与刚才的写在同一槽位上判定）。
#>
<#
  消费侧解析：**在读/发布资格内**把"当前有效代次"解析成一组可核对对象。

  这是写侧与读侧共用的一条边界（PKG-L07c P0 补强，裁决 §3.2）：
    * **明确 generation**：当前有效代次只由槽位指针指定。没有指针 = 该槽位没有可采用的
      当前有效代次（POINTER-MISSING），绝不退化成"读最新收据文件"这种隐含指针。
    * **三类凭据必须自洽**：槽位指针 ↔ 不可变代次档案 ↔ 声明收据路径。任何一处指向别的
      代次、或内容摘要不同，都拒绝（"失败竞争者把自己的收据写进共享槽位"正是被这一条挡住）。
    * **产物内容身份**必须等于该代次登记的 artifact_digest/artifact_length，因此
      "产物已替换、收据未落地"的不完整代次必然被拒绝，且不会被当成上一成功版本。
#>
function Resolve-LoaderGenerationUnderLock {
  param(
    [Parameter(Mandatory = $true)][hashtable]$Context,
    [Parameter(Mandatory = $true)][object]$Slot,
    [switch]$VerifyIdentity
  )

  $artifact = $Context.Artifact
  $export = $Context.Export
  $artifactId = [string]$artifact.id
  $target = [string]$artifact.target
  $profile = [string]$Context.Profile
  $repoPath = [string]$Context.RepoPath
  $stablePath = [string]$Context.StablePath
  $receiptPath = [string]$Context.ReceiptPath

  if (-not (Test-Path -LiteralPath $receiptPath -PathType Leaf)) {
    $stableExists = Test-Path -LiteralPath $stablePath -PathType Leaf
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-MISSING' `
      -ArtifactId $artifactId `
      -Target $target `
      -Profile $profile `
      -Candidates @($stablePath, $Slot.pointer_path) `
      -Detail ("稳定导出路径缺少导出记录：receipt=$receiptPath（导出物本身：{0}）" -f $(if ($stableExists) { '存在，但来源不可核对' } else { '不存在' })) `
      -Remediation "正常构建模式下重新执行打包以重新生成导出；--no-build 模式下请先执行一次正常构建。不要手工复制 DLL 到 bin 或稳定导出路径")
  }

  $declaredReceipt = $null
  try {
    $declaredReceipt = Get-Content -Raw -LiteralPath $receiptPath -Encoding UTF8 | ConvertFrom-Json
  } catch {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-INVALID' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @($receiptPath) -Detail "导出记录无法解析：$($_.Exception.Message)" `
      -Remediation "删除不可解析的记录后执行正常构建；不要手工编辑收据")
  }

  if ([int]$declaredReceipt.schema -ne $script:LoaderReceiptSchema) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-SCHEMA' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (schema=$($declaredReceipt.schema))") -Detail "导出记录 schema=$($declaredReceipt.schema) 与本流程要求的 $($script:LoaderReceiptSchema) 不一致" `
      -Remediation "执行正常构建以重新生成当前 schema 的记录（schema 3 起代次由槽位指针与不可变代次档案共同固定）")
  }
  if ([string]$declaredReceipt.artifact_id -ne $artifactId) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (artifact_id=$($declaredReceipt.artifact_id))") `
      -Detail '导出记录不属于该 artifact' -Remediation '执行正常构建以重新生成记录')
  }
  if ([string]$declaredReceipt.file_identity.package_target -ne $target) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (package_target=$($declaredReceipt.file_identity.package_target))") `
      -Detail '导出记录的包内目标与 manifest 不一致' -Remediation '执行正常构建以重新生成记录')
  }
  if ([string]$declaredReceipt.build_identity.profile -ne $profile) {
    throw (New-LoaderFailureException `
      -Category 'PROFILE-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (profile=$($declaredReceipt.build_identity.profile))") `
      -Detail '导出记录属于其它构建配置' `
      -Remediation "用与发布一致的 -Configuration 重新构建并打包")
  }
  # 代次完整性：收据是"本代次完成"的唯一标记，必须自带发布代次并指向同一个共享输出槽位。
  if (-not $declaredReceipt.publication -or [string]$declaredReceipt.publication.state -ne 'complete') {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-INCOMPLETE' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (publication.state=$($declaredReceipt.publication.state), generation=$($declaredReceipt.publication.generation))") `
      -Detail '导出记录没有标记为完整代次（缺少 publication 块或 state 不是 complete）：无法证明产物与收据属于同一代次' `
      -Remediation '执行正常构建重新导出；如果这是中断留下的产物，它属于不完整代次，必须重新发布而不是直接采用')
  }
  if ([string]$declaredReceipt.publication.slot_key -ne $Slot.key) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-SLOT-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$receiptPath (slot_key=$($declaredReceipt.publication.slot_key))", "current_slot=$($Slot.key)", $Slot.identity) `
      -Detail '导出记录属于另一个共享输出槽位（不同的规范输出目录 / 目标文件 / profile）' `
      -Remediation '按当前 manifest 的输出槽位重新构建导出；不要把别处的稳定导出复制过来')
  }

  # ---- 槽位指针：唯一指定当前有效代次 ----
  $pointer = Read-LoaderSlotPointer -Slot $Slot
  if (-not $pointer) {
    throw (New-LoaderFailureException `
      -Category 'POINTER-MISSING' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @([string]$Slot.pointer_path, "receipt_generation=$($declaredReceipt.publication.generation)") `
      -Detail '共享输出槽位没有"当前有效代次"指针：无法确定本收据是否仍是该槽位的当前有效代次（该状态属于不完整发布，不得直接采用）' `
      -Remediation '执行正常构建重新发布该槽位；不要在缺少指针时把声明收据当成当前有效代次')
  }
  if ([string]$pointer.slot_key -ne $Slot.key) {
    throw (New-LoaderFailureException `
      -Category 'POINTER-SLOT-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$($Slot.pointer_path) (slot_key=$($pointer.slot_key))", "current_slot=$($Slot.key)") `
      -Detail '槽位指针属于另一个共享输出槽位' `
      -Remediation '排查槽位目录是否被人工搬运/复制；用正常构建重新发布')
  }
  if ([string]$pointer.artifact_id -ne $artifactId) {
    throw (New-LoaderFailureException `
      -Category 'POINTER-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$($Slot.pointer_path) (artifact_id=$($pointer.artifact_id))") `
      -Detail '槽位指针登记的 artifact 与本次要消费的不一致' `
      -Remediation '排查槽位目录是否被人工搬运/复制；用正常构建重新发布')
  }
  $declaredReceiptFull = [System.IO.Path]::GetFullPath($receiptPath)
  $pointerReceiptFull = ''
  if ($pointer.declared_receipt_path) { $pointerReceiptFull = [System.IO.Path]::GetFullPath([string]$pointer.declared_receipt_path) }
  if ($pointerReceiptFull -and -not $pointerReceiptFull.Equals($declaredReceiptFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw (New-LoaderFailureException `
      -Category 'POINTER-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$($Slot.pointer_path) (declared_receipt_path=$pointerReceiptFull)", "manifest_receipt_path=$declaredReceiptFull") `
      -Detail '槽位指针登记的声明收据路径与本次 manifest 声明的路径不一致：无法证明本收据就是指针指向的凭据' `
      -Remediation '确认 manifest 的 export.receipt 未被改动；用正常构建重新发布')
  }

  $generation = [string]$pointer.generation
  if (-not $generation) {
    throw (New-LoaderFailureException `
      -Category 'POINTER-INVALID' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @([string]$Slot.pointer_path) `
      -Detail '槽位指针没有登记 generation：无法取得明确代次' `
      -Remediation '执行正常构建重新发布该槽位（不要手工编辑指针）')
  }
  if ([string]$declaredReceipt.publication.generation -ne $generation) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-GENERATION-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @(
        "receipt_generation=$($declaredReceipt.publication.generation)",
        "pointer_generation=$generation",
        [string]$Slot.pointer_path
      ) `
      -Detail '声明收据不是当前有效代次：产物与收据的提交之间可能发生过中断或并发替换（失败竞争者的收据不得进入共享有效槽位）' `
      -Remediation '执行正常构建重新导出该槽位；不要手工把旧代次收据配给新一代产物，也不要接受失败方写入的收据')
  }

  # ---- 不可变代次档案：与指针、声明收据三方自洽 ----
  $generationRecordPath = Resolve-LoaderGenerationRecordPath -Slot $Slot -Generation $generation
  if (-not (Test-Path -LiteralPath $generationRecordPath -PathType Leaf)) {
    throw (New-LoaderFailureException `
      -Category 'GENERATION-RECORD-MISSING' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @($generationRecordPath, "pointer_generation=$generation") `
      -Detail '当前有效代次在代次档案里没有收据副本：无法追溯核对这一代次（代次收据必须可追溯保留）' `
      -Remediation '执行正常构建重新发布；不要删除代次档案里的收据')
  }
  $generationRecord = $null
  try { $generationRecord = Get-Content -Raw -LiteralPath $generationRecordPath -Encoding UTF8 | ConvertFrom-Json } catch { $generationRecord = $null }
  if (-not $generationRecord) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-INVALID' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @($generationRecordPath) -Detail '代次档案里的收据无法解析' `
      -Remediation '不要手工编辑代次档案；执行正常构建重新发布')
  }
  $recordDigest = Get-LoaderReceiptDigest -Receipt $generationRecord
  if ($pointer.receipt_digest -and [string]$pointer.receipt_digest -ne [string]$recordDigest) {
    throw (New-LoaderFailureException `
      -Category 'GENERATION-CONFLICT' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @(
        "generation_record=$generationRecordPath (receipt_digest=$recordDigest)",
        "pointer_receipt_digest=$($pointer.receipt_digest)",
        "generation=$generation"
      ) `
      -Detail '同一 generation 的代次档案内容与指针登记的 receipt_digest 不一致（同代次对应了不同收据内容）' `
      -Remediation '不要覆盖/编辑代次档案或指针；排查是谁改写了共享输出槽位后用正常构建重新发布')
  }
  $declaredDigest = Get-LoaderReceiptDigest -Receipt $declaredReceipt
  if ([string]$generationRecord.publication.artifact_digest -ne [string]$pointer.artifact_digest) {
    throw (New-LoaderFailureException `
      -Category 'GENERATION-CONFLICT' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @(
        "generation_record_artifact_digest=$($generationRecord.publication.artifact_digest)",
        "pointer_artifact_digest=$($pointer.artifact_digest)"
      ) `
      -Detail '代次档案与指针登记的产物内容身份不一致（同一代次对应了不同产物摘要）' `
      -Remediation '排查共享输出槽位是否被改写；用正常构建重新发布')
  }

  # ---- 产物内容身份：必须等于该代次登记的 artifact_digest ----
  $stableIdentity = Get-LoaderFileIdentity $stablePath
  if (-not $stableIdentity) {
    throw (New-LoaderFailureException `
      -Category 'SOURCE-MISSING' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @($stablePath) -Detail '稳定导出文件不存在' `
      -Remediation "执行正常构建以重新导出；不要手工补文件")
  }
  $expectedArtifactDigest = [string]$declaredReceipt.publication.artifact_digest
  $expectedArtifactLength = [long]$declaredReceipt.publication.artifact_length
  if (-not $expectedArtifactDigest -or -not $declaredReceipt.publication.PSObject.Properties['artifact_length']) {
    throw (New-LoaderFailureException `
      -Category 'RECEIPT-INCOMPLETE' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @($receiptPath) `
      -Detail '导出记录没有登记产物内容身份（artifact_digest/artifact_length）：无法证明产物属于该代次' `
      -Remediation '执行正常构建重新导出；不要沿用缺少代次产物身份的旧记录')
  }
  if (
    $stableIdentity.length -ne $expectedArtifactLength -or
    $stableIdentity.sha256 -ne $expectedArtifactDigest
  ) {
    throw (New-LoaderFailureException `
      -Category 'CONTENT-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @(
        "$stablePath (actual=$($stableIdentity.length)/$($stableIdentity.sha256))",
        "generation=$generation (artifact=$expectedArtifactLength/$expectedArtifactDigest)"
      ) `
      -Detail '稳定导出文件内容与当前有效代次登记的产物身份不一致：incomplete_generation_or_tampered=true（产物已替换但收据未落地的"不完整代次"，或产物被手工替换／并发覆盖）。两种情形都必须拒绝，且不得把它当作上一成功版本' `
      -Remediation '执行正常构建以重新导出；如果是并发发布者留下的中间状态，先确认没有第二个打包者在写同一输出槽位。不要手工替换稳定导出文件，也不要把上一代收据配给这一代产物')
  }

  $expectedMachine = Get-LoaderArchitectureMachine ([string]$export.architecture)
  $actualMachine = Get-LoaderPeMachine $stablePath
  if ($actualMachine -ne $expectedMachine) {
    throw (New-LoaderFailureException `
      -Category 'ARCH-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @("$stablePath (machine=$actualMachine)") `
      -Detail "稳定导出文件架构不匹配：expected=$expectedMachine actual=$actualMachine" `
      -Remediation '执行正常构建以重新导出正确架构的 Loader')
  }

  if ($VerifyIdentity) {
    # 与"本次要打包的源码 / 配置 / 版本"一致性（--no-build 的接受条件）。
    $contractHash = Get-LoaderArtifactContractHash -Artifact $artifact
    if ([string]$declaredReceipt.artifact_contract_sha256 -ne $contractHash) {
      throw (New-LoaderFailureException `
        -Category 'RECEIPT-STALE' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @("$receiptPath (artifact_contract_sha256=$($declaredReceipt.artifact_contract_sha256))") `
        -Detail "manifest 中该 artifact 的声明（source/target/build/export）与导出记录不一致" `
        -Remediation '执行正常构建以重新生成记录；如果是刚修改了声明，必须重新构建')
    }

    $sourceIdentity = Get-LoaderSourceIdentity -RepoPath $repoPath -Spec $export.source_identity -Profile $profile
    if ([string]$declaredReceipt.build_identity.source_identity_sha256 -ne $sourceIdentity.tree_sha256) {
      throw (New-LoaderFailureException `
        -Category 'RECEIPT-STALE' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @("$receiptPath (source_identity=$($declaredReceipt.build_identity.source_identity_sha256))", "current=$($sourceIdentity.tree_sha256)") `
        -Detail '构建入口的源码/配置哈希与导出记录不一致（记录早于当前源码）' `
        -Remediation '执行正常构建（不要用 --no-build）重新生成导出与记录')
    }

    foreach ($pair in @(
        @{ name = 'lockfile'; actual = $Context.LockfileSha256; recorded = [string]$declaredReceipt.build_identity.lockfile_sha256 },
        @{ name = 'build_entry_manifest'; actual = $Context.BuildEntryManifestSha256; recorded = [string]$declaredReceipt.build_identity.build_entry_manifest_sha256 },
        @{ name = 'build_target'; actual = [string]$Context.BuildTarget; recorded = [string]$declaredReceipt.build_identity.build_target },
        @{ name = 'cargo_target_dir'; actual = [string]$Context.CargoTargetDir; recorded = [string]$declaredReceipt.build_identity.cargo_target_dir }
      )) {
      if ($pair.actual -ne $pair.recorded) {
        throw (New-LoaderFailureException `
          -Category 'RECEIPT-STALE' -ArtifactId $artifactId -Target $target -Profile $profile `
          -Candidates @("$receiptPath ($($pair.name)=$($pair.recorded))", "current=$($pair.actual)") `
          -Detail "导出记录的 $($pair.name) 与本次发布输入不一致" `
          -Remediation '执行正常构建（不要用 --no-build）重新生成导出与记录')
      }
    }

    $currentReleaseVersion = [string]$Context.ReleaseVersion
    $recordedReleaseVersion = [string]$declaredReceipt.build_identity.release_version
    if ($currentReleaseVersion -and $recordedReleaseVersion -and $currentReleaseVersion -ne $recordedReleaseVersion) {
      throw (New-LoaderFailureException `
        -Category 'RECEIPT-STALE' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @("$receiptPath (release_version=$recordedReleaseVersion)", "current=$currentReleaseVersion") `
        -Detail '导出记录的发布版本与本次要打包的版本不一致' `
        -Remediation '执行正常构建（不要用 --no-build）重新生成导出与记录')
    }
    if ($currentReleaseVersion -and -not $recordedReleaseVersion) {
      throw (New-LoaderFailureException `
        -Category 'RECEIPT-INCOMPLETE' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @($receiptPath) `
        -Detail '导出记录没有记录发布版本，无法与本次版本核对' `
        -Remediation '执行正常构建（不要用 --no-build）重新生成导出与记录')
    }
  }

  # 最后一关：声明收据与代次档案必须**逐字同代次同内容**。
  # （放在身份校验之后，使"内容陈旧"先以 RECEIPT-STALE 呈现；非身份类改写在这里判为冲突。）
  if ([string]$declaredDigest -ne [string]$recordDigest) {
    throw (New-LoaderFailureException `
      -Category 'GENERATION-CONFLICT' -ArtifactId $artifactId -Target $target -Profile $profile `
      -Candidates @(
        "declared_receipt=$receiptPath (receipt_digest=$declaredDigest)",
        "generation_record=$generationRecordPath (receipt_digest=$recordDigest)",
        "generation=$generation"
      ) `
      -Detail '同一 generation 对应了两份不同内容的收据（声明收据与代次档案不一致）：代次是不可变发布标识，判为冲突' `
      -Remediation '不要改写声明收据或代次档案；排查是谁向共享有效槽位发布了收据后重新发布一个新代次')
  }

  return [pscustomobject]@{
    receipt = $declaredReceipt
    stable_identity = $stableIdentity
    generation = $generation
    receipt_digest = $declaredDigest
    pointer = $pointer
    pointer_path = $Slot.pointer_path
    generation_record = $generationRecord
    generation_record_path = $generationRecordPath
    generation_record_relative = (ConvertTo-LoaderSlotRelativePath -SlotDir $Slot.slot_dir -Path $generationRecordPath)
  }
}

<#
  发布资格校验**入口**（消费端一致性边界）。

  读侧与写侧遵守同一条边界：只有先取得该输出槽位的读取权，才去核对"产物 + 收据"。
  发布者正在连续替换凭据时，消费端不会被插入读取；如果发布者持有发布权，本调用等到
  超时并给出明确的 EXPORT-SLOT-BUSY（fail-closed），而不是读到一个"看起来存在"的中间状态。

  Slot：导出端已在同一槽位持有发布权时传入（保证后续读与刚才的写在同一槽位上判定）。
#>
function Assert-LoaderExportReceipt {
  param(
    [Parameter(Mandatory = $true)][hashtable]$Context,
    [switch]$VerifyIdentity,
    [object]$Slot = $null
  )

  $artifact = $Context.Artifact
  if (-not $Slot) {
    $Slot = Get-LoaderPublishSlot `
      -Destination ([string]$Context.StablePath) `
      -RepoPath ([string]$Context.RepoPath) `
      -Target ([string]$artifact.target) `
      -Profile ([string]$Context.Profile)
  }
  $lock = Enter-LoaderPublishSlot `
    -Slot $Slot `
    -Purpose 'consume' `
    -ArtifactId ([string]$artifact.id) `
    -Target ([string]$artifact.target) `
    -Profile ([string]$Context.Profile)
  try {
    return (Assert-LoaderExportReceiptUnderLock -Context $Context -VerifyIdentity:$VerifyIdentity -Slot $Slot)
  } finally {
    Exit-LoaderPublishSlot -Lock $lock
  }
}

# 兼容入口：调用方必须已持有该槽位的发布/读取权（见 Assert-LoaderExportReceipt）。
function Assert-LoaderExportReceiptUnderLock {
  param(
    [Parameter(Mandatory = $true)][hashtable]$Context,
    [switch]$VerifyIdentity,
    [Parameter(Mandatory = $true)][object]$Slot
  )

  $resolved = Resolve-LoaderGenerationUnderLock -Context $Context -VerifyIdentity:$VerifyIdentity -Slot $Slot
  return [pscustomobject]@{
    receipt = $resolved.receipt
    stable_identity = $resolved.stable_identity
    generation = $resolved.generation
    receipt_digest = $resolved.receipt_digest
    pointer = $resolved.pointer
    generation_record_path = $resolved.generation_record_path
  }
}

<#
  消费端**固定流程**（裁决 §3.2）：取得读取资格 → 读取明确 generation → 核对收据与产物
  → 复制到本次打包的**独立 staging** → 核对复制内容 → 固定本次消费收据 → 释放读取权。

  返回的消费收据自带 generation / artifact_digest / receipt_digest / build_input_digest /
  consumer_run_id 与 staging 内容身份，**报告只允许从这份固定收据生成**：
  此后即使槽位合法更新到新一代次，本次报告的字段也不会漂移（也不得重新读共享槽位）。

  ConsumeStagingDirectory 必须是**本次打包自己的**目录（不得是共享槽位目录本身），
  否则"复制到独立 staging"这一条就没有意义。
#>
function Use-LoaderExportGeneration {
  param(
    [Parameter(Mandatory = $true)][hashtable]$Context,
    [Parameter(Mandatory = $true)][string]$ConsumerRunId,
    [Parameter(Mandatory = $true)][string]$ConsumeStagingDirectory,
    [switch]$VerifyIdentity,
    [object]$Slot = $null,
    [string]$StagedFileName
  )

  $artifact = $Context.Artifact
  $artifactId = [string]$artifact.id
  $target = [string]$artifact.target
  $profile = [string]$Context.Profile
  $repoPath = [string]$Context.RepoPath
  $stablePath = [string]$Context.StablePath
  if (-not $ConsumerRunId) {
    throw [System.InvalidOperationException]::new("consume requires an explicit consumer_run_id (artifact=$artifactId)")
  }
  if (-not $ConsumeStagingDirectory) {
    throw [System.InvalidOperationException]::new("consume requires an independent staging directory (artifact=$artifactId)")
  }

  if (-not $Slot) {
    $Slot = Get-LoaderPublishSlot `
      -Destination $stablePath `
      -RepoPath $repoPath `
      -Target $target `
      -Profile $profile
  }
  $stagingFull = [System.IO.Path]::GetFullPath($ConsumeStagingDirectory).TrimEnd('\', '/')
  $slotPrefix = $Slot.slot_dir.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
  if ($stagingFull.StartsWith($slotPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or $stagingFull.Equals($Slot.slot_dir.TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)) {
    throw [System.InvalidOperationException]::new("consume staging must live outside the shared slot: $stagingFull (slot=$($Slot.slot_dir))")
  }

  # ① 取得读取资格（与写侧同一把排他权）
  $lock = Enter-LoaderPublishSlot `
    -Slot $Slot `
    -Purpose 'consume' `
    -ArtifactId $artifactId `
    -Target $target `
    -Profile $profile
  try {
    # ② 读取明确 generation（来自槽位指针）+ 校对三类凭据与产物
    $resolved = Resolve-LoaderGenerationUnderLock -Context $Context -VerifyIdentity:$VerifyIdentity -Slot $Slot
    $generation = [string]$resolved.generation
    $artifactDigest = [string]$resolved.receipt.publication.artifact_digest
    $artifactLength = [long]$resolved.receipt.publication.artifact_length

    # ③ 复制到本次打包的独立 staging
    $artifactStagingDir = Join-Path $stagingFull $artifactId
    New-Item -ItemType Directory -Force -Path $artifactStagingDir | Out-Null
    $stagedName = $(if ($StagedFileName) { $StagedFileName } else { [System.IO.Path]::GetFileName($stablePath) })
    $stagedPath = Join-Path $artifactStagingDir $stagedName
    Copy-Item -LiteralPath $stablePath -Destination $stagedPath -Force

    # ④ 核对复制内容（staging 字节必须仍是该代次登记的产物身份）
    $stagedIdentity = Get-LoaderFileIdentity $stagedPath
    if (-not $stagedIdentity) {
      throw (New-LoaderFailureException `
        -Category 'CONSUMER-STAGING-MISSING' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @($stagedPath) -Detail '复制到本次独立 staging 的产物无法读取身份' `
        -Remediation '检查磁盘/权限后重试；不要在无法核对 staged 内容的情况下继续打包')
    }
    if ($stagedIdentity.sha256 -ne $artifactDigest -or $stagedIdentity.length -ne $artifactLength) {
      throw (New-LoaderFailureException `
        -Category 'CONSUMER-STAGING-MISMATCH' -ArtifactId $artifactId -Target $target -Profile $profile `
        -Candidates @(
          "staged=$stagedPath ($($stagedIdentity.length)/$($stagedIdentity.sha256))",
          "generation=$generation (artifact=$artifactLength/$artifactDigest)"
        ) `
        -Detail '本次独立 staging 的内容与所消费代次登记的产物身份不一致' `
        -Remediation '重试消费；若重复出现，检查共享输出槽位与磁盘是否被并发写入')
    }

    # ⑤ 固定本次消费收据：此后报告只依赖本对象，不再读取共享槽位。
    $fixedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    $stagedRelative = $null
    $repoPrefixLocal = $repoPath.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    if ($stagedIdentity.path.StartsWith($repoPrefixLocal, [System.StringComparison]::OrdinalIgnoreCase)) {
      $stagedRelative = $stagedIdentity.path.Substring($repoPrefixLocal.Length).Replace('\', '/')
    }
    $slotDirRelative = $null
    if ($Slot.slot_dir.StartsWith($repoPrefixLocal, [System.StringComparison]::OrdinalIgnoreCase)) {
      $slotDirRelative = $Slot.slot_dir.Substring($repoPrefixLocal.Length).Replace('\', '/')
    }
    return [pscustomobject]@{
      schema = 1
      kind = 'webview2-loader-generation-consumption'
      artifact_id = $artifactId
      target = $target
      profile = $profile
      slot_key = $Slot.key
      slot_identity = $Slot.identity
      slot_dir = $Slot.slot_dir
      slot_dir_relative = $slotDirRelative
      destination = $Slot.destination
      generation = $generation
      producer_run_id = [string]$resolved.receipt.publication.producer_run_id
      consumer_run_id = $ConsumerRunId
      artifact_digest = $artifactDigest
      artifact_length = $artifactLength
      receipt_digest = [string]$resolved.receipt_digest
      build_input_digest = [string]$resolved.receipt.publication.build_input_digest
      receipt_path = [System.IO.Path]::GetFullPath([string]$Context.ReceiptPath)
      generation_record = $resolved.generation_record_relative
      generation_record_path = $resolved.generation_record_path
      pointer_path = $Slot.pointer_path
      pointer_generation_at_consumption = [string]$resolved.pointer.generation
      pointer_committed_utc = [string]$resolved.pointer.committed_utc
      supersedes_generation = [string]$resolved.pointer.supersedes_generation
      consumed_staging_path = $stagedRelative
      consumed_staging_absolute = $stagedIdentity.path
      consumed_staging_sha256 = $stagedIdentity.sha256
      consumed_staging_length = $stagedIdentity.length
      consumed_content_matches_generation = ($stagedIdentity.sha256 -eq $artifactDigest)
      consumption_verified = $true
      consumption_source = 'pointer+generation-record+declared-receipt three-way verified, then copied to run-private staging'
      read_right = 'held from generation resolution until the staging copy was verified'
      fixed_at_utc = $fixedAtUtc
      report_generation_rule = '包报告只从本固定收据生成：报告生成阶段不得重新读取共享槽位的最新收据；槽位后续合法更新到新一代次不影响本报告引用的代次'
      receipt = $resolved.receipt
      stable_identity = $resolved.stable_identity
    }
  } finally {
    Exit-LoaderPublishSlot -Lock $lock
  }
}

