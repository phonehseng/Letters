# exports docx files to pdf with a private, invisible copy of microsoft
# word, through its com automation. letters runs this with windows
# powershell and waits for it with a time limit.
#
# the rules it keeps:
# - its own copy of word: com must have started a new process, with no
#   documents in it. if it handed over a copy that was already running,
#   nothing about that copy is changed and the script stops
# - that copy is identified twice: it is the one new winword process, and
#   its main window carries a unique title this script gives it (the
#   title lives only as long as the process; nothing is saved)
# - alerts off, invisible, documents opened read only, without adding them
#   to recent files, with a made-up password so a locked file fails at once
# - word's application options are never changed
# - only our own copy is ever quit, and letters ends that process itself
#   if it is still there afterwards
#
# input: a jobs file, one "docx<tab>pdf" per line (utf-8)
# what it prints, one line each, for letters to read:
#   ours=<process id>          as soon as our copy is known
#   shared=<process id>        com gave us a copy that isn't ours: stopped
#   build=<word build>
#   exported=<job number>      a pdf was written
#   failed=<job number>:<why>  it wasn't
#   done                       every job was tried
#   error=<message>            word couldn't be started or used
param(
    [Parameter(Mandatory = $true)][string]$JobsPath,
    [Parameter(Mandatory = $true)][string]$Marker
)
$ErrorActionPreference = 'Stop'

function Say([string]$text) {
    [Console]::Out.WriteLine($text)
    [Console]::Out.Flush()
}

Add-Type -Namespace Letters -Name ExactWindows -MemberDefinition @'
[DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr window, out uint processId);
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern System.IntPtr FindWindow(string className, string windowName);
'@

function WordProcesses {
    return @(Get-Process -Name 'WINWORD' -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
}

function ProcessOfMarkedWindow([string]$title) {
    $window = [Letters.ExactWindows]::FindWindow('OpusApp', $title)
    if ($window -eq [System.IntPtr]::Zero) { return 0 }
    $id = [uint32]0
    [void][Letters.ExactWindows]::GetWindowThreadProcessId($window, [ref]$id)
    return [int]$id
}

$before = WordProcesses
$password = 'letters-no-password'
$missing = [Type]::Missing

try {
    $word = New-Object -ComObject 'Word.Application'
}
catch {
    Say ("error=" + $_.Exception.Message)
    exit 3
}

$processId = 0
$ours = $false
try {
    $new = @(WordProcesses | Where-Object { $before -notcontains $_ })
    if ($word.Documents.Count -eq 0 -and $new.Count -ge 1) {
        $word.Caption = $Marker
        $marked = ProcessOfMarkedWindow $Marker
        if ($marked -ne 0 -and $new -contains $marked) { $processId = $marked }
        elseif ($marked -eq 0 -and $new.Count -eq 1) { $processId = $new[0] }
        $ours = $processId -ne 0
    }
}
catch { $ours = $false }

if (-not $ours) {
    Say "shared=$processId"
    try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($word) } catch {}
    exit 4
}
Say "ours=$processId"

$exitCode = 0
try {
    Say ("build=" + $word.Build)
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $word.ScreenUpdating = $false
    $jobs = @(Get-Content -LiteralPath $JobsPath -Encoding UTF8 | Where-Object { $_ -ne '' })
    for ($index = 0; $index -lt $jobs.Count; $index++) {
        $parts = $jobs[$index] -split "`t"
        $document = $null
        try {
            # file, confirm conversions, read only, add to recent files,
            # password, template password, revert, write password,
            # template write password, format, encoding, visible
            $document = $word.Documents.Open($parts[0], $false, $true, $false, $password, $password, $false, $password, $password, 0, $missing, $false)
            # pagination must be final before the export
            $document.Repaginate()
            # file, pdf (17), open after, optimise for print, whole document,
            # from, to, content only, no properties, keep irm, no bookmarks,
            # no structure tags, bitmap missing fonts, not pdf/a
            $document.ExportAsFixedFormat($parts[1], 17, $false, 0, 0, 1, 1, 0, $false, $true, 0, $false, $true, $false)
            Say "exported=$index"
        }
        catch {
            Say ("failed=" + $index + ":" + $_.Exception.Message)
        }
        finally {
            if ($null -ne $document) { try { $document.Close(0) } catch {} }
        }
    }
    Say "done"
}
catch {
    Say ("error=" + $_.Exception.Message)
    $exitCode = 2
}
finally {
    # our documents are closed already, so there is nothing to save
    try { $word.Quit() } catch {}
    try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($word) } catch {}
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
exit $exitCode
