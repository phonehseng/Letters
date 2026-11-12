# makes the document icons Windows shows for files Letters opens: a white
# page with a folded corner, the Letters icon in the middle and the file's
# extension underneath. one .ico per type, into app/shell/icons/documents.
#
# run from the repository root with Windows PowerShell:
#   powershell -ExecutionPolicy Bypass -File tools\icon\make-document-icons.ps1
#
# each .ico holds one 256 pixel png, which Windows scales for smaller
# views (an ico may hold a png since Windows Vista).

Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$source = Join-Path $PSScriptRoot "icon.png"
$out = Join-Path $root "app\shell\icons\documents"
New-Item -ItemType Directory -Force $out | Out-Null

# the same names the installer hook uses (app/shell/installer-hooks.nsh)
$types = "pdf", "docx", "odt", "xlsx", "ods", "pptx", "odp", "rtf", "png", "jpg", "tif", "bmp", "gif", "webp"

function New-DocumentPng([string]$label) {
    $size = 256
    $bitmap = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bitmap)
    $g.SmoothingMode = "AntiAlias"
    $g.TextRenderingHint = "AntiAliasGridFit"
    $g.Clear([System.Drawing.Color]::Transparent)

    # the page: 176 wide, 232 tall, with a 48 pixel fold at the top right
    $left = 40; $top = 12; $right = 216; $bottom = 244; $fold = 48
    $page = New-Object System.Drawing.Drawing2D.GraphicsPath
    $page.AddLine($left, $top, $right - $fold, $top)
    $page.AddLine($right - $fold, $top, $right, $top + $fold)
    $page.AddLine($right, $top + $fold, $right, $bottom)
    $page.AddLine($right, $bottom, $left, $bottom)
    $page.CloseFigure()
    $g.FillPath([System.Drawing.Brushes]::White, $page)
    $edge = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 160, 160, 168)), 4
    $g.DrawPath($edge, $page)
    $corner = New-Object System.Drawing.Drawing2D.GraphicsPath
    $corner.AddLine($right - $fold, $top, $right - $fold, $top + $fold)
    $corner.AddLine($right - $fold, $top + $fold, $right, $top + $fold)
    $corner.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 226, 226, 232))), $corner)
    $g.DrawPath($edge, $corner)

    # the app's own icon, so the file says which app opens it
    $icon = [System.Drawing.Image]::FromFile($source)
    $g.DrawImage($icon, 72, 64, 112, 112)
    $icon.Dispose()

    # the extension, in the system font
    $font = New-Object System.Drawing.Font "Segoe UI", 30, ([System.Drawing.FontStyle]::Bold), ([System.Drawing.GraphicsUnit]::Pixel)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = "Center"
    $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 60, 60, 67))
    $g.DrawString($label.ToUpper(), $font, $brush, (New-Object System.Drawing.RectangleF $left, 188, ($right - $left), 48), $format)
    $g.Dispose()

    $stream = New-Object System.IO.MemoryStream
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    return ,$stream.ToArray()
}

# an .ico file with one png image inside: a 6 byte header, a 16 byte
# directory entry (0 width and height mean 256), then the png
function Write-Ico([byte[]]$png, [string]$path) {
    $file = New-Object System.IO.MemoryStream
    $writer = New-Object System.IO.BinaryWriter $file
    $writer.Write([UInt16]0); $writer.Write([UInt16]1); $writer.Write([UInt16]1)
    $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([UInt16]1); $writer.Write([UInt16]32)
    $writer.Write([UInt32]$png.Length); $writer.Write([UInt32]22)
    $writer.Write($png)
    $writer.Flush()
    [System.IO.File]::WriteAllBytes($path, $file.ToArray())
}

foreach ($type in $types) {
    $png = New-DocumentPng $type
    Write-Ico $png (Join-Path $out "$type.ico")
    Write-Output "made $type.ico"
}
