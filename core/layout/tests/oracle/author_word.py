"""makes documents the way a student would, inside microsoft word itself.

the synthetic corpus is written by letters-office, so it only ever shows
our writer's xml. these documents are made through word's object model
from word's own normal template (its default styles, theme, settings and
table style), then saved by word, so the scores also cover files as people
really have them.

three kinds of defaults are used:
- "aptos": the normal template as it is on this machine (aptos 11 or 12)
- "calibri": the older office defaults, set on the document's own styles
  (calibri 11, 1.15 lines, 10 pt after, calibri light headings)
- "mla": times new roman 12, double spaced, a running head with the page

word is started privately and invisibly (see run_oracles.PrivateWord for
the safety rules: our own process only, recorded by id, always ended).
nothing in word's options or its normal template is changed; every style
change is made on the new document only.

usage: python author_word.py <out dir>
writes <name>.docx files and manifest.json ({name, class, heldOut}).
"""

from __future__ import annotations

import json
import random
import struct
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_oracles import PrivateWord, process_ids  # noqa: E402

WORDS = (
    "the of and a to in is that for it as was with be by on not this are or which from an at "
    "but their study students learning evidence research between however results experiment "
    "measured carefully although important describes chapter analysis university particular "
    "understanding significant relationship environment development information approximately "
    "communication interpretation responsibility characteristics nevertheless laboratory "
    "temperature observation hypothesis conclusion literature historical economic policy theory "
    "method sample data value model time people water small large early first second third "
    "several different always never often rarely through during within without against towards "
    "argument essay paragraph author's readers well-known twenty-one 1984 3.5 e.g."
).split()

# word's built-in style numbers (the same in every language)
STYLE_NORMAL = -1
STYLE_HEADING1 = -2
STYLE_HEADING2 = -3
STYLE_TITLE = -63
STYLE_LIST_BULLET = -49
STYLE_LIST_NUMBER = -50
STYLE_CAPTION = -35
FORMAT_DOCX = 16
WRAP_SQUARE = 0
FIELD_PAGE = 33


def sentence(rng: random.Random, words: int) -> str:
    chosen = [rng.choice(WORDS) for _ in range(words)]
    chosen[0] = chosen[0][:1].upper() + chosen[0][1:]
    for i in range(1, len(chosen) - 1):
        if rng.randrange(11) == 0:
            chosen[i] += ","
    return " ".join(chosen) + "."


def prose(rng: random.Random, words: int) -> str:
    out, count = [], 0
    while count < words:
        length = 6 + rng.randrange(17)
        out.append(sentence(rng, length))
        count += length
    return " ".join(out)


def png(path: Path, width: int = 64, height: int = 48) -> None:
    """a small gradient png, written without any imaging library."""
    rows = b"".join(
        b"\x00" + bytes(v for x in range(width) for v in (x * 255 // width, y * 255 // height, 160))
        for y in range(height)
    )

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


class Builder:
    """appends paragraphs and tables at the end of a new document."""

    def __init__(self, app, defaults: str):
        self.doc = app.Documents.Add(Visible=False)
        self.defaults = defaults
        if defaults == "calibri":
            self.calibri_defaults()
        elif defaults == "mla":
            self.mla_defaults()

    def calibri_defaults(self) -> None:
        normal = self.doc.Styles(STYLE_NORMAL)
        normal.Font.Name = "Calibri"
        normal.Font.Size = 11
        normal.ParagraphFormat.SpaceAfter = 10
        normal.ParagraphFormat.LineSpacingRule = 5  # multiple
        normal.ParagraphFormat.LineSpacing = 13.8  # 1.15 lines
        for style, size in ((STYLE_HEADING1, 16), (STYLE_HEADING2, 13)):
            self.doc.Styles(style).Font.Name = "Calibri Light"
            self.doc.Styles(style).Font.Size = size

    def mla_defaults(self) -> None:
        normal = self.doc.Styles(STYLE_NORMAL)
        normal.Font.Name = "Times New Roman"
        normal.Font.Size = 12
        normal.ParagraphFormat.SpaceAfter = 0
        normal.ParagraphFormat.SpaceBefore = 0
        normal.ParagraphFormat.LineSpacingRule = 2  # double

    def end(self):
        end = self.doc.Content.End - 1
        return self.doc.Range(end, end)

    def paragraph(self, text: str, style=None, alignment=None, first_line=None):
        start = self.doc.Content.End - 1
        place = self.end()
        place.InsertAfter(text)
        place.InsertParagraphAfter()
        written = self.doc.Range(start, start + len(text))
        if style is not None:
            written.Style = self.doc.Styles(style)
        if alignment is not None:
            written.ParagraphFormat.Alignment = alignment
        if first_line is not None:
            written.ParagraphFormat.FirstLineIndent = first_line
        return written

    def footnote_after(self, written, text: str) -> None:
        at = written.End
        self.doc.Footnotes.Add(Range=self.doc.Range(at, at), Text=text)

    def table(self, rows: int, columns: int, texts, autofit: int, header: bool = True):
        table = self.doc.Tables.Add(self.end(), rows, columns, 1, autofit)
        for r in range(rows):
            for c in range(columns):
                table.Cell(r + 1, c + 1).Range.Text = texts(r, c)
        if header:
            table.Rows(1).HeadingFormat = -1
        if autofit == 1:
            table.AutoFitBehavior(1)
        return table

    def save(self, path: Path) -> None:
        self.doc.SaveAs2(FileName=str(path), FileFormat=FORMAT_DOCX)
        self.doc.Close(SaveChanges=0)


def essay(app, defaults: str, seed: int, out: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    if defaults == "mla":
        header = b.doc.Sections(1).Headers(1).Range
        header.Text = "Smith "
        header.ParagraphFormat.Alignment = 2
        # the field goes at the end of the header's text, inside the header story
        place = b.doc.Sections(1).Headers(1).Range
        place.MoveEnd(1, -1)
        place.Collapse(0)
        b.doc.Fields.Add(place, FIELD_PAGE)
        for line in ("Jordan Smith", "Professor Alvarez", "English 101", "12 November 2026"):
            b.paragraph(line)
        b.paragraph(sentence(rng, 6), alignment=1)
        for _ in range(14):
            b.paragraph(prose(rng, 90 + rng.randrange(90)), first_line=36)
        b.paragraph("Works Cited", alignment=1)
        for _ in range(4):
            p = b.paragraph(prose(rng, 30))
            p.ParagraphFormat.LeftIndent = 36
            p.ParagraphFormat.FirstLineIndent = -36
    else:
        b.paragraph(sentence(rng, 5), style=STYLE_TITLE)
        for section in range(4):
            b.paragraph(sentence(rng, 4), style=STYLE_HEADING1)
            for _ in range(3):
                b.paragraph(prose(rng, 60 + rng.randrange(100)))
            if section == 1:
                for _ in range(4):
                    b.paragraph(prose(rng, 8 + rng.randrange(30)), style=STYLE_LIST_BULLET)
            if section == 2:
                for _ in range(4):
                    b.paragraph(prose(rng, 8 + rng.randrange(30)), style=STYLE_LIST_NUMBER)
                b.paragraph(sentence(rng, 3), style=STYLE_HEADING2)
                b.paragraph(prose(rng, 120))
    b.save(out)


def tables(app, defaults: str, seed: int, out: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    b.paragraph(prose(rng, 50))
    lengths = [[1 + rng.randrange(14) for _ in range(4)] for _ in range(40)]
    b.table(40, 4, lambda r, c: sentence(rng, lengths[r][c]) if r else f"Column {c + 1}", 0)
    b.paragraph(prose(rng, 40))
    widths = [[1 + rng.randrange(6) for _ in range(3)] for _ in range(8)]
    b.table(8, 3, lambda r, c: sentence(rng, widths[r][c] * (4 if c == 1 else 1)), 1)
    b.paragraph(prose(rng, 40))
    b.table(6, 5, lambda r, c: sentence(rng, 1 + rng.randrange(8)), 2)
    b.paragraph(prose(rng, 40))
    b.save(out)


def footnotes(app, defaults: str, seed: int, out: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    for i in range(16):
        written = b.paragraph(prose(rng, 50 + rng.randrange(60)), first_line=36)
        b.footnote_after(written, prose(rng, 10 + rng.randrange(150 if i % 6 == 3 else 40)))
    b.save(out)


def columns(app, defaults: str, seed: int, out: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    for i in range(20):
        written = b.paragraph(prose(rng, 50 + rng.randrange(80)), alignment=3)
        if i % 3 == 1:
            b.footnote_after(written, prose(rng, 10 + rng.randrange(30)))
    b.doc.PageSetup.TextColumns.SetCount(2)
    b.save(out)


def floating(app, defaults: str, seed: int, out: Path, picture: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    for i in range(6):
        written = b.paragraph(prose(rng, 110))
        anchor = written.Paragraphs(1).Range
        if i % 2 == 0:
            shape = b.doc.Shapes.AddPicture(str(picture), False, True, 72 + 60 * i, 20, 144, 108, anchor)
        else:
            shape = b.doc.Shapes.AddTextbox(1, 200, 10, 180, 70, anchor)
            shape.TextFrame.TextRange.Text = sentence(rng, 14)
        shape.WrapFormat.Type = WRAP_SQUARE
    b.save(out)


def lab_report(app, defaults: str, seed: int, out: Path, picture: Path) -> None:
    rng = random.Random(seed)
    b = Builder(app, defaults)
    b.paragraph(sentence(rng, 6), style=STYLE_TITLE)
    for index, part in enumerate(("Introduction", "Method", "Results", "Discussion")):
        b.paragraph(part, style=STYLE_HEADING1)
        b.paragraph(prose(rng, 80 + rng.randrange(60)))
        if index == 2:
            b.table(6, 4, lambda r, c: f"{rng.random() * 100:.2f}" if r else f"Trial {c + 1}", 1)
        figure = b.paragraph("", alignment=1)
        b.doc.InlineShapes.AddPicture(str(picture), False, True, b.doc.Range(figure.Start, figure.Start)).Width = 216 + 18 * index
        b.paragraph(f"Figure {index + 1}. {sentence(rng, 10)}", style=STYLE_CAPTION)
    b.save(out)


PLAN = [
    ("w_essay_aptos", essay, "aptos"),
    ("w_essay_calibri", essay, "calibri"),
    ("w_essay_mla", essay, "mla"),
    ("w_essay_aptos_b", essay, "aptos"),
    ("w_tables_aptos", tables, "aptos"),
    ("w_tables_calibri", tables, "calibri"),
    ("w_tables_mla", tables, "mla"),
    ("w_tables_aptos_b", tables, "aptos"),
    ("w_footnotes_mla", footnotes, "mla"),
    ("w_footnotes_aptos", footnotes, "aptos"),
    ("w_columns_aptos", columns, "aptos"),
    ("w_columns_calibri", columns, "calibri"),
    ("w_floating_aptos", floating, "aptos"),
    ("w_floating_calibri", floating, "calibri"),
    ("w_lab_calibri", lab_report, "calibri"),
    ("w_lab_aptos", lab_report, "aptos"),
]


def main(arguments: list[str]) -> int:
    out_dir = Path(arguments[0]).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    picture = out_dir / "figure.png"
    png(picture)
    before = process_ids("WINWORD.EXE")
    word = PrivateWord(timeout_seconds=300)
    manifest = []
    try:
        for index, (name, make, defaults) in enumerate(PLAN):
            target = out_dir / f"{name}.docx"
            word.arm()
            try:
                if make in (floating, lab_report):
                    make(word.app, defaults, 7000 + index, target, picture)
                else:
                    make(word.app, defaults, 7000 + index, target)
                status = "ok"
            except Exception as error:  # one failure must not stop the others
                status = f"error: {error}"
            finally:
                word.disarm()
            print(f"{name}: {status}", flush=True)
            # every second document of a kind is held out from fitting
            manifest.append({"name": name, "class": "word-authored", "heldOut": index % 2 == 1, "status": status})
    finally:
        word.quit()
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
    left = process_ids("WINWORD.EXE") - before
    print(json.dumps({"word pid": word.pid, "new WINWORD left running": sorted(left)}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
