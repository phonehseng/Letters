"""the converter's docx package: frames for lines, anchored drawings for the rest.

per page: one 1 pt paragraph that carries every floating drawing of the page
(pictures, shapes, rasterised vectors, rotated text boxes, all behind text),
then one framed paragraph per line. pages of the same size share a section;
a change of size starts a new section, so each page keeps its own size.
"""

from __future__ import annotations

import math
import zipfile
from dataclasses import dataclass, field
from xml.sax.saxutils import escape

import docx_writer as dw

EMU_PER_PT = 12700
NS = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" '
    'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" '
    'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
    'xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" mc:Ignorable="w14"'
)
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
MAIN = "application/vnd.openxmlformats-officedocument.wordprocessingml"


def emu(points: float) -> int:
    return round(points * EMU_PER_PT)


def hex_color(rgb: tuple[float, float, float] | None) -> str:
    rgb = rgb or (0.0, 0.0, 0.0)
    return "".join(f"{max(0, min(255, round(c * 255))):02X}" for c in rgb)


@dataclass
class CharRun:
    char: str
    family: str
    half_points: int
    color: str
    spacing_twips: int
    hidden: bool


# neighbouring runs that differ in any property become separate text objects in
# word's pdf (e11), and only a glyph that starts a text object is printed exactly
# where word_writer_model says. the east asian language is the one property that
# changes nothing for latin text, so it alternates from glyph to glyph.
SPLIT_LANGUAGES = ("en-US", "en-GB")


def runs_xml(runs: list[CharRun]) -> str:
    parts = []
    for index, run in enumerate(runs):
        props = [
            f'<w:rFonts w:ascii="{run.family}" w:hAnsi="{run.family}" w:cs="{run.family}" w:eastAsia="{run.family}"/>'
        ]
        if run.hidden:
            props.append("<w:vanish/>")
        if run.color != "000000":
            props.append(f'<w:color w:val="{run.color}"/>')
        if run.spacing_twips:
            props.append(f'<w:spacing w:val="{run.spacing_twips}"/>')
        props.append(
            f'<w:sz w:val="{run.half_points}"/><w:szCs w:val="{run.half_points}"/>'
            f'<w:lang w:eastAsia="{SPLIT_LANGUAGES[index % 2]}"/><w14:ligatures w14:val="none"/>'
        )
        parts.append(f'<w:r><w:rPr>{"".join(props)}</w:rPr><w:t xml:space="preserve">{escape(run.char)}</w:t></w:r>')
    return "".join(parts)


def line_paragraph_props(line_twips: int) -> str:
    return (
        f"{dw.LAYOUT_NEUTRAL_PPR}"
        f'<w:spacing w:before="0" w:after="0" w:line="{line_twips}" w:lineRule="exact"/>'
        '<w:ind w:left="0" w:right="0" w:firstLine="0"/>'
    )


def frame_paragraph(runs: list[CharRun], x_twips: int, y_twips: int, width_twips: int, line_twips: int) -> str:
    frame = (
        f'<w:framePr w:w="{width_twips}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" '
        f'w:x="{x_twips}" w:y="{y_twips}"/>'
    )
    return f"<w:p><w:pPr>{frame}{line_paragraph_props(line_twips)}</w:pPr>{runs_xml(runs)}</w:p>"


def _anchor(
    shape_id: int,
    left_pt: float,
    top_pt: float,
    width_pt: float,
    height_pt: float,
    graphic: str,
    behind: bool,
    rotation_deg: float = 0.0,
) -> str:
    return (
        f'<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{251658240 + shape_id}" '
        f'behindDoc="{1 if behind else 0}" locked="1" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="page"><wp:posOffset>{emu(left_pt)}</wp:posOffset></wp:positionH>'
        f'<wp:positionV relativeFrom="page"><wp:posOffset>{emu(top_pt)}</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{max(1, emu(width_pt))}" cy="{max(1, emu(height_pt))}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>'
        f'<wp:wrapNone/><wp:docPr id="{shape_id}" name="Item {shape_id}"/><wp:cNvGraphicFramePr/>{graphic}</wp:anchor>'
    )


def _xfrm(width_pt: float, height_pt: float, rotation_deg: float = 0.0, flip_h=False, flip_v=False) -> str:
    rot = f' rot="{round(rotation_deg * 60000) % 21600000}"' if rotation_deg else ""
    flips = (' flipH="1"' if flip_h else "") + (' flipV="1"' if flip_v else "")
    return f'<a:xfrm{rot}{flips}><a:off x="0" y="0"/><a:ext cx="{max(1, emu(width_pt))}" cy="{max(1, emu(height_pt))}"/></a:xfrm>'


def picture_drawing(
    shape_id: int,
    relationship: str,
    left_pt: float,
    top_pt: float,
    width_pt: float,
    height_pt: float,
    rotation_deg: float = 0.0,
    flip_h=False,
    flip_v=False,
) -> str:
    graphic = (
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic>'
        f'<pic:nvPicPr><pic:cNvPr id="{shape_id}" name="Picture {shape_id}"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{relationship}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr>{_xfrm(width_pt, height_pt, rotation_deg, flip_h, flip_v)}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
        "</pic:pic></a:graphicData></a:graphic>"
    )
    return f"<w:r><w:drawing>{_anchor(shape_id, left_pt, top_pt, width_pt, height_pt, graphic, True)}</w:drawing></w:r>"


def _wps(
    shape_id: int,
    left_pt,
    top_pt,
    width_pt,
    height_pt,
    sp_pr: str,
    body: str = "<wps:bodyPr/>",
    text: str = "",
    behind=True,
) -> str:
    graphic = (
        '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
        f"<wps:wsp><wps:cNvSpPr{' txBox="1"' if text else ''}/><wps:spPr>{sp_pr}</wps:spPr>{text}{body}</wps:wsp>"
        "</a:graphicData></a:graphic>"
    )
    return (
        '<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>'
        f"{_anchor(shape_id, left_pt, top_pt, width_pt, height_pt, graphic, behind)}"
        "</w:drawing></mc:Choice></mc:AlternateContent></w:r>"
    )


def rect_drawing(
    shape_id: int, left_pt, top_pt, width_pt, height_pt, fill: str | None, line: str | None, line_pt: float
) -> str:
    fill_xml = f'<a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>' if fill else "<a:noFill/>"
    line_xml = (
        f'<a:ln w="{emu(line_pt)}"><a:solidFill><a:srgbClr val="{line}"/></a:solidFill></a:ln>'
        if line
        else "<a:ln><a:noFill/></a:ln>"
    )
    sp_pr = f'{_xfrm(width_pt, height_pt)}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill_xml}{line_xml}'
    return _wps(shape_id, left_pt, top_pt, width_pt, height_pt, sp_pr)


def line_drawing(shape_id: int, x0, y0, x1, y1, color: str, line_pt: float) -> str:
    """a straight segment; coordinates top-down in points from the page's top left."""
    left, top = min(x0, x1), min(y0, y1)
    width, height = abs(x1 - x0), abs(y1 - y0)
    flip_h = x1 < x0
    flip_v = (y1 < y0) != flip_h
    sp_pr = (
        f'{_xfrm(width, height, flip_h=False, flip_v=flip_v)}<a:prstGeom prst="line"><a:avLst/></a:prstGeom>'
        f'<a:ln w="{emu(line_pt)}"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:ln>'
    )
    return _wps(shape_id, left, top, width, height, sp_pr)


def rotated_text_drawing(
    shape_id: int,
    runs: list[CharRun],
    origin_top_down: tuple[float, float],
    angle_deg: float,
    width_pt: float,
    line_twips: int,
    baseline_pt: float,
) -> str:
    """a text box rotated about its centre so its first pen origin lands on origin_top_down.

    frames cannot rotate, so rotated pdf text goes into a drawingml text box with
    zero insets; word turns shapes clockwise, pdf angles are counter-clockwise."""
    height = line_twips / dw.TWIPS_PER_PT
    clockwise = -angle_deg
    radians = math.radians(clockwise)
    # start point relative to the box centre, before rotation
    sx, sy = -width_pt / 2, baseline_pt - height / 2
    rx = sx * math.cos(radians) - sy * math.sin(radians)
    ry = sx * math.sin(radians) + sy * math.cos(radians)
    centre_x, centre_y = origin_top_down[0] - rx, origin_top_down[1] - ry
    left, top = centre_x - width_pt / 2, centre_y - height / 2
    paragraph = f"<w:p><w:pPr>{line_paragraph_props(line_twips)}</w:pPr>{runs_xml(runs)}</w:p>"
    sp_pr = f'{_xfrm(width_pt, height, clockwise)}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln>'
    # wrap="square": with wrap="none" word moved the line's start whenever its content got
    # wider or narrower (round 4, slides), so a re-planned line never settled; the box is
    # 20 pt wider than the line, so square wrapping never breaks it
    body = '<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t" upright="0"><a:noAutofit/></wps:bodyPr>'
    text = f"<wps:txbx><w:txbxContent>{paragraph}</w:txbxContent></wps:txbx>"
    return _wps(shape_id, left, top, width_pt, height, sp_pr, body, text, behind=False)


def anchor_paragraph(drawings: str) -> str:
    return (
        f"<w:p><w:pPr>{dw.LAYOUT_NEUTRAL_PPR}"
        '<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/><w:rPr><w:sz w:val="2"/></w:rPr></w:pPr>'
        f"{drawings}</w:p>"
    )


def section_properties(width_pt: float, height_pt: float) -> str:
    orient = ' w:orient="landscape"' if width_pt > height_pt else ""
    return (
        f'<w:sectPr><w:pgSz w:w="{round(width_pt * 20)}" w:h="{round(height_pt * 20)}"{orient}/>'
        '<w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/>'
        '<w:cols w:space="0"/><w:docGrid w:linePitch="360"/></w:sectPr>'
    )


def section_break_paragraph(width_pt: float, height_pt: float) -> str:
    """ends a section of this page size; the next page starts a new one."""
    return (
        f"<w:p><w:pPr>{dw.LAYOUT_NEUTRAL_PPR}"
        '<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/>'
        f'{section_properties(width_pt, height_pt)}<w:rPr><w:sz w:val="2"/></w:rPr></w:pPr></w:p>'
    )


@dataclass
class Package:
    body: list[str] = field(default_factory=list)
    fonts: list[dw.EmbeddedFont] = field(default_factory=list)
    media: list[tuple[str, bytes]] = field(default_factory=list)
    final_section: str = ""
    next_shape_id: int = 1

    def shape_id(self) -> int:
        self.next_shape_id += 1
        return self.next_shape_id

    def add_media(self, extension: str, data: bytes) -> str:
        self.media.append((f"image{len(self.media) + 1}.{extension}", data))
        return f"rIdImage{len(self.media)}"

    def save(self, path: str) -> str:
        document = (
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>'
            + "".join(self.body)
            + self.final_section
            + "</w:body></w:document>"
        )
        family = self.fonts[0].family if self.fonts else "Arial"
        font_table, font_rels = dw._font_table_xml(family, self.fonts)
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
            package.writestr("[Content_Types].xml", self._content_types())
            package.writestr("_rels/.rels", dw.PACKAGE_RELS)
            package.writestr("word/document.xml", document)
            package.writestr("word/_rels/document.xml.rels", self._document_rels())
            package.writestr("word/styles.xml", dw._styles_xml(family, 10))
            package.writestr("word/settings.xml", dw._settings_xml(bool(self.fonts)))
            package.writestr("word/fontTable.xml", font_table)
            if self.fonts:
                package.writestr("word/_rels/fontTable.xml.rels", font_rels)
            for number, font in enumerate(self.fonts, start=1):
                package.writestr(f"word/fonts/font{number}.odttf", dw.obfuscate_font(font.font_bytes, font.font_key))
            for name, data in self.media:
                package.writestr(f"word/media/{name}", data)
        return path

    def _content_types(self) -> str:
        return (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/>'
            f'<Default Extension="odttf" ContentType="{dw.OBFUSCATED_FONT_TYPE}"/>'
            f'<Override PartName="/word/document.xml" ContentType="{MAIN}.document.main+xml"/>'
            f'<Override PartName="/word/styles.xml" ContentType="{MAIN}.styles+xml"/>'
            f'<Override PartName="/word/settings.xml" ContentType="{MAIN}.settings+xml"/>'
            f'<Override PartName="/word/fontTable.xml" ContentType="{MAIN}.fontTable+xml"/>'
            "</Types>"
        )

    def _document_rels(self) -> str:
        rels = [
            f'<Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/>',
            f'<Relationship Id="rId2" Type="{REL}/settings" Target="settings.xml"/>',
            f'<Relationship Id="rId3" Type="{REL}/fontTable" Target="fontTable.xml"/>',
        ]
        for index, (name, _) in enumerate(self.media, start=1):
            rels.append(f'<Relationship Id="rIdImage{index}" Type="{REL}/image" Target="media/{name}"/>')
        return (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            + "".join(rels)
            + "</Relationships>"
        )
