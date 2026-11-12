<#
turns a .docx into a pdf with microsoft word, for "Export as PDF".

  -Path in.docx -Out out.pdf

the same careful approach as core/office/tests/tools/word_check.ps1: it
starts its own hidden word, opens a copy read-only with alerts off, and
only ever stops the word it started (word started for automation carries
the /Automation switch). word windows the person has open are never
touched. a watchdog ends the private word if it hangs on a hidden dialog.
#>
param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Out
)

$ErrorActionPreference = "Stop"

function Get-AutomationWords {
    @(Get-CimInstance Win32_Process -Filter "Name='WINWORD.EXE'" |
        Where-Object { $_.CommandLine -like "*/Automation*" } |
        ForEach-Object { $_.ProcessId })
}

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class LettersWindows {
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
"@

# the process behind the word we were handed, found from one of its own
# windows. counting new processes is not enough: another program driving
# word at the same moment would make the count unclear
function Get-WordProcessId($word) {
    $probe = $word.Documents.Add()
    try {
        $hwnd = [IntPtr]$probe.ActiveWindow.Hwnd
        [uint32]$id = 0
        [void][LettersWindows]::GetWindowThreadProcessId($hwnd, [ref]$id)
        return [int]$id
    }
    finally {
        $probe.Close(0)
    }
}

function Start-PrivateWord {
    $word = New-Object -ComObject Word.Application
    $id = 0
    try { $id = Get-WordProcessId $word } catch { $id = 0 }
    if ((Get-AutomationWords) -notcontains $id) {
        # not certainly a word started for us: ask it to quit through com,
        # which only ever ends the instance com gave us, and stop
        try { $word.Quit(0) } catch { }
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
        throw "could not identify the private Word (process $id)"
    }
    $watchdog = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList @(
        "-NoProfile", "-Command", "Start-Sleep -Seconds 120; Stop-Process -Id $id -Force -ErrorAction SilentlyContinue")
    $session = @{ Word = $word; Pid = $id; Watchdog = $watchdog }
    try {
        $word.Visible = $false
        $word.DisplayAlerts = 0
        $word.ScreenUpdating = $false
    }
    catch {
        Stop-PrivateWord $session
        throw
    }
    return $session
}

function Stop-PrivateWord($session) {
    try { $session.Word.Quit(0) } catch { }
    try { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($session.Word) } catch { }
    for ($try = 0; $try -lt 40; $try++) {
        if (-not (Get-Process -Id $session.Pid -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 250
    }
    if (Get-Process -Id $session.Pid -ErrorAction SilentlyContinue) {
        Stop-Process -Id $session.Pid -Force -ErrorAction SilentlyContinue
    }
    if ($session.Watchdog -and -not $session.Watchdog.HasExited) {
        Stop-Process -Id $session.Watchdog.Id -Force -ErrorAction SilentlyContinue
    }
}

$session = $null
$failed = $false
try {
    $session = Start-PrivateWord
    $full = (Resolve-Path $Path).Path
    # FileName, ConfirmConversions, ReadOnly, AddToRecentFiles
    $doc = $session.Word.Documents.Open($full, $false, $true, $false)
    try {
        # 17 is pdf; OpenAfterExport false
        $doc.ExportAsFixedFormat([IO.Path]::GetFullPath($Out), 17, $false)
    }
    finally {
        $doc.Close(0)
    }
}
catch {
    [Console]::Error.WriteLine("word_export.ps1 line $($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)")
    $failed = $true
}
finally {
    if ($session) { Stop-PrivateWord $session }
}
if ($failed) { exit 1 }
