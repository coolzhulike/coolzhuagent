$ErrorActionPreference = 'Stop'

$workspace = Split-Path -Parent $PSScriptRoot
$packageScript = Join-Path $PSScriptRoot 'package-all.ps1'
$workspaceFull = [System.IO.Path]::GetFullPath($workspace).TrimEnd('\', '/')
$tmpRootFull = [System.IO.Path]::GetFullPath((Join-Path $workspaceFull 'tmp')).TrimEnd('\', '/')
$fixtureRoot = [System.IO.Path]::GetFullPath((Join-Path $tmpRootFull 'package-webview2-loader-contract')).TrimEnd('\', '/')
$tmpPrefix = $tmpRootFull + [System.IO.Path]::DirectorySeparatorChar
if (-not $fixtureRoot.StartsWith($tmpPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "fixture root must stay inside the current worktree tmp directory: $fixtureRoot"
}

function Write-FixtureBytes {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][byte[]]$Bytes
  )
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [System.IO.File]::WriteAllBytes($Path, $Bytes)
}

function New-LoaderFixture {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][int]$CandidateCount
  )

  $caseRoot = Join-Path $fixtureRoot $Name
  $releaseRoot = Join-Path $caseRoot 'release'
  $sourceRelative = "tmp/package-webview2-loader-contract/$Name/release/WebView2Loader.dll"
  $manifestPath = Join-Path $caseRoot 'manifest.json'
  $manifest = [ordered]@{
    package_root = "tmp/package-webview2-loader-contract/$Name/package"
    artifacts = @(
      [ordered]@{
        id = 'gui-desktop.webview2-loader'
        source = $sourceRelative
        target = 'bin/WebView2Loader.dll'
      }
    )
    resources = @()
  }

  New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
  $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding UTF8
  for ($index = 1; $index -le $CandidateCount; $index++) {
    $candidate = Join-Path $releaseRoot "build\webview2-com-sys-contract-$index\out\x64\WebView2Loader.dll"
    Write-FixtureBytes -Path $candidate -Bytes ([byte[]](0x57, 0x56, $index, 0x01))
  }

  return [pscustomobject]@{
    caseRoot = $caseRoot
    manifestPath = $manifestPath
    packagePath = Join-Path $caseRoot 'package'
    sourcePath = Join-Path $releaseRoot 'WebView2Loader.dll'
    singleCandidatePath = Join-Path $releaseRoot 'build\webview2-com-sys-contract-1\out\x64\WebView2Loader.dll'
  }
}

function Invoke-PackageFixture {
  param([Parameter(Mandatory = $true)][object]$Fixture)

  Push-Location $workspace
  try {
    & $packageScript `
      -Manifest $Fixture.manifestPath `
      -Configuration release `
      -SkipBuild `
      -ReportPath (Join-Path $Fixture.caseRoot 'report.json') | Out-Null
    return $null
  } catch {
    return $_.Exception.Message
  } finally {
    Pop-Location
  }
}

try {
  if (Test-Path -LiteralPath $fixtureRoot) {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null

  $missing = New-LoaderFixture -Name 'missing' -CandidateCount 0
  $missingError = Invoke-PackageFixture -Fixture $missing
  if ($missingError -notmatch 'no x64 WebView2Loader\.dll candidate') {
    throw "missing candidate must fail closed with a precise x64 error; got: $missingError"
  }

  $single = New-LoaderFixture -Name 'single' -CandidateCount 1
  $singleError = Invoke-PackageFixture -Fixture $single
  if ($singleError) {
    throw "single candidate should be published: $singleError"
  }
  $publishedSingle = Join-Path $single.packagePath 'bin\WebView2Loader.dll'
  if (-not (Test-Path -LiteralPath $publishedSingle -PathType Leaf)) {
    throw "single candidate was not copied to package: $publishedSingle"
  }
  $singleHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $single.singleCandidatePath).Hash
  $publishedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $publishedSingle).Hash
  if ($singleHash -ne $publishedHash) {
    throw 'single candidate was copied with different bytes'
  }

  $multiple = New-LoaderFixture -Name 'multiple' -CandidateCount 2
  $multipleError = Invoke-PackageFixture -Fixture $multiple
  if ($multipleError -notmatch 'ambiguous WebView2 loader sources' -or $multipleError -notmatch 'found 2') {
    throw "multiple candidates must fail closed as ambiguous; got: $multipleError"
  }

  Write-Output 'PASS package-webview2-loader'
} finally {
  if (Test-Path -LiteralPath $fixtureRoot) {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
  }
}
