"""makes documents inside libreoffice writer itself, from its default
template, and saves them as odt: files as libreoffice users have them.

run with libreoffice's own python (it has the uno bridge):
  <libreoffice>\\program\\python.exe author_lo.py <out dir>

safety: libreoffice runs headless with a private profile folder next to
the output, listening on a pipe named for this run only; the process this
script started is asked to quit and always waited for, and if it does not
end in time only its own process tree is ended. nothing in the person's
own profile or settings is touched.

writes <name>.odt files and manifest.json ({name, class, heldOut}).
"""

from __future__ import annotations

import json
import os
import random
import subprocess
import sys
import time
from pathlib import Path

import uno  # noqa: F401  (libreoffice's python only)
from com.sun.star.beans import PropertyValue

PROGRAM = Path(sys.executable).resolve().parent
SOFFICE = PROGRAM / "soffice.com"

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

PARAGRAPH_BREAK = 0  # com.sun.star.text.ControlCharacter.PARAGRAPH_BREAK
JUSTIFIED = 2  # com.sun.star.style.ParagraphAdjust.BLOCK


def sentence(rng: random.Random, words: int) -> str:
    chosen = [rng.choice(WORDS) for _ in range(words)]
    chosen[0] = chosen[0][:1].upper() + chosen[0][1:]
    return " ".join(chosen) + "."


def prose(rng: random.Random, words: int) -> str:
    out, count = [], 0
    while count < words:
        length = 6 + rng.randrange(17)
        out.append(sentence(rng, length))
        count += length
    return " ".join(out)


def prop(name: str, value) -> PropertyValue:
    item = PropertyValue()
    item.Name = name
    item.Value = value
    return item


class Writer:
    """one new writer document, filled from its end."""

    def __init__(self, desktop):
        self.doc = desktop.loadComponentFromURL("private:factory/swriter", "_blank", 0, (prop("Hidden", True),))
        self.text = self.doc.getText()
        self.cursor = self.text.createTextCursor()
        self.first = True

    def styles_named(self, family: str):
        return self.doc.getStyleFamilies().getByName(family)

    def paragraph(self, text: str, style: str | None = None, justified: bool = False):
        if not self.first:
            self.text.insertControlCharacter(self.cursor, PARAGRAPH_BREAK, False)
        self.first = False
        if style:
            try:
                self.cursor.setPropertyValue("ParaStyleName", style)
            except Exception:
                pass
        else:
            self.cursor.setPropertyValue("ParaStyleName", "Standard")
        self.cursor.setPropertyValue("ParaAdjust", JUSTIFIED if justified else 0)
        self.text.insertString(self.cursor, text, False)

    def footnote(self, text: str) -> None:
        note = self.doc.createInstance("com.sun.star.text.Footnote")
        self.text.insertTextContent(self.cursor, note, False)
        note.getText().setString(text)

    def table(self, rows: int, columns: int, texts, repeat_header: bool = True) -> None:
        self.text.insertControlCharacter(self.cursor, PARAGRAPH_BREAK, False)
        table = self.doc.createInstance("com.sun.star.text.TextTable")
        table.initialize(rows, columns)
        self.text.insertTextContent(self.cursor, table, False)
        for r in range(rows):
            for c in range(columns):
                cell = table.getCellByPosition(c, r)
                cell.setString(texts(r, c))
        table.setPropertyValue("RepeatHeadline", repeat_header)
        self.first = False

    def page_style(self):
        return self.styles_named("PageStyles").getByName("Standard")

    def save(self, path: Path) -> None:
        self.doc.storeToURL(path.resolve().as_uri(), (prop("FilterName", "writer8"),))
        self.doc.close(True)


def essay(w: Writer, rng: random.Random, variant: str) -> None:
    body = "Text Body" if variant == "body" else None
    w.paragraph(sentence(rng, 5), "Title")
    for section in range(4):
        w.paragraph(sentence(rng, 4), "Heading 1")
        for _ in range(3):
            w.paragraph(prose(rng, 60 + rng.randrange(100)), body, justified=variant == "justified")
        if section == 1:
            for _ in range(4):
                w.paragraph(prose(rng, 8 + rng.randrange(30)), "List Bullet")
        if section == 2:
            w.paragraph(sentence(rng, 3), "Heading 2")
            w.paragraph(prose(rng, 120), body)


def sans(w: Writer, rng: random.Random, variant: str) -> None:
    # the cursor's font applies to the text inserted after it is set
    w.cursor.setPropertyValue("CharFontName", "Liberation Sans")
    for _ in range(10):
        w.paragraph(prose(rng, 70 + rng.randrange(90)))
    del variant


def tables(w: Writer, rng: random.Random, variant: str) -> None:
    w.paragraph(prose(rng, 50))
    lengths = [[1 + rng.randrange(14) for _ in range(4)] for _ in range(36)]
    w.table(36, 4, lambda r, c: sentence(rng, lengths[r][c]) if r else f"Column {c + 1}")
    w.paragraph(prose(rng, 40))
    w.table(6, 3, lambda r, c: sentence(rng, 1 + rng.randrange(6)))
    w.paragraph(prose(rng, 40))
    del variant


def footnotes(w: Writer, rng: random.Random, variant: str) -> None:
    for i in range(16):
        w.paragraph(prose(rng, 50 + rng.randrange(60)), "Text Body" if variant == "body" else None)
        w.footnote(prose(rng, 10 + rng.randrange(150 if i % 6 == 3 else 40)))


def columns(w: Writer, rng: random.Random, variant: str) -> None:
    for i in range(20):
        w.paragraph(prose(rng, 50 + rng.randrange(80)), justified=True)
        if i % 3 == 1 and variant == "notes":
            w.footnote(prose(rng, 10 + rng.randrange(30)))
    style = w.page_style()
    columns_object = style.getPropertyValue("TextColumns")
    columns_object.setColumnCount(2)
    style.setPropertyValue("TextColumns", columns_object)


def headers(w: Writer, rng: random.Random, variant: str) -> None:
    style = w.page_style()
    style.setPropertyValue("HeaderIsOn", True)
    header = style.getPropertyValue("HeaderText")
    header.setString("Smith ")
    field = w.doc.createInstance("com.sun.star.text.TextField.PageNumber")
    field.setPropertyValue("NumberingType", 4)  # arabic
    field.setPropertyValue("SubType", 1)  # current page
    header.insertTextContent(header.getEnd(), field, False)
    if variant == "footer":
        style.setPropertyValue("FooterIsOn", True)
        style.getPropertyValue("FooterText").setString(sentence(rng, 8))
    for _ in range(14):
        w.paragraph(prose(rng, 80 + rng.randrange(80)))


PLAN = [
    ("lo_essay_default", essay, "default"),
    ("lo_essay_body", essay, "body"),
    ("lo_essay_justified", essay, "justified"),
    ("lo_sans", sans, "default"),
    ("lo_tables", tables, "default"),
    ("lo_tables_b", tables, "default"),
    ("lo_footnotes", footnotes, "default"),
    ("lo_footnotes_body", footnotes, "body"),
    ("lo_columns", columns, "plain"),
    ("lo_columns_notes", columns, "notes"),
    ("lo_headers", headers, "header"),
    ("lo_headers_footer", headers, "footer"),
]


def connect(pipe: str, deadline: float):
    local = uno.getComponentContext()
    resolver = local.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", local)
    while True:
        try:
            context = resolver.resolve(f"uno:pipe,name={pipe};urp;StarOffice.ComponentContext")
            return context.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", context)
        except Exception:
            if time.monotonic() > deadline:
                raise
            time.sleep(0.5)


def main(arguments: list[str]) -> int:
    out_dir = Path(arguments[0]).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    profile = (out_dir.parent / "lo_author_profile").resolve()
    profile.mkdir(parents=True, exist_ok=True)
    pipe = f"letters_author_{os.getpid()}"
    command = [
        str(SOFFICE),
        f"-env:UserInstallation={profile.as_uri()}",
        "--headless",
        "--invisible",
        "--norestore",
        "--nologo",
        f"--accept=pipe,name={pipe};urp;",
    ]
    process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    manifest = []
    try:
        desktop = connect(pipe, time.monotonic() + 90)
        for index, (name, make, variant) in enumerate(PLAN):
            try:
                writer = Writer(desktop)
                make(writer, random.Random(9000 + index), variant)
                writer.save(out_dir / f"{name}.odt")
                status = "ok"
            except Exception as error:
                status = f"error: {error}"
            print(f"{name}: {status}", flush=True)
            manifest.append({"name": name, "class": "libreoffice-authored", "heldOut": index % 2 == 1, "status": status})
        try:
            desktop.terminate()
        except Exception:
            pass  # the bridge goes away as it quits
    finally:
        try:
            process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(process.pid)], capture_output=True, check=False)
            process.wait()
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(json.dumps({"libreoffice pid": process.pid, "exit": process.returncode}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
