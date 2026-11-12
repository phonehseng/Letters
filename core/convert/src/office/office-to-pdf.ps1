# turns one office document into a pdf with microsoft office, through its
# com automation. letters runs this with windows powershell and waits for
# it with a time limit.
#
# the rules it keeps:
# - its own, invisible copy of the office program, with alerts off. if
#   the program was already running and com handed us that copy instead
#   (powerpoint only ever runs once), nothing about it is changed, and it
#   is left running
# - the document is opened read only, without adding it to recent files,
#   and with a made-up password so a locked file fails at once instead
#   of asking for one
# - only our own copy is ever quit
#
# what it prints, one line each, for letters to read:
#   ours=<process id>   (or shared=<process id>) as soon as it is known,
#                       so letters can stop that copy if it hangs
#   done                when the pdf is written
#   error=<message>     when it couldn't be
param(
    [Parameter(Mandatory = $true)][ValidateSet('Word', 'Excel', 'PowerPoint')][string]$App,
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'

function Say([string]$text) {
    [Console]::Out.WriteLine($text)
    [Console]::Out.Flush()
}

Add-Type -Namespace Letters -Name Windows -MemberDefinition @'
[DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr window, out uint processId);
'@

function ProcessOfWindow([long]$handle) {
    $id = [uint32]0
    [void][Letters.Windows]::GetWindowThreadProcessId([System.IntPtr]$handle, [ref]$id)
    return [int]$id
}

$processName = @{ Word = 'WINWORD'; Excel = 'EXCEL'; PowerPoint = 'POWERPNT' }[$App]
$before = @(Get-Process -Name $processName -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$password = 'letters-no-password'
$missing = [Type]::Missing

try {
    $office = New-Object -ComObject "$App.Application"
}
catch {
    Say ("error=" + $_.Exception.Message)
    exit 3
}

# which process com gave us: the window of excel and powerpoint says it
# directly; for word, the one copy that wasn't running before
$processId = 0
try {
    if ($App -eq 'Word') {
        $new = @(Get-Process -Name $processName -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id })
        if ($new.Count -eq 1) { $processId = $new[0].Id }
    }
    else {
        $processId = ProcessOfWindow $office.HWND
    }
}
catch { $processId = 0 }
$ours = ($processId -ne 0) -and ($before -notcontains $processId)
if ($ours) { Say "ours=$processId" } else { Say "shared=$processId" }

$document = $null
$exitCode = 0
try {
    switch ($App) {
        'Word' {
            if ($ours) {
                $office.Visible = $false
                $office.DisplayAlerts = 0
            }
            # file, confirm conversions, read only, add to recent files,
            # password, template password, revert, write password,
            # template write password, format, encoding, visible
            $document = $office.Documents.Open($InputPath, $false, $true, $false, $password, $password, $false, $password, $password, 0, $missing, $false)
            if ($processId -eq 0) {
                # another copy of word started at the same moment, so the
                # document's window says which one is ours
                try {
                    $processId = ProcessOfWindow $document.ActiveWindow.Hwnd
                    $ours = ($processId -ne 0) -and ($before -notcontains $processId)
                    if ($ours) { Say "ours=$processId" }
                }
                catch {}
            }
            # 17 is pdf
            $document.ExportAsFixedFormat($OutputPath, 17)
            $document.Close(0)
            $document = $null
        }
        'Excel' {
            if ($ours) {
                $office.Visible = $false
                $office.DisplayAlerts = $false
                $office.AskToUpdateLinks = $false
                $office.EnableEvents = $false
            }
            # file, update links (no), read only, format, password
            $document = $office.Workbooks.Open($InputPath, 0, $true, $missing, $password)
            # 0 is pdf
            $document.ExportAsFixedFormat(0, $OutputPath)
            $document.Close($false)
            $document = $null
        }
        'PowerPoint' {
            if ($ours) { $office.DisplayAlerts = 1 }
            # read only, not untitled, no window
            $document = $office.Presentations.Open($InputPath, -1, 0, 0)
            # 32 is pdf
            $document.SaveAs($OutputPath, 32)
            $document.Close()
            $document = $null
        }
    }
    Say "done"
}
catch {
    Say ("error=" + $_.Exception.Message)
    $exitCode = 2
}
finally {
    if ($null -ne $document) {
        try { if ($App -eq 'Word') { $document.Close(0) } elseif ($App -eq 'Excel') { $document.Close($false) } else { $document.Close() } } catch {}
    }
    if ($ours) {
        # our document is closed already, so there is nothing to save.
        # (word's quit can't be given arguments from powershell: they are
        # passed by reference, and a plain 0 is refused.)
        try { $office.Quit() } catch {}
    }
    try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($office) } catch {}
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
exit $exitCode
