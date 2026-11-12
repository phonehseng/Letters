# opens decks in a powerpoint this script starts (minimised, ended by
# pid in a finally block, never one that was already running) and
# reports, per deck, whether powerpoint opened it without a repair
# prompt, and every shape: slide, name, left, top, width, height
# (points) and its text. the output is json lines for the tests.
#
# usage: powershell -File powerpoint_check.ps1 -Decks <file> [<file>...]

param([Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Decks)

$ErrorActionPreference = "Stop"
if (@(Get-Process POWERPNT -ErrorAction SilentlyContinue).Count -gt 0) {
    throw "powerpoint is already running; close it first (this script only drives a powerpoint it starts)"
}
$app = $null
$powerpointPid = $null
try {
    $app = New-Object -ComObject PowerPoint.Application
    $powerpointPid = @(Get-Process POWERPNT)[0].Id
    foreach ($path in $Decks) {
        $result = [ordered]@{ deck = $path; opened = $false; error = $null; shapes = @() }
        try {
            # read-only, untitled off, no window; a file that needs repair
            # raises an error here instead of showing the repair prompt
            $deck = $app.Presentations.Open($path, -1, 0, 0)
            $result.opened = $true
            foreach ($slide in $deck.Slides) {
                foreach ($shape in $slide.Shapes) {
                    $text = ""
                    if ($shape.HasTextFrame -eq -1) { $text = $shape.TextFrame.TextRange.Text }
                    $result.shapes += [ordered]@{ slide = $slide.SlideIndex; name = $shape.Name; left = $shape.Left; top = $shape.Top; width = $shape.Width; height = $shape.Height; text = $text }
                }
            }
            $deck.Close()
        } catch {
            $result.error = $_.Exception.Message
        }
        Write-Output ($result | ConvertTo-Json -Compress -Depth 4)
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
        if (Get-Process -Id $powerpointPid -ErrorAction SilentlyContinue) { Stop-Process -Id $powerpointPid -Force }
    }
}
