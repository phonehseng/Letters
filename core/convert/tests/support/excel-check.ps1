# opens a workbook in a private, invisible excel and prints what it holds,
# one "name=value" line each. alerts stay on, so a repair prompt would
# block the open, and the test's time limit turns that into a failure.
param([string]$Path, [string]$Cells)
$ErrorActionPreference = 'Stop'
$excel = New-Object -ComObject Excel.Application
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $true
    $book = $excel.Workbooks.Open($Path, 0, $true)
    "name=" + $book.Name
    "sheets=" + $book.Worksheets.Count
    $sheet = $book.Worksheets.Item(1)
    "sheet1=" + $sheet.Name
    foreach ($address in $Cells.Split(',')) {
        $range = $sheet.Range($address)
        "$address.value=" + $range.Value2
        "$address.text=" + $range.Text
        "$address.merged=" + $range.MergeCells
    }
    $book.Close($false)
}
finally {
    $excel.Quit()
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
}
