"""a small writer for ordinary, flowing word documents (not the converter's output).

the corpus needs pdfs the way word really makes them, so these documents use
what students use: styles, bullet and numbered lists, tables with rules, two
columns, footnotes, superscripts, pictures and drawn shapes. fonts are
embedded in the docx, so word lays out with the open-licensed fonts without
installing anything.
"""

from __future__ import annotations

import uuid
import zipfile
from dataclasses import dataclass, field
from xml.sax.saxutils import escape

from docx_writer import obfuscate_font

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
EMU_PER_PT = 12700


def run(
    text: str,
    *,
    font: str = "Crimson Text",
    size: float | None = None,
    bold=False,
    italic=False,
    color: str | None = None,
    vert: str | None = None,
    kern: bool = False,
) -> str:
    """a run; bold and italic pick the style's own family (see corpus_builder.fonts)."""
    if bold:
        font = f"{font} Bold"
    elif italic:
        font = f"{font} Italic"
    props = [f'<w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/>']
    if color:
        props.append(f'<w:color w:val="{color}"/>')
    if kern:
        props.append('<w:kern w:val="16"/>')
    if size:
        props.append(f'<w:sz w:val="{round(size * 2)}"/><w:szCs w:val="{round(size * 2)}"/>')
    if vert:
        props.append(f'<w:vertAlign w:val="{vert}"/>')
    return f'<w:r><w:rPr>{"".join(props)}</w:rPr><w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def paragraph(
    runs: str,
    *,
    style: str | None = None,
    align: str | None = None,
    after: float | None = None,
    before: float | None = None,
    numbering: tuple[int, int] | None = None,
    keep_next=False,
    section: str = "",
) -> str:
    props = []
    if style:
        props.append(f'<w:pStyle w:val="{style}"/>')
    if keep_next:
        props.append("<w:keepNext/>")
    if numbering:
        props.append(f'<w:numPr><w:ilvl w:val="{numbering[1]}"/><w:numId w:val="{numbering[0]}"/></w:numPr>')
    if before is not None or after is not None:
        props.append(f'<w:spacing w:before="{round((before or 0) * 20)}" w:after="{round((after or 0) * 20)}"/>')
    if align:
        props.append(f'<w:jc w:val="{align}"/>')
    return f"<w:p><w:pPr>{''.join(props)}{section}</w:pPr>{runs}</w:p>"


def section_properties(
    width_pt: float,
    height_pt: float,
    margin_pt: float,
    columns: int = 1,
    column_gap_pt: float = 18,
    continuous: bool = False,
) -> str:
    kind = '<w:type w:val="continuous"/>' if continuous else ""
    orient = ' w:orient="landscape"' if width_pt > height_pt else ""
    m = round(margin_pt * 20)
    return (
        f'<w:sectPr>{kind}<w:pgSz w:w="{round(width_pt * 20)}" w:h="{round(height_pt * 20)}"{orient}/>'
        f'<w:pgMar w:top="{m}" w:right="{m}" w:bottom="{m}" w:left="{m}" w:header="720" w:footer="720" w:gutter="0"/>'
        f'<w:cols w:num="{columns}" w:space="{round(column_gap_pt * 20)}"/></w:sectPr>'
    )


def table(rows: list[list[str]], widths_pt: list[float], *, header=True, font_size: float = 10) -> str:
    """a grid table with single rules, the header row bold."""
    border = '<w:{0} w:val="single" w:sz="6" w:space="0" w:color="333333"/>'
    borders = "".join(border.format(side) for side in ("top", "left", "bottom", "right", "insideH", "insideV"))
    grid = "".join(f'<w:gridCol w:w="{round(w * 20)}"/>' for w in widths_pt)
    body = []
    for index, cells in enumerate(rows):
        tcs = []
        for cell, width in zip(cells, widths_pt, strict=True):
            text = run(cell, bold=header and index == 0, size=font_size)
            tcs.append(
                f'<w:tc><w:tcPr><w:tcW w:w="{round(width * 20)}" w:type="dxa"/></w:tcPr>'
                f"{paragraph(text, after=0, before=0)}</w:tc>"
            )
        body.append(f"<w:tr>{''.join(tcs)}</w:tr>")
    return (
        f'<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>{borders}</w:tblBorders>'
        f'<w:tblCellMar><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tblCellMar></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>{''.join(body)}</w:tbl>"
    )


def footnote_reference(footnote_id: int) -> str:
    return f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{footnote_id}"/></w:r>'


def inline_picture(relationship_id: str, width_pt: float, height_pt: float, picture_id: int, name: str) -> str:
    cx, cy = round(width_pt * EMU_PER_PT), round(height_pt * EMU_PER_PT)
    return (
        '<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{picture_id}" name="{name}"/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'<pic:pic><pic:nvPicPr><pic:cNvPr id="{picture_id}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{relationship_id}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
        "</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )


def anchored_shape(
    shape_id: int,
    x_pt: float,
    y_pt: float,
    w_pt: float,
    h_pt: float,
    *,
    geometry="rect",
    fill: str | None = None,
    line: str | None = None,
    line_pt: float = 0.75,
) -> str:
    """a shape positioned against its paragraph, like a hand-drawn chart element."""
    cx, cy = round(w_pt * EMU_PER_PT), round(h_pt * EMU_PER_PT)
    fill_xml = f'<a:solidFill><a:srgbClr val="{fill}"/></a:solidFill>' if fill else "<a:noFill/>"
    line_xml = (
        f'<a:ln w="{round(line_pt * EMU_PER_PT)}"><a:solidFill><a:srgbClr val="{line}"/></a:solidFill></a:ln>'
        if line
        else "<a:ln><a:noFill/></a:ln>"
    )
    return (
        '<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>'
        f'<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{251658240 + shape_id}" '
        'behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="column"><wp:posOffset>{round(x_pt * EMU_PER_PT)}</wp:posOffset></wp:positionH>'
        f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>{round(y_pt * EMU_PER_PT)}</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>'
        f'<wp:docPr id="{shape_id}" name="Shape {shape_id}"/><wp:cNvGraphicFramePr/>'
        '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">'
        f'<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        f'<a:prstGeom prst="{geometry}"><a:avLst/></a:prstGeom>{fill_xml}{line_xml}</wps:spPr>'
        "<wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice></mc:AlternateContent></w:r>"
    )


@dataclass
class FontFamily:
    name: str
    files: dict[str, bytes]  # regular, bold, italic, boldItalic


@dataclass
class WordDocument:
    body: list[str] = field(default_factory=list)
    footnotes: list[str] = field(default_factory=list)  # paragraph xml per footnote, ids from 1
    media: list[tuple[str, bytes]] = field(default_factory=list)  # (file name, bytes)
    fonts: list[FontFamily] = field(default_factory=list)
    body_font: str = "Crimson Text"
    body_size: float = 11
    heading_font: str = "Lato"
    final_section: str = ""

    def add_media(self, name: str, data: bytes) -> str:
        self.media.append((name, data))
        return f"rIdMedia{len(self.media)}"

    def add_footnote(self, text_runs: str) -> int:
        self.footnotes.append(text_runs)
        return len(self.footnotes)

    def save(self, path: str) -> str:
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as package:
            package.writestr("[Content_Types].xml", self._content_types())
            package.writestr("_rels/.rels", _package_rels())
            package.writestr("word/_rels/document.xml.rels", self._document_rels())
            package.writestr(
                "word/document.xml",
                f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>'
                + "".join(self.body)
                + self.final_section
                + "</w:body></w:document>",
            )
            package.writestr("word/styles.xml", self._styles())
            package.writestr("word/settings.xml", _settings())
            package.writestr("word/numbering.xml", _numbering(self.body_font))
            package.writestr("word/footnotes.xml", self._footnotes())
            table_xml, rels_xml, parts = self._font_table()
            package.writestr("word/fontTable.xml", table_xml)
            package.writestr("word/_rels/fontTable.xml.rels", rels_xml)
            for name, data in parts:
                package.writestr(f"word/fonts/{name}", data)
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
            '<Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/>'
            f'<Override PartName="/word/document.xml" ContentType="{MAIN}.document.main+xml"/>'
            f'<Override PartName="/word/styles.xml" ContentType="{MAIN}.styles+xml"/>'
            f'<Override PartName="/word/settings.xml" ContentType="{MAIN}.settings+xml"/>'
            f'<Override PartName="/word/numbering.xml" ContentType="{MAIN}.numbering+xml"/>'
            f'<Override PartName="/word/footnotes.xml" ContentType="{MAIN}.footnotes+xml"/>'
            f'<Override PartName="/word/fontTable.xml" ContentType="{MAIN}.fontTable+xml"/>'
            "</Types>"
        )

    def _document_rels(self) -> str:
        rels = [
            f'<Relationship Id="rIdStyles" Type="{REL}/styles" Target="styles.xml"/>',
            f'<Relationship Id="rIdSettings" Type="{REL}/settings" Target="settings.xml"/>',
            f'<Relationship Id="rIdNumbering" Type="{REL}/numbering" Target="numbering.xml"/>',
            f'<Relationship Id="rIdFootnotes" Type="{REL}/footnotes" Target="footnotes.xml"/>',
            f'<Relationship Id="rIdFonts" Type="{REL}/fontTable" Target="fontTable.xml"/>',
        ]
        for index, (name, _) in enumerate(self.media, start=1):
            rels.append(f'<Relationship Id="rIdMedia{index}" Type="{REL}/image" Target="media/{name}"/>')
        return (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            + "".join(rels)
            + "</Relationships>"
        )

    def _styles(self) -> str:
        body = self.body_font
        head = self.heading_font

        def style(style_id, name, run_props, para_props="", based="Normal", kind="paragraph"):
            return (
                f'<w:style w:type="{kind}" w:styleId="{style_id}"><w:name w:val="{name}"/><w:basedOn w:val="{based}"/>'
                f"<w:qFormat/><w:pPr>{para_props}</w:pPr><w:rPr>{run_props}</w:rPr></w:style>"
            )

        fonts = f'<w:rFonts w:ascii="{head} Bold" w:hAnsi="{head} Bold" w:cs="{head} Bold"/>'
        caption_fonts = f'<w:rFonts w:ascii="{head} Italic" w:hAnsi="{head} Italic" w:cs="{head} Italic"/>'
        return (
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS}>'
            "<w:docDefaults><w:rPrDefault><w:rPr>"
            f'<w:rFonts w:ascii="{body}" w:hAnsi="{body}" w:cs="{body}" w:eastAsia="{body}"/>'
            f'<w:sz w:val="{round(self.body_size * 2)}"/><w:szCs w:val="{round(self.body_size * 2)}"/>'
            '<w:lang w:val="en-US"/></w:rPr></w:rPrDefault>'
            '<w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="264" w:lineRule="auto"/></w:pPr></w:pPrDefault>'
            "</w:docDefaults>"
            '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
            + style("Title", "Title", fonts + '<w:sz w:val="44"/><w:color w:val="1F3864"/>', '<w:spacing w:after="160"/>')
            + style("Heading1", "heading 1", fonts + '<w:sz w:val="30"/><w:color w:val="1F3864"/>',
                    '<w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="0"/>')
            + style("Heading2", "heading 2", fonts + '<w:sz w:val="24"/>',
                    '<w:keepNext/><w:spacing w:before="160" w:after="60"/><w:outlineLvl w:val="1"/>')
            + style("Caption", "caption", caption_fonts + '<w:sz w:val="17"/><w:color w:val="404040"/>', '<w:spacing w:after="160"/>')
            + style("FootnoteText", "footnote text", '<w:sz w:val="16"/>', '<w:spacing w:after="0" w:line="240" w:lineRule="auto"/>')
            + '<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/>'
            '<w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>'
            "</w:styles>"
        )  # fmt: skip

    def _footnotes(self) -> str:
        notes = [
            '<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>',
            '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>',
        ]  # fmt: skip
        for index, runs in enumerate(self.footnotes, start=1):
            notes.append(
                f'<w:footnote w:id="{index}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr>'
                f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>'
                f"{run(' ')}{runs}</w:p></w:footnote>"
            )
        return (
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {NS}>{"".join(notes)}</w:footnotes>'
        )

    def _font_table(self) -> tuple[str, str, list[tuple[str, bytes]]]:
        entries, rels, parts = [], [], []
        for font in self.fonts:
            embeds = []
            for style, tag in (("regular", "embedRegular"), ("bold", "embedBold"), ("italic", "embedItalic")):
                if style in font.files:
                    key = "{" + str(uuid.uuid4()).upper() + "}"
                    number = len(parts) + 1
                    parts.append((f"font{number}.odttf", obfuscate_font(font.files[style], key)))
                    rels.append(
                        f'<Relationship Id="rIdF{number}" Type="{REL}/font" Target="fonts/font{number}.odttf"/>'
                    )
                    embeds.append(f'<w:{tag} r:id="rIdF{number}" w:fontKey="{key}"/>')
            entries.append(f'<w:font w:name="{font.name}"><w:charset w:val="00"/>{"".join(embeds)}</w:font>')
        table = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts {NS}>{"".join(entries)}</w:fonts>'
        rels_xml = (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            + "".join(rels)
            + "</Relationships>"
        )
        return table, rels_xml, parts


def _package_rels() -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>'
    )


def _settings() -> str:
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {NS}>'
        '<w:embedTrueTypeFonts/><w:defaultTabStop w:val="720"/>'
        '<w:footnotePr><w:footnote w:id="-1"/><w:footnote w:id="0"/></w:footnotePr>'
        '<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>'
        "</w:settings>"
    )


def _numbering(bullet_font: str) -> str:
    """a bullet list (numId 1) and a decimal list (numId 2), the bullet drawn with the body font."""
    bullet = (
        '<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>'
        '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="•"/>'
        '<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="540" w:hanging="270"/></w:pPr>'
        f'<w:rPr><w:rFonts w:ascii="{bullet_font}" w:hAnsi="{bullet_font}"/></w:rPr></w:lvl>'
        '<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="–"/>'
        '<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="900" w:hanging="270"/></w:pPr>'
        f'<w:rPr><w:rFonts w:ascii="{bullet_font}" w:hAnsi="{bullet_font}"/></w:rPr></w:lvl></w:abstractNum>'
    )
    decimal = (
        '<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>'
        '<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/>'
        '<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="540" w:hanging="300"/></w:pPr></w:lvl></w:abstractNum>'
    )
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {NS}>{bullet}{decimal}'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
        "</w:numbering>"
    )
