# downloads the tesseract ocr engine (apache-2.0) and its libraries into
# vendor/tesseract, with english, spanish, french and german language data.
# run once after cloning, from the repo root:  .\tools\fetch-tesseract.ps1
#
# why these files: the conda-forge project publishes tesseract for windows as
# plain archives (no installer, no administrator rights). every library it
# needs is permissively licensed except libiconv (lgpl-2.1, loaded as its own
# dll) and microsoft's visual c++ runtime. the msys2 build was ruled out
# because its libtiff links jbigkit, which is gpl-2.0. see core/ocr/README.md.
#
# every archive is pinned by name and checked against the sha256 that
# anaconda.org publishes for it. the language files come from the
# tessdata_fast 4.1.0 release (apache-2.0), which publishes no checksums, so
# the sha256 values below were recorded when the script was written.

$ErrorActionPreference = 'Stop'
# windows powershell 5.1 doesn't always offer tls 1.2, which github and
# conda require; without it the download fails as a closed connection
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

$channel = 'https://conda.anaconda.org/conda-forge'
$tessdataUrl = 'https://github.com/tesseract-ocr/tessdata_fast/raw/4.1.0'
$repoRoot = Split-Path -Parent $PSScriptRoot
$target = Join-Path $repoRoot 'vendor\tesseract'
$work = Join-Path $env:TEMP 'letters-tesseract-download'
# use the tar that ships with windows: it reads zip (.conda) and zstd, and
# other tars on the path (git's, for example) read "C:" as a host name
$tar = "$env:SystemRoot\System32\tar.exe"

# each package and the program files tesseract.exe actually loads from it.
# the list was found by following the dll imports of tesseract.exe; files
# a package carries that tesseract never loads are left out.
$packages = @(
    @{ file = 'win-64/tesseract-5.5.4-h8c7c6a5_0.conda'; sha256 = '67af4d943866603827aa4029ec0f2442644b3abe58c2313e89cfcc8be9e06897'; keep = @('tesseract.exe', 'tesseract55.dll') }
    @{ file = 'win-64/leptonica-1.87.0-h5b5c448_2.conda'; sha256 = 'fed6e72a8414f9af148e32685b3cc4be5a371fe99c8f180e0bbd30e454e2bc89'; keep = @('leptonica-1.87.0.dll') }
    @{ file = 'win-64/libarchive-3.8.11-lgpl_h933c176_1.conda'; sha256 = '5c3e469d94b562235f80536e9f18642d0473cb77742d27369be2049c067df62b'; keep = @('archive.dll') }
    @{ file = 'win-64/libcurl-8.23.0-hdb0ef4a_0.conda'; sha256 = 'd3088e1fd049fe0c1905ba6174e77279ddb4817739a204aaf4bd31d59d38ea6a'; keep = @('libcurl.dll') }
    @{ file = 'win-64/libssh2-1.11.1-h734d217_1.conda'; sha256 = '1097b08429b4f53f5751a32c6d99a083d227ca7f7812c0b239eea124cec073a0'; keep = @('libssh2.dll') }
    @{ file = 'win-64/libpsl-0.23.1-h9b16d47_1.conda'; sha256 = 'e53fe3b09f82b59b644264133d6d148ac31bb5451b7583cff55690bf038f2f62'; keep = @('psl-5.dll') }
    @{ file = 'win-64/openssl-3.6.5-hf411b9b_0.conda'; sha256 = 'bc44625e0c6d83f3e02b5f6adc01e0e3ca7001d3d94e24959591a57f5a011b9a'; keep = @('libcrypto-3-x64.dll') }
    @{ file = 'win-64/libxml2-16-2.15.4-h3cfd58e_1.conda'; sha256 = '819c4edd7b7bcfa9461aeccb56c0a7e9fa7c251163333e64222c142499822958'; keep = @('libxml2.dll') }
    @{ file = 'win-64/icu-78.3-h5112557_2.conda'; sha256 = '75c549b55b673e15de8785a8e5dd85bca7eb612eee0ff4dc8d7bdaa15eacbdbb'; keep = @('icuuc78.dll', 'icudt78.dll') }
    @{ file = 'win-64/libiconv-1.18-hc1393d2_3.conda'; sha256 = '35e04e3ddac7720fc7c550a1c9f998604299d6a7bd1f3ec9b2825d825061daa2'; keep = @('iconv.dll', 'charset.dll') }
    @{ file = 'win-64/libtiff-4.7.2-hef6d50c_2.conda'; sha256 = '61a557e19917a459f871471ca523b93973372ebf01a97e5802b56d069a2789b8'; keep = @('tiff.dll') }
    @{ file = 'win-64/libjpeg-turbo-3.2.1-hfd05255_0.conda'; sha256 = '195be7b2ff1eeff02b31cf366437a38d84560c2f6c4facd56b2969024f50957f'; keep = @('jpeg8.dll') }
    @{ file = 'win-64/libpng-1.6.59-hdc8cecf_0.conda'; sha256 = '36050852deae6edc98d7b71489b8d07254e2753c69bd524bf5784927cd1f0b2c'; keep = @('libpng16.dll') }
    @{ file = 'win-64/libwebp-base-1.6.0-h4d5522a_1.conda'; sha256 = '4470d98d3178b0d45492eed4808afe421d2e7808352b8d06c2dc29166b75d94d'; keep = @('libwebp.dll', 'libwebpmux.dll', 'libsharpyuv.dll') }
    @{ file = 'win-64/giflib-6.1.3-h918f4b6_2.conda'; sha256 = 'a1568e6b67ba46c96be71e071b527450ea3b229e60f636133d3cba8018f1ac03'; keep = @('gif-7.dll') }
    @{ file = 'win-64/openjpeg-2.5.4-h90fa87c_2.conda'; sha256 = 'd1c630ccd9ae0898b48d84e986f4c330f66a25e535f99efe8cbf03f042b33da5'; keep = @('openjp2.dll') }
    @{ file = 'win-64/lerc-4.3.0-hd936e49_0.conda'; sha256 = 'e96230bcb2e65df55c14c52ef2cbd5b688482fbb8760de59688879edcdfa3679'; keep = @('Lerc.dll') }
    @{ file = 'win-64/libdeflate-1.26-h1a1d4e4_0.conda'; sha256 = 'f51f28e894579387a531c53d97e058642ecfb232c4cb156c43c9121a9044f3d8'; keep = @('deflate.dll') }
    @{ file = 'win-64/libzlib-1.3.2-hfd05255_3.conda'; sha256 = '0629c2cc0404d3bb29d6baa7b4ba62da80797015e86de050db81ea5a07050527'; keep = @('zlib.dll') }
    @{ file = 'win-64/zstd-1.5.7-h534d264_7.conda'; sha256 = 'ca7daae4f218a11fab82cc2857f0ea518ec3f46acec60490485347a4c22c6b3e'; keep = @('zstd.dll') }
    @{ file = 'win-64/liblzma-5.8.3-hfd05255_1.conda'; sha256 = 'd36c4a1e1f80fd08e18a407e03622ff2f34dfdd022da6488ad19603dea19e6d5'; keep = @('liblzma.dll') }
    @{ file = 'win-64/lz4-c-1.11.1-h6a83c73_0.conda'; sha256 = '2a24b345cf469865d5d84d6daf4cc7b02cf135af1f88041c3933ac58ca523139'; keep = @('lz4.dll') }
    @{ file = 'win-64/bzip2-1.0.8-h0ad9c76_10.conda'; sha256 = '04767466ee9227c9c57ab2c6503e0149177d34111c7418d2f420297acb1eb229'; keep = @('libbz2.dll') }
    # the visual c++ runtime tesseract was built with. its terms (copied to
    # licenses/) allow installing and using it; how an installer should
    # provide it is a packaging question, see core/ocr/README.md
    @{ file = 'win-64/vc14_runtime-14.51.36247-h072b54c_43.conda'; sha256 = '02e21ff2ff06c7e45e10b72953cb2b1739e8dad7a4a8a49c814897cfabefaada'; keep = @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll') }
)

# english, spanish, french and german, plus "osd", which finds which way up
# a page is. more languages can be added the same way.
$languages = @(
    @{ name = 'eng'; sha256 = '7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2' }
    @{ name = 'spa'; sha256 = '6f2e04d02774a18f01bed44b1111f2cd7f3ba7ac9dc4373cd3f898a40ea6b464' }
    @{ name = 'fra'; sha256 = 'ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca' }
    @{ name = 'deu'; sha256 = '19d219bbb6672c869d20a9636c6816a81eb9a71796cb93ebe0cb1530e2cdb22d' }
    @{ name = 'osd'; sha256 = '9cf5d576fcc47564f11265841e5ca839001e7e6f38ff7f7aacf46d15a96b00ff' }
)

# downloads a file, or stops with a sentence a person can act on instead of
# a bare web error such as "(403) Forbidden"
function Get-Download([string]$url, [string]$file) {
    try {
        Invoke-WebRequest $url -OutFile $file -UseBasicParsing
        return
    } catch {
        $problem = $_.Exception
    }
    $status = 0
    if ($problem.Response) { $status = [int]$problem.Response.StatusCode }
    if ($status -eq 403) {
        $why = 'The server refused it (403). GitHub and conda turn away downloads from some networks and when too many arrive at once: wait a few minutes and run this again, or try from another network.'
    } elseif ($status -eq 404) {
        $why = 'It isn''t there (404): the address in this script may be out of date.'
    } elseif ($status -eq 0) {
        $why = "The connection failed ($($problem.Message)). Check that this computer can reach github.com and conda.anaconda.org; a proxy or firewall may be blocking them."
    } else {
        $why = "The server answered $status. Try again later."
    }
    throw "Couldn't download $(Split-Path $file -Leaf) from $url. $why Nothing was installed."
}

function Get-Checked([string]$url, [string]$file, [string]$sha256) {
    Get-Download $url $file
    $actual = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $sha256) {
        throw "$url did not match its checksum (expected $sha256, got $actual). nothing was installed."
    }
}

# a .conda file is a zip holding two zstd tarballs: the files and the metadata
function Expand-Conda([string]$archive, [string]$into) {
    New-Item -ItemType Directory -Force $into | Out-Null
    & $tar -xf $archive -C $into
    if ($LASTEXITCODE -ne 0) { throw "could not unpack $archive" }
    foreach ($inner in Get-ChildItem $into -Filter '*.tar.zst') {
        & $tar -xf $inner.FullName -C $into
        if ($LASTEXITCODE -ne 0) { throw "could not unpack $($inner.Name)" }
        Remove-Item $inner.FullName
    }
}

if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force $work | Out-Null
$staging = Join-Path $work 'tesseract'
$bin = Join-Path $staging 'bin'
$licenses = Join-Path $staging 'licenses'
$tessdata = Join-Path $staging 'tessdata'
New-Item -ItemType Directory -Force $bin, $licenses, $tessdata | Out-Null

foreach ($package in $packages) {
    $name = [IO.Path]::GetFileNameWithoutExtension($package.file)
    Write-Host "fetching $name"
    $archive = Join-Path $work "$name.conda"
    Get-Checked "$channel/$($package.file)" $archive $package.sha256
    $unpacked = Join-Path $work $name
    Expand-Conda $archive $unpacked
    Remove-Item $archive

    foreach ($wanted in $package.keep) {
        $found = Get-ChildItem $unpacked -Recurse -File -Filter $wanted | Select-Object -First 1
        if ($null -eq $found) { throw "$wanted was not in $name" }
        Copy-Item $found.FullName $bin -Force
    }
    $licenseDir = Join-Path $unpacked 'info\licenses'
    if (Test-Path $licenseDir) {
        Copy-Item $licenseDir (Join-Path $licenses $name) -Recurse -Force
    }
    # the configs folder lets tesseract understand output names like "tsv"
    $configs = Join-Path $unpacked 'Library\share\tessdata\configs'
    if (Test-Path $configs) { Copy-Item $configs $tessdata -Recurse -Force }
    Remove-Item $unpacked -Recurse -Force
}

foreach ($language in $languages) {
    Write-Host "fetching language data $($language.name)"
    $file = Join-Path $tessdata "$($language.name).traineddata"
    Get-Checked "$tessdataUrl/$($language.name).traineddata" $file $language.sha256
}
Get-Download "$tessdataUrl/LICENSE" (Join-Path $licenses 'tessdata_fast-LICENSE.txt')

Set-Content (Join-Path $staging 'VERSION') 'tesseract 5.5.4 (conda-forge h8c7c6a5_0), tessdata_fast 4.1.0'

# replace the old folder only once everything has arrived and checked out
if (Test-Path $target) { Remove-Item $target -Recurse -Force }
New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
Move-Item $staging $target
Remove-Item $work -Recurse -Force

& (Join-Path $target 'bin\tesseract.exe') --version | Select-Object -First 1
Write-Host "tesseract is ready in $target"
