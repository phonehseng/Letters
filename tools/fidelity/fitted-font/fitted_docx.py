"""docx xml for fitted-font text: whole segments in one run, with the opentype features word should apply.

the round 4 recipe writes one run per character so that word cannot merge
glyphs into one pdf text object; here the opposite is wanted: the whole
segment must reach word's shaper as one piece so a gsub rule can replace it.
the package itself is the prototype's (docx_out.Package); probes that must
test a run with no ligature setting at all use write_probe_docx, whose
styles set no default for it.
"""

from __future__ import annotations

import fidelity_path  # noqa: F401  (puts the round 4 modules on sys.path)

# isort: split

import zipfile
from dataclasses import dataclass
from xml.sax.saxutils import escape

import docx_writer as dw
from prototype import docx_out

W14 = "http://schemas.microsoft.com/office/word/2010/wordml"


@dataclass(frozen=True)
class RunFeatures:
    """the w14 run properties that switch opentype features on (iso 29500 w14 extensions).

    ligatures: None writes no element, otherwise a w14:ligatures value such as
    "standard", "all" or "none". stylistic_set: 1..20 writes w14:stylisticSets."""

    ligatures: str | None = "standard"
    contextual: bool = False
    stylistic_set: int | None = None
    east_asian_lang: str | None = None


@dataclass(frozen=True)
class TextRun:
    text: str
    family: str
    half_points: int
    color: str = "000000"
    hidden: bool = False
    spacing_twips: int = 0
    features: RunFeatures = RunFeatures()


def _w14_props(features: RunFeatures) -> str:
    # w14 elements have a fixed order in the schema: ligatures, ..., stylisticSets, cntxtAlts
    parts = []
    if features.ligatures is not None:
        parts.append(f'<w14:ligatures w14:val="{features.ligatures}"/>')
    if features.stylistic_set is not None:
        parts.append(f'<w14:stylisticSets><w14:styleSet w14:id="{features.stylistic_set}"/></w14:stylisticSets>')
    if features.contextual:
        parts.append("<w14:cntxtAlts/>")
    return "".join(parts)


def run_xml(run: TextRun) -> str:
    props = [f'<w:rFonts w:ascii="{run.family}" w:hAnsi="{run.family}" w:cs="{run.family}" w:eastAsia="{run.family}"/>']
    if run.hidden:
        props.append("<w:vanish/>")
    if run.color != "000000":
        props.append(f'<w:color w:val="{run.color}"/>')
    if run.spacing_twips:
        props.append(f'<w:spacing w:val="{run.spacing_twips}"/>')
    props.append(f'<w:sz w:val="{run.half_points}"/><w:szCs w:val="{run.half_points}"/>')
    if run.features.east_asian_lang:
        props.append(f'<w:lang w:eastAsia="{run.features.east_asian_lang}"/>')
    props.append(_w14_props(run.features))
    return f'<w:r><w:rPr>{"".join(props)}</w:rPr><w:t xml:space="preserve">{escape(run.text)}</w:t></w:r>'


def frame_paragraph(runs: list[TextRun], x_twips: int, y_twips: int, width_twips: int, line_twips: int) -> str:
    frame = (
        f'<w:framePr w:w="{width_twips}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" '
        f'w:x="{x_twips}" w:y="{y_twips}"/>'
    )
    body = "".join(run_xml(run) for run in runs)
    return f"<w:p><w:pPr>{frame}{docx_out.line_paragraph_props(line_twips)}</w:pPr>{body}</w:p>"


def _plain_styles(family: str) -> str:
    # like docx_writer's styles but with no w14:ligatures default, so a run
    # without the element shows what word does on its own
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {dw.NAMESPACES}>'
        "<w:docDefaults><w:rPrDefault><w:rPr>"
        f'<w:rFonts w:ascii="{family}" w:hAnsi="{family}" w:cs="{family}" w:eastAsia="{family}"/>'
        '<w:sz w:val="24"/><w:szCs w:val="24"/><w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/>'
        "</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>"
        f"{dw.LAYOUT_NEUTRAL_PPR}"
        '<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
        "</w:pPr></w:pPrDefault></w:docDefaults>"
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
        "</w:styles>"
    )


def write_probe_docx(
    path: str,
    body_xml: str,
    fonts: list[dw.EmbeddedFont],
    page_width_pt: float = 612,
    page_height_pt: float = 792,
    embed_flag: bool = True,
    extra_compat_xml: str = "",
) -> str:
    """a one-section package with every font embedded as an obfuscated .odttf part."""
    family = fonts[0].family if fonts else "Arial"
    document = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {dw.NAMESPACES}><w:body>'
        f"{body_xml}{dw._section_xml(page_width_pt, page_height_pt)}</w:body></w:document>"
    )
    font_table, font_rels = dw._font_table_xml(family, fonts)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
        package.writestr("[Content_Types].xml", dw._content_types_xml(bool(fonts)))
        package.writestr("_rels/.rels", dw.PACKAGE_RELS)
        package.writestr("word/document.xml", document)
        package.writestr("word/_rels/document.xml.rels", dw.DOCUMENT_RELS)
        package.writestr("word/styles.xml", _plain_styles(family))
        package.writestr("word/settings.xml", dw._settings_xml(embed_flag, 15, extra_compat_xml))
        package.writestr("word/fontTable.xml", font_table)
        if fonts:
            package.writestr("word/_rels/fontTable.xml.rels", font_rels)
        for number, font in enumerate(fonts, start=1):
            package.writestr(f"word/fonts/font{number}.odttf", dw.obfuscate_font(font.font_bytes, font.font_key))
    return path
