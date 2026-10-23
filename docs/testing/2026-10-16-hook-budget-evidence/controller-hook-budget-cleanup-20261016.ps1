$ErrorActionPreference = 'Stop'
$hookRoot = [IO.Path]::GetFullPath('D:\coolzhuagent\tmp\goal-32wbs-acceptance-20261013')
$hookScratch = Join-Path $hookRoot 'hook-budget-20261016'
$hookReview = Get-Content -LiteralPath (Join-Path $hookRoot 'controller-hook-budget-review-20261016.json') -Raw | ConvertFrom-Json
if ($hookReview.state -ne 'completed_with_remaining_gates' -or $hookReview.passed -ne 8) { throw '证据尚未审核，保留诊断程序' }
$hookTargets = @('command-diagnostic.exe', 'command-diagnostic.pdb', 'quoted helper.exe') | ForEach-Object { [IO.Path]::GetFullPath((Join-Path $hookScratch $_)) }
$hookProcesses = @(Get-CimInstance Win32_Process -ErrorAction Stop)
$hookRecords = @()
foreach ($hookTarget in $hookTargets) {
    if (-not $hookTarget.StartsWith($hookScratch + '\', [StringComparison]::OrdinalIgnoreCase)) { throw '删除路径越界' }
    if (-not (Test-Path -LiteralPath $hookTarget)) { continue }
    $hookItem = Get-Item -LiteralPath $hookTarget -Force
    if ($hookItem.PSIsContainer -or ($hookItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw '仅处理明确的普通文件' }
    $hookParent = $hookItem.Directory
    while ($hookParent -and $hookParent.FullName.Length -ge $hookRoot.Length) {
        if ($hookParent.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '祖先目录为重解析点，保留文件' }
        $hookParent = $hookParent.Parent
    }
    if (@($hookProcesses | Where-Object { $_.ExecutablePath -and $_.ExecutablePath -ieq $hookTarget }).Count) { throw '诊断程序仍在运行，保留文件' }
    $hookRecords += [pscustomobject]@{ path = $hookTarget; bytes = $hookItem.Length; sha256 = (Get-FileHash -LiteralPath $hookTarget -Algorithm SHA256).Hash.ToLowerInvariant() }
}
# 只删除这三个可再生的普通文件；没有递归删除，日志、源码、数据库和编译缓存全部保留。
foreach ($hookRecord in $hookRecords) { Remove-Item -LiteralPath $hookRecord.path -Force }
$hookResult = [pscustomobject]@{
    state = 'completed'
    removed = $hookRecords
    removed_bytes = ($hookRecords | Measure-Object -Property bytes -Sum).Sum
    retained = '日志、源码、原始失败、数据库夹具、活动发布包和可复用编译缓存'
}
$hookResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $hookRoot 'controller-hook-budget-cleanup-20261016.json') -Encoding UTF8
$hookResult | ConvertTo-Json -Depth 6
