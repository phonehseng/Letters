# converts decks with a headless libreoffice impress that has its own
# private profile, and waits for it every time: pdf references for
# libreoffice, and libreoffice's own odp and pptx saves for the
# back-and-forth baseline.
#
# usage: powershell -File libreoffice_convert.ps1 -Inputs <folder> -Pattern *.pptx -To pdf -Out <folder>

param(
    [Parameter(Mandatory = $true)][string]$Inputs,
    [Parameter(Mandatory = $true)][string]$Pattern,
    [Parameter(Mandatory = $true)][string]$To,
    [Parameter(Mandatory = $true)][string]$Out
)

$ErrorActionPreference = "Stop"
# LETTERS_SOFFICE if set, else libreoffice unpacked into this user's programs
$soffice = if ($env:LETTERS_SOFFICE) { $env:LETTERS_SOFFICE } else { Join-Path $env:LOCALAPPDATA 'Programs\LibreOffice\program\soffice.com' }
$profile = Join-Path (Split-Path $Out -Parent) "lo_profile"
$profileUrl = "file:///" + ($profile -replace "\\", "/")
New-Item -ItemType Directory -Force $Out | Out-Null
$files = @(Get-ChildItem $Inputs -Filter $Pattern | ForEach-Object { $_.FullName })
if ($files.Count -eq 0) { throw "no files match $Pattern in $Inputs" }
# soffice.com stays in the foreground until the conversion is done;
# Start-Process -Wait makes that explicit and gives the exit code
$arguments = @("-env:UserInstallation=$profileUrl", "--headless", "--norestore", "--convert-to", "`"$To`"", "--outdir", "`"$Out`"") + ($files | ForEach-Object { "`"$_`"" })
$process = Start-Process -FilePath $soffice -ArgumentList $arguments -Wait -PassThru -NoNewWindow
Write-Output "libreoffice exited with $($process.ExitCode) after converting $($files.Count) files"
