param(
    [string]$OutputPath = ""
)

Add-Type -AssemblyName System.Drawing

$packageRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $packageRoot "assets\ui-redesign\concepts\ui-layout-geometry-spec-2026-06-13.png"
}

$canvasWidth = 1920
$canvasHeight = 1080
$bitmap = New-Object System.Drawing.Bitmap $canvasWidth, $canvasHeight
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$graphics.Clear([System.Drawing.Color]::FromArgb(255, 4, 12, 22))

$gridPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(34, 62, 198, 255)), 1
for ($x = 0; $x -le $canvasWidth; $x += 120) {
    $graphics.DrawLine($gridPen, $x, 0, $x, $canvasHeight)
}
for ($y = 0; $y -le $canvasHeight; $y += 60) {
    $graphics.DrawLine($gridPen, 0, $y, $canvasWidth, $y)
}

$labelFont = New-Object System.Drawing.Font "Microsoft YaHei UI", 22, ([System.Drawing.FontStyle]::Bold)
$detailFont = New-Object System.Drawing.Font "Consolas", 16, ([System.Drawing.FontStyle]::Regular)
$smallFont = New-Object System.Drawing.Font "Consolas", 13, ([System.Drawing.FontStyle]::Regular)
$whiteBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(245, 248, 252))
$mutedBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(190, 205, 220))
$goldBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 213, 82))
$cyanBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(62, 198, 255))

function Draw-Region {
    param(
        [System.Drawing.Rectangle]$Rect,
        [System.Drawing.Color]$Fill,
        [System.Drawing.Color]$Stroke,
        [string]$Name,
        [string]$Geometry,
        [string]$Percent
    )

    $fillBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(82, $Fill))
    $strokePen = New-Object System.Drawing.Pen $Stroke, 3
    $graphics.FillRectangle($fillBrush, $Rect)
    $graphics.DrawRectangle($strokePen, $Rect)

    $textX = $Rect.X + 16
    $textY = $Rect.Y + 14
    $graphics.DrawString($Name, $labelFont, $whiteBrush, $textX, $textY)
    $graphics.DrawString($Geometry, $detailFont, $goldBrush, $textX, $textY + 38)
    $graphics.DrawString($Percent, $smallFont, $mutedBrush, $textX, $textY + 66)

    $fillBrush.Dispose()
    $strokePen.Dispose()
}

Draw-Region ([System.Drawing.Rectangle]::new(12, 8, 432, 286)) `
    ([System.Drawing.Color]::FromArgb(43, 94, 138)) ([System.Drawing.Color]::FromArgb(255, 213, 82)) `
    "A  AGENT OVERVIEW" "x=12 y=8 w=432 h=286" "x=.625% y=.741% w=22.5% h=26.5%"

Draw-Region ([System.Drawing.Rectangle]::new(456, 8, 1008, 286)) `
    ([System.Drawing.Color]::FromArgb(92, 56, 20)) ([System.Drawing.Color]::FromArgb(255, 170, 38)) `
    "B  BAKED THRONE + LOGO" "x=456 y=8 w=1008 h=286" "x=23.75% y=.741% w=52.5% h=26.5% | 3.524:1"

Draw-Region ([System.Drawing.Rectangle]::new(1476, 8, 432, 286)) `
    ([System.Drawing.Color]::FromArgb(43, 94, 138)) ([System.Drawing.Color]::FromArgb(255, 213, 82)) `
    "C  TASK STATUS" "x=1476 y=8 w=432 h=286" "x=76.875% y=.741% w=22.5% h=26.5%"

$dockRect = [System.Drawing.Rectangle]::new(12, 306, 1896, 58)
$dockFill = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(82, 26, 80, 92))
$dockPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(62, 198, 255)), 3
$graphics.FillRectangle($dockFill, $dockRect)
$graphics.DrawRectangle($dockPen, $dockRect)
$graphics.DrawString("D  WINDOW DOCK", $labelFont, $whiteBrush, 30, 316)
$graphics.DrawString("x=12 y=306 w=1896 h=58  |  x=.625% y=28.333% w=98.75% h=5.37%", $smallFont, $goldBrush, 300, 323)
$dockFill.Dispose()
$dockPen.Dispose()

$workbenchRect = [System.Drawing.Rectangle]::new(12, 376, 1896, 692)
$workbenchPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 213, 82)), 3
$graphics.DrawRectangle($workbenchPen, $workbenchRect)
$graphics.DrawString("E  ACTIVE WORKBENCH  x=12 y=376 w=1896 h=692  |  98.75% x 64.074%", $smallFont, $goldBrush, 1040, 380)
$workbenchPen.Dispose()

Draw-Region ([System.Drawing.Rectangle]::new(20, 384, 1880, 552)) `
    ([System.Drawing.Color]::FromArgb(12, 64, 86)) ([System.Drawing.Color]::FromArgb(62, 198, 255)) `
    "E1  WINDOW CONTENT / MESSAGE STREAM" "x=20 y=384 w=1880 h=552" "x=1.042% y=35.556% w=97.917% h=51.111%"

Draw-Region ([System.Drawing.Rectangle]::new(20, 948, 1880, 112)) `
    ([System.Drawing.Color]::FromArgb(86, 55, 14)) ([System.Drawing.Color]::FromArgb(255, 170, 38)) `
    "E2  CHAT COMPOSER" "x=20 y=948 w=1880 h=112" "x=1.042% y=87.778% w=97.917% h=10.37% | 16% of workbench"

$outputDirectory = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
$bitmap.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)

$gridPen.Dispose()
$labelFont.Dispose()
$detailFont.Dispose()
$smallFont.Dispose()
$whiteBrush.Dispose()
$mutedBrush.Dispose()
$goldBrush.Dispose()
$cyanBrush.Dispose()
$graphics.Dispose()
$bitmap.Dispose()

Write-Output $OutputPath
