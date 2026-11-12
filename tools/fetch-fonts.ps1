# downloads the fonts that ship with Letters into vendor\fonts.
#
# every font here is under the SIL Open Font License 1.1, which allows
# bundling and embedding; each family's licence text is downloaded with it.
# files come from the google/fonts repository at one pinned commit (the
# same one the online library's catalogue uses), and each file's SHA-256
# is checked, so a changed or damaged download is refused.
#
# usage, from the repository root:   .\tools\fetch-fonts.ps1

$ErrorActionPreference = "Stop"

$commit = "c30b221beb0c1da59d563feb91ec8656a1fa972a"
$library = "https://raw.githubusercontent.com/google/fonts/$commit/ofl"
# tinos has no licence file in google/fonts at that commit; its own
# repository's is used, also pinned
$tinosLicense = "https://raw.githubusercontent.com/googlefonts/tinos/4106aab86ddefe26d58a69745e04ae9479d22993/OFL.txt"

# folder/file, sha-256. the metric-compatible families come first:
# Carlito (Calibri), Caladea (Cambria), Arimo (Arial), Tinos (Times New
# Roman), Cousine (Courier New); then a small general set
$files = @(
    "carlito/Carlito-Regular.ttf|f6418f708baede9789daef5d458c0f53d2a888af9820e8062934e504fedc6595",
    "carlito/Carlito-Bold.ttf|bb5d20f79b82599ec72983597437373a80f2d2085fa91fc144fd74e876a594db",
    "carlito/Carlito-Italic.ttf|0b019225e58d702bfedcbd35c21696769f8ee115cb6343f84c2f240312450d1c",
    "carlito/Carlito-BoldItalic.ttf|b32928186c119599e03ca6a1ffc680fdcb7fac95772f4b95d989cf6cd3861517",
    "carlito/OFL.txt|58402f82a7c332a700294988fe7554fbb0a63a8d27ccc1ee3bbc640311990a00",
    "caladea/Caladea-Regular.ttf|f1e899278b7b4491aba5b6a8253c4b04c050cc59b21865be5c37559a775153cd",
    "caladea/Caladea-Bold.ttf|ae3cb2dcbc925809dd29d2a44e9802211cab66be541bacbfc9c08c74b27c3742",
    "caladea/Caladea-Italic.ttf|4359a8e24f748b6447b1ff6d7a174febe70961d29f8bb8634b56dacd740a3deb",
    "caladea/Caladea-BoldItalic.ttf|ccabaa7b7e2fdf253d2b1a5fa699dd8a3df8d835a9eb285ad82631a677eb76c0",
    "caladea/OFL.txt|ccdab61d371d8c8683a128a92cd7d498dbdb1d37689f7cb21f1bf6b16658d213",
    "arimo/Arimo[wght].ttf|e43898b143ec826ac8cb4034816458a7047fbe0836558de2a1f8c6223ae3e0ca",
    "arimo/Arimo-Italic[wght].ttf|a80fc54fd0233c1dfe298577c4d00f5ae81d5bb83510975e473c47e699b7f4ed",
    "arimo/OFL.txt|11cce536cd2f3864d767003af5dcd739e2e15818cf2279b6175edeadd3960992",
    "tinos/Tinos-Regular.ttf|60a0e8ef0c04dd5dd69ffe91025fa2ae5836cbd35600a82ba031977557e2cb61",
    "tinos/Tinos-Bold.ttf|393269dbab8899f938db19783eca5eac92eb431f7ae0ab45b8349ca895f1a06b",
    "tinos/Tinos-Italic.ttf|5942266ed398b155d7dc23e36833e7ec6be988f2439bdbeb8ef1bede808eaa91",
    "tinos/Tinos-BoldItalic.ttf|a5de79f0fe863ea0954757acb3d47b3ccd0a930ce3dd5b97230cd3866790a06e",
    "tinos/OFL.txt|cb3382d4643e8b02c12e322c220a3c76a5020d667e4fd4e7c75e744cca6caa6b",
    "cousine/Cousine-Regular.ttf|1da22250675fc4c42fcf3a9736c44bc0570516105331443b663fd5cfbd1412fe",
    "cousine/Cousine-Bold.ttf|17c8a7245156d2253531c9e529474937b09d9f641c5ae7695c5e33f22822eef4",
    "cousine/Cousine-Italic.ttf|ea2a76ae3d0ece9cd59f0d30fdc08dd70e8f5f457beee5b0852a7b50c2286c7c",
    "cousine/Cousine-BoldItalic.ttf|848e858726fee0ae27b754e4cd6a2755209bf1428a8c91f747696d58c33906c3",
    "cousine/OFL.txt|b81c4d4dc0a9f72c9155e78187316e016e2012a8102468804173dc61468b906d",
    "inter/Inter[opsz,wght].ttf|29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
    "inter/Inter-Italic[opsz,wght].ttf|acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6",
    "inter/OFL.txt|5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57",
    "sourceserif4/SourceSerif4[opsz,wght].ttf|97b2d4da6e3cb494b5a1e66ae176914d852ccabef49e0c02c0df25f3e39aca0b",
    "sourceserif4/SourceSerif4-Italic[opsz,wght].ttf|15fbc7e4679489a501998c3669272637a6646388ef7e4bd77eebb5bf967a1f42",
    "sourceserif4/OFL.txt|5f94c3fd3a23131a417ab5a0c8452de57e70c3cfb9f604d88241f7065ebf9fd9",
    "sourcesans3/SourceSans3[wght].ttf|042fe2cc0b933e328410d7acbd0aa6a1873dca5aef81875f4bc214b08825c7b9",
    "sourcesans3/SourceSans3-Italic[wght].ttf|39e3ab05ccd7cb94907c31005bb5bec1d5432f0b096a2b782976e217a540eb6c",
    "sourcesans3/OFL.txt|09746787287a289323b0ec3cff4d1a4a801331b82b7207c1e186f5d26619a392",
    "notosans/NotoSans[wdth,wght].ttf|bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d",
    "notosans/OFL.txt|cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    "notoserif/NotoSerif[wdth,wght].ttf|4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713",
    "notoserif/OFL.txt|cee9892f9f0cc8fe882c9e9537ee6a89621d86ee7ceaf70b02e2b2b1c25c061a",
    "ebgaramond/EBGaramond[wght].ttf|ef9512f92f6d579e5dc75af59a5a4b1b8b47d2eda89e00b954d44520e5369027",
    "ebgaramond/EBGaramond-Italic[wght].ttf|bba2c4499c93c9612b90b9825d32b07da52fce2fe57562a1eb6b833553f93c4e",
    "ebgaramond/OFL.txt|0985066662eb755ed3683ae5482a81a9195b49ce3f7e165cc2388b3dbece7dd7",
    "jetbrainsmono/JetBrainsMono[wght].ttf|48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
    "jetbrainsmono/JetBrainsMono-Italic[wght].ttf|85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558",
    "jetbrainsmono/OFL.txt|b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d",
    "caveat/Caveat[wght].ttf|0bdb6b660482d31531b3945849fba5916b3ef8695da7024a9e6b9ee3c4157988",
    "caveat/OFL.txt|1f9d81d094273d82f3898a1ee8b598a717d050ecbf5ff7bede105b704880157b",
    "atkinsonhyperlegible/AtkinsonHyperlegible-Regular.ttf|7fb917c89019896d0b52ee84b7cbb3304c18cb90b19a62f5e32712bd23e97669",
    "atkinsonhyperlegible/AtkinsonHyperlegible-Bold.ttf|5a3b0c8cc8ca545155150b4512a1fa248298df121c50d6557e651e61fbdab92f",
    "atkinsonhyperlegible/AtkinsonHyperlegible-Italic.ttf|021beda4d3c6edfc78872e436d74009f9a1bcb294331908fe5747c61d3dcc514",
    "atkinsonhyperlegible/AtkinsonHyperlegible-BoldItalic.ttf|7a46f1b0ef2f64b61065c1a6b5a11fcb9b019b44062d88ba121a083cc39e86bf",
    "atkinsonhyperlegible/OFL.txt|f32d22b3908fcad2c86a74000614ec22e6a7f66ea7e867e616026a27aebdc143"
)

$root = Join-Path (Split-Path $PSScriptRoot -Parent) "vendor\fonts"
$client = New-Object System.Net.WebClient

foreach ($line in $files) {
    $name, $expected = $line.Split("|")
    $folder, $file = $name.Split("/")
    $target = [System.IO.Path]::Combine($root, $folder, $file)
    New-Item -ItemType Directory -Force -Path ([System.IO.Path]::Combine($root, $folder)) | Out-Null

    # an existing file with the right hash is kept
    if ((Test-Path -LiteralPath $target) -and ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLower() -eq $expected)) {
        continue
    }
    if ($name -eq "tinos/OFL.txt") {
        $url = $tinosLicense
    } else {
        $url = "$library/$folder/" + [System.Uri]::EscapeDataString($file)
    }
    Write-Host "downloading $name"
    # webclient, because Invoke-WebRequest reads [ ] in file names as wildcards
    $client.DownloadFile($url, $target)
    $actual = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) {
        Remove-Item -LiteralPath $target
        throw "$name has the wrong checksum ($actual). Nothing was kept; try again later."
    }
}

Write-Host "the bundled fonts are in $root"
