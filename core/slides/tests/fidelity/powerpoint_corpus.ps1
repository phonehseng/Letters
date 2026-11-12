# builds the graded corpus of decks with powerpoint's own automation and
# exports each one to pdf, so powerpoint is the reference for its decks.
#
# safety: powerpoint cannot run invisible, so it runs minimised. the
# script refuses to start when any powerpoint is already running (com
# would attach to it), remembers the pid of the one it starts and ends
# only that one, in a finally block.
#
# usage: powershell -File powerpoint_corpus.ps1 -Out <folder> [-ExportOnly <folder of pptx>]

param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$ExportOnly = ""
)

$ErrorActionPreference = "Stop"
$themes = "C:\Program Files\Microsoft Office\Root\Document Themes 16"
# office enumerations used below, named so the calls read as sentences
$layoutTitle = 1; $layoutText = 2; $layoutTwoColumn = 3; $layoutTitleOnly = 11; $layoutBlank = 12
$shapeRectangle = 1; $shapeRoundedRectangle = 5; $shapeOval = 9; $shapeRightArrow = 33
$shapeStar5 = 92; $shapeChevron = 52; $shapeTriangle = 7
$connectorStraight = 1; $connectorElbow = 2
$autoSizeShapeToFitText = 1; $autoSizeTextToFitShape = 2
$saveAsPdf = 32; $saveAsPptx = 24

function New-TestPicture([string]$path) {
    # a picture with distinct quarters, so crops are visible in renders
    Add-Type -AssemblyName System.Drawing
    $bitmap = New-Object System.Drawing.Bitmap 400, 300
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.FillRectangle([System.Drawing.Brushes]::SteelBlue, 0, 0, 200, 150)
    $graphics.FillRectangle([System.Drawing.Brushes]::Orange, 200, 0, 200, 150)
    $graphics.FillRectangle([System.Drawing.Brushes]::SeaGreen, 0, 150, 200, 150)
    $graphics.FillRectangle([System.Drawing.Brushes]::Crimson, 200, 150, 200, 150)
    $graphics.Dispose()
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
}

function Add-Bullets($shape, [string[]]$lines, [int[]]$levels) {
    $shape.TextFrame.TextRange.Text = ($lines -join "`r")
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $shape.TextFrame.TextRange.Paragraphs($i + 1).IndentLevel = $levels[$i]
    }
}

function Build-TitleAndBullets($app, [string]$theme, [string]$path) {
    $deck = $app.Presentations.Add(0)
    if ($theme) { $deck.ApplyTheme((Join-Path $themes $theme)) }
    $title = $deck.Slides.Add(1, $layoutTitle)
    $title.Shapes.Item(1).TextFrame.TextRange.Text = "Photosynthesis"
    $title.Shapes.Item(2).TextFrame.TextRange.Text = "How plants turn light into sugar"
    $content = $deck.Slides.Add(2, $layoutText)
    $content.Shapes.Item(1).TextFrame.TextRange.Text = "The two stages"
    Add-Bullets $content.Shapes.Item(2) @("Light-dependent reactions", "Happen in the thylakoids", "Make ATP and NADPH", "The Calvin cycle", "Fixes carbon dioxide into sugar") @(1, 2, 2, 1, 2)
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-TwoContent($app, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutTwoColumn)
    $slide.Shapes.Item(1).TextFrame.TextRange.Text = "Mitosis and meiosis"
    Add-Bullets $slide.Shapes.Item(2) @("Mitosis", "Two identical cells", "Growth and repair") @(1, 2, 2)
    Add-Bullets $slide.Shapes.Item(3) @("Meiosis", "Four different cells", "Makes gametes") @(1, 2, 2)
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-Table($app, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutTitleOnly)
    $slide.Shapes.Item(1).TextFrame.TextRange.Text = "Planet facts"
    $table = $slide.Shapes.AddTable(4, 3, 60, 140, 600, 200).Table
    $rows = @(@("Planet", "Moons", "Day (hours)"), @("Earth", "1", "24"), @("Mars", "2", "24.6"), @("Jupiter", "95", "9.9"))
    for ($r = 0; $r -lt 4; $r++) { for ($c = 0; $c -lt 3; $c++) { $table.Cell($r + 1, $c + 1).Shape.TextFrame.TextRange.Text = $rows[$r][$c] } }
    $table.Cell(4, 1).Merge($table.Cell(4, 2))
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-Pictures($app, [string]$picture, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutBlank)
    $plain = $slide.Shapes.AddPicture($picture, 0, -1, 40, 40, 300, 225)
    $cropped = $slide.Shapes.AddPicture($picture, 0, -1, 380, 40, 300, 225)
    $cropped.PictureFormat.CropLeft = 60
    $cropped.PictureFormat.CropTop = 30
    $rotated = $slide.Shapes.AddPicture($picture, 0, -1, 200, 300, 200, 150)
    $rotated.Rotation = 20
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-Shapes($app, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutBlank)
    $rounded = $slide.Shapes.AddShape($shapeRoundedRectangle, 40, 40, 200, 120)
    $rounded.Adjustments.Item(1) = 0.35
    $rounded.TextFrame.TextRange.Text = "Rounded"
    $arrow = $slide.Shapes.AddShape($shapeRightArrow, 280, 40, 220, 120)
    $arrow.Adjustments.Item(1) = 0.3
    $arrow.Adjustments.Item(2) = 0.7
    $star = $slide.Shapes.AddShape($shapeStar5, 540, 40, 140, 140)
    $oval = $slide.Shapes.AddShape($shapeOval, 40, 220, 180, 120)
    $oval.Fill.TwoColorGradient(1, 1)
    $oval.Fill.ForeColor.RGB = 0x3366CC
    $oval.Fill.BackColor.RGB = 0xFFCC66
    $chevron = $slide.Shapes.AddShape($shapeChevron, 260, 230, 160, 100)
    $chevron.Rotation = 30
    $triangle = $slide.Shapes.AddShape($shapeTriangle, 460, 230, 140, 120)
    $triangle.Line.Weight = 4
    $triangle.Line.DashStyle = 4
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-GroupsAndConnectors($app, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutBlank)
    $a = $slide.Shapes.AddShape($shapeRectangle, 60, 60, 150, 80)
    $a.TextFrame.TextRange.Text = "Input"
    $b = $slide.Shapes.AddShape($shapeRectangle, 420, 260, 150, 80)
    $b.TextFrame.TextRange.Text = "Output"
    $line = $slide.Shapes.AddConnector($connectorElbow, 0, 0, 10, 10)
    $line.ConnectorFormat.BeginConnect($a, 4)
    $line.ConnectorFormat.EndConnect($b, 2)
    $line.Line.Weight = 2
    $c = $slide.Shapes.AddShape($shapeOval, 520, 40, 80, 80)
    $d = $slide.Shapes.AddShape($shapeTriangle, 610, 40, 80, 80)
    $group = $slide.Shapes.Range([object[]]@($c.Name, $d.Name)).Group()
    $group.Rotation = 15
    $straight = $slide.Shapes.AddConnector($connectorStraight, 60, 400, 400, 460)
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-Autofit($app, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $slide = $deck.Slides.Add(1, $layoutText)
    $slide.Shapes.Item(1).TextFrame.TextRange.Text = "Causes of the French Revolution"
    $lines = @()
    $levels = @()
    for ($i = 1; $i -le 14; $i++) { $lines += "Point number $i about money, bread prices and the estates general"; $levels += (1 + ($i % 2)) }
    Add-Bullets $slide.Shapes.Item(2) $lines $levels
    $box = $slide.Shapes.AddTextbox(1, 40, 380, 300, 60)
    $box.TextFrame2.AutoSize = $autoSizeTextToFitShape
    $box.TextFrame.TextRange.Text = "A small box with far too much text in it so that powerpoint has to shrink it to fit inside."
    $slide.NotesPage.Shapes.Placeholders(2).TextFrame.TextRange.Text = "Speak about the bread prices first."
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Build-SlideSize($app, [int]$width, [int]$height, [string]$path) {
    $deck = $app.Presentations.Add(0)
    $deck.PageSetup.SlideWidth = $width
    $deck.PageSetup.SlideHeight = $height
    $slide = $deck.Slides.Add(1, $layoutText)
    $slide.Shapes.Item(1).TextFrame.TextRange.Text = "A different slide size"
    Add-Bullets $slide.Shapes.Item(2) @("Placeholders scale with the slide", "Text keeps its size") @(1, 1)
    $deck.SaveAs($path, $saveAsPptx)
    $deck.Close()
}

function Export-Pdf($app, [string]$pptx, [string]$pdf) {
    # read-only, untitled, no window: nothing in the file changes
    $deck = $app.Presentations.Open($pptx, -1, 0, 0)
    $deck.SaveAs($pdf, $saveAsPdf)
    $deck.Close()
}

$before = @(Get-Process POWERPNT -ErrorAction SilentlyContinue)
if ($before.Count -gt 0) { throw "powerpoint is already running; close it first (this script only drives a powerpoint it starts)" }
New-Item -ItemType Directory -Force $Out | Out-Null
$app = $null
$powerpointPid = $null
try {
    $app = New-Object -ComObject PowerPoint.Application
    $started = @(Get-Process POWERPNT -ErrorAction SilentlyContinue)
    if ($started.Count -ne 1) { throw "expected exactly one powerpoint process, found $($started.Count)" }
    $powerpointPid = $started[0].Id
    Write-Output "powerpoint pid $powerpointPid"
    $app.DisplayAlerts = 1
    if ($ExportOnly -eq "") {
        $picture = Join-Path $Out "quarters.png"
        New-TestPicture $picture
        Build-TitleAndBullets $app "" (Join-Path $Out "pp_title_bullets.pptx")
        Build-TitleAndBullets $app "Facet.thmx" (Join-Path $Out "pp_theme_facet.pptx")
        Build-TitleAndBullets $app "Ion.thmx" (Join-Path $Out "pp_theme_ion.pptx")
        Build-TitleAndBullets $app "Retrospect.thmx" (Join-Path $Out "pp_theme_retrospect.pptx")
        Build-TwoContent $app (Join-Path $Out "pp_two_content.pptx")
        Build-Table $app (Join-Path $Out "pp_table.pptx")
        Build-Pictures $app $picture (Join-Path $Out "pp_pictures.pptx")
        Build-Shapes $app (Join-Path $Out "pp_shapes.pptx")
        Build-GroupsAndConnectors $app (Join-Path $Out "pp_groups_connectors.pptx")
        Build-Autofit $app (Join-Path $Out "pp_autofit_notes.pptx")
        Build-SlideSize $app 720 540 (Join-Path $Out "pp_size_4x3.pptx")
        Build-SlideSize $app 842 595 (Join-Path $Out "pp_size_a4.pptx")
        $source = $Out
    } else {
        $source = $ExportOnly
    }
    foreach ($file in Get-ChildItem $source | Where-Object { $_.Extension -in ".pptx", ".odp" }) {
        $pdf = Join-Path $Out ($file.BaseName + ".powerpoint.pdf")
        # one deck powerpoint cannot open must not stop the others
        try {
            Export-Pdf $app $file.FullName $pdf
            Write-Output "exported $($file.Name)"
        } catch {
            Write-Output "powerpoint could not export $($file.Name): $($_.Exception.Message)"
        }
    }
}
finally {
    if ($app -ne $null) {
        try { $app.Quit() } catch {}
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($app)
    }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
    if ($powerpointPid -ne $null) {
        Start-Sleep -Seconds 2
        $left = Get-Process -Id $powerpointPid -ErrorAction SilentlyContinue
        if ($left) { Stop-Process -Id $powerpointPid -Force; Write-Output "ended powerpoint $powerpointPid" }
    }
}
