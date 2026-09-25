[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [string]$NativeHostPath,
    [string]$ExtensionDirectory = (Join-Path $PSScriptRoot '..\modules\browser-extension'),
    [ValidateSet('All', 'Chrome', 'Edge')]
    [string]$Browser = 'All',
    [switch]$Unregister
)

$ErrorActionPreference = 'Stop'
$hostName = 'com.coolzhu.agent.browser_bridge'
$manifestPath = Join-Path $ExtensionDirectory 'manifest.json'
$identityPath = Join-Path $ExtensionDirectory 'extension-identity.json'

function Get-ChromiumExtensionId([string]$PublicKey) {
    $bytes = [Convert]::FromBase64String($PublicKey)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $hash = $sha.ComputeHash($bytes) } finally { $sha.Dispose() }
    $alphabet = 'abcdefghijklmnop'
    $builder = New-Object System.Text.StringBuilder
    foreach ($value in $hash[0..15]) {
        [void]$builder.Append($alphabet[$value -shr 4])
        [void]$builder.Append($alphabet[$value -band 15])
    }
    $builder.ToString()
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Browser extension manifest was not found: $manifestPath"
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if (-not $manifest.key) { throw 'Browser extension manifest has no stable public key.' }
$extensionId = Get-ChromiumExtensionId $manifest.key

if (Test-Path -LiteralPath $identityPath -PathType Leaf) {
    $identity = Get-Content -LiteralPath $identityPath -Raw | ConvertFrom-Json
    if ($identity.extension_id -ne $extensionId) {
        throw "Extension identity mismatch: manifest=$extensionId identity=$($identity.extension_id)"
    }
}

$targets = @()
if ($Browser -in @('All', 'Chrome')) {
    $targets += [pscustomobject]@{
        Name = 'chrome'
        Registry = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
    }
}
if ($Browser -in @('All', 'Edge')) {
    $targets += [pscustomobject]@{
        Name = 'edge'
        Registry = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
    }
}

$stateDirectory = Join-Path $env:LOCALAPPDATA 'CoolzhuAgent\browser-native-host'
if (-not $Unregister) {
    $resolvedHost = (Resolve-Path -LiteralPath $NativeHostPath).Path
    if (-not $resolvedHost.EndsWith('.exe', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'NativeHostPath must point to the native host executable.'
    }
    New-Item -ItemType Directory -Path $stateDirectory -Force | Out-Null
}

foreach ($target in $targets) {
    $nativeManifest = Join-Path $stateDirectory "$hostName.$($target.Name).json"
    if ($Unregister) {
        if ($PSCmdlet.ShouldProcess($target.Registry, 'Remove native messaging registration')) {
            Remove-Item -LiteralPath $target.Registry -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $nativeManifest -Force -ErrorAction SilentlyContinue
        }
        continue
    }
    $payload = [ordered]@{
        name = $hostName
        description = 'Coolzhu Agent authenticated DOM bridge'
        path = $resolvedHost
        type = 'stdio'
        allowed_origins = @("chrome-extension://$extensionId/")
    } | ConvertTo-Json -Depth 4
    if ($PSCmdlet.ShouldProcess($nativeManifest, 'Write native messaging manifest')) {
        Set-Content -LiteralPath $nativeManifest -Value $payload -Encoding UTF8
        New-Item -Path $target.Registry -Force | Out-Null
        Set-Item -LiteralPath $target.Registry -Value $nativeManifest
    }
}

[pscustomobject]@{
    ExtensionId = $extensionId
    AllowedOrigin = "chrome-extension://$extensionId/"
    NativeHost = if ($Unregister) { $null } else { $resolvedHost }
    Browser = $Browser
    Registered = -not $Unregister
}
