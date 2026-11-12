"""one private, invisible microsoft word instance for a batch of exports, edits and prints.

the owner keeps word open with their own documents, so this never attaches to
a running instance. DispatchEx starts a new process; we give it a unique
caption, find its window and read the process id from it, so we always know
exactly which WINWORD.EXE is ours even when other people start word at the
same time. every started pid is appended to out\\word_pids.log, and the
instance is quit in a finally block.
"""

from __future__ import annotations

import os
import subprocess
import time
import uuid

import pythoncom
import win32com.client
import win32gui
import win32process

HERE = os.path.dirname(os.path.abspath(__file__))
PID_LOG = os.path.join(HERE, "out", "word_pids.log")
# the owner's own word windows: never touched, whatever happens
PROTECTED_PIDS = {34608, 23956}

WD_EXPORT_FORMAT_PDF = 17
WD_EXPORT_OPTIMIZE_FOR_PRINT = 0
WD_EXPORT_OPTIMIZE_FOR_SCREEN = 1
WD_FORMAT_PDF = 17
WD_DO_NOT_SAVE_CHANGES = 0
WD_ALERTS_NONE = 0
WORD_WINDOW_CLASS = "OpusApp"


def word_process_ids() -> set[int]:
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


def _pid_of_caption(caption: str, timeout_seconds: float = 10.0) -> int | None:
    # word's main window has the class OpusApp and, once Caption is set, our title
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        handle = win32gui.FindWindow(WORD_WINDOW_CLASS, caption)
        if handle:
            return win32process.GetWindowThreadProcessId(handle)[1]
        time.sleep(0.2)
    return None


def _log_pid(pid: int | None, event: str) -> None:
    os.makedirs(os.path.dirname(PID_LOG), exist_ok=True)
    with open(PID_LOG, "a", encoding="utf-8") as handle:
        handle.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} {event} {pid}\n")


class WordSession:
    """use as a context manager: `with WordSession() as session: session.export(...)`."""

    def __init__(self):
        self.word = None
        self.pid: int | None = None

    def __enter__(self) -> WordSession:
        pythoncom.CoInitialize()
        self.word = win32com.client.DispatchEx("Word.Application")
        caption = f"letters-fitted-{uuid.uuid4().hex[:12]}"
        self.word.Caption = caption
        self.pid = _pid_of_caption(caption)
        _log_pid(self.pid, "started")
        if self.word.Documents.Count != 0:
            # an instance that already has documents is somebody else's
            raise RuntimeError("DispatchEx returned a word instance that already has documents")
        self.word.Visible = False
        self.word.DisplayAlerts = WD_ALERTS_NONE
        self.word.ScreenUpdating = False
        return self

    @property
    def version(self) -> str:
        return f"{self.word.Version} build {self.word.Build}"

    def open(self, docx_path: str, read_only: bool = True):
        document = self.word.Documents.Open(
            FileName=os.path.abspath(docx_path),
            ConfirmConversions=False,
            ReadOnly=read_only,
            AddToRecentFiles=False,
            Visible=False,
            NoEncodingDialog=True,
        )
        document.Repaginate()
        return document

    def export(self, docx_path: str, pdf_path: str, **overrides) -> float:
        """open read-only, export as pdf with ExportAsFixedFormat, close; returns seconds."""
        document = self.open(docx_path)
        try:
            return export_open_document(document, pdf_path, **overrides)
        finally:
            document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)

    def __exit__(self, *exc) -> None:
        try:
            if self.word is not None:
                for index in range(self.word.Documents.Count, 0, -1):
                    try:
                        self.word.Documents(index).Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
                    except Exception:
                        pass
                self.word.Quit(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
        except Exception:
            pass
        finally:
            self.word = None
            pythoncom.CoUninitialize()
            self._make_sure_it_exited()

    def _make_sure_it_exited(self) -> None:
        if self.pid is None:
            return
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and self.pid in word_process_ids():
            time.sleep(0.25)
        if self.pid not in word_process_ids():
            _log_pid(self.pid, "exited")
            return
        # only our own process, identified by its caption, and never the owner's
        if self.pid not in PROTECTED_PIDS:
            subprocess.run(["taskkill", "/PID", str(self.pid), "/F"], capture_output=True, check=False)
            _log_pid(self.pid, "ended after quit was ignored")


def export_open_document(document, pdf_path: str, **overrides) -> float:
    pdf_path = os.path.abspath(pdf_path)
    if os.path.exists(pdf_path):
        os.remove(pdf_path)
    arguments = {
        "ExportFormat": WD_EXPORT_FORMAT_PDF,
        "OpenAfterExport": False,
        "OptimizeFor": WD_EXPORT_OPTIMIZE_FOR_PRINT,
        "Range": 0,
        "Item": 0,
        "IncludeDocProps": False,
        "KeepIRM": True,
        "CreateBookmarks": 0,
        "DocStructureTags": False,
        "BitmapMissingFonts": True,
        "UseISO19005_1": False,
    }
    arguments.update(overrides)
    started = time.perf_counter()
    document.ExportAsFixedFormat(OutputFileName=pdf_path, **arguments)
    return time.perf_counter() - started
