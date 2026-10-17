#requires -Version 5.1
# 真实回归（172 失败现场 + 175 收紧要求）：vctip 常驻留在受管 Job 内超 30s 排空上限
# 导致 build-capture 125。测试用冻结 build-capture.exe 走真实 Invoke-ManagedBuildCapture
#（Windows Job + 双管道 + 30s 排空门禁），断言受管 rustc->link 构建在抑制下完整排空；
# 并断言账本只对本轮 stable->marked 的条目还原，绝不猜测清除既有 unstable。
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/managed-build-capture.ps1')

$helper = 'D:/coolzhuagent/tmp/goal-32wbs-acceptance-20261013/logs/capture-bootstrap-pkg-run-release-20261013-150811627-18006fab/build-capture.exe'
$helperSha = 'da14c21cec8a8562813adaa56efcdae0e516e1a266f0eac9a6a21e1469cb1866'
$rustc = (Get-Command rustc -ErrorAction SilentlyContinue).Source
if (-not $rustc) { $rustc = Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe' }
foreach ($probe in @($helper, $rustc)) { if (-not (Test-Path -LiteralPath $probe)) { throw ('必需输入缺失：' + $probe) } }

# 临时目录/日志全部在统一验收根（COOLZHU_ACCEPTANCE_TMP_ROOT，缺失时回退固定默认）。
$acc = $env:COOLZHU_ACCEPTANCE_TMP_ROOT
if (-not $acc) { $acc = 'D:/coolzhuagent/tmp/goal-32wbs-acceptance-20261013' }
$tmp = Join-Path $acc ('vctip-suppression-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $tmp | Out-Null

# 先记录作用域内原状态，断言本轮不会篡改既有 unstable。
$before = @{}
foreach ($h in @(Get-LinkerToolsetVctip)) {
  $info = (& $h.FullName -unstable:info 2>&1 | Out-String)
  if ($info -match "VCTIP is '(\w+)'") { $before[$h.FullName] = $Matches[1] }
}
try {
  Set-Content (Join-Path $tmp 'probe.rs') 'fn main(){}'
  $r = Invoke-ManagedBuildCapture -HelperPath $helper -ExpectedHelperSha256 $helperSha `
    -Command $rustc -Arguments @('-O', (Join-Path $tmp 'probe.rs'), '-o', (Join-Path $tmp 'probe.exe')) `
    -WorkingDirectory $tmp -StdoutPath (Join-Path $tmp 'probe.stdout.log') -StderrPath (Join-Path $tmp 'probe.stderr.log')
  if ($r.exit_code -ne 0) { throw ('受管采集退出码 ' + $r.exit_code + '（vctip 常驻会返回125）') }
  if (-not (Test-Path -LiteralPath (Join-Path $tmp 'probe.exe'))) { throw '链接产物缺失' }
  $stderrText = Get-Content -LiteralPath $r.helper_diagnostic_path -Raw -ErrorAction SilentlyContinue
  if ($stderrText -match 'BUILD-CAPTURE-FAILED') { throw ('受管采集失败: ' + $stderrText) }
  Write-Output 'PASS 受管 Windows Job rustc->link 构建在 vctip 抑制下完整排空'
} finally {
  # 状态账本核对：原 unstable 必须仍 unstable（未被清除）；本轮改过的必须还原 stable。
  $after = @{}
  foreach ($h in @(Get-LinkerToolsetVctip)) {
    $info = (& $h.FullName -unstable:info 2>&1 | Out-String)
    if ($info -match "VCTIP is '(\w+)'") { $after[$h.FullName] = $Matches[1] }
  }
  foreach ($path in $before.Keys) {
    if ($before[$path] -eq 'unstable' -and $after[$path] -ne 'unstable') {
      Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
      throw ('既有 unstable 被清除：' + $path)
    }
  }
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
