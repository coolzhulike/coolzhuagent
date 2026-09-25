param(
    [Parameter(Mandatory = $true)]
    [string]$SourcePath
)

Add-Type -AssemblyName System.Drawing

$packageRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $packageRoot "assets\ui-redesign\throne-cat"
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
Copy-Item -LiteralPath $SourcePath -Destination (Join-Path $outputRoot "source-sheet.png") -Force

$frames = @(
    @{ Name = "cat-01-sleep.png"; X = 26; Y = 20 },
    @{ Name = "cat-02-stretch.png"; X = 464; Y = 20 },
    @{ Name = "cat-03-groom.png"; X = 901; Y = 20 },
    @{ Name = "cat-04-tail-left.png"; X = 1338; Y = 20 },
    @{ Name = "cat-05-tail-right.png"; X = 26; Y = 458 },
    @{ Name = "cat-06-look-back.png"; X = 464; Y = 458 },
    @{ Name = "cat-07-rest.png"; X = 901; Y = 458 }
)

$source = [System.Drawing.Bitmap]::FromFile((Resolve-Path -LiteralPath $SourcePath))
$cellWidth = 402
$cellHeight = 405
$outputSize = 512

foreach ($frame in $frames) {
    $target = New-Object System.Drawing.Bitmap $outputSize, $outputSize, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($target)
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half

    $scale = [Math]::Min($outputSize / $cellWidth, $outputSize / $cellHeight)
    $drawWidth = [int][Math]::Round($cellWidth * $scale)
    $drawHeight = [int][Math]::Round($cellHeight * $scale)
    $drawX = [int](($outputSize - $drawWidth) / 2)
    $drawY = [int](($outputSize - $drawHeight) / 2)
    $graphics.DrawImage(
        $source,
        [System.Drawing.Rectangle]::new($drawX, $drawY, $drawWidth, $drawHeight),
        [System.Drawing.Rectangle]::new($frame.X, $frame.Y, $cellWidth, $cellHeight),
        [System.Drawing.GraphicsUnit]::Pixel
    )
    $graphics.Dispose()

    for ($y = 0; $y -lt $outputSize; $y++) {
        for ($x = 0; $x -lt $outputSize; $x++) {
            $pixel = $target.GetPixel($x, $y)
            $magentaScore = ($pixel.R + $pixel.B) - (2 * $pixel.G)
            if ($pixel.R -gt 135 -and $pixel.B -gt 120 -and $pixel.G -lt 125 -and $magentaScore -gt 180) {
                $target.SetPixel($x, $y, [System.Drawing.Color]::Transparent)
            }
        }
    }

    $target.Save((Join-Path $outputRoot $frame.Name), [System.Drawing.Imaging.ImageFormat]::Png)
    $target.Dispose()
}

$source.Dispose()
Write-Output $outputRoot
