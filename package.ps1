param(
  [Parameter(Position = 0)]
  [ValidateSet('all')]
  [string]$Command = 'all',
  [ValidateSet('debug', 'release')]
  [string]$Configuration = 'debug',
  [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

switch ($Command) {
  'all' {
    & (Join-Path $PSScriptRoot 'scripts/package-all.ps1') -Configuration $Configuration -SkipBuild:$SkipBuild
    exit 0
  }
}
