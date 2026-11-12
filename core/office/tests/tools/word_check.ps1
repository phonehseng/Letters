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

# the private word session is shared with the product's word bridge
. "$PSScriptRoot\..\..\src\bridge\word_session.ps1"

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
            shapeList  = @(foreach ($shape in $doc.Shapes) {
                    $text = ""
                    try { if ($shape.TextFrame.HasText) { $text = $shape.TextFrame.TextRange.Text.TrimEnd([char]13) } } catch { }
                    [ordered]@{ name = $shape.Name; type = $shape.Type; left = [math]::Round($shape.Left * 20); top = [math]::Round($shape.Top * 20); width = [math]::Round($shape.Width * 20); height = [math]::Round($shape.Height * 20); rotation = $shape.Rotation; relativeHorizontal = $shape.RelativeHorizontalPosition; relativeVertical = $shape.RelativeVerticalPosition; wrap = $shape.WrapFormat.Type; text = $text }
                })
            inlinePictures = @(foreach ($picture in $doc.InlineShapes) { [ordered]@{ width = [math]::Round($picture.Width * 20); height = [math]::Round($picture.Height * 20) } })
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
