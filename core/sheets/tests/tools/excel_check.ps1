# a private, hidden excel answering questions about a workbook (see below)
<#
asks a private, hidden microsoft excel about a workbook, for the
verification harness and the round-trip checks.

  -Mode values  -Path book.xlsx -Sheet Cases -Column 3 -Out values.tsv
      recalculates everything and writes, for every used row, the row,
      the value's kind (n, s, b, e, blank), the value and the shown text
  -Mode check   -Path book.xlsx -Out counts.tsv
      opens the file and writes what excel sees in it (sheets, merged
      ranges, validations, conditional formats, shapes, charts, notes,
      names) and whether excel had to repair it
  -Mode resave  -Path book.xlsx -Out copy.xlsx
      saves the workbook again as excel itself writes it
  -Mode decorate -Path book.xlsx -Out copy.xlsx
      adds a chart and a shape, then saves as excel writes it

the one rule: only ever stop the excel this script started. it is found
by the process that owns its main window (Application.Hwnd); any other
excel, and every word, is never touched. the file is opened read-only
with macros disabled and alerts off, and a watchdog ends our excel if
the script hangs.
#>
param(
    [Parameter(Mandatory = $true)][string]$Mode,
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$Out,
    [string]$Sheet = "",
    [int]$Column = 1
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class LettersExcelWindows {
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
"@ -ErrorAction SilentlyContinue

# excel's error values come through com as these numbers (CVErr codes)
$errorNames = @{
    2000 = "#NULL!"; 2007 = "#DIV/0!"; 2015 = "#VALUE!"; 2023 = "#REF!"; 2029 = "#NAME?";
    2036 = "#NUM!"; 2042 = "#N/A"; 2043 = "#GETTING_DATA"; 2045 = "#SPILL!"; 2050 = "#CALC!"
}

function Start-PrivateExcel {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$found = 0
    [void][LettersExcelWindows]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$found)
    if ($found -eq 0) {
        try { $excel.Quit() } catch { }
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
        throw "could not identify the private Excel process"
    }
    $id = [int]$found
    $watchdog = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList @(
        "-NoProfile", "-Command", "Start-Sleep -Seconds 600; Stop-Process -Id $id -Force -ErrorAction SilentlyContinue")
    $session = @{ Excel = $excel; Pid = $id; Watchdog = $watchdog }
    try {
        Set-Up $excel
    }
    catch {
        Stop-PrivateExcel $session
        throw
    }
    return $session
}

# a freshly started excel can answer "busy" (0x800AC472) for a moment,
# so each setting is retried for a few seconds
function Set-Up($excel) {
    for ($try = 0; $try -lt 40; $try++) {
        try {
            # an excel started for automation is already invisible; setting
            # Visible again is refused while it starts, so it is left alone
            $excel.DisplayAlerts = $false
            # 3 is msoAutomationSecurityForceDisable: macros never run
            $excel.AutomationSecurity = 3
            return
        }
        catch {
            Start-Sleep -Milliseconds 250
        }
    }
    throw "the private Excel did not become ready"
}

function Stop-PrivateExcel($session) {
    try { $session.Excel.Quit() } catch { }
    try { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($session.Excel) } catch { }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
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

function Describe-Value($value) {
    if ($null -eq $value) { return @("blank", "") }
    if ($value -is [bool]) { return @("b", $(if ($value) { "TRUE" } else { "FALSE" })) }
    if ($value -is [int]) {
        $code = $value + 2146828288
        if ($errorNames.ContainsKey($code)) { return @("e", $errorNames[$code]) }
    }
    if ($value -is [double]) { return @("n", $value.ToString("R", [Globalization.CultureInfo]::InvariantCulture)) }
    return @("s", [string]$value)
}

function Clean($text) {
    return ([string]$text).Replace("`t", " ").Replace("`r", " ").Replace("`n", " ")
}

function Write-Values($workbook) {
    $sheet = $workbook.Worksheets.Item($Sheet)
    $last = $sheet.UsedRange.Row + $sheet.UsedRange.Rows.Count - 1
    $lines = New-Object System.Collections.Generic.List[string]
    $cells = $sheet.Range($sheet.Cells.Item(1, $Column), $sheet.Cells.Item($last, $Column))
    # one call for every value: per-cell com calls would take minutes
    $values = $cells.Value2
    for ($row = 1; $row -le $last; $row++) {
        $value = if ($last -eq 1) { $values } else { $values[$row, 1] }
        $kind, $text = Describe-Value $value
        $shown = $sheet.Cells.Item($row, $Column).Text
        $lines.Add("$row`t$kind`t$(Clean $text)`t$(Clean $shown)")
    }
    [IO.File]::WriteAllLines($Out, $lines, (New-Object Text.UTF8Encoding $false))
}

function Write-Counts($workbook) {
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("name`t$($workbook.Name)")
    $lines.Add("sheets`t$($workbook.Worksheets.Count)")
    $lines.Add("names`t$($workbook.Names.Count)")
    foreach ($sheet in $workbook.Worksheets) {
        $merged = 0
        foreach ($cell in $sheet.UsedRange.Cells) {
            $area = $cell.MergeArea
            if ($cell.MergeCells -and $area.Row -eq $cell.Row -and $area.Column -eq $cell.Column) { $merged++ }
        }
        $validations = 0
        try { $validations = $sheet.Cells.SpecialCells(-4174).Count } catch { }
        $lines.Add("sheet`t$($sheet.Name)`tmerged=$merged`tvalidations=$validations`tformats=$($sheet.Cells.FormatConditions.Count)`tshapes=$($sheet.Shapes.Count)`tcharts=$($sheet.ChartObjects().Count)`tnotes=$($sheet.Comments.Count)`ttables=$($sheet.ListObjects.Count)`thyperlinks=$($sheet.Hyperlinks.Count)`tused=$($sheet.UsedRange.Address(0, 0))")
    }
    [IO.File]::WriteAllLines($Out, $lines, (New-Object Text.UTF8Encoding $false))
}

$session = Start-PrivateExcel
try {
    $full = (Resolve-Path $Path).Path
    # UpdateLinks 0, ReadOnly true; CorruptLoad stays normal so a broken
    # file fails to open instead of being silently repaired
    $workbook = $session.Excel.Workbooks.Open($full, 0, $true)
    switch ($Mode) {
        "values" {
            $session.Excel.CalculateFull()
            Write-Values $workbook
        }
        "check" { Write-Counts $workbook }
        "resave" {
            # 51 is xlOpenXMLWorkbook (.xlsx)
            $workbook.SaveAs($Out, 51)
        }
        "decorate" {
            # a chart of A1:B4 and a rectangle on the first sheet, then saved
            # as excel writes it, so the file has excel's own drawing parts
            # not $sheet: powershell names ignore case, and the [string]
            # parameter $Sheet would turn the worksheet into text
            $firstSheet = $workbook.Worksheets.Item(1)
            $chart = $firstSheet.ChartObjects().Add(300, 20, 300, 200)
            $chart.Chart.SetSourceData($firstSheet.Range("A1:B4"))
            # 1 is msoShapeRectangle
            [void]$firstSheet.Shapes.AddShape(1, 320, 240, 80, 40)
            $workbook.SaveAs($Out, 51)
        }
        default { throw "unknown mode $Mode" }
    }
    $workbook.Close($false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($workbook)
}
finally {
    Stop-PrivateExcel $session
}
