# ============================================================================
# 构建身份分离与报告治理（RD4-06 / 第五轮裁决 B-1、B-2）
#
# 本文件是"三个分开的身份"的唯一实现点（package-all.ps1 与 build-msi.ps1
# 只负责调用与落盘，不在别处重复实现哈希口径）：
#
#   1) source_snapshot_digest  本次使用了哪份第一方源码与构建资源快照？
#        = H( scope_descriptor || file_set_digest )
#        scope = manifest 声明的 roots + 排除/放行规则（边界可审计）
#        file_set = 本次枚举到的全部文件 path|length|sha256（含新增/删除）
#   2) build_input_digest      该快照配合什么锁文件、工具链、target、profile、
#                              features 与构建配置？—— 描述符列表的规范哈希
#   3) payload_digest          最终生成/分发了哪些二进制与资源？
#                              —— 暂存包根目录的文件清单规范哈希
#
# 三个身份**互不冒充**：
#   * 声明文件哈希集合（webview2-loader 收据里的 source_identity_sha256）
#     只是 tauri 构建入口这一子范围，**不是**全量源码快照；
#   * 载荷哈希（payload_digest）**不是**源码身份；
#   * source_commit 在当前工作树**不是**源码权威（可为空）。
#
# 并发编辑：不使用"首尾各哈希一次"的假一致。见 New-SourceSnapshot 的两遍
# 静默枚举与 Assert-SourceSnapshotComparable 的冻结记录机制。
#
# 术语（裁决纠正）：
#   git ls-files 默认列举**索引中的已跟踪文件**；结果为 0 不能独立证明没有 .git。
#   准确口径是"当前源码没有被该提交有效覆盖" ⇒ vcs_state = untracked_snapshot。
# ============================================================================

$script:BuildIdentitySchema = 1
$script:IdentityHashAlgorithm = 'SHA256'

<#
  默认排除规则（单一事实来源）。
  设计依据：
    * 与 .gitignore / package-safety 的"禁止进入发布物"分类一致；
    * `.coolzhu/` 是**运行态**（用户会话、sqlite、插件运行时），只有
      `.coolzhu/plugins/`（workspace 成员 crate）由 allow_path_patterns 放行；
    * 模型权重、用户配置、凭据、运行数据库、无关历史 target 一律不得进入快照；
    * `gen/` 是 tauri-build 每次构建都会重写的生成目录，纳入快照会让身份自我失效。
  注意：`bin/` **不排除**——Rust 的 `src/bin` 是第一方源码目录。
#>
$script:SourceSnapshotDefaultExcludePatterns = @(
  # ---- 版本控制 / 工具状态 ----
  '(^|/)\.git(/|$)'
  '(^|/)\.claude(/|$)'
  '(^|/)\.superpowers(/|$)'
  '(^|/)\.claw-agents(/|$)'
  '(^|/)\.playwright-cli(/|$)'
  '(^|/)\.sandbox-home(/|$)'
  '(^|/)\.sandbox-tmp(/|$)'
  # 运行态目录（用户会话 / sqlite / 插件运行时状态）；.coolzhu/plugins 由放行规则保留
  '(^|/)\.coolzhu(/|$)'
  # ---- 构建与分发产物 ----
  '(^|/)target(/|$)'
  '(^|/)node_modules(/|$)'
  '(^|/)dist(/|$)'
  '(^|/)tmp(/|$)'
  '(^|/)logs?(/|$)'
  '(^|/)sessions?(/|$)'
  '(^|/)backups?(?:-[^/]+)?(/|$)'
  '(^|/)__pycache__(/|$)'
  '(^|/)test-results(/|$)'
  '(^|/)goal-artifacts(/|$)'
  # tauri-build 生成：每次构建重写，纳入快照会让身份自我失效（见 §3.3 决策记录）
  '(^|/)gen(/|$)'
  # ---- 用户配置 / 凭据 / 运行数据库 ----
  'web-sessions'
  '(^|/)\.env($|\.)'
  '\.(sqlite3?|db)(-(wal|shm|journal))?$'
  'coolzhu\.toml$'
  '(^|/)(credentials?|secrets?|token-cache|credential-cache|access-token|refresh-token|session-token|auth-token)(\.[^/]+)?$'
  '\.(pem|key|pfx|p12)$'
  # ---- 模型权重 / 第三方二进制 ----
  '\.(gguf|onnx|safetensors|pth|pt|ckpt|mlmodel|tflite)$'
  '\.(bin|dll|so|dylib|exe|msi|zip|7z|pdb|lib|obj|class|jar)$'
  # ---- 日志与编辑器临时文件 ----
  '\.(log|bak|old|orig|rej|pyc|pyo|swp)$'
  '~$'
  '(^|/)\.DS_Store$'
  '(^|/)Thumbs\.db$'
)

<#
  "已知第一方源码/资源扩展名"：**只用于可见性报告**（unclassified_files），
  不参与纳入判定。纳入判定是**排除白名单之外的一切**——这样"快照边界"是可审计的
  （可以断言"声明 roots 下除排除项外都被覆盖"），而不是靠一份可能漏项的扩展名清单。
#>
$script:SourceSnapshotDefaultKnownExtensions = @(
  '.rs', '.cs', '.csproj', '.sln', '.props', '.targets', '.manifest',
  '.js', '.mjs', '.cjs', '.ts', '.tsx', '.jsx', '.html', '.htm', '.css', '.scss',
  '.json', '.toml', '.lock', '.yaml', '.yml', '.ini', '.conf', '.config',
  '.md', '.txt', '.py', '.ps1', '.psm1', '.psd1', '.sh', '.cmd', '.bat',
  '.wxs', '.wxi', '.xml', '.vcxproj', '.rc',
  '.png', '.svg', '.ico', '.gif', '.webp', '.jpg', '.jpeg', '.bmp',
  '.wav', '.mp3', '.webm', '.mp4', '.ogg', '.woff', '.woff2', '.ttf',
  '.gitignore', '.gitattributes', '.editorconfig'
)

# 载荷清单载体自身不能进入 payload_digest（自引用会把摘要变成不可验证的固定点）。
$script:PayloadInventoryCarrierName = 'payload-inventory.json'
# 源码快照冻结记录目录（相对仓库根）。
$script:SourceSnapshotFreezeRoot = 'tmp/source-snapshots'
# 报告保留索引与 latest 指针所在目录（相对仓库根）。
$script:PackageReportRoot = 'tmp/package-reports'

function New-IdentityFailureText {
  param(
    [Parameter(Mandatory = $true)][string]$Category,
    [Parameter(Mandatory = $true)][string]$Detail,
    [string]$Remediation
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("[$Category]")
  $lines.Add("detail: $Detail")
  if ($Remediation) {
    $lines.Add("next: $Remediation")
  }
  return ($lines -join "`n")
}

function New-IdentityFailureException {
  param(
    [Parameter(Mandatory = $true)][string]$Category,
    [Parameter(Mandatory = $true)][string]$Detail,
    [string]$Remediation
  )

  return [System.InvalidOperationException]::new(
    (New-IdentityFailureText -Category $Category -Detail $Detail -Remediation $Remediation))
}

<#
  规范哈希输入：**排序后的行**，LF 连接，末尾一个 LF，UTF-8 无 BOM。
  排序使用 Ordinal 比较（不是 Sort-Object 的区域敏感比较），否则同一份内容在
  不同 locale 下会算出不同摘要。
#>
function Get-IdentityCanonicalLines {
  param([AllowEmptyCollection()][AllowNull()][string[]]$Lines = @())

  $copy = [string[]]@($Lines | Where-Object { $_ -ne $null })
  [array]::Sort($copy, [System.StringComparer]::Ordinal)
  return $copy
}

function Get-IdentityHashFromLines {
  param([AllowEmptyCollection()][AllowNull()][string[]]$Lines = @())

  $canonical = Get-IdentityCanonicalLines -Lines $Lines
  $text = (($canonical -join "`n") + "`n")
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-IdentityHashFromBytes {
  param([Parameter(Mandatory = $true)][byte[]]$Bytes)

  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $sha.Dispose()
  }
}

function Get-IdentityFileHash {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function ConvertTo-IdentityPosixPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  return ([string]$Path).Replace('\', '/')
}

function Get-IdentityRepoRelativePath {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Path
  )

  $repoFull = [System.IO.Path]::GetFullPath($RepoPath).TrimEnd('\', '/')
  $prefix = $repoFull + [System.IO.Path]::DirectorySeparatorChar
  $target = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  if ($target.Equals($repoFull, [System.StringComparison]::OrdinalIgnoreCase)) { return '.' }
  if (-not $target.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "path must stay inside the workspace: $target"
  }
  return (ConvertTo-IdentityPosixPath $target.Substring($prefix.Length))
}

function Test-IdentityPatternMatch {
  param(
    [AllowEmptyString()][string]$RelativePath,
    [AllowEmptyCollection()][string[]]$Patterns = @()
  )

  foreach ($pattern in @($Patterns)) {
    if (-not $pattern) { continue }
    if ($RelativePath -match $pattern) { return $true }
  }
  return $false
}

<#
  VCS 参考状态（裁决 B-1 第 1、2、6 条）。

  * **不改变** Git 状态：只读 rev-parse / ls-files / status，不执行 init/add/commit/tag。
  * HEAD 只登记为 vcs_reference_commit（种子参考），**不是** source_commit 权威。
  * 索引为空 ⇒ vcs_state = untracked_snapshot，dirty_against_commit = not_evaluable。
    绝不把"没有被跟踪的变更"写成 dirty = false。
#>
function Get-VcsReferenceState {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  $state = [ordered]@{
    vcs_state = 'no_vcs'
    vcs_reference_commit = $null
    vcs_reference_commit_kind = 'seed-reference-only'
    source_commit = $null
    source_commit_authority = 'not-authoritative'
    dirty_against_commit = 'not_evaluable'
    dirty_against_commit_note = '当前工作树没有被该提交有效覆盖，无法按提交号判定"是否含未提交修改"'
    tracked_index_file_count = 0
    git_available = $false
    evidence = $null
  }

  try {
    $headOutput = @(& git -C $RepoPath rev-parse --verify HEAD 2>$null)
    $headExit = $LASTEXITCODE
  } catch {
    $headOutput = @()
    $headExit = 1
  }
  if ($headExit -ne 0 -or @($headOutput).Count -eq 0) {
    $state.evidence = 'git rev-parse --verify HEAD 未返回提交（或 git 不可用）'
    return [pscustomobject]$state
  }

  $candidate = ([string]($headOutput | Select-Object -First 1)).Trim()
  if ($candidate -notmatch '^[0-9a-fA-F]{40}$') {
    $state.evidence = "git rev-parse HEAD 返回的不是 40 位提交号：$candidate"
    return [pscustomobject]$state
  }
  $state.git_available = $true
  $state.vcs_reference_commit = $candidate.ToLowerInvariant()

  try {
    # 只读列举**索引中的已跟踪文件**；结果为 0 仅表示"当前源码没有被该提交有效覆盖"。
    $tracked = @(& git -C $RepoPath ls-files 2>$null)
    if ($LASTEXITCODE -eq 0) {
      $state.tracked_index_file_count = @($tracked | Where-Object { $_ }).Count
    }
  } catch {
    # 保持 0；下面的 untracked_snapshot 分支是 fail-honest 的保守取值。
  }

  if ($state.tracked_index_file_count -eq 0) {
    $state.vcs_state = 'untracked_snapshot'
    $state.evidence = ('git rev-parse HEAD = {0}（种子参考提交）；git ls-files 索引已跟踪文件数 = 0 ⇒ 当前源码没有被该提交有效覆盖' -f $state.vcs_reference_commit)
    return [pscustomobject]$state
  }

  $state.vcs_state = 'tracked_worktree'
  try {
    $porcelain = @(& git -C $RepoPath status --porcelain 2>$null)
    if ($LASTEXITCODE -eq 0) {
      $changed = @($porcelain | Where-Object { $_ }).Count
      $state.dirty_against_commit = ($changed -gt 0)
      $state.dirty_against_commit_note = ('git status --porcelain 变更行数 = {0}' -f $changed)
      $state.source_commit = $state.vcs_reference_commit
      $state.source_commit_authority = 'tracked-commit'
      $state.evidence = ('索引已跟踪 {0} 个文件；工作树相对 {1} 的变更行数 = {2}' -f $state.tracked_index_file_count, $state.vcs_reference_commit, $changed)
    }
  } catch {
    $state.dirty_against_commit = 'not_evaluable'
    $state.dirty_against_commit_note = 'git status 执行失败，无法判定工作树是否与提交一致'
  }
  return [pscustomobject]$state
}

<#
  解析 manifest 的 source_snapshot 声明。**未声明则返回 $null**（不计算、不冒充）：
  测试夹具（无该声明）因此不会被强制纳入源码快照语义。
#>
function Get-SourceSnapshotScope {
  param(
    [Parameter(Mandatory = $true)][object]$ManifestData,
    [AllowEmptyCollection()][string[]]$ExtraExcludePatterns = @()
  )

  if (-not $ManifestData.PSObject.Properties['source_snapshot'] -or -not $ManifestData.source_snapshot) {
    return $null
  }
  $spec = $ManifestData.source_snapshot
  $roots = @($spec.roots | Where-Object { $_ } | ForEach-Object { (ConvertTo-IdentityPosixPath ([string]$_)).TrimEnd('/') })
  if ($roots.Count -eq 0) {
    throw (New-IdentityFailureException -Category 'SOURCE-SNAPSHOT-SCOPE-INVALID' `
      -Detail 'package manifest 声明了 source_snapshot 但没有 roots' `
      -Remediation '在 config/package-manifest.json 的 source_snapshot.roots 列出第一方源码根')
  }

  $excludes = [System.Collections.Generic.List[string]]::new()
  foreach ($pattern in @($script:SourceSnapshotDefaultExcludePatterns)) { $excludes.Add([string]$pattern) }
  foreach ($pattern in @($spec.extra_exclude_path_patterns)) { if ($pattern) { $excludes.Add([string]$pattern) } }
  foreach ($pattern in @($ExtraExcludePatterns)) { if ($pattern) { $excludes.Add([string]$pattern) } }

  $allows = @($spec.allow_path_patterns | Where-Object { $_ } | ForEach-Object { [string]$_ })
  $allowReparse = @($spec.allow_reparse_points | Where-Object { $_ } | ForEach-Object { [string]$_ })
  $known = @($spec.known_source_extensions | Where-Object { $_ } | ForEach-Object { ([string]$_).ToLowerInvariant() })
  if ($known.Count -eq 0) { $known = @($script:SourceSnapshotDefaultKnownExtensions) }

  $external = @()
  foreach ($entry in @($spec.external_path_dependencies)) {
    if (-not $entry) { continue }
    $external += [pscustomobject]@{
      path = (ConvertTo-IdentityPosixPath ([string]$entry.path))
      reason = [string]$entry.reason
      handling = $(if ($entry.handling) { [string]$entry.handling } else { 'registered-only-not-followed' })
    }
  }

  return [pscustomobject]@{
    schema = $(if ($spec.schema) { [int]$spec.schema } else { $script:BuildIdentitySchema })
    roots = $roots
    exclude_path_patterns = @($excludes)
    extra_exclude_path_patterns = @($spec.extra_exclude_path_patterns | Where-Object { $_ })
    allow_path_patterns = $allows
    allow_reparse_points = $allowReparse
    known_source_extensions = $known
    external_path_dependencies = $external
    declared_by = 'config/package-manifest.json#source_snapshot'
  }
}

<#
  快照边界的规范描述（进入 source_snapshot_digest，避免不同边界下相同文件集互相冒充）。
  `match_base=root-relative` 必须进摘要：排除/放行规则是相对**各声明 root** 匹配的，
  同一组规则在"仓库相对路径"口径下不是同一个边界。
#>
function Get-SourceSnapshotScopeDescriptorLines {
  param([Parameter(Mandatory = $true)][object]$Scope)

  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add('match_base=root-relative')
  foreach ($root in @($Scope.roots)) { $lines.Add("root=$root") }
  foreach ($pattern in @($Scope.exclude_path_patterns)) { $lines.Add("exclude=$pattern") }
  foreach ($pattern in @($Scope.allow_path_patterns)) { $lines.Add("allow=$pattern") }
  foreach ($pattern in @($Scope.allow_reparse_points)) { $lines.Add("allow_reparse=$pattern") }
  return $lines.ToArray()
}

<#
  枚举源码快照文件集。

  * 仅枚举声明 roots；目录级排除在遍历时**剪枝**（同时避免走进 target / node_modules）。
  * 排除/放行规则匹配的是**相对所属 root 的路径**（match_base=root-relative）：
    这样"声明 roots 定义范围、排除规则定义范围内部要剔除什么"两件事互不干扰，
    也不会因为工作区位于某个被排除命名（例如仓库内 tmp/）之下而整体失效。
    条目里记录的仍然是仓库相对路径，便于与清单/差异核对。
  * roots 本身受信（roots 是显式声明的边界）；排除规则只作用于其内部条目。
  * **不追随**重解析点（junction / symlink）：按裁决要求显式登记而不是无边界追随；
    需要追随时必须在 manifest 的 allow_reparse_points 显式声明。
  * 纳入判定 = 声明 roots 下"除排除项外的一切"（可审计边界），不是扩展名白名单。
  * 已知扩展名之外的文件进入 unclassified_files（可见性报告，不改变纳入结果）。
#>
function Get-SourceSnapshotEntries {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][object]$Scope
  )

  $repoFull = [System.IO.Path]::GetFullPath($RepoPath).TrimEnd('\', '/')
  $entries = [System.Collections.Generic.List[object]]::new()
  $reparse = [System.Collections.Generic.List[object]]::new()
  $unclassified = [System.Collections.Generic.List[string]]::new()
  $missingRoots = [System.Collections.Generic.List[string]]::new()
  $extensionHistogram = @{}

  $stack = [System.Collections.Generic.Stack[object]]::new()
  foreach ($root in @($Scope.roots)) {
    $full = [System.IO.Path]::GetFullPath((Join-Path $repoFull $root))
    if (-not (Test-Path -LiteralPath $full)) {
      $missingRoots.Add($root)
      continue
    }
    $stack.Push([pscustomobject]@{ Full = $full; Relative = $root; MatchPath = $root; IsDeclaredRoot = $true })
  }

  while ($stack.Count -gt 0) {
    $node = $stack.Pop()
    $full = [string]$node.Full
    $relative = [string]$node.Relative
    $matchPath = [string]$node.MatchPath
    $isDeclaredRoot = ($node.PSObject.Properties['IsDeclaredRoot'] -and $node.IsDeclaredRoot -eq $true)

    $item = Get-Item -LiteralPath $full -Force -ErrorAction SilentlyContinue
    if (-not $item) { continue }

    if ($item.PSIsContainer) {
      # 声明 roots 本身受信（roots 是显式声明的边界）；排除规则只作用于其内部条目。
      if ($matchPath -and -not $isDeclaredRoot) {
        # 拆成两条语句：Windows PowerShell 5.1 不允许把续行放在行尾的二元运算符之后。
        $directoryExcluded = Test-IdentityPatternMatch -RelativePath $matchPath -Patterns @($Scope.exclude_path_patterns)
        $directoryAllowed = Test-IdentityPatternMatch -RelativePath $matchPath -Patterns @($Scope.allow_path_patterns)
        if ($directoryExcluded -and -not $directoryAllowed) {
          continue
        }
      }
      foreach ($child in @(Get-ChildItem -LiteralPath $full -Force -ErrorAction SilentlyContinue | Sort-Object Name)) {
        $childRelative = $(if ($relative) { "$relative/$($child.Name)" } else { [string]$child.Name })
        # 排除/放行匹配用 root 相对路径（match_base=root-relative，已进入 scope 摘要）。
        $childMatchPath = $(if ($isDeclaredRoot) { [string]$child.Name } else { "$matchPath/$($child.Name)" })
        $isReparse = (($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
        if ($isReparse -and -not (Test-IdentityPatternMatch -RelativePath $childMatchPath -Patterns @($Scope.allow_reparse_points))) {
          # 显式登记，不追随：junction 可能指到 target 或工作区外，无边界追随会污染身份。
          $reparse.Add([pscustomobject]@{
            path = $childRelative
            kind = $(if ($child.PSIsContainer) { 'directory' } else { 'file' })
            handling = 'not-followed-registered-only'
          })
          continue
        }
        if ($child.PSIsContainer) {
          $stack.Push([pscustomobject]@{ Full = $child.FullName; Relative = $childRelative; MatchPath = $childMatchPath; IsDeclaredRoot = $false })
          continue
        }
        $childExcluded = Test-IdentityPatternMatch -RelativePath $childMatchPath -Patterns @($Scope.exclude_path_patterns)
        $childAllowed = Test-IdentityPatternMatch -RelativePath $childMatchPath -Patterns @($Scope.allow_path_patterns)
        if ($childExcluded -and -not $childAllowed) {
          continue
        }
        if ($child.Length -eq 0 -and $child.Name -eq '.gitkeep') { continue }

        $hash = Get-IdentityFileHash -Path $child.FullName
        $entries.Add([pscustomobject]@{
          path = $childRelative
          length = [long]$child.Length
          sha256 = $hash
        })
        $extension = [System.IO.Path]::GetExtension($child.Name).ToLowerInvariant()
        if ($extension) {
          if ($extensionHistogram.ContainsKey($extension)) {
            $extensionHistogram[$extension] = [int]$extensionHistogram[$extension] + 1
          } else {
            $extensionHistogram[$extension] = 1
          }
          if (@($Scope.known_source_extensions) -notcontains $extension) {
            $unclassified.Add($childRelative)
          }
        } elseif (@($Scope.known_source_extensions) -notcontains $child.Name.ToLowerInvariant()) {
          $unclassified.Add($childRelative)
        }
      }
      continue
    }

    $rootExcluded = Test-IdentityPatternMatch -RelativePath $matchPath -Patterns @($Scope.exclude_path_patterns)
    $rootAllowed = Test-IdentityPatternMatch -RelativePath $matchPath -Patterns @($Scope.allow_path_patterns)
    if ($rootExcluded -and -not $rootAllowed -and -not $isDeclaredRoot) {
      continue
    }
    $hash = Get-IdentityFileHash -Path $full
    $entries.Add([pscustomobject]@{
      path = $relative
      length = [long]$item.Length
      sha256 = $hash
    })
    $extension = [System.IO.Path]::GetExtension($item.Name).ToLowerInvariant()
    if ($extension) {
      if ($extensionHistogram.ContainsKey($extension)) {
        $extensionHistogram[$extension] = [int]$extensionHistogram[$extension] + 1
      } else {
        $extensionHistogram[$extension] = 1
      }
      if (@($Scope.known_source_extensions) -notcontains $extension) {
        $unclassified.Add($relative)
      }
    }
  }

  return [pscustomobject]@{
    entries = @($entries)
    skipped_reparse_points = @($reparse)
    unclassified_files = @($unclassified)
    missing_roots = @($missingRoots)
    extension_histogram = $extensionHistogram
  }
}

function Get-SourceSnapshotEntryLines {
  param([AllowEmptyCollection()][object[]]$Entries = @())

  $lines = [System.Collections.Generic.List[string]]::new()
  foreach ($entry in @($Entries)) {
    $lines.Add(('file={0}|{1}|{2}' -f $entry.path, $entry.length, $entry.sha256))
  }
  return $lines.ToArray()
}

<#
  构造一次源码快照：**两遍完整静默枚举**（重新遍历 + 重新哈希）。

  这不是"只在首尾各哈希一次"：任何在枚举过程中发生的编辑/新增/删除都会让第一遍
  与第二遍的文件集或内容哈希不一致，从而在这里 fail-closed，而不是被当成"一致"。
  两遍一致后得到的 {scope_digest, file_set_digest, source_snapshot_digest} 才允许
  作为冻结输入写入冻结记录。
#>
function New-SourceSnapshot {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][object]$Scope,
    [int]$Passes = 2
  )

  if ($Passes -lt 2) {
    throw (New-IdentityFailureException -Category 'SOURCE-SNAPSHOT-PASS-COUNT' `
      -Detail "静默校验至少需要两遍枚举，收到 Passes=$Passes" `
      -Remediation '不要降低枚举遍数：单遍枚举无法区分"内容稳定"与"枚举期间被改过"')
  }

  $scopeLines = Get-SourceSnapshotScopeDescriptorLines -Scope $Scope
  $scopeDigest = Get-IdentityHashFromLines -Lines $scopeLines

  $first = $null
  $second = $null
  for ($pass = 1; $pass -le $Passes; $pass++) {
    $enumeration = Get-SourceSnapshotEntries -RepoPath $RepoPath -Scope $Scope
    $entryLines = Get-SourceSnapshotEntryLines -Entries $enumeration.entries
    $snapshot = [pscustomobject]@{
      pass = $pass
      entries = @($enumeration.entries)
      entry_lines = $entryLines
      file_set_digest = Get-IdentityHashFromLines -Lines $entryLines
      scope_digest = $scopeDigest
      unclassified_files = @($enumeration.unclassified_files)
      skipped_reparse_points = @($enumeration.skipped_reparse_points)
      missing_roots = @($enumeration.missing_roots)
      extension_histogram = $enumeration.extension_histogram
    }
    if ($pass -eq 1) { $first = $snapshot; continue }
    $second = $snapshot
    if ($second.file_set_digest -ne $first.file_set_digest) {
      $diff = Compare-SourceSnapshot -Left $first -Right $second
      $detail = ($diff.summary_lines -join '; ')
      throw (New-IdentityFailureException -Category 'SOURCE-SNAPSHOT-UNSTABLE' `
        -Detail ("源码快照在静默枚举期间发生变化（pass1={0} pass2={1}）：{2}" -f $first.file_set_digest, $second.file_set_digest, $detail) `
        -Remediation '停止并发编辑活动树，或在受控不变输入集合上构建；不要用"首尾各哈希一次"掩盖变化')
    }
  }

  $sourceSnapshotLines = [System.Collections.Generic.List[string]]::new()
  foreach ($line in $scopeLines) { $sourceSnapshotLines.Add("scope:$line") }
  $sourceSnapshotLines.Add("scope_digest=$scopeDigest")
  $sourceSnapshotLines.Add("file_set_digest=$($first.file_set_digest)")

  $totalBytes = 0L
  foreach ($entry in @($first.entries)) { $totalBytes += [long]$entry.length }

  return [pscustomobject]@{
    schema = $script:BuildIdentitySchema
    scope_digest = $scopeDigest
    file_set_digest = $first.file_set_digest
    source_snapshot_digest = Get-IdentityHashFromLines -Lines $sourceSnapshotLines.ToArray()
    file_count = @($first.entries).Count
    total_bytes = $totalBytes
    passes = $Passes
    quiescent = $true
    scope = [pscustomobject]@{
      declared_by = $Scope.declared_by
      roots = @($Scope.roots)
      exclude_path_patterns = @($Scope.exclude_path_patterns)
      allow_path_patterns = @($Scope.allow_path_patterns)
      allow_reparse_points = @($Scope.allow_reparse_points)
      external_path_dependencies = @($Scope.external_path_dependencies)
    }
    scope_lines = @($scopeLines)
    entries = @($first.entries)
    unclassified_files = @($first.unclassified_files)
    skipped_reparse_points = @($first.skipped_reparse_points)
    missing_roots = @($first.missing_roots)
    extension_histogram = $first.extension_histogram
  }
}

<#
  两份快照的差异（**含新增/删除**，不只比较原清单里的文件）。
#>
function Compare-SourceSnapshot {
  param(
    [Parameter(Mandatory = $true)][object]$Left,
    [Parameter(Mandatory = $true)][object]$Right
  )

  $leftMap = @{}
  foreach ($entry in @($Left.entries)) { $leftMap[[string]$entry.path] = $entry }
  $rightMap = @{}
  foreach ($entry in @($Right.entries)) { $rightMap[[string]$entry.path] = $entry }

  $added = [System.Collections.Generic.List[string]]::new()
  $removed = [System.Collections.Generic.List[string]]::new()
  $modified = [System.Collections.Generic.List[string]]::new()
  foreach ($key in $rightMap.Keys) {
    if (-not $leftMap.ContainsKey($key)) { $added.Add($key) }
  }
  foreach ($key in $leftMap.Keys) {
    if (-not $rightMap.ContainsKey($key)) {
      $removed.Add($key)
      continue
    }
    if ([string]$leftMap[$key].sha256 -ne [string]$rightMap[$key].sha256) { $modified.Add($key) }
  }

  $addedSorted = Get-IdentityCanonicalLines -Lines $added.ToArray()
  $removedSorted = Get-IdentityCanonicalLines -Lines $removed.ToArray()
  $modifiedSorted = Get-IdentityCanonicalLines -Lines $modified.ToArray()

  $summary = [System.Collections.Generic.List[string]]::new()
  $summary.Add(('added={0}' -f $addedSorted.Count))
  $summary.Add(('removed={0}' -f $removedSorted.Count))
  $summary.Add(('modified={0}' -f $modifiedSorted.Count))
  if ($addedSorted.Count -gt 0) { $summary.Add('added_paths=' + (($addedSorted | Select-Object -First 10) -join ',')) }
  if ($removedSorted.Count -gt 0) { $summary.Add('removed_paths=' + (($removedSorted | Select-Object -First 10) -join ',')) }
  if ($modifiedSorted.Count -gt 0) { $summary.Add('modified_paths=' + (($modifiedSorted | Select-Object -First 10) -join ',')) }

  return [pscustomobject]@{
    identical = ($addedSorted.Count -eq 0 -and $removedSorted.Count -eq 0 -and $modifiedSorted.Count -eq 0)
    left_digest = [string]$Left.file_set_digest
    right_digest = [string]$Right.file_set_digest
    added = $addedSorted
    removed = $removedSorted
    modified = $modifiedSorted
    added_count = $addedSorted.Count
    removed_count = $removedSorted.Count
    modified_count = $modifiedSorted.Count
    summary_lines = $summary.ToArray()
  }
}

<#
  冻结记录：把本次源码快照的文件清单写进 tmp/source-snapshots/source-snapshot-<digest>.json。
  写入是**一次性的**：同名（同 digest）文件已存在时，重算其 entries 摘要必须仍等于 digest，
  否则判定为冲突/损坏并 fail-closed（不允许静默覆盖冻结输入）。
  返回记录路径与其内容哈希（内容哈希登记在包报告中，因为报告在包外，可以承载它）。
#>
function Write-SourceSnapshotFreezeRecord {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][object]$Snapshot
  )

  $root = Join-Path ([System.IO.Path]::GetFullPath($RepoPath)) $script:SourceSnapshotFreezeRoot
  New-Item -ItemType Directory -Force -Path $root | Out-Null
  $path = Join-Path $root ("source-snapshot-{0}.json" -f $Snapshot.source_snapshot_digest)

  if (Test-Path -LiteralPath $path -PathType Leaf) {
    $existing = $null
    try {
      $existing = Get-Content -Raw -LiteralPath $path -Encoding UTF8 | ConvertFrom-Json
    } catch {
      throw (New-IdentityFailureException -Category 'SOURCE-SNAPSHOT-FREEZE-CORRUPT' `
        -Detail "冻结记录无法解析：$path（$($_.Exception.Message)）" `
        -Remediation '人工核对后删除该损坏记录并重新构建；不要在无法核对时覆盖冻结输入')
    }
    $existingLines = Get-SourceSnapshotEntryLines -Entries @($existing.entries)
    $existingDigest = Get-IdentityHashFromLines -Lines $existingLines
    if ($existingDigest -ne $Snapshot.file_set_digest) {
      throw (New-IdentityFailureException -Category 'SOURCE-SNAPSHOT-FREEZE-COLLISION' `
        -Detail ("冻结记录 {0} 的文件清单摘要（{1}）与本次快照（{2}）不一致" -f $path, $existingDigest, $Snapshot.file_set_digest) `
        -Remediation '这是摘要冲突或记录被改写；先人工核对再继续，不要覆盖冻结记录')
    }
    return [pscustomobject]@{
      path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path)
      absolute_path = $path
      content_sha256 = Get-IdentityFileHash -Path $path
      reused_existing_record = $true
    }
  }

  $record = [ordered]@{
    schema = $script:BuildIdentitySchema
    kind = 'source-snapshot-freeze'
    source_snapshot_digest = $Snapshot.source_snapshot_digest
    scope_digest = $Snapshot.scope_digest
    file_set_digest = $Snapshot.file_set_digest
    recorded_at_utc = (Get-Date).ToUniversalTime().ToString('o')
    file_count = $Snapshot.file_count
    total_bytes = $Snapshot.total_bytes
    scope = $Snapshot.scope
    scope_lines = @($Snapshot.scope_lines)
    unclassified_files = @($Snapshot.unclassified_files)
    skipped_reparse_points = @($Snapshot.skipped_reparse_points)
    missing_roots = @($Snapshot.missing_roots)
    entries = @($Snapshot.entries)
    retention_note = '冻结记录是"本次构建用了哪些源码字节"的核对依据；普通临时清理不得删除被报告引用的记录。'
  }
  $json = $record | ConvertTo-Json -Depth 10
  [System.IO.File]::WriteAllText($path, $json, [System.Text.UTF8Encoding]::new($false))

  return [pscustomobject]@{
    path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path)
    absolute_path = $path
    content_sha256 = Get-IdentityFileHash -Path $path
    reused_existing_record = $false
  }
}

<#
  build_input_digest 的描述符：回答"该快照配合什么锁文件、工具链、target、
  profile、features 和构建配置"。

  独立 Tauri 项目是**独立 Cargo 项目**：它的真实构建入口与锁文件（src-tauri/Cargo.toml、
  src-tauri/Cargo.lock）在这里显式登记，不能只记顶层 workspace 的 lockfile。
#>
function Get-BuildInputDescriptors {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][object]$ManifestData,
    [Parameter(Mandatory = $true)][string]$ManifestPath,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][object]$CargoIdentity,
    [Parameter(Mandatory = $true)][string]$SourceSnapshotDigest
  )

  $descriptors = [System.Collections.Generic.List[string]]::new()
  $descriptors.Add("schema=$script:BuildIdentitySchema")
  $descriptors.Add("source_snapshot_digest=$SourceSnapshotDigest")
  $descriptors.Add("profile=$Configuration")
  $descriptors.Add("package_manifest=$((Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $ManifestPath))|sha256=$((Get-IdentityFileHash -Path $ManifestPath))")
  $descriptors.Add("cargo_version=$([string]$CargoIdentity.cargo_version)")
  $descriptors.Add("cargo_host_target=$([string]$CargoIdentity.host_target)")
  $descriptors.Add("cargo_build_target=$([string]$CargoIdentity.build_target)")

  # 工具链（rustc 版本与 host；缺失时显式记 not-available，不编造）
  $rustcLine = 'rustc=not-available'
  try {
    $rustcOutput = @(& rustc -vV 2>$null)
    if ($LASTEXITCODE -eq 0 -and @($rustcOutput).Count -gt 0) {
      $rustcLine = 'rustc=' + (([string]($rustcOutput | Select-Object -First 1)).Trim())
    }
  } catch {
    $rustcLine = 'rustc=not-available'
  }
  $descriptors.Add($rustcLine)

  # 影响构建解析/编译的环境变量：只记取值，不落任何凭据内容
  foreach ($name in @('CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_HOME', 'RUSTUP_TOOLCHAIN', 'COOLZHU_RELEASE_VERSION', 'COOLZHU_BUILD_TARGET', 'COOLZHU_BUILD_DATE')) {
    $value = [Environment]::GetEnvironmentVariable($name, 'Process')
    $descriptors.Add(('env:{0}={1}' -f $name, $(if ([string]::IsNullOrWhiteSpace($value)) { '(unset)' } else { $value })))
  }

  # 工作区内影响构建的配置文件（存在才登记；用户级 cargo 配置不纳入，避免把用户配置/凭据带进身份）
  foreach ($relative in @('.cargo/config.toml', '.cargo/config', 'rust-toolchain.toml', 'rust-toolchain', '.config/cargo.toml')) {
    $full = Join-Path ([System.IO.Path]::GetFullPath($RepoPath)) $relative
    if (Test-Path -LiteralPath $full -PathType Leaf) {
      $descriptors.Add(('workspace_config={0}|sha256={1}' -f $relative, (Get-IdentityFileHash -Path $full)))
    } else {
      $descriptors.Add(("workspace_config=$relative|absent"))
    }
  }

  # 顶层 workspace 锁文件 + 各构建入口声明（含独立 Tauri 项目的 lockfile / build.rs / tauri.conf.json）
  $declaredInputs = [System.Collections.Generic.List[string]]::new()
  foreach ($path in @($ManifestData.'build_inputs'.files)) { if ($path) { $declaredInputs.Add([string]$path) } }
  $declaredInputs.Add('Cargo.lock')
  foreach ($artifact in @($ManifestData.artifacts)) {
    if ($artifact.export) {
      foreach ($path in @($artifact.export.build_entry_manifest, $artifact.export.lockfile)) {
        if ($path) { $declaredInputs.Add([string]$path) }
      }
      foreach ($path in @($artifact.export.source_identity.files)) {
        if ($path) { $declaredInputs.Add([string]$path) }
      }
    }
  }

  $seen = @{}
  foreach ($relative in @($declaredInputs)) {
    $relativePosix = (ConvertTo-IdentityPosixPath $relative).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration)
    if (-not $relativePosix) { continue }
    if ($seen.ContainsKey($relativePosix)) { continue }
    $seen[$relativePosix] = $true
    $full = Join-Path ([System.IO.Path]::GetFullPath($RepoPath)) $relativePosix
    if (Test-Path -LiteralPath $full -PathType Leaf) {
      $descriptors.Add(('input_file={0}|length={1}|sha256={2}' -f $relativePosix, (Get-Item -LiteralPath $full -Force).Length, (Get-IdentityFileHash -Path $full)))
    } else {
      throw (New-IdentityFailureException -Category 'BUILD-INPUT-MISSING' `
        -Detail "构建输入文件缺失：$relativePosix" `
        -Remediation '补齐锁文件/构建入口声明后再构建；不要在缺失锁文件的情况下发布')
    }
  }

  # target-dir、features 与逐 artifact 构建参数（features 属于构建输入，必须登记）
  foreach ($artifact in @($ManifestData.artifacts)) {
    $artifactId = [string]$artifact.id
    if (-not $artifact.build) {
      $descriptors.Add(("artifact={0}|build=none" -f $artifactId))
      continue
    }
    $args = @($artifact.build.args | ForEach-Object { ([string]$_).Replace('{profile}', $Configuration).Replace('{configuration}', $Configuration) })
    $features = @()
    for ($index = 0; $index -lt $args.Count; $index++) {
      if (($args[$index] -eq '--features' -or $args[$index] -eq '-F') -and ($index + 1) -lt $args.Count) {
        $features += [string]$args[$index + 1]
      } elseif ([string]$args[$index] -like '--features=*') {
        $features += ([string]$args[$index]).Substring('--features='.Length)
      }
    }
    $targetDir = 'target'
    for ($index = 0; $index -lt $args.Count; $index++) {
      if ($args[$index] -eq '--target-dir' -and ($index + 1) -lt $args.Count) { $targetDir = [string]$args[$index + 1] }
    }
    $releaseAppended = $(if ($artifact.build.append_release_arg -eq $false) { 'false' } else { 'true' })
    $packageToken = $false
    for ($index = 0; $index -lt $args.Count; $index++) {
      if ($args[$index] -eq '-p' -and ($index + 1) -lt $args.Count) { $packageToken = $true }
    }
    $descriptors.Add(('artifact={0}|profile={1}|target_dir={2}|features={3}|release_arg_appended={4}|build_capture={5}|args={6}' -f `
      $artifactId, $Configuration, $targetDir, $(if ($features.Count -gt 0) { ($features -join ',') } else { 'default' }), $releaseAppended, $(if ($artifact.build.capture) { [string]$artifact.build.capture } else { 'none' }), ($args -join ' ')))
    if (-not $packageToken) {
      $descriptors.Add(("artifact={0}|package_selector=none" -f $artifactId))
    }
  }

  return $descriptors.ToArray()
}

function Get-BuildInputDigest {
  param([AllowEmptyCollection()][string[]]$DescriptorLines = @())

  return Get-IdentityHashFromLines -Lines $DescriptorLines
}

<#
  载荷清单：最终生成/分发了哪些二进制与资源。
  ExcludeRelativePaths 用于排除清单载体自身（自引用固定点）。
#>
function Get-PayloadInventory {
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [AllowEmptyCollection()][string[]]$ExcludeRelativePaths = @()
  )

  if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
    throw "payload root not found: $Root"
  }
  $resolvedRoot = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\', '/')
  $rootPrefix = $resolvedRoot + [System.IO.Path]::DirectorySeparatorChar
  $excluded = @($ExcludeRelativePaths | ForEach-Object { (ConvertTo-IdentityPosixPath ([string]$_)).TrimStart('/') })

  $entries = [System.Collections.Generic.List[object]]::new()
  foreach ($file in @(Get-ChildItem -LiteralPath $resolvedRoot -File -Recurse -Force)) {
    $relative = (ConvertTo-IdentityPosixPath $file.FullName.Substring($rootPrefix.Length))
    if ($excluded -contains $relative) { continue }
    $entries.Add([pscustomobject]@{
      path = $relative
      length = [long]$file.Length
      sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant()
    })
  }

  $lines = [System.Collections.Generic.List[string]]::new()
  $totalBytes = 0L
  foreach ($entry in @($entries)) {
    $lines.Add(('file={0}|{1}|{2}' -f $entry.path, $entry.length, $entry.sha256))
    $totalBytes += [long]$entry.length
  }

  return [pscustomobject]@{
    root = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $resolvedRoot)
    digest = Get-IdentityHashFromLines -Lines $lines.ToArray()
    file_count = @($entries).Count
    total_bytes = $totalBytes
    excluded_self_carrier = $excluded
    entries = @($entries)
  }
}

function Get-PackageReportId {
  param(
    [Parameter(Mandatory = $true)][string]$Configuration,
    [string]$Prefix = 'pkg-report',
    # 传入时与报告文件名共用同一时间戳（报告 ID 与文件名的可读一致性）；
    # 唯一性仍由末尾的随机后缀保证。
    [string]$Stamp
  )

  $stampValue = $(if ($Stamp) { [string]$Stamp } else { Get-Date -Format 'yyyyMMdd-HHmmssfff' })
  $suffix = [guid]::NewGuid().ToString('N').Substring(0, 8)
  return ('{0}-{1}-{2}-{3}' -f $Prefix, $Configuration, $stampValue, $suffix)
}

<#
  报告内容哈希的规范形式：把报告的所有叶子节点展开成 "<路径>=<值>" 行，
  Ordinal 排序后 LF 连接、UTF-8 无 BOM、SHA256。

  为什么不是"直接哈希文件字节"：报告的 content_sha256 字段就在报告内部，
  直接哈希自身字节是自引用固定点。这里把该字段**排除在哈希范围之外**，
  于是"读取报告 → 重算 → 与字段比对"是可行且确定的。
  排序使哈希与属性书写顺序、序列化器实现无关。
#>
function Add-IdentityCanonicalLeaf {
  param(
    # AllowNull 是必须的：报告里本来就有 null 叶子（例如 report_identity.content_sha256
    # 在被赋值之前），Mandatory 参数默认拒绝 $null，会在绑定期就抛错而不是进入函数体。
    [Parameter(Mandatory = $true)][AllowNull()]$Value,
    [AllowEmptyString()][string]$Path,
    [Parameter(Mandatory = $true)]$Lines
  )

  if ($null -eq $Value) {
    $Lines.Add(('{0}=null' -f $Path))
    return
  }
  if ($Value -is [bool]) {
    $Lines.Add(('{0}={1}' -f $Path, $(if ($Value) { 'true' } else { 'false' })))
    return
  }
  if ($Value -is [string]) {
    $Lines.Add(('{0}={1}' -f $Path, $Value))
    return
  }
  if ($Value -is [System.Collections.IDictionary]) {
    foreach ($key in @($Value.Keys)) {
      $childPath = $(if ($Path) { '{0}.{1}' -f $Path, $key } else { [string]$key })
      Add-IdentityCanonicalLeaf -Value $Value[$key] -Path $childPath -Lines $Lines
    }
    return
  }
  if ($Value -is [System.Management.Automation.PSCustomObject]) {
    foreach ($property in @($Value.PSObject.Properties)) {
      $childPath = $(if ($Path) { '{0}.{1}' -f $Path, $property.Name } else { [string]$property.Name })
      Add-IdentityCanonicalLeaf -Value $property.Value -Path $childPath -Lines $Lines
    }
    return
  }
  if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
    $items = @($Value)
    $Lines.Add(('{0}.count={1}' -f $Path, $items.Count))
    for ($index = 0; $index -lt $items.Count; $index++) {
      Add-IdentityCanonicalLeaf -Value $items[$index] -Path ('{0}[{1}]' -f $Path, $index) -Lines $Lines
    }
    return
  }
  if ($Value -is [double] -or $Value -is [single] -or $Value -is [decimal]) {
    $Lines.Add(('{0}={1}' -f $Path, ([Convert]::ToDouble($Value)).ToString('R', [System.Globalization.CultureInfo]::InvariantCulture)))
    return
  }
  $Lines.Add(('{0}={1}' -f $Path, ([Convert]::ToString($Value, [System.Globalization.CultureInfo]::InvariantCulture))))
}

function Get-PackageReportContentLines {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [string]$ExcludedLeafPath = 'report_identity.content_sha256'
  )

  $lines = [System.Collections.Generic.List[string]]::new()
  Add-IdentityCanonicalLeaf -Value $Report -Path '' -Lines $lines
  $filtered = @($lines | Where-Object { $_ -notlike "$ExcludedLeafPath=*" })
  return $filtered
}

function Get-PackageReportContentHash {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [string]$ExcludedLeafPath = 'report_identity.content_sha256'
  )

  return Get-IdentityHashFromLines -Lines (Get-PackageReportContentLines -Report $Report -ExcludedLeafPath $ExcludedLeafPath)
}

<#
  内容哈希必须在**落盘文档**上计算，而不是在内存对象上：
  验证方只有文件（读文件 → 重算），PowerShell 的 ConvertTo-Json/ConvertFrom-Json
  往返会把空数组/单元素数组等归一化，若生产方直接在内存对象上算，就会出现
  "生产方算出 A、验证方算出 B"的假身份。
  因此这里的口径是：先把报告序列化再解析（往返归一化），在归一化结果上取规范哈希。
#>
function Get-NormalizedPackageReportHash {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [int]$Depth = 12
  )

  $json = $Report | ConvertTo-Json -Depth $Depth
  $normalized = $json | ConvertFrom-Json
  return Get-PackageReportContentHash -Report $normalized
}

<#
  报告治理：把报告身份（唯一 ID + 内容哈希）写入 report_identity，然后落盘。
  返回值给出落盘后的文件字节哈希（供产物清单/installer report 引用）。
#>
<#
  报告身份块的**唯一构造点**：报告由生产脚本在写产物清单之前先构造（清单要引用
  报告 ID 与内容哈希），因此必须与 Write-GovernedPackageReport 使用同一份构造，
  否则两边字段取值不一致会让"预算哈希"与"落盘哈希"漂移（本仓库的
  [REPORT-CONTENT-DRIFT] 守卫就是这么抓到重复定义的）。
#>
function New-PackageReportIdentity {
  param(
    [Parameter(Mandatory = $true)][string]$ReportId,
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [string]$GeneratedBy = 'scripts/package-all.ps1'
  )

  return [ordered]@{
    schema = $script:BuildIdentitySchema
    report_id = $ReportId
    content_sha256 = $null
    content_hash_algorithm = 'sha256'
    content_hash_scope = 'canonical sorted leaf paths; report_identity.content_sha256 excluded'
    generated_by = $GeneratedBy
    report_path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $Path)
  }
}

function Write-GovernedPackageReport {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ReportId,
    [string]$GeneratedBy = 'scripts/package-all.ps1'
  )

  $identity = New-PackageReportIdentity -ReportId $ReportId -Path $Path -RepoPath $RepoPath -GeneratedBy $GeneratedBy
  # 报告对象可能是 [ordered] 字典或 PSCustomObject：两条路径都要按"同名键覆盖"处理。
  # 注意：OrderedDictionary 的键**不会**出现在 $x.PSObject.Properties 里，必须先判 IDictionary。
  if ($Report -is [System.Collections.IDictionary]) {
    $Report['report_identity'] = $identity
  } elseif ($Report.PSObject.Properties['report_identity']) {
    $Report.report_identity = $identity
  } else {
    $Report | Add-Member -NotePropertyName 'report_identity' -NotePropertyValue $identity -Force
  }
  $identity.content_sha256 = Get-NormalizedPackageReportHash -Report $Report

  $parent = Split-Path -Parent $Path
  if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
  $json = $Report | ConvertTo-Json -Depth 12
  [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))

  return [pscustomobject]@{
    report_id = $ReportId
    report_path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $Path)
    absolute_path = $Path
    content_sha256 = [string]$identity.content_sha256
    file_sha256 = Get-IdentityFileHash -Path $Path
  }
}

<#
  校验报告：内容哈希必须能从报告自身重算出来。
#>
# ============================================================================
# 失败诊断与发布资格分离（RD4-06 / 第六轮裁决 §三）
# 权威口径：docs/analysis/2026-09-21-integration-review/
#           a2-workspace-source-and-frozen-parent-context.md 第 9 节第一项
#
# 逐条对应：
#   1) 维持**拒绝发布**不稳定输入所生成的包；不允许用 `quiescent=false` 之类的
#      字段给混合输入产物补一张"可发布收据"。
#   2) 但**必须**输出失败诊断报告：拒绝的是"成功报告"与"有效发布收据"，
#      **不拒绝**失败诊断。失败诊断落点与成功报告分开（见下方的 failures/ 分区），
#      因此既不会覆盖上一份成功报告，也不会被 -ReportPath 的消费者当成成功报告。
#   3) 失败诊断记录：运行 ID / 阶段与失败原因 / 声明的源码与构建输入范围 /
#      已观察到的输入变化 / 各阶段退出码 / 已产生但未获准发布的临时产物 /
#      release_eligible=false。
#   4) 前后两次采样的差异一律标为"已观察变化"（observed-between-two-samples），
#      **不得**称为完整写入历史。
#   5) 失败后：不覆盖上一份成功报告、不更新"最新有效包"指针、不进入签名/安装/分发；
#      临时产物移入失败隔离区（与可发布产物分区）且永不自动晋升。
#   6) 状态分开表达：live_worktree_changed / build_snapshot_integrity /
#      build_input_digest / validation_snapshot_digest / release_eligible。
#   7) 允许"活动树继续编辑"只发生在"构建与测试只用不可变快照"时；本实现直接从
#      活动树构建，因此只承认"构建前后两次独立采样逐文件一致"这一种稳定证据，
#      活动树被编辑即拒绝发布。
#   8) 已声明不属于构建输入的日志/临时输出变化不得被误判为源码变化：范围规则
#      显式声明在 manifest 的 release_policy.non_build_input_paths，并由
#      Get-PackagePathClassification 逐路径给出可审查的分类。
#   9) --no-build 必须消费"匹配快照、配置与产物"的有效收据：收据本身只覆盖构建
#      入口子范围（receipt.build_identity.source_snapshot_digest 恒为 $null，
#      source_snapshot_digest_available=$false），因此这里另建
#      **产物—快照关联记录**（成功运行写入）与**禁用记录**（失败运行写入）。
#      关联缺失一律记为"未能确认"（not-confirmed），不乐观放行。
# ============================================================================

$script:PackageFailureReportSubdir = 'failures'
$script:PackageFailureQuarantineRoot = 'tmp/package-failures'
$script:ArtifactReleaseStatusFileName = 'artifact-release-status.json'
$script:ArtifactReleaseStatusKeepPerArtifact = 20
$script:PackageFailureReportIdPrefix = 'pkg-report-failure'
$script:PackageRunStatusPrefix = 'latest-run'
$script:PackageFailureReportKind = 'package-report-failure'

function Test-PackagePathInsideRoot {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Root
  )

  $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
  $fullRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
  if ($fullPath.Equals($fullRoot, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
  return $fullPath.StartsWith($fullRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)
}

function ConvertTo-IdentityBoolOrUnknown {
  param([AllowNull()]$Value)

  # 三态：$true / $false / 'not-confirmed'。用于"未能确认"不得写成 false 的字段。
  if ($null -eq $Value) { return 'not-confirmed' }
  if ($Value -is [bool]) { return $Value }
  $text = ([string]$Value).Trim().ToLowerInvariant()
  if ($text -eq 'true') { return $true }
  if ($text -eq 'false') { return $false }
  return 'not-confirmed'
}

function Get-PackageReportRootPath {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  $repoFull = [System.IO.Path]::GetFullPath($RepoPath)
  return (Join-Path $repoFull ($script:PackageReportRoot -replace '/', '\'))
}

function Get-PackageFailureReportRoot {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  return (Join-Path (Get-PackageReportRootPath -RepoPath $RepoPath) $script:PackageFailureReportSubdir)
}

function Get-PackageLatestPointerPath {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration
  )

  return (Join-Path (Get-PackageReportRootPath -RepoPath $RepoPath) "latest-$Configuration.json")
}

function Get-PackageRunStatusPath {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration
  )

  return (Join-Path (Get-PackageReportRootPath -RepoPath $RepoPath) "$($script:PackageRunStatusPrefix)-$Configuration.json")
}

function Get-PackageFailureQuarantineRoot {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  return (Join-Path ([System.IO.Path]::GetFullPath($RepoPath)) ($script:PackageFailureQuarantineRoot -replace '/', '\'))
}

function Get-PackageArtifactReleaseStatusPath {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  return (Join-Path (Get-PackageReportRootPath -RepoPath $RepoPath) $script:ArtifactReleaseStatusFileName)
}

function New-PackageArtifactReleaseStatusDocument {
  return [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = 'package-artifact-release-status'
    generated_by = 'scripts/lib/build-identity.ps1'
    updated_at = $null
    associations = @()
    revocations = @()
    runs = @()
    note = 'associations = 已通过构建后校验的产物内容身份与其源码快照/构建输入摘要的绑定（供 --no-build 确认"匹配快照"）；revocations = 由失败运行登记的禁用内容身份（不稳定输入的裸产物不得当来源）；runs = 最近运行结果（不替代 latest 指针，失败运行不更新 latest）。'
  }
}

function Read-PackageArtifactReleaseStatus {
  param([Parameter(Mandatory = $true)][string]$RepoPath)

  $path = Get-PackageArtifactReleaseStatusPath -RepoPath $RepoPath
  $document = New-PackageArtifactReleaseStatusDocument
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    return [pscustomobject]@{ path = $path; exists = $false; document = $document }
  }
  try {
    $parsed = Get-Content -Raw -LiteralPath $path -Encoding UTF8 | ConvertFrom-Json
  } catch {
    throw (New-IdentityFailureException -Category 'ARTIFACT-RELEASE-STATUS-INVALID' `
      -Detail "产物释放状态库无法解析：$path（$($_.Exception.Message)）" `
      -Remediation '先人工核对状态库；无法核对时不得在 --no-build 下发布（未确认即拒绝），也不要把状态库当作可发布收据')
  }
  $document.associations = @($parsed.associations | Where-Object { $_ })
  $document.revocations = @($parsed.revocations | Where-Object { $_ })
  $document.runs = @($parsed.runs | Where-Object { $_ })
  $document.updated_at = [string]$parsed.updated_at
  return [pscustomobject]@{ path = $path; exists = $true; document = $document }
}

function Write-PackageArtifactReleaseStatus {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)]$Document
  )

  $path = Get-PackageArtifactReleaseStatusPath -RepoPath $RepoPath
  $parent = Split-Path -Parent $path
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
  $Document.updated_at = (Get-Date).ToUniversalTime().ToString('o')
  $temp = Join-Path $parent ('.tmp-artifact-release-status-' + [guid]::NewGuid().ToString('N'))
  [System.IO.File]::WriteAllText($temp, ($Document | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
  # 原子替换：状态库是"产物能否被当作来源"的唯一链，不允许出现半成品。
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    $backup = Join-Path $parent ('.bak-artifact-release-status-' + [guid]::NewGuid().ToString('N'))
    try {
      [System.IO.File]::Replace($temp, $path, $backup, $true)
    } finally {
      if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue }
    }
  } else {
    [System.IO.File]::Move($temp, $path)
  }
  return $path
}

<#
  按 artifact+profile 限制历史条数：保留最近 Keep 条（新条目追加在尾部，
  因此从尾部往回数 Keep 条，其余丢弃）。
#>
function Remove-ExcessPackageArtifactRecords {
  param(
    [AllowEmptyCollection()][object[]]$Records = @(),
    [int]$Keep = 20
  )

  $all = @($Records)
  if ($all.Count -eq 0) { return @() }
  $counts = @{}
  $kept = [System.Collections.Generic.List[object]]::new()
  for ($index = $all.Count - 1; $index -ge 0; $index--) {
    $record = $all[$index]
    $key = ('{0}|{1}' -f [string]$record.artifact_id, [string]$record.profile)
    if (-not $counts.ContainsKey($key)) { $counts[$key] = 0 }
    if ([int]$counts[$key] -ge $Keep) { continue }
    $counts[$key] = [int]$counts[$key] + 1
    $kept.Add($record)
  }
  $ordered = @($kept)
  [array]::Reverse($ordered)
  return $ordered
}

<#
  登记"产物内容身份 ← 源码快照 / 构建输入摘要"的关联记录（成功运行、构建后校验通过时写入）。
  --no-build 只有找到绑定到**本次**快照与构建输入摘要的关联，才能确认"产物来自对该快照
  做过的、事后通过校验的构建"。
#>
function Add-PackageArtifactAssociations {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][string]$RunId,
    [string]$ReportId,
    [string]$SourceSnapshotDigest,
    [string]$BuildInputDigest,
    [AllowEmptyCollection()][object[]]$Entries = @()
  )

  if (-not $Entries -or @($Entries).Count -eq 0) { return $null }
  $read = Read-PackageArtifactReleaseStatus -RepoPath $RepoPath
  $document = $read.document
  $recordedAt = (Get-Date).ToUniversalTime().ToString('o')
  $newRecords = [System.Collections.Generic.List[object]]::new()
  foreach ($entry in @($Entries)) {
    $newRecords.Add([pscustomobject]@{
      artifact_id = [string]$entry.artifact_id
      profile = $Configuration
      target = [string]$entry.target
      source_path = [string]$entry.source_path
      source_sha256 = [string]$entry.source_sha256
      source_length = [long]$entry.source_length
      has_export = [bool]$entry.has_export
      stable_export_sha256 = [string]$entry.stable_export_sha256
      receipt_path = [string]$entry.receipt_path
      producer_build_script_rerun = $entry.producer_build_script_rerun
      build_mode = [string]$entry.build_mode
      source_snapshot_digest = $SourceSnapshotDigest
      build_input_digest = $BuildInputDigest
      run_id = $RunId
      report_id = $ReportId
      recorded_at_utc = $recordedAt
      note = '该内容身份由一次"构建后校验通过"的运行产出；--no-build 只有在快照/构建输入摘要与本次一致时才可据此确认来源'
    }) | Out-Null
  }
  # 同 artifact+profile+内容身份 的旧记录被新记录取代（保留最新一条）。
  $existing = [System.Collections.Generic.List[object]]::new()
  foreach ($record in @($document.associations)) {
    $superseded = $false
    foreach ($new in $newRecords) {
      if (
        [string]$record.artifact_id -eq [string]$new.artifact_id -and
        [string]$record.profile -eq [string]$new.profile -and
        [string]$record.source_sha256 -eq [string]$new.source_sha256
      ) {
        $superseded = $true
        break
      }
    }
    if (-not $superseded) { $existing.Add($record) }
  }
  foreach ($new in $newRecords) { $existing.Add($new) }
  $document.associations = Remove-ExcessPackageArtifactRecords -Records $existing.ToArray() -Keep $script:ArtifactReleaseStatusKeepPerArtifact
  return (Write-PackageArtifactReleaseStatus -RepoPath $RepoPath -Document $document)
}

<#
  登记**禁用**记录（失败运行写入）：本次运行观察到的活动树变化意味着相关产物是
  "混合输入产物"，其内容身份不得再被 --no-build 当作来源。
#>
function Add-PackageArtifactRevocations {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][string]$RunId,
    [Parameter(Mandatory = $true)][string]$Reason,
    [string]$Category,
    [string]$Stage,
    [AllowEmptyCollection()][object[]]$Entries = @()
  )

  if (-not $Entries -or @($Entries).Count -eq 0) { return $null }
  $read = Read-PackageArtifactReleaseStatus -RepoPath $RepoPath
  $document = $read.document
  $recordedAt = (Get-Date).ToUniversalTime().ToString('o')
  $newRecords = [System.Collections.Generic.List[object]]::new()
  foreach ($entry in @($Entries)) {
    $identity = @([string]$entry.source_sha256, [string]$entry.stable_export_sha256) | Where-Object { $_ }
    if (@($identity).Count -eq 0) { continue }
    $newRecords.Add([pscustomobject]@{
      artifact_id = [string]$entry.artifact_id
      profile = $Configuration
      source_path = [string]$entry.source_path
      source_sha256 = [string]$entry.source_sha256
      stable_export_sha256 = [string]$entry.stable_export_sha256
      receipt_path = [string]$entry.receipt_path
      run_id = $RunId
      category = $Category
      stage = $Stage
      reason = $Reason
      recorded_at_utc = $recordedAt
      note = '该内容身份来自一次输入已被观察到变化的运行；除非后续有"构建后校验通过"的新关联记录，否则 --no-build 必须拒绝把它当作来源'
    }) | Out-Null
  }
  if ($newRecords.Count -eq 0) { return $null }
  $existing = [System.Collections.Generic.List[object]]::new()
  foreach ($record in @($document.revocations)) {
    $superseded = $false
    foreach ($new in $newRecords) {
      if (
        [string]$record.artifact_id -eq [string]$new.artifact_id -and
        [string]$record.profile -eq [string]$new.profile -and
        [string]$record.source_sha256 -eq [string]$new.source_sha256 -and
        [string]$record.stable_export_sha256 -eq [string]$new.stable_export_sha256
      ) {
        $superseded = $true
        break
      }
    }
    if (-not $superseded) { $existing.Add($record) }
  }
  foreach ($new in $newRecords) { $existing.Add($new) }
  $document.revocations = Remove-ExcessPackageArtifactRecords -Records $existing.ToArray() -Keep $script:ArtifactReleaseStatusKeepPerArtifact
  return (Write-PackageArtifactReleaseStatus -RepoPath $RepoPath -Document $document)
}

function ConvertTo-PackageIdentityPathKey {
  param([AllowEmptyString()][string]$Path)

  # 身份键里的路径一律用"仓库相对 posix 小写"形式，避免分隔符/大小写造成同一条目匹配不到。
  if (-not $Path) { return '' }
  $normalized = (ConvertTo-IdentityPosixPath ([string]$Path)).TrimStart('/')
  if ($normalized.StartsWith('./')) { $normalized = $normalized.Substring(2) }
  return $normalized.ToLowerInvariant()
}

<#
  禁用记录匹配口径 = artifact + profile + **声明的来源路径** + 内容身份。
  路径必须参与匹配：同一份字节被放到不同声明路径时，它们是不同的声明输入，
  不能因为字节相同就跨路径互相禁用（否则一个夹具失败会误封另一个无关用例）。
#>
function Get-PackageArtifactRevocation {
  param(
    [Parameter(Mandatory = $true)]$StatusDocument,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [Parameter(Mandatory = $true)][string]$Profile,
    [string]$SourcePath,
    [string]$SourceSha256,
    [string]$StableExportSha256,
    [string]$ReceiptPath
  )

  $sourceKey = ConvertTo-PackageIdentityPathKey $SourcePath
  foreach ($record in @($StatusDocument.revocations)) {
    if ([string]$record.artifact_id -ne $ArtifactId) { continue }
    if ([string]$record.profile -ne $Profile) { continue }
    $recordPathKey = ConvertTo-PackageIdentityPathKey ([string]$record.source_path)
    if ($recordPathKey -and $sourceKey -and $recordPathKey -ne $sourceKey) { continue }
    $matched = $false
    if ($SourceSha256 -and [string]$record.source_sha256 -eq $SourceSha256) { $matched = $true }
    if ($StableExportSha256 -and [string]$record.stable_export_sha256 -eq $StableExportSha256) { $matched = $true }
    if ($ReceiptPath -and [string]$record.receipt_path) {
      $recorded = ([string]$record.receipt_path).Replace('\', '/').ToLowerInvariant()
      $current = ([string]$ReceiptPath).Replace('\', '/').ToLowerInvariant()
      if ($recorded -eq $current) { $matched = $true }
    }
    if ($matched) { return $record }
  }
  return $null
}

function Get-PackageArtifactAssociation {
  param(
    [Parameter(Mandatory = $true)]$StatusDocument,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [Parameter(Mandatory = $true)][string]$Profile,
    [string]$SourcePath,
    [string]$SourceSha256,
    [string]$StableExportSha256,
    [string]$SourceSnapshotDigest,
    [string]$BuildInputDigest
  )

  $sourceKey = ConvertTo-PackageIdentityPathKey $SourcePath
  foreach ($record in @($StatusDocument.associations)) {
    if ([string]$record.artifact_id -ne $ArtifactId) { continue }
    if ([string]$record.profile -ne $Profile) { continue }
    $recordPathKey = ConvertTo-PackageIdentityPathKey ([string]$record.source_path)
    if ($recordPathKey -and $sourceKey -and $recordPathKey -ne $sourceKey) { continue }
    $identityMatched = $false
    if ($SourceSha256 -and [string]$record.source_sha256 -eq $SourceSha256) { $identityMatched = $true }
    if ($StableExportSha256 -and [string]$record.stable_export_sha256 -eq $StableExportSha256) { $identityMatched = $true }
    if (-not $identityMatched) { continue }
    # 关联必须绑定到**本次**快照与构建输入摘要；摘要缺失时不接受（未确认不放行）。
    if (-not $SourceSnapshotDigest) { continue }
    if (-not $BuildInputDigest) { continue }
    if ([string]$record.source_snapshot_digest -ne $SourceSnapshotDigest) { continue }
    if ([string]$record.build_input_digest -ne $BuildInputDigest) { continue }
    return $record
  }
  return $null
}

<#
  清理"已被一次构建后校验通过的新构建取代"的禁用记录。
  调用方负责只传**可清理**的条目（例如：生产者本次真正重跑 / 产物文件在本次运行中被重写 /
  产物来源本身就落在本次已验证不变的源码快照范围内）。函数本身不再放宽，只按 identity 清理。
#>
function Clear-PackageArtifactRevocations {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [AllowEmptyCollection()][object[]]$Entries = @()
  )

  if (-not $Entries -or @($Entries).Count -eq 0) { return @() }
  $read = Read-PackageArtifactReleaseStatus -RepoPath $RepoPath
  $document = $read.document
  $cleared = [System.Collections.Generic.List[object]]::new()
  $kept = [System.Collections.Generic.List[object]]::new()
  foreach ($record in @($document.revocations)) {
    $remove = $false
    foreach ($entry in @($Entries)) {
      if ([string]$record.artifact_id -ne [string]$entry.artifact_id) { continue }
      if ([string]$record.profile -ne $Configuration) { continue }
      $sourceMatched = ($entry.source_sha256 -and [string]$record.source_sha256 -eq [string]$entry.source_sha256)
      $stableMatched = ($entry.stable_export_sha256 -and [string]$record.stable_export_sha256 -eq [string]$entry.stable_export_sha256)
      if ($sourceMatched -or $stableMatched) { $remove = $true; break }
    }
    if ($remove) { $cleared.Add($record) } else { $kept.Add($record) }
  }
  if ($cleared.Count -eq 0) { return @() }
  $document.revocations = $kept.ToArray()
  [void](Write-PackageArtifactReleaseStatus -RepoPath $RepoPath -Document $document)
  return $cleared.ToArray()
}

function Assert-PackageArtifactNotRevoked {
  param(
    [Parameter(Mandatory = $true)]$StatusDocument,
    [Parameter(Mandatory = $true)][string]$ArtifactId,
    [Parameter(Mandatory = $true)][string]$Profile,
    [Parameter(Mandatory = $true)][string]$Target,
    [string]$SourcePath,
    [string]$SourceSha256,
    [string]$StableExportSha256,
    [string]$ReceiptPath
  )

  $record = Get-PackageArtifactRevocation -StatusDocument $StatusDocument -ArtifactId $ArtifactId -Profile $Profile `
    -SourcePath $SourcePath -SourceSha256 $SourceSha256 -StableExportSha256 $StableExportSha256 -ReceiptPath $ReceiptPath
  if ($record) {
    throw (New-IdentityFailureException -Category 'ARTIFACT-REVOKED' `
      -Detail ("artifact={0} target={1} profile={2} 的内容身份被登记为不可发布来源（来自运行 {3}：{4}；原因：{5}）" -f `
        $ArtifactId, $Target, $Profile, [string]$record.run_id, [string]$record.reason, [string]$record.category) `
      -Remediation '该产物来自"输入已被观察到变化"的运行，属于混合输入产物。执行一次**正常构建**打包（不要用 --no-build 复用既有二进制/导出物）以生成新的、与当前快照绑定的产物')
  }
  return $true
}

function Test-IdentityFalseValue {
  param([AllowNull()]$Value)

  if ($null -eq $Value) { return $false }
  if ($Value -is [bool]) { return (-not $Value) }
  $text = ([string]$Value).Trim().ToLowerInvariant()
  return ($text -eq 'false')
}

<#
  报告消费侧门禁（裁决第 2、9 条）：**成功报告与有效发布收据**可以拒绝，
  失败诊断与 release_eligible=false 的报告不得被当作发布来源/发布收据。
  历史（治理前）报告没有该字段时不据此外推，返回 evidence = 'release-eligibility-field-absent'。
#>
function Assert-PackageReportReleaseEligible {
  param(
    [Parameter(Mandatory = $true)]$Report,
    [string]$ReportPath,
    [string]$Purpose = 'release'
  )

  $reasons = [System.Collections.Generic.List[string]]::new()
  $kind = ''
  if ($Report.PSObject.Properties['kind']) { $kind = [string]$Report.kind }
  if ($kind -eq $script:PackageFailureReportKind) {
    $reasons.Add(('报告本身是失败诊断（kind={0}），不是可发布收据' -f $kind))
  }
  if ([string]$Report.report_status -eq 'failed') {
    $reasons.Add('report_status=failed')
  }
  $evidence = 'release-eligibility-field-absent'
  if ($Report.PSObject.Properties['release_eligibility']) {
    $evidence = 'release_eligibility'
    if (Test-IdentityFalseValue -Value $Report.release_eligibility.release_eligible) {
      $reasons.Add('release_eligibility.release_eligible=false')
      foreach ($reason in @($Report.release_eligibility.reasons)) {
        if ($reason) { $reasons.Add(('  - ' + [string]$reason)) }
      }
    }
  } elseif ($Report.PSObject.Properties['release_eligible']) {
    $evidence = 'release_eligible'
    if (Test-IdentityFalseValue -Value $Report.release_eligible) {
      $reasons.Add('release_eligible=false')
    }
  }

  if ($reasons.Count -gt 0) {
    $reportLabel = $(if ($ReportPath) { $ReportPath } else { '(in-memory report)' })
    $detailLines = [System.Collections.Generic.List[string]]::new()
    $detailLines.Add(('purpose={0} report={1}' -f $Purpose, $reportLabel))
    foreach ($reason in $reasons) { $detailLines.Add('- ' + [string]$reason) }
    throw (New-IdentityFailureException -Category 'REPORT-NOT-RELEASE-ELIGIBLE' `
      -Detail ($detailLines -join "`n") `
      -Remediation '不要用失败诊断或"未确认输入"的报告作为发布收据。重新执行一次正常构建打包（不要用 --no-build/--skip 复用既有产物），或先修复报告里列出的未确认项')
  }
  return [pscustomobject]@{
    report_id = [string]$Report.report_identity.report_id
    release_eligible = $true
    evidence = $evidence
    verified = $true
  }
}

function New-PackageReleaseGate {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    # pass | fail | not-confirmed
    [Parameter(Mandatory = $true)][string]$Result,
    [string]$Evidence,
    [string]$Note
  )

  return [pscustomobject]@{
    gate = $Name
    result = $Result
    evidence = $Evidence
    note = $Note
  }
}

<#
  发布资格：把"能不能发布"拆成**可分别回答**的门，每门给出 pass/fail/not-confirmed 与证据。
  release_eligible 只在所有门都 pass 时为 true；任何 not-confirmed 都**不得**乐观放行。
  字段名沿用裁决给出的同义口径（live_worktree_changed / build_snapshot_integrity /
  build_input_digest / validation_snapshot_digest / release_eligible）。
#>
function New-PackageReleaseEligibility {
  param(
    [Parameter(Mandatory = $true)][string]$Configuration,
    [AllowNull()]$LiveWorktreeChanged,
    [Parameter(Mandatory = $true)][string]$BuildSnapshotIntegrity,
    [AllowNull()][string]$SourceSnapshotDigest,
    [AllowNull()][string]$BuildInputDigest,
    [AllowNull()][string]$ValidationSnapshotDigest,
    [AllowEmptyCollection()][object[]]$Gates = @(),
    [AllowEmptyCollection()][object[]]$ArtifactProvenance = @(),
    # PR-PKG-02（第八轮裁决）：已核验的**外部构建输入**（registry/crate/version/checksum）。
    [AllowEmptyCollection()][object[]]$ExternalInputs = @(),
    [AllowEmptyCollection()][string[]]$Reasons = @(),
    [AllowEmptyCollection()][string[]]$Limitations = @(),
    [AllowEmptyCollection()][object[]]$ClearedRevocations = @()
  )

  $notPassed = @($Gates | Where-Object { [string]$_.result -ne 'pass' })
  return [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    release_eligible = ($notPassed.Count -eq 0)
    configuration = $Configuration
    live_worktree_changed = (ConvertTo-IdentityBoolOrUnknown -Value $LiveWorktreeChanged)
    live_worktree_changed_note = 'true/false = 构建前后两次独立采样之间是否观察到活动树变化；未声明源码快照范围时为 not-confirmed（未确认），绝不写成 false。'
    build_snapshot_integrity = $BuildSnapshotIntegrity
    build_snapshot_integrity_note = 'verified-unchanged-live-tree = 前后两次采样逐文件一致（本实现唯一的稳定证据）；not-established = 观察到变化；not-confirmed = 无法判定。'
    source_snapshot_digest = $SourceSnapshotDigest
    build_input_digest = $BuildInputDigest
    validation_snapshot_digest = $ValidationSnapshotDigest
    validation_snapshot_digest_note = '校验采样（构建后第二次独立枚举）得到的输入摘要；它是"用于校验的采样"，不是"构建所用快照的副本摘要"。'
    immutable_build_snapshot = $false
    immutable_build_snapshot_note = '本实现直接从活动工作树构建（cargo working_dir=仓库根），没有"从不可变快照副本构建"。因此只有整树字节一致才认定输入稳定；活动树在采样之间被编辑即拒绝发布。'
    gates = @($Gates)
    gates_note = '任一门不是 pass（fail 或 not-confirmed）⇒ release_eligible=false。not-confirmed 表示"未能确认是否受影响"，不是通过。'
    artifact_provenance = @($ArtifactProvenance)
    # 外部构建输入（PR-PKG-02）：来源/版本/checksum 已与 Cargo.lock 逐字比对通过。
    # 措辞刻意区分"已验证"与"完整"：`source_snapshot_verified` 只说明**源码快照侧**的检查
    # 通过了（范围/稳定性/roots），它**不**声称"源码快照包含了全部二进制来源"——
    # 第三方 registry 产物由 `external_inputs` 单独承担。
    external_inputs = @($ExternalInputs)
    external_inputs_verified = $(if (@($ExternalInputs).Count -gt 0) { $true } else { 'not-applicable' })
    external_inputs_note = 'external_inputs_verified=true 表示本次出现的第三方 registry 构建产物均已按 manifest 登记的 crate/version/checksum 与 Cargo.lock 逐字比对通过；not-applicable 表示本次没有出现需要登记的外部输入。'
    source_snapshot_verified = (@($Gates | Where-Object { @('declared_source_snapshot_scope','source_input_stability','declared_roots_present') -contains [string]$_.name -and [string]$_.result -ne 'pass' }).Count -eq 0)
    source_snapshot_verified_note = 'source_snapshot_verified=true 只表示**源码快照侧**的范围、稳定性与 roots 检查通过；它**不**等于"源码快照包含全部二进制来源"。'
    cleared_artifact_revocations = @($ClearedRevocations)
    reasons = @($Reasons)
    limitations = @($Limitations)
    refusal_note = 'release_eligible=false 的报告不是可发布收据：不得据此进入签名/安装/分发；也不得用 quiescent=false 之类的字段给它补一张可发布收据。'
  }
}

<#
<#
  "已声明不属于构建输入"的路径（裁决第 8 条）：日志 / 临时输出 / 运行态 / 构建输出目录。
  这些路径上的变化**不得**被误判为源码变化；与源码快照的排除规则不同，这份声明回答的是
  "它为什么不算构建输入"，因此要带 reason，使范围规则显式、可审查。
  manifest 的 release_policy.non_build_input_paths 可追加（同 pattern 覆盖 reason）。
#>
$script:PackageNonBuildInputDeclarations = @(
  @{ pattern = '(^|/)tmp(/|$)'; reason = '临时脚本、日志与打包中间产物（冻结记录/报告/失败隔离区/备份都落在这里）'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)target(/|$)'; reason = '各 crate 的构建输出目录：其身份由 payload_digest 与产物—快照关联记录承担，不是源码'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)logs?(/|$)'; reason = '运行日志输出'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)sessions?(/|$)'; reason = '会话运行态数据'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)\.coolzhu(/|$)'; reason = '运行态（用户会话/sqlite/插件运行时状态），仅 .coolzhu/plugins 由 allow_path_patterns 放行'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = 'web-sessions'; reason = '会话数据库/JSON 运行态'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)__pycache__(/|$)'; reason = 'Python 字节码缓存（不是源码）'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)package(/|$)'; reason = '打包输出目录（可发布产物本身，不是构建输入）'; declared_by = 'scripts/lib/build-identity.ps1#default' }
  @{ pattern = '(^|/)dist(/|$)'; reason = '分发输出目录（不是构建输入）'; declared_by = 'scripts/lib/build-identity.ps1#default' }
)

function Get-PackageNonBuildInputDeclarations {
  param([AllowNull()]$ManifestData)

  $declarations = [System.Collections.Generic.List[object]]::new()
  $seen = @{}
  foreach ($entry in @($script:PackageNonBuildInputDeclarations)) {
    $declarations.Add([pscustomobject]@{
      pattern = [string]$entry.pattern
      reason = [string]$entry.reason
      declared_by = [string]$entry.declared_by
    }) | Out-Null
    $seen[[string]$entry.pattern] = $true
  }
  if ($ManifestData -and $ManifestData.PSObject.Properties['release_policy'] -and $ManifestData.release_policy) {
    foreach ($entry in @($ManifestData.release_policy.non_build_input_paths)) {
      if (-not $entry) { continue }
      $pattern = [string]$entry.pattern
      if (-not $pattern) { continue }
      if ($seen.ContainsKey($pattern)) {
        # 同 pattern 由 manifest 覆盖 reason（声明方是 manifest）。
        $existing = @($declarations | Where-Object { $_.pattern -eq $pattern })
        foreach ($item in $existing) {
          $item.reason = ('[manifest 覆盖] ' + [string]$entry.reason)
          $item.declared_by = 'config/package-manifest.json#release_policy.non_build_input_paths'
        }
        continue
      }
      $seen[$pattern] = $true
      $declarations.Add([pscustomobject]@{
        pattern = $pattern
        reason = [string]$entry.reason
        declared_by = 'config/package-manifest.json#release_policy.non_build_input_paths'
      }) | Out-Null
    }
  }
  return $declarations.ToArray()
}

<#
  路径的分类（裁决第 7、8 条的可审查范围规则）：
    in-declared-source-snapshot-scope  在声明 roots 下且未被排除 ⇒ 属于源码快照范围
    declared-source-snapshot-excluded  在 roots 下但命中排除规则（例如日志/运行态）
    declared-build-input               显式声明的构建输入文件（锁文件/构建入口/安装器定义）
    declared-non-build-input           显式声明"不属于构建输入"的路径（日志/临时输出）
    outside-declared-scope             不在任何声明 roots 之下
    undetermined                       规则不足，无法判定（不乐观放行）
#>
function Get-PackagePathClassification {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [object]$Scope,
    [AllowEmptyCollection()][string[]]$BuildInputFiles = @(),
    [AllowEmptyCollection()][string[]]$NonBuildInputPatterns = @()
  )

  $normalized = (ConvertTo-IdentityPosixPath ([string]$Path)).TrimStart('/')
  if ($normalized.StartsWith('./')) { $normalized = $normalized.Substring(2) }
  $result = [ordered]@{
    path = $normalized
    classification = 'undetermined'
    matched_rule = $null
    declared_root = $null
    matched_non_build_input_pattern = $null
  }

  # 先定位所属声明 root，并把匹配探针定为 **root 相对** 路径（与快照的 match_base 一致）：
  # 否则工作区/夹具恰好位于 tmp/ 之类的被排除命名之下时会被整体误判。
  $matchedRoot = $null
  if ($Scope) {
    foreach ($root in @($Scope.roots)) {
      if ($normalized -eq $root -or $normalized.StartsWith("$root/")) {
        if (-not $matchedRoot -or $root.Length -gt $matchedRoot.Length) { $matchedRoot = $root }
      }
    }
  }
  $result.declared_root = $matchedRoot
  $probe = $(if ($matchedRoot) { $normalized.Substring($matchedRoot.Length).TrimStart('/') } else { $normalized })
  if (-not $probe) { $probe = $normalized }
  foreach ($pattern in @($NonBuildInputPatterns)) {
    if ($pattern -and $probe -match $pattern) { $result.matched_non_build_input_pattern = [string]$pattern; break }
  }

  # 顺序很重要：**显式声明为构建输入**的路径（即使位于 tmp/ 之类的"非构建输入"命名之下）
  # 仍然属于构建输入，改动它必须被当成相关输入变化。
  foreach ($input in @($BuildInputFiles)) {
    $candidate = (ConvertTo-IdentityPosixPath ([string]$input)).TrimStart('/')
    if ($candidate -and $normalized -eq $candidate) {
      $result.classification = 'declared-build-input'
      $result.matched_rule = 'manifest.build_inputs / artifact.export'
      return [pscustomobject]$result
    }
  }

  if (-not $Scope) {
    $result.classification = $(if ($result.matched_non_build_input_pattern) { 'declared-non-build-input' } else { 'undetermined' })
    $result.matched_rule = $(if ($result.matched_non_build_input_pattern) { 'release_policy.non_build_input_paths' } else { 'no-declared-source-snapshot-scope' })
    return [pscustomobject]$result
  }

  if (-not $matchedRoot) {
    $result.classification = $(if ($result.matched_non_build_input_pattern) { 'declared-non-build-input' } else { 'outside-declared-scope' })
    $result.matched_rule = $(if ($result.matched_non_build_input_pattern) { 'release_policy.non_build_input_paths' } else { 'not-under-declared-roots' })
    return [pscustomobject]$result
  }

  # 排除/放行规则按 root 相对路径匹配（与 New-SourceSnapshot 的 match_base=root-relative 一致）。
  $segments = @($probe -split '/' | Where-Object { $_ })
  $excludedBy = $null
  $allowedBy = $null
  $accumulated = ''
  foreach ($segment in $segments) {
    $accumulated = $(if ($accumulated) { "$accumulated/$segment" } else { $segment })
    if (-not $excludedBy) {
      $excludePattern = $null
      foreach ($pattern in @($Scope.exclude_path_patterns)) {
        if ($pattern -and $segment -match $pattern) { $excludePattern = $pattern; break }
        if ($pattern -and $accumulated -match $pattern) { $excludePattern = $pattern; break }
      }
      if ($excludePattern) { $excludedBy = $excludePattern }
    }
    if (-not $allowedBy) {
      foreach ($pattern in @($Scope.allow_path_patterns)) {
        if ($pattern -and ($segment -match $pattern -or $accumulated -match $pattern)) { $allowedBy = $pattern; break }
      }
    }
  }
  if ($excludedBy -and -not $allowedBy) {
    $result.classification = 'declared-source-snapshot-excluded'
    $result.matched_rule = $excludedBy
    return [pscustomobject]$result
  }
  $result.classification = 'in-declared-source-snapshot-scope'
  $result.matched_rule = ('root={0}' -f $matchedRoot)
  return [pscustomobject]$result
}

function Compare-BuildInputDescriptors {
  param(
    [AllowEmptyCollection()][string[]]$BaselineLines = @(),
    [AllowEmptyCollection()][string[]]$CurrentLines = @()
  )

  $baseline = @($BaselineLines | Where-Object { $_ })
  $current = @($CurrentLines | Where-Object { $_ })
  $added = [System.Collections.Generic.List[string]]::new()
  $removed = [System.Collections.Generic.List[string]]::new()
  foreach ($line in $current) {
    if ($baseline -notcontains $line) { $added.Add($line) }
  }
  foreach ($line in $baseline) {
    if ($current -notcontains $line) { $removed.Add($line) }
  }
  $addedSorted = Get-IdentityCanonicalLines -Lines $added.ToArray()
  $removedSorted = Get-IdentityCanonicalLines -Lines $removed.ToArray()
  return [pscustomobject]@{
    comparable = $true
    identical = ($addedSorted.Count -eq 0 -and $removedSorted.Count -eq 0)
    baseline_digest = Get-IdentityHashFromLines -Lines $baseline
    current_digest = Get-IdentityHashFromLines -Lines $current
    added = $addedSorted
    removed = $removedSorted
    changed_descriptor_lines = @($addedSorted + $removedSorted)
  }
}

function Get-PackageFailureCategory {
  param([AllowEmptyString()][string]$Message)

  $text = [string]$Message
  $match = [regex]::Match($text, '^\s*\[([A-Z0-9][A-Z0-9\-]*)\]')
  if (-not $match.Success) {
    $match = [regex]::Match($text, '\[([A-Z][A-Z0-9\-]{2,})\]')
  }
  if ($match.Success) { return $match.Groups[1].Value }
  return 'UNCLASSIFIED'
}

<#
  把本次运行已产生的暂存产物移入失败隔离区（分区）：
  * 只允许移动"本次运行自己初始化过的 staging"（调用方用 $stagingInitialized 保证）；
  * 隔离区位于 tmp/package-failures/<run-id>/payload，与包根互相包含时拒绝移动；
  * 移走之后包根为空 ⇒ 现场不再存在"看起来像可发布包"的目录；
  * 隔离区**永不自动晋升**：没有任何路径会把隔离区内容复制回包根。
#>
function Move-PackageStagingToQuarantine {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$PackageRootPath,
    [Parameter(Mandatory = $true)][string]$RunId
  )

  $quarantineRoot = Get-PackageFailureQuarantineRoot -RepoPath $RepoPath
  $destination = Join-Path (Join-Path $quarantineRoot $RunId) 'payload'
  $result = [ordered]@{
    attempted = $false
    moved = $false
    quarantine_root = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $quarantineRoot)
    destination = $null
    destination_relative = $null
    moved_file_count = 0
    files = @()
    skipped_reason = $null
    auto_promotion = 'never'
  }
  if (-not (Test-Path -LiteralPath $PackageRootPath -PathType Container)) {
    $result.skipped_reason = 'staging 根目录不存在：本次运行没有产出暂存产物'
    return [pscustomobject]$result
  }
  if (
    (Test-PackagePathInsideRoot -Path $destination -Root $PackageRootPath) -or
    (Test-PackagePathInsideRoot -Path $PackageRootPath -Root $quarantineRoot)
  ) {
    $result.skipped_reason = '隔离区与 PackageRoot 互相包含，拒绝移动（避免分区边界被误判）'
    return [pscustomobject]$result
  }
  $children = @(Get-ChildItem -LiteralPath $PackageRootPath -Force -ErrorAction SilentlyContinue)
  $result.attempted = $true
  if ($children.Count -eq 0) {
    $result.skipped_reason = 'staging 为空：本次运行在失败前没有产出暂存产物'
    return [pscustomobject]$result
  }
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  foreach ($child in $children) {
    Move-Item -LiteralPath $child.FullName -Destination $destination -Force
  }
  $prefix = $destination.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
  $files = [System.Collections.Generic.List[object]]::new()
  foreach ($file in @(Get-ChildItem -LiteralPath $destination -File -Recurse -Force)) {
    $files.Add([pscustomobject]@{
      path = (ConvertTo-IdentityPosixPath $file.FullName.Substring($prefix.Length))
      length = [long]$file.Length
      sha256 = (Get-IdentityFileHash -Path $file.FullName)
    })
  }
  $result.moved = $true
  $result.destination = $destination
  $result.destination_relative = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $destination)
  $result.moved_file_count = @($files).Count
  $result.files = @($files)
  return [pscustomobject]$result
}

<#
  失败诊断文档（裁决第 3 条的最小字段集）。
  与成功报告**分开落点**（failures/ 分区），因此不会覆盖上一份成功报告；
  文档自身仍带 report_identity（唯一 ID + 可重算内容哈希），可被独立核对。
#>
function New-PackageFailureDiagnostic {
  param(
    [Parameter(Mandatory = $true)][string]$RunId,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ManifestPath,
    [string]$PackageRootPath,
    [Parameter(Mandatory = $true)]$Failure,
    [AllowNull()]$BuildContext,
    [AllowNull()]$DeclaredInputScope,
    [AllowNull()]$SourceSnapshotPre,
    [AllowNull()]$SourceSnapshotPost,
    [AllowNull()]$SnapshotComparison,
    [AllowNull()]$BuildInputBaseline,
    [AllowNull()]$BuildInputCurrent,
    [AllowNull()]$BuildInputComparison,
    [AllowNull()]$InputScopeConfirmation,
    [AllowEmptyCollection()][object[]]$StageRecords = @(),
    [AllowNull()]$Quarantine,
    [AllowNull()]$PreviousPointerState,
    [AllowNull()]$LatestPointerVerification,
    [AllowEmptyCollection()][object[]]$ArtifactRevocations = @(),
    [AllowNull()]$ReleaseEligibility,
    [AllowEmptyCollection()][object[]]$ArtifactResults = @(),
    [AllowEmptyCollection()][object[]]$ExportSummaries = @(),
    # RD4-06：失败发生在哪个阶段（prepare / build）+ 实际改变的路径（前后摘要 + 发生阶段）
    # + 准备/冻结状态。裁决 §3.1 要求诊断显著列出这三项与 release_eligible=false。
    [string]$RunPhase = 'build',
    [AllowEmptyCollection()][object[]]$ChangedPaths = @(),
    [AllowNull()]$PreparationAndFreeze = $null
  )

  $manifestFullPath = [System.IO.Path]::GetFullPath($ManifestPath)
  $packageRootRelative = $null
  if ($PackageRootPath -and (Test-Path -LiteralPath $PackageRootPath)) {
    $packageRootRelative = Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $PackageRootPath
  }

  # 4) 差异必须标明是"已观察变化"，不得称为完整写入历史。
  $observedChanges = [ordered]@{
    evidence_kind = 'observed-between-two-samples'
    is_complete_write_history = $false
    note = '这里记录的是"构建前后两次独立采样之间的差异"，属于**已观察变化**：它既不是完整写入历史（采样之间的写入无法穷尽），也不主张对未列入路径做过任何判定。'
    source_snapshot = [ordered]@{
      comparable = $(if ($SnapshotComparison) { $true } else { $false })
      pre_build_digest = $(if ($SourceSnapshotPre) { [string]$SourceSnapshotPre.source_snapshot_digest } else { $null })
      post_build_digest = $(if ($SourceSnapshotPost) { [string]$SourceSnapshotPost.source_snapshot_digest } else { $null })
      identical = $(if ($SnapshotComparison) { $SnapshotComparison.identical } else { 'not-confirmed' })
      added_count = $(if ($SnapshotComparison) { [int]$SnapshotComparison.added_count } else { 0 })
      removed_count = $(if ($SnapshotComparison) { [int]$SnapshotComparison.removed_count } else { 0 })
      modified_count = $(if ($SnapshotComparison) { [int]$SnapshotComparison.modified_count } else { 0 })
      added_paths = $(if ($SnapshotComparison) { @($SnapshotComparison.added) } else { @() })
      removed_paths = $(if ($SnapshotComparison) { @($SnapshotComparison.removed) } else { @() })
      modified_paths = $(if ($SnapshotComparison) { @($SnapshotComparison.modified) } else { @() })
    }
    build_inputs = [ordered]@{
      comparable = $(if ($BuildInputComparison) { [bool]$BuildInputComparison.comparable } else { $false })
      pre_build_digest = $(if ($BuildInputBaseline) { [string]$BuildInputBaseline.digest } else { $null })
      post_build_digest = $(if ($BuildInputCurrent) { [string]$BuildInputCurrent.digest } else { $null })
      identical = $(if ($BuildInputComparison) { [bool]$BuildInputComparison.identical } else { 'not-confirmed' })
      changed_descriptor_lines = $(if ($BuildInputComparison) { @($BuildInputComparison.changed_descriptor_lines) } else { @() })
      unconfirmed_reason = $(if ($BuildInputCurrent) { [string]$BuildInputCurrent.unconfirmed_reason } else { 'baseline-not-available' })
    }
    classification = $(if ($SnapshotComparison) { @($SnapshotComparison.classification) } else { @() })
    classification_note = '逐路径给出范围分类（in-declared-source-snapshot-scope / declared-source-snapshot-excluded / declared-build-input / declared-non-build-input / outside-declared-scope / undetermined），使"是否属于构建输入"可审查；已声明不属于构建输入的日志/临时输出变化不据此判为源码变化。'
  }

  $diagnostic = [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = $script:PackageFailureReportKind
    report_status = 'failed'
    run_id = $RunId
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    configuration = $Configuration
    manifest = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $manifestFullPath)
    manifest_sha256 = Get-IdentityFileHash -Path $manifestFullPath
    package_root = $packageRootRelative
    generated_by = 'scripts/package-all.ps1'
    failure_diagnostic_note = '这是**失败诊断**，不是成功报告、也不是可发布收据：它存在的唯一目的是让"为什么没有获准发布"可被核对。'
    release_eligible = $false
    release_eligibility = $ReleaseEligibility
    build_context = $BuildContext
    # RD4-06：失败发生在哪个阶段 + 准备/冻结状态（裁决 §3.1）
    run_phase = $RunPhase
    run_phase_note = 'prepare = 准备/冻结阶段；build = 正式构建阶段（消费冻结输入）'
    preparation_and_freeze = $PreparationAndFreeze
    # RD4-06：**实际改变的路径**（逐条带前后摘要与发生阶段）——诊断显著列出，不藏在细节里。
    changed_paths = @($ChangedPaths)
    changed_paths_rule = '逐条给出 path / kind / before_summary / after_summary / phase；证据类型是"两次采样之间的已观察变化"（observed-between-two-samples），不是完整写入历史'
    changed_path_count = @($ChangedPaths).Count
    # 3) 阶段与失败原因
    failure = [ordered]@{
      stage = [string]$Failure.stage
      category = [string]$Failure.category
      message = [string]$Failure.message
      detail_lines = @($Failure.detail_lines)
      exception_type = [string]$Failure.exception_type
      remediation_hint = [string]$Failure.remediation_hint
    }
    # 3) 各阶段退出码（真实退出码；非进程类阶段显式记 not-applicable，不伪造 0）
    stage_exit_codes = @($StageRecords)
    stage_exit_codes_note = '构建阶段记录真实进程退出码；非进程阶段记 outcome，退出码为 not-applicable（不伪造 0）。'
    # 3) 声明的源码／构建输入范围
    declared_input_scope = $DeclaredInputScope
    # 3)+7) 输入范围是否可确定
    input_scope_confirmation = $InputScopeConfirmation
    observed_input_changes = $observedChanges
    # 3) 已产生但未获准发布的临时产物（与可发布产物分区）
    produced_not_released = [ordered]@{
      release_eligible = $false
      partition = 'failure-quarantine'
      auto_promotion = 'never'
      quarantine = $Quarantine
      note = '这些文件是本次失败运行已产生的临时产物，仅作诊断保留；它们位于失败隔离区，与可发布产物分区，且没有任何自动晋升路径把它们变成发布物。'
    }
    # 5) 不得覆盖上一份成功报告 / 不得更新"最新有效包"
    previous_success_report = [ordered]@{
      latest_pointer_path = $(if ($PreviousPointerState) { [string]$PreviousPointerState.path } else { $null })
      pointer_exists = $(if ($PreviousPointerState) { [bool]$PreviousPointerState.exists } else { $false })
      previous_report_id = $(if ($PreviousPointerState) { [string]$PreviousPointerState.report_id } else { $null })
      previous_report_path = $(if ($PreviousPointerState) { [string]$PreviousPointerState.report_path } else { $null })
      previous_content_sha256 = $(if ($PreviousPointerState) { [string]$PreviousPointerState.content_sha256 } else { $null })
      overwritten_by_this_run = $false
      pointer_updated_by_this_run = $false
      pointer_verification = $LatestPointerVerification
    }
    latest_valid_package_pointer = $(if ($PreviousPointerState) { [string]$PreviousPointerState.path } else { $null })
    latest_valid_package_updated = $false
    signing_or_distribution = 'not-attempted'
    signing_or_distribution_note = '失败运行不进入签名/安装/分发步骤；本诊断本身也不构成发布证据。'
    # 9) 由失败运行登记的禁用内容身份（不稳定构建的裸产物不得再被 --no-build 当作来源）
    artifact_revocations = @($ArtifactRevocations)
    artifact_revocations_note = '这些内容身份来自"输入已被观察到变化"的运行；后续 --no-build 命中同一内容身份一律拒绝，除非有新的"构建后校验通过"关联记录。'
    artifacts_observed = @($ArtifactResults)
    exported_artifacts_observed = @($ExportSummaries)
    report_identity_note = '失败诊断与成功报告共用同一套报告身份治理：report_id 唯一、content_sha256 可从落盘文档重算。'
  }
  return $diagnostic
}

<#
  运行状态台账：**不替代** latest 指针（成功报告指针），只回答"最近一次运行成不成、有没有可发布收据"。
  失败运行写 latest-run-<config>.json 但**不写** latest-<config>.json。
#>
function Write-PackageRunStatus {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration,
    # succeeded | failed
    [Parameter(Mandatory = $true)][string]$Outcome,
    [Parameter(Mandatory = $true)][bool]$ReleaseEligible,
    [string]$RunId,
    [string]$ReportId,
    [string]$ReportPath,
    [string]$ReportContentSha256,
    [string]$FailureReportId,
    [string]$FailureReportPath,
    [string]$FailureReportContentSha256,
    [string]$Stage,
    [string]$Category,
    [string]$SourceSnapshotDigest,
    [string]$BuildInputDigest,
    [string]$ValidationSnapshotDigest,
    [string]$Note
  )

  $path = Get-PackageRunStatusPath -RepoPath $RepoPath -Configuration $Configuration
  $parent = Split-Path -Parent $path
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
  $pointers = Read-PackageLatestPointerState -RepoPath $RepoPath -Configuration $Configuration
  $status = [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = 'package-run-status'
    configuration = $Configuration
    outcome = $Outcome
    release_eligible = $ReleaseEligible
    run_id = $RunId
    stage = $Stage
    category = $Category
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    report_id = $ReportId
    report_path = $ReportPath
    report_content_sha256 = $ReportContentSha256
    failure_report_id = $FailureReportId
    failure_report_path = $FailureReportPath
    failure_report_content_sha256 = $FailureReportContentSha256
    source_snapshot_digest = $SourceSnapshotDigest
    build_input_digest = $BuildInputDigest
    validation_snapshot_digest = $ValidationSnapshotDigest
    # 失败运行不更新"最新有效包"指针：台账如实记录指针仍然指向哪一份成功报告。
    latest_valid_package_pointer = $pointers.path
    latest_valid_package_pointer_updated_by_this_run = ($Outcome -eq 'succeeded')
    latest_valid_package_report_id = $pointers.report_id
    latest_valid_package_report_path = $pointers.report_path
    note = $Note
  }
  [System.IO.File]::WriteAllText($path, ($status | ConvertTo-Json -Depth 6), [System.Text.UTF8Encoding]::new($false))
  return [pscustomobject]@{ path = $path; status = $status }
}

<#
  读取"最新有效包"指针的**当前**状态（只读，不写）。
  失败诊断要证明"没有覆盖上一份成功报告 / 没有更新指针"，就需要在失败前后各读一次。
#>
function Read-PackageLatestPointerState {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$Configuration
  )

  $path = Get-PackageLatestPointerPath -RepoPath $RepoPath -Configuration $Configuration
  $state = [ordered]@{
    path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path)
    absolute_path = $path
    exists = $false
    file_sha256 = $null
    report_id = $null
    report_path = $null
    content_sha256 = $null
  }
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return [pscustomobject]$state }
  $state.exists = $true
  $state.file_sha256 = Get-IdentityFileHash -Path $path
  try {
    $parsed = Get-Content -Raw -LiteralPath $path -Encoding UTF8 | ConvertFrom-Json
  } catch {
    $state.report_id = 'unparsable-pointer'
    return [pscustomobject]$state
  }
  $state.report_id = [string]$parsed.report_id
  $state.report_path = [string]$parsed.report_path
  $state.content_sha256 = [string]$parsed.report_content_sha256
  return [pscustomobject]$state
}

function Assert-PackageReportContentHash {
  param([Parameter(Mandatory = $true)]$Report)

  if (-not $Report.PSObject.Properties['report_identity']) {
    throw (New-IdentityFailureException -Category 'REPORT-IDENTITY-MISSING' `
      -Detail '报告缺少 report_identity（唯一 ID + 内容哈希）' `
      -Remediation '用 scripts/package-all.ps1 重新生成报告；不要手工拼报告')
  }
  $recorded = [string]$Report.report_identity.content_sha256
  if (-not $recorded) {
    throw (New-IdentityFailureException -Category 'REPORT-IDENTITY-MISSING' `
      -Detail '报告没有 report_identity.content_sha256' `
      -Remediation '用 scripts/package-all.ps1 重新生成报告')
  }
  $recomputed = Get-PackageReportContentHash -Report $Report
  if ($recomputed -ne $recorded) {
    throw (New-IdentityFailureException -Category 'REPORT-CONTENT-MISMATCH' `
      -Detail ("报告内容哈希不一致（记录={0} 重算={1}）" -f $recorded, $recomputed) `
      -Remediation '报告被改写或生成器口径变化；以重新生成的报告为准，不要沿用旧引用')
  }
  return [pscustomobject]@{
    report_id = [string]$Report.report_identity.report_id
    content_sha256 = $recomputed
    verified = $true
  }
}

# ============================================================================
# RD4-06 准备/冻结分离（裁决 §3.1 / §C-24）
#
# 权威口径（round7-rulings-and-gates.md §3.1）：
#   * **不设** Cargo.lock / tauri.conf.json 的"构建中变更白名单"。
#   * **准备阶段**：解析／更新依赖、生成必要配置 → 展示变化并按既有变更政策确认 →
#     **冻结完整输入快照**。
#   * **正式构建阶段**：只消费冻结输入 → 构建、测试、导出、打包 →
#     **任一声明输入改变即拒绝发布**。
#   * Cargo 正式入口用**锁定语义**（既有离线要求之上的 `--locked`，等价 `--frozen`）。
#   * 生成式配置：能在准备阶段生成的，生成后**计入冻结输入**；需构建中产生的中间文件
#     放**已声明的派生输出位置**并记录生成来源；**不得**发现被改写后把它从输入清单
#     移除以换取绿色结果。
#   * 诊断显著列出**实际改变的路径、前后摘要、发生阶段**与 `release_eligible=false`。
#
# 落点：冻结记录 tmp/package-freeze/package-input-freeze-<config>-<freeze_id>.json（内容寻址、
# 一次性写入），指针 tmp/package-freeze/latest-<config>.json 由**准备阶段**更新。
# ============================================================================

$script:PackageInputFreezeRoot = 'tmp/package-freeze'
$script:PackagePrepareReportSubdir = 'prepare'
$script:PackagePrepareReportKind = 'package-preparation-record'
$script:PackageInputFreezeKind = 'package-input-freeze'
$script:PackageInputFreezePointerKind = 'package-input-freeze-pointer'
# 正式构建阶段的 Cargo 锁定语义（写进冻结记录，供审查与核对）。
$script:CargoLockSemantics = [ordered]@{
  prepare = '解析/更新依赖：默认只做只读解析核对（cargo metadata --locked --offline）；确需更新锁定内容时必须显式 -UpdateDependencies'
  build = '正式构建：--locked --offline（等价 --frozen：锁定 + 离线）；锁文件在构建期被改写即失败'
  no_whitelist = '--locked 在需要改锁文件时失败；本流程不提供"构建中改锁文件"的白名单'
}

function Get-PackageInputFreezeRoot {
  param([Parameter(Mandatory = $true)][string]$RepoPath)
  return (Join-Path ([System.IO.Path]::GetFullPath($RepoPath)) $script:PackageInputFreezeRoot)
}

function Get-PackagePrepareReportRoot {
  param([Parameter(Mandatory = $true)][string]$RepoPath)
  return (Join-Path (Get-PackageReportRootPath -RepoPath $RepoPath) $script:PackagePrepareReportSubdir)
}

<#
  从构建输入描述符行里取出"输入文件"的路径/长度/摘要（供前后摘要使用）。
  描述符行形如 input_file=<posix 相对路径>|length=<n>|sha256=<hex>。
#>
function Get-PackageBuildInputFileFacts {
  param([AllowEmptyCollection()][string[]]$DescriptorLines = @())

  $facts = [ordered]@{}
  foreach ($line in @($DescriptorLines)) {
    $match = [regex]::Match([string]$line, '^input_file=([^|]+)\|length=(\d+)\|sha256=([0-9a-fA-F]+)$')
    if (-not $match.Success) { continue }
    $facts[$match.Groups[1].Value] = [pscustomobject]@{
      length = [long]$match.Groups[2].Value
      sha256 = $match.Groups[3].Value.ToLowerInvariant()
    }
  }
  return $facts
}

<#
  "实际改变的路径"条目：带**前后摘要**与**发生阶段**（裁决 §3.1 最后一句）。
#>
function New-PackageChangedPathEntry {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Kind,
    [AllowNull()][string]$Before,
    [AllowNull()][string]$After,
    [Parameter(Mandatory = $true)][string]$Phase,
    [string]$Classification,
    [string]$Note
  )

  return [pscustomobject]@{
    path = $Path
    kind = $Kind
    before_summary = $Before
    after_summary = $After
    phase = $Phase
    classification = $Classification
    note = $Note
    evidence_kind = 'observed-between-two-samples'
  }
}

<#
  冻结记录文档（内容寻址：freeze_id = 所有冻结输入的规范摘要）。
  它登记的是"正式构建阶段被允许消费的完整输入集合"：构建输入描述符（逐文件摘要）、
  准备阶段生成的配置（生成后计入冻结输入）、声明派生输出位置与生成来源。
#>
function New-PackageInputFreezeDocument {
  param(
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][string]$RunId,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ManifestPath,
    [AllowNull()][string]$SourceSnapshotDigest,
    [Parameter(Mandatory = $true)][string]$BuildInputDigest,
    [AllowEmptyCollection()][string[]]$BuildInputDescriptorLines = @(),
    [AllowNull()]$CargoIdentity,
    [AllowNull()]$Preparation = $null,
    [AllowEmptyCollection()][object[]]$GeneratedConfigs = @(),
    [AllowEmptyCollection()][object[]]$DerivedOutputs = @(),
    [AllowEmptyCollection()][object[]]$PreviousFreeze = @(),
    [AllowEmptyCollection()][object[]]$ChangedPaths = @(),
    [bool]$ChangesAccepted = $false
  )

  $manifestFull = [System.IO.Path]::GetFullPath($ManifestPath)
  $generatedLines = [System.Collections.Generic.List[string]]::new()
  foreach ($entry in @($GeneratedConfigs)) {
    $generatedLines.Add(('generated_config={0}|length={1}|sha256={2}' -f [string]$entry.path, [long]$entry.length, [string]$entry.sha256))
  }
  $frozenLines = [System.Collections.Generic.List[string]]::new()
  foreach ($line in @($BuildInputDescriptorLines)) { if ($line) { $frozenLines.Add([string]$line) } }
  foreach ($line in @($generatedLines)) { $frozenLines.Add($line) }

  $freezeId = Get-IdentityHashFromLines -Lines @($frozenLines.ToArray())

  return [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = $script:PackageInputFreezeKind
    freeze_id = $freezeId
    configuration = $Configuration
    prepared_by_run_id = $RunId
    phase = 'prepare'
    frozen_at_utc = (Get-Date).ToUniversalTime().ToString('o')
    package_manifest = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $manifestFull)
    package_manifest_sha256 = Get-IdentityFileHash -Path $manifestFull
    source_snapshot_digest = $SourceSnapshotDigest
    build_input_digest = $BuildInputDigest
    build_input_descriptor_lines = @($frozenLines.ToArray())
    build_input_file_facts = (Get-PackageBuildInputFileFacts -DescriptorLines @($BuildInputDescriptorLines))
    generated_configs = @($GeneratedConfigs)
    generated_config_rule = '能在准备阶段生成的配置在准备阶段生成，**生成后计入冻结输入**（其摘要进入 freeze_id）；正式构建阶段不得再改写它们'
    derived_outputs = @($DerivedOutputs)
    derived_output_rule = '需构建中产生的中间文件只能落在**已声明的派生输出位置**，并记录生成来源；派生输出不属于冻结输入，但必须与冻结输入区分登记'
    preparation = $Preparation
    cargo_lock_semantics = $script:CargoLockSemantics
    cargo_identity = [ordered]@{
      cargo_version = $(if ($CargoIdentity) { [string]$CargoIdentity.cargo_version } else { $null })
      host_target = $(if ($CargoIdentity) { [string]$CargoIdentity.host_target } else { $null })
      build_target = $(if ($CargoIdentity) { [string]$CargoIdentity.build_target } else { $null })
    }
    previous_freeze = $(if ($PreviousFreeze.Count -gt 0) { $PreviousFreeze[0] } else { $null })
    changed_paths_at_prepare = @($ChangedPaths)
    changes_accepted = $ChangesAccepted
    input_set_mutation_policy = 'frozen：冻结输入清单不得在构建阶段被改写或删减（不允许"发现输入被改写后把它从清单移除"以换取 release_eligible=true）'
    downstream_rule = '正式构建阶段只消费本冻结输入；任一声明输入（含准备阶段生成的配置）改变即拒绝发布，并给出改变路径、前后摘要与发生阶段'
    immutable_build_snapshot = $false
    immutable_build_snapshot_note = '本流程仍从活动工作树构建；冻结记录登记的是"构建被允许消费的输入集合"，稳定性由构建前后两次独立采样逐项一致来证明'
  }
}

<#
  写冻结记录：内容寻址 + 一次性写入（已存在即重算核对，冲突 fail-closed）。
  同时更新 latest-<config> 指针（只允许准备阶段调用）。
#>
function Write-PackageInputFreezeRecord {
  param(
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)]$Document,
    [switch]$UpdatePointer
  )

  $root = Get-PackageInputFreezeRoot -RepoPath $RepoPath
  New-Item -ItemType Directory -Force -Path $root | Out-Null
  $path = Join-Path $root ('package-input-freeze-{0}-{1}.json' -f [string]$Document.configuration, [string]$Document.freeze_id)
  $reused = $false
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    $existing = $null
    try { $existing = Get-Content -Raw -LiteralPath $path -Encoding UTF8 | ConvertFrom-Json } catch { $existing = $null }
    if (-not $existing -or [string]$existing.freeze_id -ne [string]$Document.freeze_id) {
      throw (New-IdentityFailureException -Category 'INPUT-FREEZE-CORRUPT' `
        -Detail ("冻结记录 {0} 已存在且内容与本次冻结不一致（freeze_id={1}）" -f $path, [string]$Document.freeze_id) `
        -Remediation '冻结记录是内容寻址的一次性记录；先人工核对再处理，不要覆盖它')
    }
    $reused = $true
  } else {
    $json = $Document | ConvertTo-Json -Depth 12
    [System.IO.File]::WriteAllText($path, $json, [System.Text.UTF8Encoding]::new($false))
  }

  $pointerPath = $null
  if ($UpdatePointer) {
    $pointerPath = Join-Path $root ('latest-{0}.json' -f [string]$Document.configuration)
    $pointer = [ordered]@{
      schema = [int]$script:BuildIdentitySchema
      kind = $script:PackageInputFreezePointerKind
      configuration = [string]$Document.configuration
      updated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      freeze_id = [string]$Document.freeze_id
      freeze_record = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path)
      source_snapshot_digest = [string]$Document.source_snapshot_digest
      build_input_digest = [string]$Document.build_input_digest
      prepared_by_run_id = [string]$Document.prepared_by_run_id
      changed_path_count = @($Document.changed_paths_at_prepare).Count
      rule = '准备阶段产出/更新冻结指针；正式构建阶段只有显式指定 -FreezeRecordPath 时消费它'
    }
    [System.IO.File]::WriteAllText($pointerPath, ($pointer | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
  }

  return [pscustomobject]@{
    path = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $path)
    absolute_path = $path
    content_sha256 = Get-IdentityFileHash -Path $path
    freeze_id = [string]$Document.freeze_id
    reused_existing_record = $reused
    pointer_path = $(if ($pointerPath) { Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $pointerPath } else { $null })
  }
}

function Read-PackageInputFreezeRecord {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw (New-IdentityFailureException -Category 'INPUT-FREEZE-MISSING' `
      -Detail "冻结记录不存在：$Path" `
      -Remediation '先跑准备阶段（-Prepare）产出冻结记录，再在正式构建阶段用 -FreezeRecordPath 消费它')
  }
  try {
    return (Get-Content -Raw -LiteralPath $Path -Encoding UTF8 | ConvertFrom-Json)
  } catch {
    throw (New-IdentityFailureException -Category 'INPUT-FREEZE-CORRUPT' `
      -Detail ("冻结记录无法解析：{0}（{1}）" -f $Path, $_.Exception.Message) `
      -Remediation '不要手工编辑冻结记录；重新跑准备阶段')
  }
}

<#
  正式构建阶段消费冻结输入前的核对（裁决 §3.1："任一声明输入改变即拒绝发布"）。

  逐项比较冻结输入集合与**本次构建开始前**重新计算的输入集合：
    * 构建输入描述符行（含每个输入文件的 sha256）：增/删/改都会命中；
    * 准备阶段生成的配置（其摘要已计入 freeze_id）：被改写即命中；
  返回的 changed_paths 带前后摘要与发生阶段（'build-preflight'）。
#>
function Compare-PackageInputFreeze {
  param(
    [Parameter(Mandatory = $true)]$Frozen,
    [AllowEmptyCollection()][string[]]$CurrentDescriptorLines = @(),
    [AllowNull()][string]$CurrentSourceSnapshotDigest,
    [AllowEmptyCollection()][object[]]$CurrentGeneratedConfigs = @(),
    [Parameter(Mandatory = $true)][string]$Phase
  )

  $changed = [System.Collections.Generic.List[object]]::new()
  $frozenLines = @($Frozen.build_input_descriptor_lines)
  $currentLines = @($CurrentDescriptorLines)
  $frozenFacts = Get-PackageBuildInputFileFacts -DescriptorLines $frozenLines
  $currentFacts = Get-PackageBuildInputFileFacts -DescriptorLines $currentLines

  foreach ($path in $frozenFacts.Keys) {
    if (-not $currentFacts.Contains($path)) {
      $changed.Add((New-PackageChangedPathEntry -Path $path -Kind 'removed' `
        -Before ("len={0} sha256={1}" -f $frozenFacts[$path].length, $frozenFacts[$path].sha256) -After $null `
        -Phase $Phase -Note '冻结时存在的构建输入在构建阶段消失'))
      continue
    }
    if ($frozenFacts[$path].sha256 -ne $currentFacts[$path].sha256) {
      $changed.Add((New-PackageChangedPathEntry -Path $path -Kind 'modified' `
        -Before ("len={0} sha256={1}" -f $frozenFacts[$path].length, $frozenFacts[$path].sha256) `
        -After ("len={0} sha256={1}" -f $currentFacts[$path].length, $currentFacts[$path].sha256) `
        -Phase $Phase -Note '声明输入在冻结之后被改写（不允许把它从输入清单移除以换取绿色结果）'))
    }
  }
  foreach ($path in $currentFacts.Keys) {
    if (-not $frozenFacts.Contains($path)) {
      $changed.Add((New-PackageChangedPathEntry -Path $path -Kind 'added' -Before $null `
        -After ("len={0} sha256={1}" -f $currentFacts[$path].length, $currentFacts[$path].sha256) `
        -Phase $Phase -Note '构建阶段出现了冻结清单之外的构建输入'))
    }
  }
  # 非文件类描述符（工具链/环境/target-dir/features/args 等）逐行比较：任一变化都要看得见。
  # generated_config= 行由下面的"准备阶段生成的配置"专项比较处理，不在此重复登记。
  foreach ($line in $frozenLines) {
    if ($line -like 'input_file=*' -or $line -like 'generated_config=*') { continue }
    if ($currentLines -notcontains $line) {
      $changed.Add((New-PackageChangedPathEntry -Path ([string]$line) -Kind 'descriptor-changed' `
        -Before ([string]$line) -After $null -Phase $Phase -Note '非文件类构建输入描述符发生变化（工具链/环境/features/args 等）'))
    }
  }
  foreach ($line in $currentLines) {
    if ($line -like 'input_file=*' -or $line -like 'generated_config=*') { continue }
    if ($frozenLines -notcontains $line) {
      $changed.Add((New-PackageChangedPathEntry -Path ([string]$line) -Kind 'descriptor-changed' `
        -Before $null -After ([string]$line) -Phase $Phase -Note '构建阶段新增了冻结清单之外的非文件类描述符'))
    }
  }
  # 准备阶段生成的配置：必须在构建阶段保持同一摘要（生成后即计入冻结输入）。
  $frozenGenerated = @{}
  foreach ($entry in @($Frozen.generated_configs)) { $frozenGenerated[[string]$entry.path] = [string]$entry.sha256 }
  $currentGenerated = @{}
  foreach ($entry in @($CurrentGeneratedConfigs)) { $currentGenerated[[string]$entry.path] = [string]$entry.sha256 }
  foreach ($path in $frozenGenerated.Keys) {
    if (-not $currentGenerated.Contains($path)) {
      $changed.Add((New-PackageChangedPathEntry -Path $path -Kind 'removed' -Before ("sha256={0}" -f $frozenGenerated[$path]) `
        -After $null -Phase $Phase -Classification 'preparation-generated-config' -Note '准备阶段生成的配置在构建阶段消失'))
      continue
    }
    if ($frozenGenerated[$path] -ne $currentGenerated[$path]) {
      $changed.Add((New-PackageChangedPathEntry -Path $path -Kind 'modified' -Before ("sha256={0}" -f $frozenGenerated[$path]) `
        -After ("sha256={0}" -f $currentGenerated[$path]) -Phase $Phase -Classification 'preparation-generated-config' `
        -Note '准备阶段生成的配置在正式构建阶段被改写：生成式配置生成后即计入冻结输入'))
    }
  }

  $changedSorted = @($changed | Sort-Object -Property path, kind)
  $identical = ($changedSorted.Count -eq 0)
  return [pscustomobject]@{
    comparable = $true
    identical = $identical
    frozen_freeze_id = [string]$Frozen.freeze_id
    frozen_build_input_digest = [string]$Frozen.build_input_digest
    current_source_snapshot_digest = $CurrentSourceSnapshotDigest
    frozen_source_snapshot_digest = [string]$Frozen.source_snapshot_digest
    changed_paths = $changedSorted
    changed_path_count = $changedSorted.Count
    phase = $Phase
    rule = '冻结输入在正式构建阶段发生变化 ⇒ 拒绝发布（release_eligible=false），并给出改变路径、前后摘要与发生阶段'
  }
}

<#
  准备阶段记录（受同一报告身份治理）：回答"准备阶段做了什么、改了什么、冻结了什么"。
  它**不是**发布收据（release_eligible=false）：准备阶段不产出可发布包。
#>
function New-PackagePreparationRecord {
  param(
    [Parameter(Mandatory = $true)][string]$RunId,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [Parameter(Mandatory = $true)][string]$RepoPath,
    [Parameter(Mandatory = $true)][string]$ManifestPath,
    [Parameter(Mandatory = $true)][string]$Outcome,
    [AllowNull()]$Freeze = $null,
    [AllowNull()]$FreezeDocument = $null,
    [AllowNull()]$DependencyResolution = $null,
    [AllowNull()]$GeneratedConfigs = $null,
    [AllowEmptyCollection()][object[]]$ChangedPaths = @(),
    [AllowEmptyCollection()][object[]]$GateResults = @(),
    [AllowEmptyCollection()][string[]]$Reasons = @(),
    [string]$RefusalCategory
  )

  $manifestFull = [System.IO.Path]::GetFullPath($ManifestPath)
  return [ordered]@{
    schema = [int]$script:BuildIdentitySchema
    kind = $script:PackagePrepareReportKind
    report_status = $Outcome
    run_phase = 'prepare'
    run_id = $RunId
    generated_at = (Get-Date).ToUniversalTime().ToString('o')
    configuration = $Configuration
    manifest = (Get-IdentityRepoRelativePath -RepoPath $RepoPath -Path $manifestFull)
    manifest_sha256 = Get-IdentityFileHash -Path $manifestFull
    generated_by = 'scripts/package-all.ps1'
    release_eligible = $false
    release_eligible_note = '准备阶段不产出可发布包，因此本记录**不是**发布收据；它只回答"依赖是否解析成功、生成了哪些配置、冻结了哪些输入、哪些输入发生变化"。'
    refusal_category = $RefusalCategory
    dependency_resolution = $DependencyResolution
    cargo_lock_semantics = $script:CargoLockSemantics
    generated_configs = $GeneratedConfigs
    changed_paths = @($ChangedPaths)
    changed_paths_rule = '准备阶段的"展示变化"：逐路径给出前后摘要与发生阶段（prepare）'
    freeze = $Freeze
    frozen_inputs = $(if ($FreezeDocument) { [ordered]@{
        freeze_id = [string]$FreezeDocument.freeze_id
        source_snapshot_digest = [string]$FreezeDocument.source_snapshot_digest
        build_input_digest = [string]$FreezeDocument.build_input_digest
        frozen_input_count = @($FreezeDocument.build_input_descriptor_lines).Count
        generated_config_count = @($FreezeDocument.generated_configs).Count
        derived_outputs = @($FreezeDocument.derived_outputs)
        changes_accepted = [bool]$FreezeDocument.changes_accepted
      } } else { $null })
    gates = @($GateResults)
    reasons = @($Reasons)
    next_phase = '正式构建阶段：用 -FreezeRecordPath <本记录引用的冻结记录> 消费冻结输入；任一声明输入改变即拒绝发布'
  }
}

<#
  失败诊断的"实际改变的路径"汇总（裁决 §3.1 最后一句）：
  把准备阶段、冻结核对、源码快照前后采样、构建输入前后采样四处观察到的变化合并成一份列表，
  每条都带**前后摘要**与**发生阶段**。前后摘要来自对应两次采样的登记值（快照条目 / 描述符行），
  因此它是"两次采样之间的已观察差异"，不是完整写入历史。
#>
function Get-PackageFailureChangedPathEntries {
  param(
    [AllowNull()]$SnapshotComparison,
    [AllowNull()]$SourceSnapshotPre,
    [AllowNull()]$SourceSnapshotPost,
    [AllowNull()]$BuildInputComparison,
    [AllowNull()]$FrozenComparison,
    [AllowEmptyCollection()][object[]]$PrepareChangedPaths = @(),
    [AllowEmptyCollection()][object[]]$FreezeChangedPaths = @()
  )

  $entries = [System.Collections.Generic.List[object]]::new()
  foreach ($entry in @($PrepareChangedPaths)) { $entries.Add($entry) }
  foreach ($entry in @($FreezeChangedPaths)) { $entries.Add($entry) }

  $preFacts = @{}
  if ($SourceSnapshotPre) { foreach ($item in @($SourceSnapshotPre.entries)) { $preFacts[[string]$item.path] = $item } }
  $postFacts = @{}
  if ($SourceSnapshotPost) { foreach ($item in @($SourceSnapshotPost.entries)) { $postFacts[[string]$item.path] = $item } }
  $snapshotSummary = {
    param($facts, $path)
    if (-not $facts.ContainsKey($path)) { return $null }
    $entry = $facts[$path]
    return ('len={0} sha256={1}' -f [long]$entry.length, [string]$entry.sha256)
  }
  if ($SnapshotComparison) {
    foreach ($path in @($SnapshotComparison.added)) {
      $afterText = & $snapshotSummary $postFacts ([string]$path)
      $entries.Add((New-PackageChangedPathEntry -Path ([string]$path) -Kind 'added' -Before $null -After $afterText `
        -Phase 'post-build-source-verification' -Classification 'in-declared-source-snapshot-scope')) | Out-Null
    }
    foreach ($path in @($SnapshotComparison.removed)) {
      $beforeText = & $snapshotSummary $preFacts ([string]$path)
      $entries.Add((New-PackageChangedPathEntry -Path ([string]$path) -Kind 'removed' -Before $beforeText -After $null `
        -Phase 'post-build-source-verification' -Classification 'in-declared-source-snapshot-scope')) | Out-Null
    }
    foreach ($path in @($SnapshotComparison.modified)) {
      $beforeText = & $snapshotSummary $preFacts ([string]$path)
      $afterText = & $snapshotSummary $postFacts ([string]$path)
      $entries.Add((New-PackageChangedPathEntry -Path ([string]$path) -Kind 'modified' -Before $beforeText -After $afterText `
        -Phase 'post-build-source-verification' -Classification 'in-declared-source-snapshot-scope')) | Out-Null
    }
  }
  if ($BuildInputComparison) {
    # 描述符差异是"加的行使当前值、减的行是基线值"：按路径配对给出前后摘要。
    $addedFacts = @{}
    $removedFacts = @{}
    foreach ($line in @($BuildInputComparison.added)) {
      $match = [regex]::Match([string]$line, '^input_file=([^|]+)\|length=(\d+)\|sha256=([0-9a-fA-F]+)$')
      if ($match.Success) { $addedFacts[$match.Groups[1].Value] = ('len={0} sha256={1}' -f $match.Groups[2].Value, $match.Groups[3].Value.ToLowerInvariant()) }
    }
    foreach ($line in @($BuildInputComparison.removed)) {
      $match = [regex]::Match([string]$line, '^input_file=([^|]+)\|length=(\d+)\|sha256=([0-9a-fA-F]+)$')
      if ($match.Success) { $removedFacts[$match.Groups[1].Value] = ('len={0} sha256={1}' -f $match.Groups[2].Value, $match.Groups[3].Value.ToLowerInvariant()) }
    }
    $paired = @{}
    foreach ($path in $addedFacts.Keys) {
      $paired[$path] = $true
      $entries.Add((New-PackageChangedPathEntry -Path $path -Kind $(if ($removedFacts.ContainsKey($path)) { 'modified' } else { 'added' }) `
        -Before $(if ($removedFacts.ContainsKey($path)) { $removedFacts[$path] } else { $null }) -After $addedFacts[$path] `
        -Phase 'build-input-recheck' -Classification 'declared-build-input' `
        -Note '声明的构建输入在本次构建期间发生变化（混合输入产物）')) | Out-Null
    }
    foreach ($path in $removedFacts.Keys) {
      if ($paired.ContainsKey($path)) { continue }
      $entries.Add((New-PackageChangedPathEntry -Path $path -Kind 'removed' -Before $removedFacts[$path] -After $null `
        -Phase 'build-input-recheck' -Classification 'declared-build-input' `
        -Note '声明的构建输入在本次构建期间消失')) | Out-Null
    }
    # 非文件类描述符（工具链/环境/features/args 等）：原样登记，便于定位。
    foreach ($line in @($BuildInputComparison.added)) {
      if ([string]$line -like 'input_file=*') { continue }
      $entries.Add((New-PackageChangedPathEntry -Path ([string]$line) -Kind 'descriptor-changed' -Before $null -After ([string]$line) `
        -Phase 'build-input-recheck' -Classification 'declared-build-input')) | Out-Null
    }
    foreach ($line in @($BuildInputComparison.removed)) {
      if ([string]$line -like 'input_file=*') { continue }
      $entries.Add((New-PackageChangedPathEntry -Path ([string]$line) -Kind 'descriptor-changed' -Before ([string]$line) -After $null `
        -Phase 'build-input-recheck' -Classification 'declared-build-input')) | Out-Null
    }
  }
  $deduped = [System.Collections.Generic.List[object]]::new()
  $seen = @{}
  foreach ($entry in $entries) {
    $key = ('{0}|{1}|{2}' -f [string]$entry.phase, [string]$entry.path, [string]$entry.kind)
    if ($seen.ContainsKey($key)) { continue }
    $seen[$key] = $true
    $deduped.Add($entry) | Out-Null
  }
  return $deduped.ToArray()
}
