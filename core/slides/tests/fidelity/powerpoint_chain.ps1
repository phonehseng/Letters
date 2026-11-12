# powerpoint's own back-and-forth, the baseline letters has to beat:
# each pptx is saved as odp by powerpoint, that odp opened again and
# saved as pptx; then the pdf of every pptx in -Export is written.
# the same safety rules as powerpoint_corpus.ps1: refuses to run beside
# another powerpoint, ends only the one it started, by pid.
#
# usage: powershell -File powerpoint_chain.ps1 -Decks <folder> -Out <folder> [-Export <folder>]

param(
    [Parameter(Mandatory = $true)][string]$Decks,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Export = ""
)

$ErrorActionPreference = "Stop"
$saveAsPptx = 24
$saveAsOdp = 35
$saveAsPdf = 32
if (@(Get-Process POWERPNT -ErrorAction SilentlyContinue).Count -gt 0) {
    throw "powerpoint is already running; close it first (this script only drives a powerpoint it starts)"
}
New-Item -ItemType Directory -Force $Out | Out-Null
$app = $null
$powerpointPid = $null
try {
    $app = New-Object -ComObject PowerPoint.Application
    $powerpointPid = @(Get-Process POWERPNT)[0].Id
    foreach ($file in Get-ChildItem $Decks -Filter *.pptx) {
        try {
            $odp = Join-Path $Out ($file.BaseName + ".powerpoint.odp")
            $deck = $app.Presentations.Open($file.FullName, -1, 0, 0)
            $deck.SaveAs($odp, $saveAsOdp)
            $deck.Close()
            $deck = $app.Presentations.Open($odp, -1, 0, 0)
            $deck.SaveAs((Join-Path $Out ($file.BaseName + ".pptx")), $saveAsPptx)
            $deck.Close()
            Write-Output "chained $($file.Name)"
        } catch {
            Write-Output "powerpoint could not chain $($file.Name): $($_.Exception.Message)"
        }
    }
    if ($Export -ne "") {
        foreach ($file in Get-ChildItem $Export -Filter *.pptx) {
            try {
                $deck = $app.Presentations.Open($file.FullName, -1, 0, 0)
                $deck.SaveAs((Join-Path $Export ($file.BaseName + ".powerpoint.pdf")), $saveAsPdf)
                $deck.Close()
            } catch {
                Write-Output "powerpoint could not export $($file.Name): $($_.Exception.Message)"
            }
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
        if (Get-Process -Id $powerpointPid -ErrorAction SilentlyContinue) { Stop-Process -Id $powerpointPid -Force }
    }
}
