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
$nested=Join-Path $repo 'nested'; [void][IO.Directory]::CreateDirectory((Join-Path $nested '.cargo'))
[IO.File]::WriteAllText((Join-Path $repo 'Cargo.lock'),'fixture-lock')
$manifest=[pscustomobject]@{artifacts=@([pscustomobject]@{id='nested-test';build=[pscustomobject]@{command='cargo';working_dir='nested';args=@('build')}})}
$manifestPath=Join-Path $repo 'manifest.json'; [IO.File]::WriteAllText($manifestPath,'fixture-manifest')
$params=@{RepoPath=$repo;ManifestData=$manifest;ManifestPath=$manifestPath;Configuration='debug';CargoIdentity=[pscustomobject]@{cargo_version='fixture';host_target='fixture';build_target='fixture'};SourceSnapshotDigest='fixture'}
$beforeNested=@(Get-BuildInputDescriptors @params)
[IO.File]::WriteAllText((Join-Path $nested '.cargo/config.toml'),'nested-secret-canary')
$afterNested=@(Get-BuildInputDescriptors @params)
if(($beforeNested -join "`n") -ceq ($afterNested -join "`n") -or ($afterNested -join "`n") -match 'nested-secret-canary'){throw '嵌套工作目录配置未绑定或正文泄漏'}
$manifest.artifacts[0].build.working_dir='.'
$rootWork=@(Get-BuildInputDescriptors @params)
if(($afterNested -join "`n") -ceq ($rootWork -join "`n")){throw '构建工作目录变化未改变输入身份'}
Write-Output 'PASS 嵌套Cargo工作目录配置及工作目录本身进入构建身份'
