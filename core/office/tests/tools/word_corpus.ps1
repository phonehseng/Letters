# makes the freeform corpus (shapes, groups, tables, linked boxes, columns, a flyer) in word.
<#
the files, for the ignored round-trip suites in tests/freeform_apps.rs:

  -OutDir folder -Picture picture.png

writes shapes.docx, groups.docx, floating_table.docx, linked_boxes.docx,
columns.docx and flyer.docx, each built through word's own object model
and saved by word, so the files are exactly what word itself writes.

it always starts its own hidden word and only ever closes that one (see
src/bridge/word_session.ps1); word windows the person has open are never
touched.
#>
param(
    [Parameter(Mandatory = $true)][string]$OutDir,
    [Parameter(Mandatory = $true)][string]$Picture
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\..\..\src\bridge\word_session.ps1"

# word colours are blue-green-red packed into one number
function Rgb([int]$r, [int]$g, [int]$b) { return $r + 256 * $g + 65536 * $b }

function Save-Document($doc, [string]$name) {
    # 16 is word's default docx format
    $doc.SaveAs2([IO.Path]::Combine($OutDir, $name), 16)
    $doc.Close(0)
}

# a document with some paragraphs of running text to anchor things to
function New-TextDocument($word, [string[]]$paragraphs) {
    $doc = $word.Documents.Add()
    $doc.Content.Text = ($paragraphs -join "`r")
    return $doc
}

function Set-PagePosition($shape, [double]$left, [double]$top) {
    $shape.RelativeHorizontalPosition = 1   # page
    $shape.RelativeVerticalPosition = 1     # page
    $shape.Left = $left
    $shape.Top = $top
}

$filler = "Shapes, groups and boxes sit on this page while the text flows around them as Word lays it out."

function New-Shapes($word) {
    $doc = New-TextDocument $word @(("Running text. " + $filler), $filler, $filler, $filler)
    $anchor = $doc.Paragraphs(1).Range
    $rect = $doc.Shapes.AddShape(1, 72, 160, 144, 72, $anchor)
    $rect.Name = "Solid Rectangle"
    $rect.Fill.ForeColor.RGB = Rgb 51 102 204
    $rect.Line.ForeColor.RGB = Rgb 20 40 80
    $rect.Line.Weight = 2
    Set-PagePosition $rect 72 160

    $round = $doc.Shapes.AddShape(5, 250, 160, 150, 80, $anchor)
    $round.Name = "Gradient Rounded"
    $round.Fill.TwoColorGradient(1, 1)
    $round.Fill.ForeColor.RGB = Rgb 255 200 0
    $round.Fill.BackColor.RGB = Rgb 200 40 40
    Set-PagePosition $round 250 160

    $oval = $doc.Shapes.AddShape(9, 430, 160, 100, 100, $anchor)
    $oval.Name = "Dashed Oval"
    $oval.Fill.Visible = 0
    $oval.Line.DashStyle = 4
    $oval.Line.Weight = 3
    $oval.Line.ForeColor.RGB = Rgb 0 128 0
    Set-PagePosition $oval 430 160

    $triangle = $doc.Shapes.AddShape(7, 72, 300, 90, 90, $anchor)
    $triangle.Name = "Rotated Triangle"
    $triangle.Rotation = 30
    $triangle.Fill.ForeColor.RGB = Rgb 120 60 160
    $triangle.Fill.Transparency = 0.5
    Set-PagePosition $triangle 72 300

    $arrow = $doc.Shapes.AddShape(33, 200, 310, 140, 60, $anchor)
    $arrow.Name = "Flipped Arrow"
    $arrow.Flip(0)
    $arrow.Fill.ForeColor.RGB = Rgb 230 120 30
    $arrow.Shadow.Visible = -1
    Set-PagePosition $arrow 200 310

    $callout = $doc.Shapes.AddShape(105, 380, 300, 150, 70, $anchor)
    $callout.Name = "Note Callout"
    $callout.TextFrame.TextRange.Text = "A note in a callout"
    $callout.Fill.ForeColor.RGB = Rgb 255 255 204
    $callout.Line.ForeColor.RGB = Rgb 0 0 0
    $callout.TextFrame.TextRange.Font.Color = Rgb 0 0 0
    Set-PagePosition $callout 380 300

    $line = $doc.Shapes.AddLine(72, 450, 300, 520, $anchor)
    $line.Name = "Arrow Line"
    $line.Line.EndArrowheadStyle = 2
    $line.Line.BeginArrowheadStyle = 6
    $line.Line.Weight = 1.5
    $line.Line.ForeColor.RGB = Rgb 200 0 0

    $builder = $doc.Shapes.BuildFreeform(1, 340, 460)
    $builder.AddNodes(0, 1, 420, 440)
    $builder.AddNodes(1, 1, 460, 470, 480, 530, 420, 560)
    $builder.AddNodes(0, 1, 340, 460)
    $free = $builder.ConvertToShape($anchor)
    $free.Name = "Freeform Path"
    $free.Fill.ForeColor.RGB = Rgb 90 200 160

    $filled = $doc.Shapes.AddShape(1, 72, 600, 120, 90, $anchor)
    $filled.Name = "Picture Fill"
    $filled.Fill.UserPicture((Resolve-Path $Picture).Path)
    Set-PagePosition $filled 72 600

    $star = $doc.Shapes.AddShape(92, 250, 600, 90, 90, $anchor)
    $star.Name = "Star Behind"
    $star.WrapFormat.Type = 5
    $star.Fill.ForeColor.RGB = Rgb 250 220 60
    Set-PagePosition $star 250 600
    Save-Document $doc "shapes.docx"
}

function Get-GroupPackage {
    $shape = {
        param($id, $name, $x, $y, $cx, $cy, $geometry, $fill, $text)
        $box = ""
        $flag = ""
        if ($text) {
            $flag = ' txBox="1"'
            $box = "<wps:txbx><w:txbxContent><w:p><w:r><w:t>$text</w:t></w:r></w:p></w:txbxContent></wps:txbx>"
        }
        "<wps:wsp><wps:cNvPr id=`"$id`" name=`"$name`"/><wps:cNvSpPr$flag/><wps:spPr><a:xfrm><a:off x=`"$x`" y=`"$y`"/><a:ext cx=`"$cx`" cy=`"$cy`"/></a:xfrm><a:prstGeom prst=`"$geometry`"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=`"$fill`"/></a:solidFill><a:ln w=`"12700`"><a:solidFill><a:srgbClr val=`"1F3864`"/></a:solidFill></a:ln></wps:spPr>$box<wps:bodyPr rot=`"0`" vert=`"horz`" wrap=`"square`" lIns=`"91440`" tIns=`"45720`" rIns=`"91440`" bIns=`"45720`" anchor=`"t`" anchorCtr=`"0`"><a:noAutofit/></wps:bodyPr></wps:wsp>"
    }
    $rectangle = & $shape 2 "Grouped Rectangle" 0 0 1524000 762000 "rect" "3C78C8" $null
    $oval = & $shape 3 "Grouped Oval" 1778000 127000 1016000 1016000 "ellipse" "DC5050" $null
    $text = & $shape 4 "Grouped Text" 0 1270000 2794000 635000 "rect" "FFFFFF" "Text inside a group"
    $xfrm = { param($cx, $cy) "<a:xfrm><a:off x=`"0`" y=`"0`"/><a:ext cx=`"$cx`" cy=`"$cy`"/><a:chOff x=`"0`" y=`"0`"/><a:chExt cx=`"$cx`" cy=`"$cy`"/></a:xfrm>" }
    $inner = "<wpg:grpSp><wpg:cNvPr id=`"5`" name=`"Inner Group`"/><wpg:cNvGrpSpPr/><wpg:grpSpPr>" + (& $xfrm 2794000 1143000) + "</wpg:grpSpPr>$rectangle$oval</wpg:grpSp>"
    $group = "<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr>" + (& $xfrm 2794000 1905000) + "</wpg:grpSpPr>$inner$text</wpg:wgp>"
    $anchor = '<wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="251659264" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>1270000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>2540000</wp:posOffset></wp:positionV><wp:extent cx="2794000" cy="1905000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Outer Group"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">' + $group + '</a:graphicData></a:graphic></wp:anchor>'
    $namespaces = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"'
    $body = "<w:document $namespaces><w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires=`"wpg`"><w:drawing>$anchor</w:drawing></mc:Choice></mc:AlternateContent></w:r><w:r><w:t>The group is anchored here.</w:t></w:r></w:p></w:body></w:document>"
    return '<?xml version="1.0" standalone="yes"?><?mso-application progid="Word.Document"?><pkg:package xmlns:pkg="http://schemas.microsoft.com/office/2006/xmlPackage">' +
    '<pkg:part pkg:name="/_rels/.rels" pkg:contentType="application/vnd.openxmlformats-package.relationships+xml"><pkg:xmlData><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships></pkg:xmlData></pkg:part>' +
    '<pkg:part pkg:name="/word/document.xml" pkg:contentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"><pkg:xmlData>' + $body + '</pkg:xmlData></pkg:part></pkg:package>'
}

function New-Groups($word) {
    $doc = New-TextDocument $word @(("Groups. " + $filler), $filler, $filler)
    # an invisible word refuses Shapes.Range().Group() ("grouping is
    # disabled"), so the group is handed to word as wordprocessingml and
    # word itself turns it into its own objects and writes the file
    $doc.Paragraphs(2).Range.InsertXML((Get-GroupPackage))
    Save-Document $doc "groups.docx"
}

function New-FloatingTable($word) {
    $doc = New-TextDocument $word @(("A floating table. " + $filler), $filler, "", $filler, $filler)
    $range = $doc.Paragraphs(3).Range
    $table = $doc.Tables.Add($range, 3, 3)
    $table.Borders.Enable = $true
    $table.AllowAutoFit = $false
    $table.Columns(1).Width = 90
    $table.Columns(2).Width = 120
    $table.Columns(3).Width = 80
    $table.Rows.HeightRule = 2
    $table.Rows.Height = 30
    $table.TopPadding = 3
    $table.LeftPadding = 6
    $table.RightPadding = 6
    $table.Rows.WrapAroundText = -1
    $table.Rows.RelativeHorizontalPosition = 1
    $table.Rows.HorizontalPosition = 260
    $table.Rows.RelativeVerticalPosition = 1
    $table.Rows.VerticalPosition = 300
    $table.Rows.DistanceLeft = 9
    $table.Rows.DistanceRight = 9
    $table.Cell(1, 1).Range.Text = "Exact rows"
    $table.Cell(1, 2).Range.Text = "Double top"
    $table.Cell(1, 2).Borders.Item(-1).LineStyle = 7
    $table.Cell(1, 3).Range.Text = "Merged down"
    $table.Cell(2, 1).Range.Text = "Upward"
    $table.Cell(2, 1).Range.Orientation = 2
    $table.Cell(2, 2).Range.Text = "Middle"
    $table.Cell(2, 2).VerticalAlignment = 1
    $table.Cell(2, 2).Shading.BackgroundPatternColor = Rgb 220 235 255
    $table.Cell(2, 2).Borders.Item(-3).LineStyle = 2
    $table.Cell(3, 1).Range.Text = "Padded"
    $table.Cell(3, 1).TopPadding = 8
    $table.Cell(3, 2).Range.Text = "Spans two"
    $nested = $table.Cell(2, 2).Tables.Add($table.Cell(2, 2).Range, 1, 2)
    $nested.Borders.Enable = $true
    $nested.Cell(1, 1).Range.Text = "in"
    $nested.Cell(1, 2).Range.Text = "side"
    $table.Cell(1, 3).Merge($table.Cell(2, 3))
    $table.Cell(3, 2).Merge($table.Cell(3, 3))
    Save-Document $doc "floating_table.docx"
}

function New-LinkedBoxes($word) {
    $doc = New-TextDocument $word @("Linked text boxes carry one story across three boxes.", $filler)
    $anchor = $doc.Paragraphs(1).Range
    $boxes = @()
    foreach ($index in 0..2) {
        $box = $doc.Shapes.AddTextbox(1, 72 + 165 * $index, 150, 150, 160, $anchor)
        $box.Name = "Story Box " + ($index + 1)
        Set-PagePosition $box (72 + 165 * $index) 150
        $boxes += $box
    }
    $boxes[0].TextFrame.Next = $boxes[1].TextFrame
    $boxes[1].TextFrame.Next = $boxes[2].TextFrame
    $story = (1..9 | ForEach-Object { "Paragraph $_ of the story flows from box to box without breaking the text apart." }) -join "`r"
    $boxes[0].TextFrame.TextRange.Text = $story
    Save-Document $doc "linked_boxes.docx"
}

function New-Columns($word) {
    $paragraphs = @("One column at the top of the page. " + $filler)
    foreach ($index in 1..6) { $paragraphs += ("Column text $index. " + $filler + " " + $filler) }
    $paragraphs += "Back to one column. " + $filler
    $doc = New-TextDocument $word $paragraphs
    # continuous breaks before the column part and before the last paragraph
    $start = $doc.Paragraphs(2).Range
    $start.Collapse(1)
    $start.InsertBreak(3)
    $last = $doc.Paragraphs($doc.Paragraphs.Count).Range
    $last.Collapse(1)
    $last.InsertBreak(3)
    $columns = $doc.Sections(2).PageSetup.TextColumns
    $columns.SetCount(3)
    $columns.EvenlySpaced = 0
    $columns.LineBetween = -1
    $columns.Item(1).Width = 100
    $columns.Item(1).SpaceAfter = 18
    $columns.Item(2).Width = 180
    $columns.Item(2).SpaceAfter = 12
    $columns.Item(3).Width = 130
    $break = $doc.Sections(2).Range.Paragraphs(4).Range
    $break.Collapse(1)
    $break.InsertBreak(8)
    Save-Document $doc "columns.docx"
}

function New-Flyer($word) {
    $doc = $word.Documents.Add()
    $anchor = $doc.Paragraphs(1).Range
    $banner = $doc.Shapes.AddShape(1, 0, 0, 612, 160, $anchor)
    $banner.Name = "Banner"
    $banner.Fill.ForeColor.RGB = Rgb 30 60 110
    $banner.Line.Visible = 0
    $banner.WrapFormat.Type = 5
    Set-PagePosition $banner 0 0
    $title = $doc.Shapes.AddTextbox(1, 54, 40, 504, 80, $anchor)
    $title.Name = "Title"
    $title.Fill.Visible = 0
    $title.Line.Visible = 0
    $title.TextFrame.TextRange.Text = "Spring Science Fair"
    $title.TextFrame.TextRange.Font.Size = 40
    $title.TextFrame.TextRange.Font.Color = Rgb 255 255 255
    $title.TextFrame.TextRange.ParagraphFormat.Alignment = 1
    Set-PagePosition $title 54 40
    $photo = $doc.Shapes.AddPicture((Resolve-Path $Picture).Path, $false, $true, 54, 200, 220, 165, $anchor)
    $photo.Name = "Flyer Picture"
    Set-PagePosition $photo 54 200
    $body = $doc.Shapes.AddTextbox(1, 300, 200, 258, 220, $anchor)
    $body.Name = "Details"
    $body.TextFrame.TextRange.Text = "Friday, 3 pm`rMain hall`rBring a project, a poster and a friend. Prizes for every year group."
    $body.TextFrame.TextRange.Font.Size = 14
    Set-PagePosition $body 300 200
    $star = $doc.Shapes.AddShape(92, 440, 600, 110, 110, $anchor)
    $star.Name = "Badge"
    $star.Fill.ForeColor.RGB = Rgb 250 200 40
    $star.TextFrame.TextRange.Text = "Free"
    Set-PagePosition $star 440 600
    $footer = $doc.Shapes.AddShape(5, 54, 720, 360, 40, $anchor)
    $footer.Name = "Footer Strip"
    $footer.Fill.ForeColor.RGB = Rgb 220 230 240
    $footer.TextFrame.TextRange.Text = "letters.example/fair"
    $footer.TextFrame.TextRange.Font.Color = Rgb 30 60 110
    Set-PagePosition $footer 54 720
    Save-Document $doc "flyer.docx"
}

$session = $null
$failed = $false
try {
    $session = Start-PrivateWord
    New-Shapes $session.Word
    New-Groups $session.Word
    New-FloatingTable $session.Word
    New-LinkedBoxes $session.Word
    New-Columns $session.Word
    New-Flyer $session.Word
}
catch {
    [Console]::Error.WriteLine("word_corpus.ps1 line $($_.InvocationInfo.ScriptLineNumber): $($_.Exception.Message)")
    $failed = $true
}
finally {
    if ($session) { Stop-PrivateWord $session }
}
if ($failed) { exit 1 }
