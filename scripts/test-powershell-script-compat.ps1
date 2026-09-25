$ErrorActionPreference = 'Stop'

$scriptsRoot = $PSScriptRoot
$failures = New-Object System.Collections.Generic.List[string]
$utf8 = New-Object System.Text.UTF8Encoding($false, $true)

Get-ChildItem -LiteralPath $scriptsRoot -Filter '*.ps1' -File |
    Sort-Object FullName |
    ForEach-Object {
        $path = $_.FullName
        $bytes = [System.IO.File]::ReadAllBytes($path)
        $hasUtf8Bom = $bytes.Length -ge 3 -and
            $bytes[0] -eq 0xEF -and
            $bytes[1] -eq 0xBB -and
            $bytes[2] -eq 0xBF

        try {
            $text = $utf8.GetString($bytes)
        }
        catch {
            $failures.Add("$($_.Name): invalid UTF-8: $($_.Exception.Message)")
            return
        }

        if ($text -match '[^\x00-\x7F]' -and -not $hasUtf8Bom) {
            $failures.Add("$($_.Name): non-ASCII PowerShell source must use UTF-8 BOM for Windows PowerShell 5.1")
        }

        $parseErrors = $null
        [void][System.Management.Automation.Language.Parser]::ParseFile(
            $path,
            [ref]$null,
            [ref]$parseErrors
        )
        foreach ($parseError in @($parseErrors)) {
            $failures.Add("$($_.Name): parser error at $($parseError.Extent.StartLineNumber):$($parseError.Extent.StartColumnNumber): $($parseError.Message)")
        }
    }

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    throw "PowerShell script compatibility failed: $($failures.Count) finding(s)"
}

Write-Host 'PowerShell script compatibility passed.'
