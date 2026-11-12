# runs the whole oracle loop: write the corpus, lay it out with microsoft
# word and libreoffice (private, invisible instances; see run_oracles.py
# for the safety rules), then score our engine and write ORACLE_REPORT.md.
#
#   .\run.ps1 -Iteration "3 tables"            # everything
#   .\run.ps1 -Iteration "3 tables" -Render    # also 300 dpi rendering scores
#   .\run.ps1 -SkipOracles                     # score again on the pdfs there
#
# pdfs already exported are kept; delete out\word, out\lo (and the *_odt
# folders) to export everything again.

param(
    [string]$Iteration = "unnamed",
    [switch]$Render,
    [switch]$SkipOracles
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $here "..\..\..\..")
$python = Join-Path $root "tools\fidelity\.venv\Scripts\python.exe"
$env:CARGO_TARGET_DIR = Join-Path $root "target-layout"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\Programs\mingw64\bin;$env:PATH"

Push-Location $here
try {
    cargo test -p letters-layout --test oracle -- --ignored generate_corpus
    if (-not $SkipOracles) {
        & $python -I run_oracles.py word out\corpus out\word --skip-existing 1
        & $python -I run_oracles.py lo out\corpus out\lo --skip-existing 1
        # libreoffice-made odt files, laid out by both applications
        & $python -I run_oracles.py lo out\corpus out\odt --to odt --skip-existing 1
        & $python -I run_oracles.py lo out\odt out\lo_odt --skip-existing 1
        & $python -I run_oracles.py word out\odt out\word_odt --skip-existing 1
        & $python -I run_oracles.py processes
    }
    $env:ORACLE_ITERATION = $Iteration
    if ($Render) { $env:ORACLE_RENDER = "1" } else { Remove-Item Env:\ORACLE_RENDER -ErrorAction SilentlyContinue }
    cargo test -p letters-layout --release --test oracle -- --ignored score_corpus --nocapture
}
finally {
    Pop-Location
}
