<#
asks microsoft word where its pages break, for Strict Word mode.

  -Path in.docx -Out layout.json

writes the page each paragraph starts on (and whether it is in a table)
and the page count. a private hidden word, opened read-only, found from its
own window and the only word this script ever stops (the same care as
word_export.ps1).
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class LettersLayoutWindows {
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
"@

function Get-AutomationWords {
    @(Get-CimInstance Win32_Process -Filter "Name='WINWORD.EXE'" | Where-Object { $_.CommandLine -like "*/Automation*" } | ForEach-Object { $_.ProcessId })
}

$word = New-Object -ComObject Word.Application
$id = 0
try {
    $probe = $word.Documents.Add()
    [uint32]$found = 0
    [void][LettersLayoutWindows]::GetWindowThreadProcessId([IntPtr]$probe.ActiveWindow.Hwnd, [ref]$found)
    $probe.Close(0)
    $id = [int]$found
}
catch { $id = 0 }
if ((Get-AutomationWords) -notcontains $id) {
    try { $word.Quit(0) } catch { }
    [Console]::Error.WriteLine("could not identify the private Word")
    exit 1
}
$watchdog = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList @("-NoProfile", "-Command", "Start-Sleep -Seconds 120; Stop-Process -Id $id -Force -ErrorAction SilentlyContinue")
$failed = $false
try {
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $doc = $word.Documents.Open((Resolve-Path $Path).Path, $false, $true, $false)
    try {
        # print layout, so pages are word's printed pages
        $doc.Repaginate()
        $paragraphs = @()
        foreach ($paragraph in $doc.Paragraphs) {
            # 3 is wdActiveEndPageNumber, 12 is wdWithInTable
            $start = $paragraph.Range.Duplicate
            $start.Collapse(1)
            $paragraphs += [ordered]@{ page = $start.Information(3); inTable = [bool]$paragraph.Range.Information(12) }
        }
        [ordered]@{ paragraphs = $paragraphs; pageCount = $doc.ComputeStatistics(2) } | ConvertTo-Json -Depth 4 -Compress | Out-File -FilePath $Out -Encoding utf8
    }
    finally { $doc.Close(0) }
}
catch {
    [Console]::Error.WriteLine("word_layout.ps1 line $($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)")
    $failed = $true
}
finally {
    try { $word.Quit(0) } catch { }
    try { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) } catch { }
    for ($try = 0; $try -lt 40 -and (Get-Process -Id $id -ErrorAction SilentlyContinue); $try++) { Start-Sleep -Milliseconds 250 }
    if (Get-Process -Id $id -ErrorAction SilentlyContinue) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    if (-not $watchdog.HasExited) { Stop-Process -Id $watchdog.Id -Force -ErrorAction SilentlyContinue }
}
if ($failed) { exit 1 }
