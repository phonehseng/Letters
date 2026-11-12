"""rewrites the prototype's docx: each line's per-character runs become fitted runs, fonts become fitted fonts.

everything else the prototype wrote (frame positions, pictures, shapes,
rotated text boxes, sections) is kept byte for byte, so any difference in
the result comes from the fitted font alone. frames appear in the document
in the order of the prototype's horizontal lines, and rotated lines' text
box paragraphs in the order of its rotated lines; their text is checked
against ours before anything is replaced.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import dataclasses
import uuid
import zipfile

from lxml import etree

import docx_writer as dw
import fitted_docx as fd
from fitted_pieces import Run
from fitted_plan import run_features
from prototype.docx_out import SPLIT_LANGUAGES

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PACKAGE_REL = "http://schemas.openxmlformats.org/package/2006/relationships"
NS = {"w": W, "r": R}


class PrototypePackage:
    def __init__(self, docx_path: str):
        with zipfile.ZipFile(docx_path) as package:
            self.parts = {name: package.read(name) for name in package.namelist()}
        self.document = etree.fromstring(self.parts["word/document.xml"])
        self.font_table = etree.fromstring(self.parts["word/fontTable.xml"])
        self.font_rels = etree.fromstring(self.parts["word/_rels/fontTable.xml.rels"])

    def line_paragraphs(self) -> tuple[list, list]:
        """(frame paragraphs, rotated text box paragraphs), each in document order."""
        frames = self.document.xpath("//w:body/w:p[w:pPr/w:framePr]", namespaces=NS)
        boxes = self.document.xpath("//w:txbxContent/w:p", namespaces=NS)
        return frames, boxes

    @staticmethod
    def paragraph_text(paragraph) -> str:
        return "".join(paragraph.xpath(".//w:r/w:t/text()", namespaces=NS))

    def replace_runs(self, paragraph, runs: list[Run], family_name) -> None:
        """one w:r per piece, alternating the east asian language from piece to piece.

        the language changes nothing for latin text, but word starts a new pdf
        text object at every run whose properties differ (recipe 6.4), so each
        piece's glyph is placed by its own Tm. inside one text object word places
        later glyphs with a Tc it carries over from earlier objects: editing a
        different line changed that Tc from -1.03 to -0.024 pt and moved an
        untouched piece by 1.0 pt (cff_font, first corpus run). shaping still
        runs across these runs, so chained rules keep working (c1: 'Hi'+'ppo')."""
        for old in paragraph.xpath("./w:r", namespaces=NS):
            paragraph.remove(old)
        for run in runs:
            first = run.items[0]
            features = run_features(run.feature) if run.pieces else fd.RunFeatures(ligatures="none")
            texts = [piece.text for piece in run.pieces] or [run.text]
            for number, text in enumerate(texts):
                language = SPLIT_LANGUAGES[number % 2]
                piece_features = dataclasses.replace(features, east_asian_lang=language)
                text_run = fd.TextRun(text, family_name(run), first.half_points, first.color, first.hidden,
                                      features=piece_features)  # fmt: skip
                markup = f'<root xmlns:w="{W}" xmlns:w14="{fd.W14}">{fd.run_xml(text_run)}</root>'
                for element in etree.fromstring(markup):
                    paragraph.append(element)

    def _font_entry(self, family: str):
        found = self.font_table.xpath(f'w:font[@w:name="{family}"]', namespaces=NS)
        return found[0] if found else None

    def set_font(self, family: str, font_bytes: bytes) -> None:
        """replace an embedded font's program, or embed a new family."""
        entry = self._font_entry(family)
        if entry is not None:
            embed = entry.find(f"{{{W}}}embedRegular")
            key = embed.get(f"{{{W}}}fontKey")
            target = self._rel_target(embed.get(f"{{{R}}}id"))
            self.parts["word/" + target] = dw.obfuscate_font(font_bytes, key)
            return
        number = len(self.font_rels) + 1
        rel_id, key, target = f"rIdFitted{number}", "{" + str(uuid.uuid4()).upper() + "}", f"fonts/fitted{number}.odttf"
        etree.SubElement(self.font_rels, f"{{{PACKAGE_REL}}}Relationship",
                         Id=rel_id, Type=dw.REL_FONT, Target=target)  # fmt: skip
        entry = etree.SubElement(self.font_table, f"{{{W}}}font", {f"{{{W}}}name": family})
        for tag, value in (("charset", "00"), ("family", "roman"), ("pitch", "variable")):
            etree.SubElement(entry, f"{{{W}}}{tag}", {f"{{{W}}}val": value})
        etree.SubElement(entry, f"{{{W}}}embedRegular", {f"{{{R}}}id": rel_id, f"{{{W}}}fontKey": key})
        self.parts["word/" + target] = dw.obfuscate_font(font_bytes, key)

    def _rel_target(self, rel_id: str) -> str:
        for relationship in self.font_rels:
            if relationship.get("Id") == rel_id:
                return relationship.get("Target")
        raise KeyError(f"fontTable.xml.rels has no relationship {rel_id}")

    def save(self, path: str) -> str:
        self.parts["word/document.xml"] = etree.tostring(self.document, xml_declaration=True, encoding="UTF-8",
                                                         standalone=True)  # fmt: skip
        self.parts["word/fontTable.xml"] = etree.tostring(self.font_table, xml_declaration=True, encoding="UTF-8",
                                                          standalone=True)  # fmt: skip
        self.parts["word/_rels/fontTable.xml.rels"] = etree.tostring(self.font_rels, xml_declaration=True,
                                                                     encoding="UTF-8", standalone=True)  # fmt: skip
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
            for name, data in self.parts.items():
                package.writestr(name, data)
        return path
