$ErrorActionPreference='Stop'
Import-Module 'C:/Windows/System32/WindowsPowerShell/v1.0/Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1' -ErrorAction Stop
& ./scripts/build-msi.ps1 -Version '0.2.116' -Configuration release
if(-not $?){throw '正式构建脚本返回失败状态'}
