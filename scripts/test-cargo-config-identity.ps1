#requires -Version 5.1
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'lib/build-identity.ps1')
$root=Join-Path (Split-Path $PSScriptRoot -Parent) ('tmp/cargo-config-identity-'+[guid]::NewGuid().ToString('N'))
$repo=Join-Path $root 'ancestor/repo'; $fixtureCargoHome=Join-Path $root 'cargo-home'
[void][IO.Directory]::CreateDirectory($repo); [void][IO.Directory]::CreateDirectory($fixtureCargoHome)
$path=Join-Path $root 'ancestor/.cargo/config.toml'
[void][IO.Directory]::CreateDirectory((Split-Path $path -Parent))
$before=@(Get-CargoConfigIdentityDescriptors $repo $fixtureCargoHome)
[IO.File]::WriteAllText($path,'secret-canary=1')
$created=@(Get-CargoConfigIdentityDescriptors $repo $fixtureCargoHome)
if(($before -join "`n") -ceq ($created -join "`n") -or ($created -join "`n") -match 'secret-canary'){throw '创建未改变身份或正文泄漏'}
[IO.File]::WriteAllText($path,'secret-canary=2')
$changed=@(Get-CargoConfigIdentityDescriptors $repo $fixtureCargoHome)
if(($created -join "`n") -ceq ($changed -join "`n")){throw '修改未改变身份'}
[IO.File]::Delete($path)
$deleted=@(Get-CargoConfigIdentityDescriptors $repo $fixtureCargoHome)
if(($before -join "`n") -cne ($deleted -join "`n")){throw '删除未恢复缺失身份'}
$user=Join-Path $fixtureCargoHome 'config'
[IO.File]::WriteAllText($user,'user-secret-canary')
$userDescriptors=@(Get-CargoConfigIdentityDescriptors $repo $fixtureCargoHome)
if(-not ($userDescriptors | Where-Object {$_ -like ('cargo_config='+$user+'|sha256=*')}) -or ($userDescriptors -join "`n") -match 'user-secret-canary'){throw '用户配置身份未登记或正文泄漏'}
Write-Output 'PASS 祖先配置创建、修改、删除均改变身份；用户配置登记且不泄漏正文'
