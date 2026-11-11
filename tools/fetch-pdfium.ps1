# downloads the prebuilt pdfium library (apache-2.0) into vendor/pdfium.
# run once after cloning, from the repo root:  .\tools\fetch-pdfium.ps1
#
# the version is pinned so everyone builds against the same engine,
# and it matches the pdfium api version the rust bindings were written for.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$version = '8125'
$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$version/pdfium-win-x64.tgz"
$repoRoot = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repoRoot 'vendor\pdfium'
$unpacked = Join-Path $env:TEMP "letters-pdfium-$version"
$archive = "$unpacked.tgz"

Invoke-WebRequest $url -OutFile $archive
New-Item -ItemType Directory -Force $unpacked, $target | Out-Null
# use the tar that ships with windows: other tars on the path (git's, for
# example) read "C:" as a remote host name and fail
& "$env:SystemRoot\System32\tar.exe" -xzf $archive -C $unpacked

# only the library and its license are needed at run time
Copy-Item (Join-Path $unpacked 'bin\pdfium.dll') $target -Force
Copy-Item (Join-Path $unpacked 'LICENSE') (Join-Path $target 'LICENSE.pdfium.txt') -Force
Set-Content (Join-Path $target 'VERSION') "chromium/$version"

Remove-Item $archive, $unpacked -Recurse -Force
Write-Host "pdfium chromium/$version is ready in $target"
