<#
starting and stopping a private, hidden microsoft word for automation.

the one rule: only ever stop the word this script started. a word the
person opened, or one another program started, is never touched. the
started word is identified by giving its main window a unique caption and
asking windows which process owns that window; if that fails, by being the
only new "/Automation" word that appeared while we started ours.
#>

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class LettersWindows {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string title);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
"@ -ErrorAction SilentlyContinue

function Get-AutomationWords {
    @(Get-CimInstance Win32_Process -Filter "Name='WINWORD.EXE'" |
        Where-Object { $_.CommandLine -like "*/Automation*" } |
        ForEach-Object { [int]$_.ProcessId })
}

function Find-WordByCaption($word) {
    $tag = "letters-" + [guid]::NewGuid().ToString("N")
    $old = $word.Caption
    $word.Caption = $tag
    $window = [LettersWindows]::FindWindow("OpusApp", $tag)
    [uint32]$found = 0
    if ($window -ne [IntPtr]::Zero) {
        [void][LettersWindows]::GetWindowThreadProcessId($window, [ref]$found)
    }
    $word.Caption = $old
    return [int]$found
}

function Start-PrivateWord {
    $before = Get-AutomationWords
    $word = New-Object -ComObject Word.Application
    $id = 0
    try { $id = Find-WordByCaption $word } catch { $id = 0 }
    if ($id -eq 0) {
        $new = @()
        for ($try = 0; $try -lt 20 -and $new.Count -eq 0; $try++) {
            Start-Sleep -Milliseconds 250
            $new = @(Get-AutomationWords | Where-Object { $before -notcontains $_ })
        }
        if ($new.Count -eq 1) { $id = $new[0] }
    }
    # it must be a word that did not exist before we asked for one
    if ($id -eq 0 -or ($before -contains $id) -or -not ((Get-AutomationWords) -contains $id)) {
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
        throw "could not start a private Word instance"
    }
    $watchdog = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList @(
        "-NoProfile", "-Command", "Start-Sleep -Seconds 300; Stop-Process -Id $id -Force -ErrorAction SilentlyContinue")
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
