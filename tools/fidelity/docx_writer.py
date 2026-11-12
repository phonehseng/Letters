"""a minimal hand-written docx package, so every byte word reads is one we chose.

no python-docx: its templates bring styles, themes and settings that would
change layout behind our back. units used below:
    twip = 1/20 pt (most of wordprocessingml)
    half-point = 1/2 pt (w:sz font size)
    emu = 1/12700 pt (drawingml)
"""

from __future__ import annotations

import uuid
import zipfile
from dataclasses import dataclass, field
from xml.sax.saxutils import escape

TWIPS_PER_PT = 20
EMU_PER_PT = 12700

NAMESPACES = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" '
    'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
    'xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" '
    'xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" '
    'mc:Ignorable="w14 wp14"'
)

REL_FONT = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font"
OBFUSCATED_FONT_TYPE = "application/vnd.openxmlformats-officedocument.obfuscatedFont"


def pt_to_twips(points: float) -> int:
    return round(points * TWIPS_PER_PT)


def pt_to_emu(points: float) -> int:
    return round(points * EMU_PER_PT)


def size_to_half_points(points: float) -> int:
    half_points = points * 2
    if abs(half_points - round(half_points)) > 1e-9:
        # word cannot store this size, the caller must know the text will not match
        raise ValueError(f"font size {points} pt is not a multiple of 0.5 pt")
    return round(half_points)


# ---------------------------------------------------------------- font embedding


def obfuscate_font(font_bytes: bytes, font_key: str) -> bytes:
    """ecma-376 part 1, 17.8.1: xor the first 32 bytes with the font key.

    the key is the guid's 16 bytes read from its last hex pair to its first."""
    hex_digits = font_key.strip("{}").replace("-", "")
    key = bytes.fromhex(hex_digits)[::-1]
    head = bytes(font_bytes[i] ^ key[i % 16] for i in range(32))
    return head + font_bytes[32:]


@dataclass
class EmbeddedFont:
    family: str
    font_bytes: bytes
    font_key: str = field(default_factory=lambda: "{" + str(uuid.uuid4()).upper() + "}")


# ---------------------------------------------------------------- run and paragraph xml

LAYOUT_NEUTRAL_PPR = (
    # each of these switches off something word would otherwise adjust on its own
    '<w:widowControl w:val="0"/>'
    "<w:suppressAutoHyphens/>"
    '<w:snapToGrid w:val="0"/>'
    '<w:autoSpaceDE w:val="0"/>'
    '<w:autoSpaceDN w:val="0"/>'
    '<w:adjustRightInd w:val="0"/>'
)


def run_xml(text: str, family: str, size_pt: float, spacing_twips: int | None = None) -> str:
    """one run of text. there is deliberately no w:kern: per the spec its presence
    switches kerning on for sizes at or above its value, so w:kern w:val="0" would
    mean kern everything. (with the gpos-only test font e3 measured no difference.)"""
    half_points = size_to_half_points(size_pt)
    spacing = f'<w:spacing w:val="{spacing_twips}"/>' if spacing_twips else ""
    return (
        "<w:r><w:rPr>"
        f'<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>'
        f'{spacing}<w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/>'
        '<w14:ligatures w14:val="none"/>'
        f'</w:rPr><w:t xml:space="preserve">{escape(text)}</w:t></w:r>'
    )


def tab_run_xml() -> str:
    return "<w:r><w:tab/></w:r>"


def paragraph_xml(
    runs: str,
    *,
    extra_ppr: str = "",
    line_twips: int,
    before_twips: int = 0,
    indent_twips: int = 0,
    tab_stops_twips: tuple[int, ...] = (),
) -> str:
    tabs = ""
    if tab_stops_twips:
        tabs = "<w:tabs>" + "".join(f'<w:tab w:val="left" w:pos="{p}"/>' for p in tab_stops_twips) + "</w:tabs>"
    return (
        "<w:p><w:pPr>"
        f"{extra_ppr}{LAYOUT_NEUTRAL_PPR}{tabs}"
        f'<w:spacing w:before="{before_twips}" w:after="0" w:line="{line_twips}" w:lineRule="exact"/>'
        f'<w:ind w:left="{indent_twips}" w:right="0" w:firstLine="0"/>'
        f"</w:pPr>{runs}</w:p>"
    )


PAGE_BREAK_PARAGRAPH_TWIPS = 20


def page_break_paragraph_xml() -> str:
    """a tiny empty paragraph that starts the next page.

    it is 1 pt high so it cannot itself spill onto another page."""
    return (
        "<w:p><w:pPr><w:pageBreakBefore/>"
        f"{LAYOUT_NEUTRAL_PPR}"
        f'<w:spacing w:before="0" w:after="0" w:line="{PAGE_BREAK_PARAGRAPH_TWIPS}" w:lineRule="exact"/>'
        '<w:rPr><w:sz w:val="2"/></w:rPr></w:pPr></w:p>'
    )


def frame_paragraph_xml(runs: str, *, x_twips: int, y_twips: int, width_twips: int, line_twips: int) -> str:
    """a paragraph in a frame positioned against the page edges.

    frames hold their own position, so body flow cannot move them."""
    frame = (
        f'<w:framePr w:w="{width_twips}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" '
        f'w:x="{x_twips}" w:y="{y_twips}"/>'
    )
    return paragraph_xml(runs, extra_ppr=frame, line_twips=line_twips)


def textbox_run_xml(
    inner_paragraphs: str, *, x_emu: int, y_emu: int, width_emu: int, height_emu: int, shape_id: int
) -> str:
    """a drawingml text box anchored to the page, no insets, no autofit, no wrapping."""
    return (
        '<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>'
        f'<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{shape_id + 251658240}" '
        'behindDoc="0" locked="1" layoutInCell="0" allowOverlap="1">'
        '<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="page"><wp:posOffset>{x_emu}</wp:posOffset></wp:positionH>'
        f'<wp:positionV relativeFrom="page"><wp:posOffset>{y_emu}</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{width_emu}" cy="{height_emu}"/>'
        '<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>'
        f'<wp:docPr id="{shape_id}" name="Text Box {shape_id}"/><wp:cNvGraphicFramePr/>'
        '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
        '<wps:wsp><wps:cNvSpPr txBox="1"/>'
        f'<wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{width_emu}" cy="{height_emu}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr>'
        f"<wps:txbx><w:txbxContent>{inner_paragraphs}</w:txbxContent></wps:txbx>"
        '<wps:bodyPr rot="0" vert="horz" wrap="none" lIns="0" tIns="0" rIns="0" bIns="0" '
        'anchor="t" anchorCtr="0" upright="1"><a:noAutofit/></wps:bodyPr>'
        "</wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice></mc:AlternateContent></w:r>"
    )


def anchor_paragraph_xml(runs: str, page_break_before: bool) -> str:
    """an empty-looking body paragraph that only carries anchored drawings."""
    break_xml = "<w:pageBreakBefore/>" if page_break_before else ""
    return (
        f"<w:p><w:pPr>{break_xml}{LAYOUT_NEUTRAL_PPR}"
        '<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr>'
        f"{runs}</w:p>"
    )


# ---------------------------------------------------------------- package parts


def _section_xml(page_width_pt: float, page_height_pt: float) -> str:
    # zero margins so indents and spacing are measured from the page edge
    return (
        "<w:sectPr>"
        f'<w:pgSz w:w="{pt_to_twips(page_width_pt)}" w:h="{pt_to_twips(page_height_pt)}"/>'
        '<w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/>'
        '<w:cols w:space="0"/><w:docGrid w:linePitch="360"/>'
        "</w:sectPr>"
    )


def _styles_xml(family: str, size_pt: float) -> str:
    half_points = size_to_half_points(size_pt)
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NAMESPACES}>'
        "<w:docDefaults><w:rPrDefault><w:rPr>"
        f'<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>'
        f'<w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/>'
        '<w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/><w14:ligatures w14:val="none"/>'
        "</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>"
        f"{LAYOUT_NEUTRAL_PPR}"
        '<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
        "</w:pPr></w:pPrDefault></w:docDefaults>"
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
        "</w:styles>"
    )


def _settings_xml(embed_fonts: bool, compatibility_mode: int = 15, extra_compat_xml: str = "") -> str:
    """document settings. extra_compat_xml holds legacy w:compat flags such as
    <w:usePrinterMetrics/>, which the schema wants before the compatSetting entries."""
    embed = "<w:embedTrueTypeFonts/>" if embed_fonts else ""
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {NAMESPACES}>'
        f'{embed}<w:defaultTabStop w:val="720"/><w:autoHyphenation w:val="false"/>'
        '<w:characterSpacingControl w:val="doNotCompress"/>'
        f"<w:compat>{extra_compat_xml}"
        '<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" '
        f'w:val="{compatibility_mode}"/>'
        "</w:compat></w:settings>"
    )


def _font_table_xml(family: str, embedded: list[EmbeddedFont]) -> tuple[str, str]:
    """one w:font per family, embedded ones point at their own obfuscated part."""
    entries = []
    relationships = []
    families = [font.family for font in embedded]
    if family not in families:
        families.insert(0, family)
    for name in families:
        embed_xml = ""
        for number, font in enumerate(embedded, start=1):
            if font.family == name:
                embed_xml = f'<w:embedRegular r:id="rIdFont{number}" w:fontKey="{font.font_key}"/>'
                relationships.append(
                    f'<Relationship Id="rIdFont{number}" Type="{REL_FONT}" Target="fonts/font{number}.odttf"/>'
                )
        entries.append(
            f'<w:font w:name="{name}"><w:charset w:val="00"/><w:family w:val="roman"/>'
            f'<w:pitch w:val="variable"/>{embed_xml}</w:font>'
        )
    table = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts {NAMESPACES}>'
        + "".join(entries)
        + "</w:fonts>"
    )
    rels_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + "".join(relationships)
        + "</Relationships>"
    )
    return table, rels_xml


def _content_types_xml(has_font: bool) -> str:
    font_default = f'<Default Extension="odttf" ContentType="{OBFUSCATED_FONT_TYPE}"/>' if has_font else ""
    main = "application/vnd.openxmlformats-officedocument.wordprocessingml"
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        f"{font_default}"
        f'<Override PartName="/word/document.xml" ContentType="{main}.document.main+xml"/>'
        f'<Override PartName="/word/styles.xml" ContentType="{main}.styles+xml"/>'
        f'<Override PartName="/word/settings.xml" ContentType="{main}.settings+xml"/>'
        f'<Override PartName="/word/fontTable.xml" ContentType="{main}.fontTable+xml"/>'
        "</Types>"
    )


PACKAGE_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
    'Target="word/document.xml"/></Relationships>'
)

OFFICE_RELS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
DOCUMENT_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    f'<Relationship Id="rId1" Type="{OFFICE_RELS}/styles" Target="styles.xml"/>'
    f'<Relationship Id="rId2" Type="{OFFICE_RELS}/settings" Target="settings.xml"/>'
    f'<Relationship Id="rId3" Type="{OFFICE_RELS}/fontTable" Target="fontTable.xml"/>'
    "</Relationships>"
)


def write_docx(
    path: str,
    body_xml: str,
    *,
    family: str,
    default_size_pt: float,
    embedded: EmbeddedFont | list[EmbeddedFont] | None,
    page_width_pt: float = 612,
    page_height_pt: float = 792,
    embed_flag: bool | None = None,
    compatibility_mode: int = 15,
    extra_compat_xml: str = "",
) -> str:
    """write the package. body_xml is a run of w:p elements, the section is added here.

    embed_flag overrides w:embedTrueTypeFonts in settings, by default it is set
    exactly when a font is embedded."""
    if embedded is None:
        embedded = []
    elif isinstance(embedded, EmbeddedFont):
        embedded = [embedded]
    if embed_flag is None:
        embed_flag = bool(embedded)
    document = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NAMESPACES}><w:body>'
        f"{body_xml}{_section_xml(page_width_pt, page_height_pt)}</w:body></w:document>"
    )
    font_table, font_rels = _font_table_xml(family, embedded)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
        package.writestr("[Content_Types].xml", _content_types_xml(bool(embedded)))
        package.writestr("_rels/.rels", PACKAGE_RELS)
        package.writestr("word/document.xml", document)
        package.writestr("word/_rels/document.xml.rels", DOCUMENT_RELS)
        package.writestr("word/styles.xml", _styles_xml(family, default_size_pt))
        package.writestr("word/settings.xml", _settings_xml(embed_flag, compatibility_mode, extra_compat_xml))
        package.writestr("word/fontTable.xml", font_table)
        if embedded:
            package.writestr("word/_rels/fontTable.xml.rels", font_rels)
        for number, font in enumerate(embedded, start=1):
            package.writestr(f"word/fonts/font{number}.odttf", obfuscate_font(font.font_bytes, font.font_key))
    return path
