$ErrorActionPreference='Stop'
Import-Module 'C:/Windows/System32/WindowsPowerShell/v1.0/Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1' -ErrorAction Stop
Get-Command Get-FileHash -ErrorAction Stop | Select-Object Name,Source | ConvertTo-Json | Set-Content -LiteralPath tmp/2026-10-08-release-106/child-hash-command.json -Encoding UTF8
& ./scripts/build-msi.ps1 -Version '0.2.106' -Configuration release
if (-not $?) {throw '正式构建脚本返回失败状态'}
