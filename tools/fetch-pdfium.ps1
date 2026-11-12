# downloads the prebuilt pdfium library (apache-2.0) into vendor/pdfium.
# run once after cloning, from the repo root:  .\tools\fetch-pdfium.ps1
#
# the version is pinned so everyone builds against the same engine,
# and it matches the pdfium api version the rust bindings were written for.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$version = '8125'

# the sha-256 of each archive, recorded when the version was pinned. a
# download that does not match is thrown away, so a tampered or damaged
# file can never end up inside the app.
$packages = @{
    'x64'   = @{ name = 'pdfium-win-x64';   sha256 = '9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b' }
    'arm64' = @{ name = 'pdfium-win-arm64'; sha256 = 'd394ba61991fb53f4d87f70988035f337d9e73f2bd2c3c1b966df8c43fcabb05' }
}

# pick the library for this computer's processor: snapdragon laptops are arm64.
# set LETTERS_PDFIUM_ARCH to x64 or arm64 to choose by hand (for cross builds).
$arch = $env:LETTERS_PDFIUM_ARCH
if (-not $arch) {
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
}
if (-not $packages.ContainsKey($arch)) {
    throw "there is no pdfium package for '$arch'. use x64 or arm64."
}
$package = $packages[$arch]

$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$version/$($package.name).tgz"
$repoRoot = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repoRoot 'vendor\pdfium'
$unpacked = Join-Path $env:TEMP "letters-pdfium-$version-$arch"
$archive = "$unpacked.tgz"

Invoke-WebRequest $url -OutFile $archive

$actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $package.sha256) {
    Remove-Item $archive -Force
    throw "the pdfium download does not match its recorded checksum (got $actual). nothing was installed. try again, and if it keeps failing do not use the file."
}

New-Item -ItemType Directory -Force $unpacked, $target | Out-Null
# use the tar that ships with windows: other tars on the path (git's, for
# example) read "C:" as a remote host name and fail
& "$env:SystemRoot\System32\tar.exe" -xzf $archive -C $unpacked

# only the library and its license are needed at run time
Copy-Item (Join-Path $unpacked 'bin\pdfium.dll') $target -Force
Copy-Item (Join-Path $unpacked 'LICENSE') (Join-Path $target 'LICENSE.pdfium.txt') -Force
Set-Content (Join-Path $target 'VERSION') "chromium/$version $arch"

Remove-Item $archive, $unpacked -Recurse -Force
Write-Host "pdfium chromium/$version ($arch) is ready in $target"
