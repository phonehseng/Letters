# opens a presentation in powerpoint without a window and prints what it
# holds, one "name=value" line each. a repair prompt would block the open,
# and the test's time limit turns that into a failure. powerpoint runs
# once per computer, so if it was already open it is left running.
param([string]$Path)
$ErrorActionPreference = 'Stop'
$wasRunning = @(Get-Process POWERPNT -ErrorAction SilentlyContinue).Count -gt 0
$powerpoint = New-Object -ComObject PowerPoint.Application
try {
    # read only, not untitled, no window
    $presentation = $powerpoint.Presentations.Open($Path, -1, 0, 0)
    "slides=" + $presentation.Slides.Count
    "width=" + $presentation.PageSetup.SlideWidth
    "height=" + $presentation.PageSetup.SlideHeight
    $index = 0
    foreach ($slide in $presentation.Slides) {
        $index += 1
        $texts = @()
        foreach ($shape in $slide.Shapes) {
            if ($shape.HasTextFrame -and $shape.TextFrame.HasText) {
                $text = $shape.TextFrame.TextRange.Text
                $texts += $text
                "box.$text=" + $shape.Left + "," + $shape.Top + "," + $shape.TextFrame.TextRange.Font.Size + "," + $shape.TextFrame.TextRange.Font.Name
            }
        }
        "slide$index.shapes=" + $slide.Shapes.Count
        "slide$index.text=" + ($texts -join '|')
    }
    $presentation.Close()
}
finally {
    if (-not $wasRunning -and $powerpoint.Presentations.Count -eq 0) {
        $powerpoint.Quit()
    }
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($powerpoint)
}
