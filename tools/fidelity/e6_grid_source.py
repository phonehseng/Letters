"""e6: where does word's 1/600 inch grid come from? only document-side changes.

each variant runs grid_probe.py's page through word and reports the coarsest grid
that line starts, baselines and pen steps sit on, plus the font sizes written:
    (a) w:compat: compatibilityMode 11 / 12 / 14 / 15, usePrinterMetrics,
        doNotUseHTMLParagraphAutoSpacing
    (b) ExportAsFixedFormat: OptimizeFor screen, UseISO19005_1, BitmapMissingFonts off
    (c) SaveAs2 with wdFormatPDF instead of ExportAsFixedFormat
    (d) Document.Compatibility(...) flags set on the open (read-only, never saved)
        document, constants read from word's own type library
    (e) font size 6 / 9 / 10.5 / 24 / 48 pt
word's application options are only read, never written: word saves them to the
user's profile on quit. the default printer is read, not changed.

usage:  .venv\\Scripts\\python.exe e6_grid_source.py
output: out\\e6\\ (probe docx and pdfs, e6_summary.json)
"""

from __future__ import annotations

import json
import os
import subprocess

import pythoncom

import grid_probe
import test_fonts

OUT_DIR = os.path.join(test_fonts.HERE, "out", "e6")
COMPATIBILITY_FLAGS_TO_TRY = (
    "wdUsePrinterMetrics",
    "wdSpacingInWholePoints",
    "wdTruncateFontHeight",
    "wdWPSpace",
    "wdExactOnTop",
    "wdNoLeading",
    "wdSubFontBySize",
    "wdDontUseHTMLParagraphAutoSpacing",
)
OPTIONS_TO_READ = ("UseSubPixelPositioning", "UseCharacterUnit", "UseNormalStyleForList", "PrintDraft")


def default_printer() -> dict:
    """the default printer as windows reports it, read only."""
    command = (
        "Get-CimInstance Win32_Printer | Where-Object Default | "
        "ForEach-Object { $_.Name + '|' + $_.HorizontalResolution + '|' + $_.VerticalResolution }"
    )
    output = subprocess.run(["powershell", "-NoProfile", "-Command", command], capture_output=True, text=True).stdout
    name, horizontal, vertical = (output.strip().split("|") + ["", "", ""])[:3]
    return {"name": name, "horizontal_dpi": horizontal, "vertical_dpi": vertical}


def enum_values(word, enum_name: str) -> dict[str, int]:
    """constants of one enum, read from word's type library without generating any files."""
    type_library, _ = word._oleobj_.GetTypeInfo().GetContainingTypeLib()
    for index in range(type_library.GetTypeInfoCount()):
        if type_library.GetDocumentation(index)[0] != enum_name:
            continue
        info = type_library.GetTypeInfo(index)
        values = {}
        for var_index in range(info.GetTypeAttr().cVars):
            description = info.GetVarDesc(var_index)
            values[info.GetNames(description.memid)[0]] = description.value
        return values
    return {}


def set_parameterised_property(com_object, name: str, argument, value) -> None:
    """document.Compatibility(x) = value, which late-bound python cannot write directly."""
    dispatch_id = com_object._oleobj_.GetIDsOfNames(name)
    com_object._oleobj_.Invoke(dispatch_id, 0, pythoncom.DISPATCH_PROPERTYPUT, 0, argument, value)


def read_options(word) -> dict:
    values = {"active_printer": str(word.ActivePrinter)}
    for name in OPTIONS_TO_READ:
        try:
            values[name] = getattr(word.Options, name)
        except Exception as error:  # not every build has every option
            values[name] = f"unavailable: {error}"
    return values


def compatibility_prepare(flag_name: str, flag_value: int):
    def prepare(word, document):
        before = document.Compatibility(flag_value)
        try:
            set_parameterised_property(document, "Compatibility", flag_value, not before)
        except Exception as error:
            # word refuses legacy flags in newer compatibility modes, that is a result too
            return {
                "flag": flag_name,
                "before": bool(before),
                "refused": str(error.excepinfo[2] if hasattr(error, "excepinfo") and error.excepinfo else error),
            }
        after = document.Compatibility(flag_value)
        document.Repaginate()
        return {"flag": flag_name, "before": bool(before), "after": bool(after)}

    return prepare


def inspect_prepare(word, document) -> dict:
    notes = read_options(word)
    notes["compatibility_enum"] = enum_values(word, "WdCompatibility")
    notes["document_compatibility_mode"] = document.CompatibilityMode
    return notes


def variants() -> list[tuple[str, dict]]:
    found = [
        ("default 12pt", {"export_options": {"prepare": inspect_prepare}}),
        ("compat mode 11", {"docx_options": {"compatibility_mode": 11}}),
        ("compat mode 12", {"docx_options": {"compatibility_mode": 12}}),
        ("compat mode 14", {"docx_options": {"compatibility_mode": 14}}),
        ("usePrinterMetrics (mode 15)", {"docx_options": {"extra_compat_xml": "<w:usePrinterMetrics/>"}}),
        (
            "usePrinterMetrics (mode 11)",
            {"docx_options": {"compatibility_mode": 11, "extra_compat_xml": "<w:usePrinterMetrics/>"}},
        ),
        (
            "doNotUseHTMLParagraphAutoSpacing",
            {"docx_options": {"extra_compat_xml": "<w:doNotUseHTMLParagraphAutoSpacing/>"}},
        ),
        ("export OptimizeFor screen", {"export_options": {"export_overrides": {"OptimizeFor": 1}}}),
        ("export UseISO19005_1 (PDF/A)", {"export_options": {"export_overrides": {"UseISO19005_1": True}}}),
        ("export BitmapMissingFonts off", {"export_options": {"export_overrides": {"BitmapMissingFonts": False}}}),
        ("SaveAs2 wdFormatPDF", {"export_options": {"use_save_as": True}}),
    ]
    for size in (6.0, 9.0, 10.5, 24.0, 48.0):
        found.append((f"size {size:g}pt", {"size_pt": size}))
    return found


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    summary = {"default_printer": default_printer()}
    print(f"default printer (read only): {summary['default_printer']}")
    results = {}
    compatibility_enum = {}
    for name, options in variants():
        file_name = name.replace(" ", "_").replace("(", "").replace(")", "").replace("/", "")
        result = grid_probe.run_grid_probe(OUT_DIR, file_name, **options)
        notes = result["word"].get("notes")
        if name == "default 12pt" and notes:
            compatibility_enum = notes.pop("compatibility_enum", {})
            print(f"word options (read only): {notes}")
        results[name] = result
        print(f"{name:<36} {grid_probe.describe(result)}")
    for flag in COMPATIBILITY_FLAGS_TO_TRY:
        if flag not in compatibility_enum:
            print(f"{flag}: not in this word's WdCompatibility enum")
            continue
        for mode in (15, 11):
            name = f"Compatibility({flag}) toggled, mode {mode}"
            result = grid_probe.run_grid_probe(
                OUT_DIR,
                f"compat_{flag}_{mode}",
                docx_options={"compatibility_mode": mode},
                export_options={"prepare": compatibility_prepare(flag, compatibility_enum[flag])},
            )
            results[name] = result
            print(f"{name:<52} {result['word'].get('notes')}  {grid_probe.describe(result)}")
    summary["variants"] = results
    summary["compatibility_enum"] = compatibility_enum
    with open(os.path.join(OUT_DIR, "e6_summary.json"), "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, default=str)


if __name__ == "__main__":
    main()
