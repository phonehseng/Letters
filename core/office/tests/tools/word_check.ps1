<#
checks docx files against microsoft word through com, for the ignored
integration tests in tests/word_com.rs.

  -Mode verify -Path in.docx -Out report.json
      opens the file read-only and reports what word sees: whether it had
      to repair the file, paragraphs (text, style, outline level), tables,
      footnotes, comments, revisions, pictures.
  -Mode create -Path out.docx -Image picture.png
      makes a document in word (headings, a list, a table, a footnote, a
      picture, a comment and a tracked change) and saves it as docx.

it always starts its own hidden word and only ever closes that one: word
windows the person already has open are never touched. a watchdog ends
the private word if it hangs on a hidden dialog.
#>
param(
    [Parameter(Mandatory = $true)][ValidateSet("verify", "create")][string]$Mode,
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$Out,
    [string]$Image
)

$ErrorActionPreference = "Stop"

# word started for automation runs as "WINWORD.EXE /Automation -Embedding".
# a word the person opened never has that switch, so these are the only
# processes this script may ever stop
function Get-AutomationWords {
    @(Get-CimInstance Win32_Process -Filter "Name='WINWORD.EXE'" |
        Where-Object { $_.CommandLine -like "*/Automation*" } |
        ForEach-Object { $_.ProcessId })
}

function Start-PrivateWord {
    $before = Get-AutomationWords
    $word = New-Object -ComObject Word.Application
    # the new process can take a moment to show up
    $new = @()
    for ($try = 0; $try -lt 20 -and $new.Count -eq 0; $try++) {
        Start-Sleep -Milliseconds 250
        $new = @(Get-AutomationWords | Where-Object { $before -notcontains $_ })
    }
    if ($new.Count -ne 1) {
        # com handed us a word that was already running, or the count is
        # unclear: touch nothing that is not certainly ours
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
        throw "could not start a private Word instance (found $($new.Count) new automation processes)"
    }
    $id = $new[0]
    # if this script dies on the way, the watchdog still ends our word
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
    # wait for it to go, so the next run cannot be handed a closing word
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

function Get-Report($word, $file) {
    $full = (Resolve-Path $file).Path
    # FileName, ConfirmConversions, ReadOnly, AddToRecentFiles. with alerts
    # off, a file word would offer to repair fails to open instead
    try {
        $doc = $word.Documents.Open($full, $false, $true, $false)
    }
    catch {
        return [ordered]@{ repaired = $true; openError = $_.Exception.Message }
    }
    try {
        # a document word had to repair opens as a new, unsaved copy, so
        # its full name is no longer the file's path
        $repaired = ($doc.FullName -ne $full)
        $paragraphs = @()
        foreach ($paragraph in $doc.Paragraphs) {
            $text = $paragraph.Range.Text
            $paragraphs += [ordered]@{
                text    = $text.TrimEnd([char]13, [char]7)
                style   = $paragraph.Style.NameLocal
                outline = $paragraph.OutlineLevel
                inTable = [bool]$paragraph.Range.Information(12)
            }
        }
        $tables = @()
        foreach ($table in $doc.Tables) {
            $tables += [ordered]@{ rows = $table.Rows.Count; columns = $table.Columns.Count; uniform = $table.Uniform }
        }
        return [ordered]@{
            repaired   = $repaired
            fullName   = $doc.FullName
            paragraphs = $paragraphs
            tables     = $tables
            footnotes  = $doc.Footnotes.Count
            endnotes   = $doc.Endnotes.Count
            comments   = $doc.Comments.Count
            revisions  = $doc.Revisions.Count
            pictures   = $doc.InlineShapes.Count
            shapes     = $doc.Shapes.Count
            sections   = $doc.Sections.Count
            headerText = $doc.Sections(1).Headers(1).Range.Text.TrimEnd([char]13)
            footerText = $doc.Sections($doc.Sections.Count).Footers(1).Range.Text.TrimEnd([char]13)
            pages      = $doc.ComputeStatistics(2)
        }
    }
    finally {
        $doc.Close(0)
    }
}

function New-WordDocument($word, $file, $picture) {
    $doc = $word.Documents.Add()
    try {
        $range = $doc.Content
        $range.Text = "Essay Title"
        $range.Style = $doc.Styles.Item(-2)        # heading 1
        $range.InsertParagraphAfter()
        $range = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $range.Text = "Background"
        $range.Style = $doc.Styles.Item(-3)        # heading 2
        $range.InsertParagraphAfter()
        $range = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $range.Text = "Body text with a note."
        $range.Style = $doc.Styles.Item(-1)        # normal
        $range.InsertParagraphAfter()
        $noteAt = $doc.Paragraphs(3).Range
        $noteAt.MoveEnd(1, -2) | Out-Null          # before the full stop
        $noteAt.Collapse(0)
        $doc.Footnotes.Add($noteAt, [Type]::Missing, "The footnote text.") | Out-Null

        $range = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $range.Text = "First item"
        $range.InsertParagraphAfter()
        $range = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $range.Text = "Second item"
        $list = $doc.Range($doc.Paragraphs($doc.Paragraphs.Count - 1).Range.Start, $doc.Paragraphs($doc.Paragraphs.Count).Range.End)
        $list.ListFormat.ApplyNumberDefault()
        $range.InsertParagraphAfter()

        $range = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $range.ListFormat.RemoveNumbers()
        $table = $doc.Tables.Add($range, 2, 2)
        $table.Borders.Enable = $true
        $table.Cell(1, 1).Range.Text = "A1"
        $table.Cell(1, 2).Range.Text = "B1"
        $table.Cell(2, 1).Range.Text = "A2"
        $table.Cell(2, 2).Range.Text = "B2"

        $end = $doc.Content
        $end.Collapse(0)
        $end.InsertParagraphAfter()
        $end = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $end.Text = "Picture follows."
        $end.InsertParagraphAfter()
        $at = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $doc.InlineShapes.AddPicture((Resolve-Path $picture).Path, $false, $true, $at) | Out-Null

        $end = $doc.Content
        $end.Collapse(0)
        $end.InsertParagraphAfter()
        $end = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $end.Text = "Reviewed sentence."
        $doc.Comments.Add($end, "Please expand.") | Out-Null
        $doc.TrackRevisions = $true
        $tracked = $doc.Paragraphs($doc.Paragraphs.Count).Range
        $tracked.MoveEnd(1, -1) | Out-Null
        $tracked.Collapse(0)
        $tracked.InsertAfter(" Added later")
        $doc.TrackRevisions = $false

        # document properties only answer late-bound calls from powershell
        $properties = $doc.BuiltInDocumentProperties
        $title = [System.__ComObject].InvokeMember("Item", "GetProperty", $null, $properties, @("Title"))
        [void][System.__ComObject].InvokeMember("Value", "SetProperty", $null, $title, @("Made in Word"))
        # 16 is the default docx format
        $doc.SaveAs2([IO.Path]::GetFullPath($file), 16)
    }
    finally {
        $doc.Close(0)
    }
}

$session = $null
$failed = $false
try {
    $session = Start-PrivateWord
    if ($Mode -eq "verify") {
        $report = Get-Report $session.Word $Path
        $report | ConvertTo-Json -Depth 6 | Out-File -FilePath $Out -Encoding utf8
    }
    else {
        New-WordDocument $session.Word $Path $Image
    }
}
catch {
    # say which line failed, which com errors otherwise hide
    [Console]::Error.WriteLine("word_check.ps1 line $($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)")
    $failed = $true
}
finally {
    if ($session) { Stop-PrivateWord $session }
}
if ($failed) { exit 1 }
