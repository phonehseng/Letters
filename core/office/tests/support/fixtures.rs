//! xml pieces shared by several tests, and one document using nearly
//! every feature at once.

#![allow(dead_code)]

use letters_office::model::*;

use super::{p, png, Docx};

pub const HEADING_STYLES: &str = "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault>\
    <w:pPrDefault><w:pPr><w:spacing w:after=\"160\"/></w:pPr></w:pPrDefault></w:docDefaults>\
    <w:latentStyles w:defLockedState=\"0\" w:count=\"376\"/>\
    <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:rPr><w:rFonts w:asciiTheme=\"minorHAnsi\" w:hAnsiTheme=\"minorHAnsi\"/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/>\
      <w:uiPriority w:val=\"9\"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Heading1\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:sz w:val=\"26\"/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Fancy\"><w:name w:val=\"Fancy\"/><w:basedOn w:val=\"Heading2\"/><w:rPr><w:i/></w:rPr></w:style>\
    <w:style w:type=\"character\" w:styleId=\"FootnoteReference\"><w:name w:val=\"footnote reference\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>";

pub const NUMBERING: &str = "<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
    <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl>\
    <w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"%2)\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"1440\" w:hanging=\"360\"/></w:pPr></w:lvl>\
    <w:lvl w:ilvl=\"2\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerRoman\"/><w:lvlText w:val=\"%3.\"/><w:lvlJc w:val=\"right\"/></w:lvl></w:abstractNum>\
    <w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"\u{2022}\"/><w:rPr><w:rFonts w:ascii=\"Symbol\" w:hAnsi=\"Symbol\" w:hint=\"default\"/></w:rPr></w:lvl></w:abstractNum>\
    <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
    <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>\
    <w:num w:numId=\"3\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>";

pub fn heading(text: &str, style: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}

pub fn list_item(text: &str, num: &str, level: u8) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

pub fn inline_image(rel: &str, cx: u64, cy: u64) -> String {
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:docPr id=\"1\" name=\"Picture 1\" descr=\"A red square\"/><wp:cNvGraphicFramePr/>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"x.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{rel}\"><a:alphaModFix amt=\"50000\"/></a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )
}

pub const FOOTNOTES: &str = "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
    <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>\
    <w:footnote w:id=\"1\"><w:p><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>\
    <w:r><w:t xml:space=\"preserve\"> See the source.</w:t></w:r></w:p></w:footnote>";

pub const COMMENTS: &str =
    "<w:comment w:id=\"0\" w:author=\"Prof\" w:date=\"2026-03-01T10:00:00Z\" w:initials=\"P\">\
    <w:p><w:r><w:annotationRef/></w:r><w:r><w:t>Cite this.</w:t></w:r></w:p></w:comment>";

/// a chart drawing: a kind of drawing the model does not understand.
pub const CHART_RUN: &str = "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"5486400\" cy=\"3200400\"/>\
    <wp:docPr id=\"5\" name=\"Chart 5\"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">\
    <c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"rIdChart\"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>";

pub const CHART_XML: &str = "<?xml version=\"1.0\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart/></c:chartSpace>";

/// a shape in word's compatibility wrapper, also not understood.
pub const SHAPE_RUN: &str = "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:anchor/></w:drawing></mc:Choice>\
    <mc:Fallback><w:pict><v:rect xmlns:v=\"urn:schemas-microsoft-com:vml\" style=\"width:10pt\"/></w:pict></mc:Fallback></mc:AlternateContent></w:r>";

pub const SDT_BLOCK: &str =
    "<w:sdt><w:sdtPr><w:alias w:val=\"Abstract\"/><w:id w:val=\"42\"/></w:sdtPr><w:sdtContent>\
    <w:p><w:r><w:t>Inside a content control</w:t></w:r></w:p></w:sdtContent></w:sdt>";

pub const MATH_PARAGRAPH: &str = "<w:p><m:oMathPara><m:oMath><m:r><m:t>E=mc</m:t></m:r><m:sSup><m:e/><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath></m:oMathPara></w:p>";

pub const UNKNOWN_RUN_PROPERTY: &str =
    "<w14:glow w14:rad=\"63500\"><w14:srgbClr w14:val=\"FF0000\"/></w14:glow>";

pub const CUSTOM_XML: &str =
    "<?xml version=\"1.0\"?><root xmlns=\"urn:example\"><student id=\"7\">Ada</student></root>";

/// nearly every feature at once, for round trips.
pub fn rich_docx() -> Vec<u8> {
    let body = format!(
        "{h1}\
         <w:p w14:paraId=\"1A2B3C4D\" w14:textId=\"77777777\" w:rsidR=\"00AB12CD\"><w:pPr><w:jc w:val=\"center\"/><w:widowControl w:val=\"0\"/></w:pPr>\
           <w:r><w:rPr><w:b/>{glow}</w:rPr><w:t xml:space=\"preserve\">Glowing </w:t></w:r>\
           <w:r><w:t>words</w:t></w:r><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"1\"/></w:r></w:p>\
         {l1}{l2}{l3}\
         <w:p><w:r><w:t xml:space=\"preserve\">The </w:t></w:r>\
           <w:del w:id=\"7\" w:author=\"Ada\" w:date=\"2026-01-02T03:04:05Z\"><w:r><w:delText>old</w:delText></w:r></w:del>\
           <w:ins w:id=\"8\" w:author=\"Ben\" w:date=\"2026-01-02T03:05:00Z\"><w:r><w:t>new</w:t></w:r></w:ins>\
           <w:commentRangeStart w:id=\"0\"/><w:r><w:t xml:space=\"preserve\"> word</w:t></w:r><w:commentRangeEnd w:id=\"0\"/>\
           <w:r><w:commentReference w:id=\"0\"/></w:r></w:p>\
         <w:p><w:hyperlink r:id=\"rIdLink\"><w:r><w:t>a link</w:t></w:r></w:hyperlink>{image}{chart}{shape}</w:p>\
         {sdt}{math}\
         <w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLook w:val=\"04A0\"/></w:tblPr>\
           <w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>\
           <w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc>\
             <w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>tall</w:t></w:r></w:p></w:tc></w:tr>\
           <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>d</w:t></w:r></w:p></w:tc>\
             <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>\
         <w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH1\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/>\
           <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr></w:pPr>\
           <w:r><w:t>End of part one</w:t></w:r></w:p>\
         {last}\
         <w:sectPr w:rsidR=\"00AB12CD\"><w:footerReference w:type=\"default\" r:id=\"rIdF1\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/>\
           <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\
           <w:cols w:space=\"720\"/><w:docGrid w:linePitch=\"360\"/></w:sectPr>",
        h1 = heading("Introduction", "Heading1"),
        glow = UNKNOWN_RUN_PROPERTY,
        l1 = list_item("first", "1", 0),
        l2 = list_item("nested", "1", 1),
        l3 = list_item("second", "1", 0),
        image = inline_image("rIdImg", 914400, 457200),
        chart = CHART_RUN,
        shape = SHAPE_RUN,
        sdt = SDT_BLOCK,
        math = MATH_PARAGRAPH,
        last = p("Last words"),
    );
    let footer = "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p>";
    let chart_rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/package\" Target=\"../embeddings/Sheet1.xlsx\"/></Relationships>";
    Docx::new(&body)
        .styles(HEADING_STYLES)
        .numbering(NUMBERING)
        .footnotes(FOOTNOTES)
        .comments(COMMENTS)
        .header("rIdH1", "word/header1.xml", &p("Running head"))
        .footer("rIdF1", "word/footer1.xml", footer)
        .image("rIdImg", "word/media/image1.png", &png())
        .link("rIdLink", "https://example.org/")
        .related(
            "rIdChart",
            "chart",
            "word/charts/chart1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            CHART_XML,
        )
        .loose_part(
            "word/charts/_rels/chart1.xml.rels",
            "",
            chart_rels.as_bytes(),
        )
        .loose_part(
            "word/embeddings/Sheet1.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            b"PK fake workbook bytes",
        )
        .loose_part("customXml/item1.xml", "", CUSTOM_XML.as_bytes())
        .build()
}

/// two text boxes, one positioned on the page and one on its paragraph.
pub fn text_box_document() -> Document {
    let mut document = Document::new();
    document
        .numbering
        .definitions
        .push(ListDefinition::standard("0", false));
    let list = document.numbering.add_instance("0");
    let mut item = Paragraph::with_text("a listed point");
    item.props.list = Some(ListRef {
        list_id: list,
        level: 0,
    });
    let mut first = TextBox::new(
        2880,
        2160,
        4320,
        1440,
        vec![
            Block::Paragraph(Paragraph {
                content: vec![
                    Inline::styled("Boxed ", vec![Mark::Bold]),
                    Inline::text("text"),
                ],
                ..Default::default()
            }),
            Block::Paragraph(item),
        ],
    );
    first.name = Some("First".into());
    first.z_order = Some(5);
    first.wrap = Wrap::Square;
    first.fill = Some("FFF2CC".into());
    let mut second = TextBox::new(1440, 6000, 3000, 2000, vec![Block::text("Second box")]);
    second.horizontal = FloatPosition::new("margin", 720);
    second.vertical = FloatPosition::new("paragraph", 240);
    second.auto_height = true;
    second.wrap = Wrap::TopAndBottom;
    second.border = None;
    second.fill = None;
    second.margins = BoxMargins {
        left: 100,
        top: 50,
        right: 100,
        bottom: 50,
    };
    second.z_order = Some(6);
    document.blocks = vec![
        Block::text("Before the boxes."),
        Block::TextBox(first),
        Block::TextBox(second),
        Block::text("After the boxes."),
    ];
    document
}
