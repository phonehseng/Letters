"""lays out the oracle corpus with microsoft word and libreoffice, as pdf files.

the layout engine is scored against what the two word processors do with the
same files, so this script asks each of them to export every corpus document
to pdf. the rust side (tests/oracle.rs) then reads glyph positions from those
pdfs with pdfium.

safety rules, because the person using this machine has word open:

- word: DispatchEx always starts a new, private word process. the process id
  is the one WINWORD.EXE that appeared while it started and whose command line
  carries /Automation; if that is not exactly one process, nothing is ever
  force-ended. documents open read-only and invisible, alerts are off, word's
  options are never changed. a watchdog thread ends exactly that process if a
  single document takes longer than the timeout, and a finally block quits it.
- libreoffice: runs headless with a private profile folder, so the person's
  own profile is never touched; each run has a timeout, and on timeout only
  the process tree started here is ended.

usage:
  python run_oracles.py word  <in dir> <out dir> [--timeout 120] [--skip-existing 1]
  python run_oracles.py lo    <in dir> <out dir> [--to pdf|odt] [--timeout 300] [--skip-existing 1]
  python run_oracles.py processes      (lists word and soffice process ids)
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

SOFFICE = Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "LibreOffice" / "program" / "soffice.com"
WD_EXPORT_FORMAT_PDF = 17
WD_DO_NOT_SAVE_CHANGES = 0
WD_ALERTS_NONE = 0


def process_ids(image: str) -> set[int]:
    """the ids of running processes with this image name (tasklist ships with windows)."""
    output = subprocess.run(
        ["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"],
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


def command_line(pid: int) -> str:
    """the command line of one process, read through cim (read only)."""
    script = f"(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}').CommandLine"
    result = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.stdout.strip()


def end_process(pid: int) -> None:
    subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True, check=False)


class PrivateWord:
    """one invisible word process that this script started and owns."""

    def __init__(self, timeout_seconds: float):
        import pythoncom
        import win32com.client

        pythoncom.CoInitialize()
        before = process_ids("WINWORD.EXE")
        self.app = win32com.client.DispatchEx("Word.Application")
        appeared = process_ids("WINWORD.EXE") - before
        # only a process that appeared now and was started for automation is ours
        owned = [pid for pid in appeared if "/automation" in command_line(pid).lower()]
        self.pid = owned[0] if len(owned) == 1 else None
        if self.app.Documents.Count != 0:
            raise RuntimeError("DispatchEx returned a word instance that already has documents")
        self.app.Visible = False
        self.app.DisplayAlerts = WD_ALERTS_NONE
        self.app.ScreenUpdating = False
        self.version = f"{self.app.Version} build {self.app.Build}"
        self.timeout = timeout_seconds
        self.watchdog: threading.Timer | None = None

    def arm(self) -> None:
        """ends our word if the next call hangs; disarmed when it returns."""
        self.disarm()
        if self.pid is not None:
            self.watchdog = threading.Timer(self.timeout, end_process, args=(self.pid,))
            self.watchdog.daemon = True
            self.watchdog.start()

    def disarm(self) -> None:
        if self.watchdog is not None:
            self.watchdog.cancel()
            self.watchdog = None

    def export(self, source: Path, pdf: Path) -> dict:
        if pdf.exists():
            pdf.unlink()
        self.arm()
        document = None
        try:
            started = time.perf_counter()
            document = self.app.Documents.Open(
                FileName=str(source),
                ConfirmConversions=False,
                ReadOnly=True,
                AddToRecentFiles=False,
                Visible=False,
                NoEncodingDialog=True,
            )
            document.Repaginate()
            document.ExportAsFixedFormat(
                OutputFileName=str(pdf),
                ExportFormat=WD_EXPORT_FORMAT_PDF,
                OpenAfterExport=False,
                OptimizeFor=0,
                Range=0,
                Item=0,
                IncludeDocProps=False,
                KeepIRM=True,
                CreateBookmarks=0,
                DocStructureTags=False,
                BitmapMissingFonts=True,
                UseISO19005_1=False,
            )
            pages = document.ComputeStatistics(2)  # wdStatisticPages
            return {"pages": pages, "seconds": round(time.perf_counter() - started, 2)}
        finally:
            if document is not None:
                try:
                    document.Close(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
                except Exception:
                    pass
            self.disarm()

    def quit(self) -> None:
        import pythoncom

        try:
            self.app.Quit(SaveChanges=WD_DO_NOT_SAVE_CHANGES)
        except Exception:
            pass
        self.app = None
        if self.pid is not None:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and self.pid in process_ids("WINWORD.EXE"):
                time.sleep(0.25)
            if self.pid in process_ids("WINWORD.EXE"):
                end_process(self.pid)
        pythoncom.CoUninitialize()


def pdf_name(source: Path) -> str:
    """docx and odt of the same name get separate pdfs: name_docx.pdf."""
    return source.stem + source.suffix.replace(".", "_") + ".pdf"


def run_word(source_dir: Path, out_dir: Path, timeout: float, skip_existing: bool = False) -> dict:
    out_dir.mkdir(parents=True, exist_ok=True)
    sources = sorted(p for p in source_dir.iterdir() if p.suffix.lower() in (".docx", ".odt"))
    if skip_existing:
        sources = [p for p in sources if not (out_dir / pdf_name(p)).exists()]
    if not sources:
        return {"oracle": "word", "documents": {}, "skipped": "nothing to do"}
    report = {"oracle": "word", "documents": {}}
    word = PrivateWord(timeout)
    report["version"] = word.version
    report["pid"] = word.pid
    try:
        for source in sources:
            pdf = out_dir / pdf_name(source)
            try:
                report["documents"][source.name] = word.export(source.resolve(), pdf.resolve())
            except Exception as error:  # one bad file must not stop the run
                report["documents"][source.name] = {"error": str(error)}
            print(f"word {source.name}: {report['documents'][source.name]}", flush=True)
    finally:
        word.quit()
    report["pid_still_running"] = word.pid in process_ids("WINWORD.EXE") if word.pid else None
    return report


def run_libreoffice(source_dir: Path, out_dir: Path, target: str, timeout: float, skip_existing: bool = False) -> dict:
    out_dir.mkdir(parents=True, exist_ok=True)
    profile = (out_dir.parent / "lo_profile").resolve()
    profile.mkdir(parents=True, exist_ok=True)
    wanted = (".docx",) if target == "odt" else (".docx", ".odt")
    sources = sorted(p for p in source_dir.iterdir() if p.suffix.lower() in wanted)
    if skip_existing:
        done = (lambda p: (out_dir / pdf_name(p)).exists()) if target == "pdf" else (lambda p: (out_dir / (p.stem + ".odt")).exists())
        sources = [p for p in sources if not done(p)]
    report = {"oracle": "libreoffice", "documents": {}}
    for source in sources:
        # a copy named by its extension keeps docx and odt pdfs apart
        started = time.perf_counter()
        command = [
            str(SOFFICE),
            f"-env:UserInstallation={profile.as_uri()}",
            "--headless",
            "--norestore",
            "--nologo",
            "--convert-to",
            target,
            "--outdir",
            str(out_dir.resolve()),
            str(source.resolve()),
        ]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            process.communicate(timeout=timeout)
            status = process.returncode
        except subprocess.TimeoutExpired:
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(process.pid)], capture_output=True, check=False)
            process.communicate()
            status = "timeout"
        produced = out_dir / (source.stem + "." + target)
        if target == "pdf" and produced.exists():
            final = out_dir / pdf_name(source)
            if final.exists():
                final.unlink()
            produced.rename(final)
            produced = final
        report["documents"][source.name] = {
            "status": status,
            "ok": produced.exists(),
            "seconds": round(time.perf_counter() - started, 2),
        }
        print(f"libreoffice {source.name}: {report['documents'][source.name]}", flush=True)
    return report


def main(arguments: list[str]) -> int:
    if not arguments:
        print(__doc__)
        return 2
    mode = arguments[0]
    if mode == "processes":
        print(json.dumps({"WINWORD.EXE": sorted(process_ids("WINWORD.EXE")),
                          "soffice.bin": sorted(process_ids("soffice.bin")),
                          "soffice.exe": sorted(process_ids("soffice.exe"))}))  # fmt: skip
        return 0
    source_dir, out_dir = Path(arguments[1]), Path(arguments[2])
    options = dict(zip(arguments[3::2], arguments[4::2]))
    if mode == "word":
        skip = options.get("--skip-existing") == "1"
        report = run_word(source_dir, out_dir, float(options.get("--timeout", 120)), skip)
    elif mode == "lo":
        target = options.get("--to", "pdf")
        skip = options.get("--skip-existing") == "1"
        report = run_libreoffice(source_dir, out_dir, target, float(options.get("--timeout", 300)), skip)
    else:
        print(__doc__)
        return 2
    (out_dir / f"{mode}_{options.get('--to', 'pdf')}_report.json").write_text(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
