# 由整包调用前完成引导；调用方负责冻结源码并在构建后核对源码快照。
function Assert-BuildCaptureCargoRecord {
  param([string]$RecordPath,[string]$RepoPath,[string]$ExecutablePath)
  $source=[IO.FileStream]::new($RecordPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  try {
    [void](Get-CargoStreamSummary -Source $source)
    $source.Position=0
    $reader=[IO.StreamReader]::new($source,[Text.UTF8Encoding]::new($false,$true),$false,4096,$true)
    $artifactMatchCount=0
    $manifest=[IO.Path]::GetFullPath((Join-Path $RepoPath 'modules/gui-web/packages/windows-process-guard/Cargo.toml'))
    $src=[IO.Path]::GetFullPath((Join-Path $RepoPath 'modules/gui-web/packages/windows-process-guard/src/bin/build-capture.rs'))
    try {
      while($null -ne ($line=[CoolzhuBoundedJsonLine]::Read($reader))) {
        if(-not $line.Trim()){continue}
        $record=ConvertFrom-IdentityJson -Json $line.TrimStart([char]0xfeff)
        if($record.reason -ne 'compiler-artifact' -or $record.target.name -ne 'build-capture'){continue}
        $artifactMatchCount++
        if($record.package_id -notmatch '#coolzhu-windows-process-guard@' -or
           [IO.Path]::GetFullPath([string]$record.manifest_path) -ine $manifest -or
           [IO.Path]::GetFullPath([string]$record.target.src_path) -ine $src -or
           @($record.target.kind).Count -ne 1 -or $record.target.kind[0] -ne 'bin' -or
           [IO.Path]::GetFullPath([string]$record.executable) -ine [IO.Path]::GetFullPath($ExecutablePath) -or
           @($record.features).Count -ne 0 -or $record.profile.test -ne $false) {
          throw '采集辅助程序 Cargo 产物身份或 feature 不符合正式引导要求'
        }
      }
    } finally { $reader.Dispose() }
    if($artifactMatchCount -ne 1){throw '采集辅助程序缺少唯一 Cargo 产物记录'}
  } finally { $source.Dispose() }
}
function New-BuildCaptureBootstrap {
  param(
    [Parameter(Mandatory=$true)][string]$RepoPath,
    [Parameter(Mandatory=$true)][string]$TargetDirectory,
    [Parameter(Mandatory=$true)][string]$EvidenceDirectory
  )
  $repo=(Resolve-Path -LiteralPath $RepoPath).Path
  $target=[IO.Path]::GetFullPath($TargetDirectory)
  $evidence=[IO.Path]::GetFullPath($EvidenceDirectory)
  if(Test-Path -LiteralPath $evidence){throw '采集引导证据目录必须为本轮新建目录'}
  [void][IO.Directory]::CreateDirectory($evidence)
  $arguments=@('build','--locked','--offline','-j1','-p','coolzhu-windows-process-guard','--bin','build-capture','--target-dir',$target)
  Push-Location $repo
  try {
    # 首次构建直接继承终端输出，不驻留整份 stdout/stderr。
    & cargo @arguments
    if($LASTEXITCODE -ne 0){throw '采集辅助程序引导构建失败'}
    $built=Join-Path $target 'debug/build-capture.exe'
    $helper=Join-Path $evidence 'build-capture.exe'
    [IO.File]::Copy($built,$helper,$false)
    $sha=(Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash.ToLowerInvariant()
    $stdout=Join-Path $evidence 'cargo.jsonl'; $stderr=Join-Path $evidence 'cargo.stderr.log'
    $receipt=Invoke-ManagedBuildCapture -HelperPath $helper -ExpectedHelperSha256 $sha -Command 'cargo' -Arguments ($arguments+@('--message-format=json-render-diagnostics')) -WorkingDirectory $repo -StdoutPath $stdout -StderrPath $stderr
    if($receipt.exit_code -ne 0){throw ('辅助程序证据构建失败：'+$receipt.exit_code)}
    Assert-BuildCaptureCargoRecord -RecordPath $stdout -RepoPath $repo -ExecutablePath $built
    if((Get-FileHash -LiteralPath $built -Algorithm SHA256).Hash -ine $sha){throw '引导副本与证据构建产物字节不一致'}
    [pscustomobject]@{helper_path=$helper;helper_sha256=$sha;raw_record_path=$stdout;stderr_path=$stderr;command='cargo';args=($arguments+@('--message-format=json-render-diagnostics'));working_dir=$repo;exit_code=$receipt.exit_code;source_snapshot_verified=$false;cargo_record_validated=$true}
  } finally { Pop-Location }
}
