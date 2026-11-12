# runs the whole oracle loop: write the corpus, have word and libreoffice
# author their own documents, lay everything out with microsoft word and
# libreoffice (private, invisible instances; see run_oracles.py for the
# safety rules), then score our engine and write ORACLE_REPORT.md.
#
#   .\run.ps1 -Iteration "3 tables"            # everything
#   .\run.ps1 -Iteration "3 tables" -Render    # also 150 dpi rendering scores
#   .\run.ps1 -SkipOracles                     # score again on the pdfs there
#
# pdfs already exported are kept; delete out\word, out\lo (and the *_odt
# folders) to export everything again. authored documents are made once;
# delete out\authored_word or out\authored_lo to make them again.

param(
    [string]$Iteration = "unnamed",
    [switch]$Render,
    [switch]$SkipOracles
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $here "..\..\..\..")
$python = Join-Path $root "tools\fidelity\.venv\Scripts\python.exe"
$loPython = Join-Path $env:LOCALAPPDATA "Programs\LibreOffice\program\python.exe"
$env:CARGO_TARGET_DIR = Join-Path $root "target-layout"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\Programs\mingw64\bin;$env:PATH"

Push-Location $here
try {
    cargo test -p letters-layout --test oracle -- --ignored generate_
    if (-not $SkipOracles) {
        if (-not (Test-Path out\authored_word\manifest.json)) {
            & $python -I author_word.py out\authored_word
        }
        if (-not (Test-Path out\authored_lo\manifest.json)) {
            & $loPython author_lo.py out\authored_lo
        }
        foreach ($source in "out\corpus", "out\authored_word", "out\authored_lo") {
            & $python -I run_oracles.py word $source out\word --skip-existing 1
            & $python -I run_oracles.py lo $source out\lo --skip-existing 1
        }
        # libreoffice-made odt files of every docx, laid out by both applications
        foreach ($source in "out\corpus", "out\authored_word") {
            & $python -I run_oracles.py lo $source out\odt --to odt --skip-existing 1
        }
        & $python -I run_oracles.py lo out\odt out\lo_odt --skip-existing 1
        # measurement probes (never scored with the corpus)
        & $python -I run_oracles.py word out\probes out\probes_word --skip-existing 1
        & $python -I run_oracles.py lo out\probes out\probes_lo --skip-existing 1
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
