"""experiment d1: do word's other pdf paths place a fitted document's ink differently?

the fitted one-line documents of b0 are converted again (two exports), then
the final docx goes out through every path below, and each pdf's ink is
checked against the source with raster_check (the b0 sources have whole
/Widths, so pdfium renders them where the spec puts them):
    ExportAsFixedFormat for print, for screen ("minimum size"), as PDF/A,
    SaveAs2 as pdf, the same docx with w:usePrinterMetrics in its settings,
    and PrintOut to the default printer "Microsoft Print to PDF" with an
    output file (the printer is only used, never changed).
a watchdog ends our own word process if printing blocks on a dialog.

usage:  ..\\.venv\\Scripts\\python.exe probe_output_paths.py
"""

from __future__ import annotations

import fidelity_path

# isort: split

import json
import os
import subprocess
import threading
import time
import zipfile

import raster_check
from fitted_convert import convert_fitted
from pdf_glyphs import drawn_glyphs
from word_session import PROTECTED_PIDS, WD_DO_NOT_SAVE_CHANGES, WD_FORMAT_PDF, WordSession, export_open_document

OUT = os.path.join(fidelity_path.OUT_DIR, "d1_output_paths")
SOURCES = ("one_line", "one_line_spaced", "repeated_lines")
PRINT_TIMEOUT_SECONDS = 90
PRINTER_METRICS_COMPAT = "<w:usePrinterMetrics/>"


def with_printer_metrics(docx: str, out_path: str) -> str:
    """the same package with w:usePrinterMetrics added to w:compat (before the compatSetting entries)."""
    with zipfile.ZipFile(docx) as source, zipfile.ZipFile(out_path, "w", zipfile.ZIP_DEFLATED) as target:
        for name in source.namelist():
            data = source.read(name)
            if name == "word/settings.xml":
                data = data.replace(b"<w:compat>", b"<w:compat>" + PRINTER_METRICS_COMPAT.encode())
            target.writestr(name, data)
    return out_path


def print_to_pdf(session: WordSession, docx: str, pdf: str) -> str:
    """PrintOut to a file through the default printer; a watchdog ends our word if it blocks."""
    if session.pid in PROTECTED_PIDS:
        raise RuntimeError("refusing to print from a word process that is not ours")
    watchdog = threading.Timer(PRINT_TIMEOUT_SECONDS, lambda: subprocess.run(
        ["taskkill", "/PID", str(session.pid), "/F"], capture_output=True, check=False))  # fmt: skip
    document = session.open(docx)
    watchdog.start()
    try:
        printer = session.word.ActivePrinter
        document.PrintOut(Background=False, OutputFileName=os.path.abspath(pdf), PrintToFile=True, Copies=1)
        deadline = time.monotonic() + 30
        while not os.path.exists(pdf) and time.monotonic() < deadline:
            time.sleep(0.25)
        return printer
    finally:
        watchdog.cancel()
        document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)


def outputs_for(session: WordSession, docx: str, folder: str) -> dict[str, str]:
    paths = {}

    def export(label: str, **overrides) -> None:
        paths[label] = os.path.join(folder, f"{label}.pdf")
        document = session.open(docx)
        try:
            export_open_document(document, paths[label], **overrides)
        finally:
            document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)

    export("export_print")
    export("export_screen", OptimizeFor=1)
    export("export_pdfa", UseISO19005_1=True)
    document = session.open(docx)
    try:
        paths["save_as_pdf"] = os.path.join(folder, "save_as_pdf.pdf")
        document.SaveAs2(FileName=os.path.abspath(paths["save_as_pdf"]), FileFormat=WD_FORMAT_PDF,
                         AddToRecentFiles=False)  # fmt: skip
    finally:
        document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
    metrics_docx = with_printer_metrics(docx, os.path.join(folder, "printer_metrics.docx"))
    paths["use_printer_metrics"] = os.path.join(folder, "use_printer_metrics.pdf")
    session.export(metrics_docx, paths["use_printer_metrics"])
    paths["print_to_pdf"] = os.path.join(folder, "print_to_pdf.pdf")
    return paths


def describe(source_pdf: str, pdf: str) -> dict:
    if not os.path.exists(pdf):
        return {"result": "no file written"}
    glyphs = drawn_glyphs(pdf)
    shifts = raster_check.summary(raster_check.line_shifts(source_pdf, pdf))
    return {"drawn_glyphs": len(glyphs), "ligature_glyphs": sum(len(g.text) > 1 for g in glyphs),
            "first_origin": (round(glyphs[0].x, 4), round(glyphs[0].y, 4)) if glyphs else None,
            "fonts_with_truetype_program": len({g.font_resource for g in glyphs if g.signature is not None}),
            **shifts}  # fmt: skip


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    results = {}
    for name in SOURCES:
        source = os.path.join(fidelity_path.OUT_DIR, "b0_sources", f"{name}.pdf")
        folder = os.path.join(OUT, name)
        with WordSession() as session:
            report, _ = convert_fitted(source, folder, "line", session)
            paths = outputs_for(session, report["docx"], folder)
            try:
                printer = print_to_pdf(session, report["docx"], paths["print_to_pdf"])
            except Exception as error:
                printer = f"print failed: {error}"
            results[name] = {"printer": printer, "word_pid": session.pid}
        for label, pdf in paths.items():
            results[name][label] = describe(source, pdf)
            print(name, label, results[name][label], flush=True)
    with open(os.path.join(OUT, "d1_results.json"), "w", encoding="utf-8") as handle:
        json.dump(results, handle, indent=1)


if __name__ == "__main__":
    main()
