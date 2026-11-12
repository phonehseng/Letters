# what letters asks of a private, hidden microsoft word: layout, pdf, conversions and batches.
<#
the modes, used only when word is installed:

  -Mode layout  -Path in.docx -Out layout.json   where word puts every line
                                                 and floating object
  -Mode pdf     -Path in.docx -Out out.pdf       the print-ready pdf
  -Mode convert -Path in -Out out -Format N      save in another format
                                                 (16 docx, 0 doc, 6 rtf,
                                                 23 odt)
  -Mode batch   -Path jobs.json -Out results.json
                                                 many pdf and convert jobs
                                                 in one private word: a
                                                 json list of { op ("pdf" or
                                                 "convert"), in, out,
                                                 format }; results.json says
                                                 per job "ok" or the error

files are opened read-only in a private hidden word (see word_session.ps1).
#>
param(
    [Parameter(Mandatory = $true)][ValidateSet("layout", "pdf", "convert", "batch")][string]$Mode,
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Format = 16
)

# a batch job is given this long on top of the start, and each job its own
# share, so a hang is ended by the watchdog instead of waiting forever
$SecondsPerBatchJob = 40

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\word_session.ps1"

function Open-ReadOnly($word, $file) {
    $full = (Resolve-Path $file).Path
    # FileName, ConfirmConversions, ReadOnly, AddToRecentFiles
    return $word.Documents.Open($full, $false, $true, $false)
}

function Get-Layout($word, $file) {
    $started = Get-Date
    $doc = Open-ReadOnly $word $file
    try {
        $window = $doc.Windows.Item(1)
        $window.View.Type = 3
        $doc.Repaginate()
        # word's paragraph starts, to give each line its paragraph number
        $starts = New-Object System.Collections.Generic.List[int]
        foreach ($paragraph in $doc.Paragraphs) { $starts.Add($paragraph.Range.Start) }
        $pane = $window.Panes.Item(1)
        $pages = @()
        for ($number = 1; $number -le $pane.Pages.Count; $number++) {
            $page = $pane.Pages.Item($number)
            $lines = @()
            foreach ($rectangle in $page.Rectangles) {
                if ($rectangle.RectangleType -ne 0) { continue }
                foreach ($line in $rectangle.Lines) {
                    $range = $line.Range
                    $story = $range.StoryType
                    $index = -1
                    if ($story -eq 1) {
                        $index = $starts.BinarySearch($range.Start)
                        if ($index -lt 0) { $index = (-bnot $index) - 1 }
                    }
                    $text = $range.Text
                    if ($text.Length -gt 24) { $text = $text.Substring(0, 24) }
                    $lines += [ordered]@{
                        top = [math]::Round($line.Top * 20); left = [math]::Round($line.Left * 20)
                        height = [math]::Round($line.Height * 20); width = [math]::Round($line.Width * 20)
                        story = $story; paragraph = $index; start = $range.Start; end = $range.End
                        text = $text.TrimEnd([char]13, [char]7)
                    }
                }
            }
            $pages += [ordered]@{ number = $number; width = [math]::Round($page.Width * 20); height = [math]::Round($page.Height * 20); lines = $lines }
        }
        $shapes = @()
        foreach ($shape in $doc.Shapes) {
            $text = ""
            try { if ($shape.TextFrame.HasText) { $text = $shape.TextFrame.TextRange.Text.TrimEnd([char]13) } } catch { }
            $shapes += [ordered]@{
                name = $shape.Name; type = $shape.Type; page = $shape.Anchor.Information(3)
                left = [math]::Round($shape.Left * 20); top = [math]::Round($shape.Top * 20)
                width = [math]::Round($shape.Width * 20); height = [math]::Round($shape.Height * 20)
                relativeHorizontal = $shape.RelativeHorizontalPosition; relativeVertical = $shape.RelativeVerticalPosition
                text = $text
            }
        }
        return [ordered]@{
            pages = $pages; shapes = $shapes; wordPages = $doc.ComputeStatistics(2)
            milliseconds = [math]::Round(((Get-Date) - $started).TotalMilliseconds)
        }
    }
    finally {
        $doc.Close(0)
    }
}


# windows powershell 5.1 passes a json array down a pipeline as one item,
# so the list is converted as an argument, which gives the jobs one by one
function Read-Jobs($file) {
    return ConvertFrom-Json -InputObject (Get-Content -Raw -Path $file)
}

function Invoke-Job($word, $job) {
    $doc = Open-ReadOnly $word $job.in
    try {
        if ($job.op -eq "pdf") {
            $doc.ExportAsFixedFormat([IO.Path]::GetFullPath($job.out), 17)
        }
        else {
            $doc.SaveAs2([IO.Path]::GetFullPath($job.out), [int]$job.format)
        }
    }
    finally { $doc.Close(0) }
}

# one failing document must not stop the others, so each job reports its
# own result
function Invoke-Batch($word, $file, $results) {
    $jobs = Read-Jobs $file
    $outcome = @()
    foreach ($job in $jobs) {
        try {
            Invoke-Job $word $job
            $outcome += "ok"
        }
        catch {
            $outcome += $_.Exception.Message
        }
    }
    ConvertTo-Json -InputObject @($outcome) -Compress | Out-File -FilePath $results -Encoding utf8
}

$watchdog = 300
if ($Mode -eq "batch") {
    $count = @(Read-Jobs $Path).Count
    $watchdog = 60 + $SecondsPerBatchJob * $count
}

$session = $null
$failed = $false
try {
    $session = Start-PrivateWord $watchdog
    switch ($Mode) {
        "batch" {
            Invoke-Batch $session.Word $Path $Out
        }
        "layout" {
            $layout = Get-Layout $session.Word $Path
            $layout | ConvertTo-Json -Depth 8 -Compress | Out-File -FilePath $Out -Encoding utf8
        }
        "pdf" {
            $doc = Open-ReadOnly $session.Word $Path
            try { $doc.ExportAsFixedFormat([IO.Path]::GetFullPath($Out), 17) } finally { $doc.Close(0) }
        }
        "convert" {
            $doc = Open-ReadOnly $session.Word $Path
            try { $doc.SaveAs2([IO.Path]::GetFullPath($Out), $Format) } finally { $doc.Close(0) }
        }
    }
}
catch {
    [Console]::Error.WriteLine("word_bridge.ps1 line $($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)")
    $failed = $true
}
finally {
    if ($session) { Stop-PrivateWord $session }
}
if ($failed) { exit 1 }
