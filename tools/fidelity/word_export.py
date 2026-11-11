"""export a docx to pdf with a private, invisible microsoft word instance.

the user may have word open with their own documents, so this never attaches to
a running instance: DispatchEx always starts a new process, and we refuse to go
on if the new instance already has documents (that would mean we got theirs).
"""

from __future__ import annotations

import os
import subprocess
import time
from collections.abc import Callable

import pythoncom
import win32com.client

WD_EXPORT_FORMAT_PDF = 17
WD_EXPORT_OPTIMIZE_FOR_PRINT = 0
WD_EXPORT_ALL_DOCUMENT = 0
WD_EXPORT_DOCUMENT_CONTENT = 0
WD_EXPORT_CREATE_NO_BOOKMARKS = 0
WD_DO_NOT_SAVE_CHANGES = 0
WD_ALERTS_NONE = 0
WD_FORMAT_PDF = 17

DEFAULT_EXPORT_ARGUMENTS = {
    "ExportFormat": WD_EXPORT_FORMAT_PDF,
    "OpenAfterExport": False,
    "OptimizeFor": WD_EXPORT_OPTIMIZE_FOR_PRINT,
    "Range": WD_EXPORT_ALL_DOCUMENT,
    "Item": WD_EXPORT_DOCUMENT_CONTENT,
    "IncludeDocProps": False,
    "KeepIRM": True,
    "CreateBookmarks": WD_EXPORT_CREATE_NO_BOOKMARKS,
    "DocStructureTags": False,
    "BitmapMissingFonts": True,
    "UseISO19005_1": False,
}


class WordAutomationError(RuntimeError):
    pass


def _word_process_ids() -> set[int]:
    # tasklist is always present on windows and avoids another dependency
    output = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq WINWORD.EXE", "/FO", "CSV", "/NH"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    ids = set()
    for line in output.splitlines():
        parts = [part.strip('"') for part in line.split('","')]
        if len(parts) > 1 and parts[1].isdigit():
            ids.add(int(parts[1]))
    return ids


def _start_private_word():
    before = _word_process_ids()
    word = win32com.client.DispatchEx("Word.Application")
    started = _word_process_ids() - before
    if word.Documents.Count != 0:
        # we were handed an instance with documents in it, so it is not ours
        raise WordAutomationError("DispatchEx returned a word instance that already has documents")
    word.Visible = False
    word.DisplayAlerts = WD_ALERTS_NONE
    word.ScreenUpdating = False
    return word, started


def _wait_for_exit(process_ids: set[int], timeout_seconds: float = 15.0) -> set[int]:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        remaining = process_ids & _word_process_ids()
        if not remaining:
            return set()
        time.sleep(0.25)
    return process_ids & _word_process_ids()


def export_docx_to_pdf(
    docx_path: str,
    pdf_path: str,
    *,
    export_overrides: dict | None = None,
    use_save_as: bool = False,
    prepare: Callable | None = None,
) -> dict:
    """open the docx read-only, export it as pdf and close everything we opened.

    export_overrides replaces ExportAsFixedFormat arguments, use_save_as writes
    the pdf with SaveAs2 instead, and prepare(word, document) may adjust the open
    document (never word's application options: those are saved to the user's
    profile when word quits) and return notes for the report.
    returns timings and the word version so reports say what produced them."""
    docx_path = os.path.abspath(docx_path)
    pdf_path = os.path.abspath(pdf_path)
    if os.path.exists(pdf_path):
        os.remove(pdf_path)
    pythoncom.CoInitialize()
    word = None
    document = None
    started_ids: set[int] = set()
    timings = {}
    try:
        t0 = time.perf_counter()
        word, started_ids = _start_private_word()
        timings["start_word_s"] = time.perf_counter() - t0
        version = f"{word.Version} build {word.Build}"

        t0 = time.perf_counter()
        document = word.Documents.Open(
            FileName=docx_path,
            ConfirmConversions=False,
            ReadOnly=True,
            AddToRecentFiles=False,
            Visible=False,
            NoEncodingDialog=True,
        )
        # make sure pagination is final before export
        document.Repaginate()
        timings["open_s"] = time.perf_counter() - t0

        notes = prepare(word, document) if prepare is not None else None

        t0 = time.perf_counter()
        if use_save_as:
            document.SaveAs2(FileName=pdf_path, FileFormat=WD_FORMAT_PDF, AddToRecentFiles=False)
        else:
            arguments = dict(DEFAULT_EXPORT_ARGUMENTS, **(export_overrides or {}))
            document.ExportAsFixedFormat(OutputFileName=pdf_path, **arguments)
        timings["export_s"] = time.perf_counter() - t0
        page_count = document.ComputeStatistics(2)  # wdStatisticPages
        return {"word_version": version, "word_pages": page_count, "timings": timings, "notes": notes}
    finally:
        if document is not None:
            try:
                document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
            except Exception:
                pass
        if word is not None:
            try:
                word.Quit(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
            except Exception:
                pass
            word = None
        # only the process that appeared when we started word is ours, and only
        # if it ignores quit do we end it, so a stuck instance does not pile up
        still_running = _wait_for_exit(started_ids) if len(started_ids) == 1 else set()
        for process_id in still_running:
            subprocess.run(["taskkill", "/PID", str(process_id), "/F"], capture_output=True, check=False)
        pythoncom.CoUninitialize()
