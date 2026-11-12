//! the libreoffice half of the freeform corpus: flat opendocument files
//! (one xml file each) that libreoffice opens and saves as .odt, so the
//! corpus files are what libreoffice itself writes. the same kinds as the
//! word corpus: shapes, groups, a floating table, linked text boxes,
//! columns and a flyer-like page.

const NAMESPACES: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
    xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" \
    xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
    xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" \
    xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" \
    xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" \
    xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
    xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" \
    office:version=\"1.3\" office:mimetype=\"application/vnd.oasis.opendocument.text\"";

const FILLER: &str = "Shapes, groups and boxes sit on this page while the text flows around them as LibreOffice lays it out.";

/// named drawing definitions every corpus file shares.
const DEFINITIONS: &str = "<draw:gradient draw:name=\"Sunrise\" draw:style=\"linear\" draw:start-color=\"#ffc800\" draw:end-color=\"#c82828\" draw:angle=\"0deg\" draw:border=\"0%\"/>\
    <draw:marker draw:name=\"Arrow\" svg:viewBox=\"0 0 20 30\" svg:d=\"M10 0l-10 30h20z\"/>\
    <draw:stroke-dash draw:name=\"Dash\" draw:style=\"rect\" draw:dots1=\"1\" draw:dots1-length=\"400%\" draw:distance=\"300%\"/>";

/// a graphic style placed relative to the page.
fn graphic(name: &str, properties: &str) -> String {
    format!(
        "<style:style style:name=\"{name}\" style:family=\"graphic\"><style:graphic-properties {properties} \
         style:horizontal-pos=\"from-left\" style:horizontal-rel=\"page\" style:vertical-pos=\"from-top\" style:vertical-rel=\"page\"/></style:style>"
    )
}

fn page_styles() -> String {
    let shapes = [
        graphic("gr1", "draw:fill=\"solid\" draw:fill-color=\"#3366cc\" draw:stroke=\"solid\" svg:stroke-color=\"#142850\" svg:stroke-width=\"0.028in\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("gr2", "draw:fill=\"gradient\" draw:fill-gradient-name=\"Sunrise\" draw:stroke=\"none\" style:wrap=\"parallel\""),
        graphic("gr3", "draw:fill=\"none\" draw:stroke=\"dash\" draw:stroke-dash=\"Dash\" svg:stroke-color=\"#008000\" svg:stroke-width=\"0.04in\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("gr4", "draw:fill=\"none\" draw:stroke=\"solid\" svg:stroke-color=\"#c80000\" svg:stroke-width=\"0.02in\" draw:marker-end=\"Arrow\" draw:marker-end-width=\"0.1in\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("gr5", "draw:fill=\"solid\" draw:fill-color=\"#783ca0\" draw:opacity=\"50%\" draw:stroke=\"none\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("gr6", "draw:fill=\"solid\" draw:fill-color=\"#ffffcc\" draw:stroke=\"solid\" svg:stroke-color=\"#000000\" svg:stroke-width=\"0.01in\" draw:textarea-vertical-align=\"middle\" fo:padding=\"0.05in\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("gr7", "draw:fill=\"solid\" draw:fill-color=\"#1e3c6e\" draw:stroke=\"none\" style:wrap=\"run-through\" style:run-through=\"background\""),
        graphic("fr1", "fo:border=\"0.5pt solid #000000\" fo:padding=\"0.05in\" draw:fill=\"none\" style:wrap=\"run-through\" style:run-through=\"foreground\""),
        graphic("fr2", "fo:border=\"none\" fo:padding=\"0in\" draw:fill=\"none\" style:wrap=\"parallel\""),
    ]
    .concat();
    format!(
        "{shapes}\
         <style:style style:name=\"Sect1\" style:family=\"section\"><style:section-properties><style:columns fo:column-count=\"3\" fo:column-gap=\"0.25in\">\
         <style:column-sep style:width=\"0.0071in\" style:color=\"#000000\" style:height=\"100%\"/>\
         <style:column style:rel-width=\"2000*\" fo:start-indent=\"0in\" fo:end-indent=\"0.125in\"/>\
         <style:column style:rel-width=\"3600*\" fo:start-indent=\"0.125in\" fo:end-indent=\"0.125in\"/>\
         <style:column style:rel-width=\"3040*\" fo:start-indent=\"0.125in\" fo:end-indent=\"0in\"/>\
         </style:columns></style:section-properties></style:style>\
         <style:style style:name=\"Tab1\" style:family=\"table\"><style:table-properties style:width=\"4in\" table:align=\"left\"/></style:style>\
         <style:style style:name=\"Row1\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"0.42in\"/></style:style>\
         <style:style style:name=\"Cell1\" style:family=\"table-cell\"><style:table-cell-properties fo:padding=\"0.05in\" fo:border=\"0.5pt solid #000000\"/></style:style>\
         <style:style style:name=\"Cell2\" style:family=\"table-cell\"><style:table-cell-properties fo:padding=\"0.1in\" fo:background-color=\"#dcebff\" style:vertical-align=\"middle\" fo:border=\"0.5pt solid #000000\"/></style:style>\
         <style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"8.5in\" fo:page-height=\"11in\" fo:margin-top=\"1in\" fo:margin-bottom=\"1in\" fo:margin-left=\"1in\" fo:margin-right=\"1in\"/></style:page-layout>"
    )
}

fn document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document {NAMESPACES}>\
         <office:styles>{DEFINITIONS}</office:styles>\
         <office:automatic-styles>{}</office:automatic-styles>\
         <office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\"/></office:master-styles>\
         <office:body><office:text>{body}</office:text></office:body></office:document>",
        page_styles()
    )
}

fn paragraph(text: &str) -> String {
    format!("<text:p>{text}</text:p>")
}

fn shapes() -> String {
    let objects = "<draw:rect draw:style-name=\"gr1\" draw:name=\"Solid Rectangle\" text:anchor-type=\"paragraph\" draw:z-index=\"0\" svg:x=\"1in\" svg:y=\"2.2in\" svg:width=\"2in\" svg:height=\"1in\"/>\
        <draw:ellipse draw:style-name=\"gr2\" draw:name=\"Gradient Oval\" text:anchor-type=\"paragraph\" draw:z-index=\"1\" svg:x=\"3.5in\" svg:y=\"2.2in\" svg:width=\"1.4in\" svg:height=\"1.4in\"/>\
        <draw:custom-shape draw:style-name=\"gr6\" draw:name=\"Rounded Note\" text:anchor-type=\"paragraph\" draw:z-index=\"2\" svg:x=\"5.4in\" svg:y=\"2.2in\" svg:width=\"2in\" svg:height=\"1in\"><text:p>A note in a shape</text:p><draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"round-rectangle\" draw:modifiers=\"3600\"/></draw:custom-shape>\
        <draw:custom-shape draw:style-name=\"gr1\" draw:name=\"Mirrored Arrow\" text:anchor-type=\"paragraph\" draw:z-index=\"3\" svg:x=\"1in\" svg:y=\"4in\" svg:width=\"2in\" svg:height=\"0.8in\"><text:p/><draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"right-arrow\" draw:mirror-horizontal=\"true\"/></draw:custom-shape>\
        <draw:line draw:style-name=\"gr4\" draw:name=\"Arrow Line\" text:anchor-type=\"paragraph\" draw:z-index=\"4\" svg:x1=\"3.5in\" svg:y1=\"4in\" svg:x2=\"5.5in\" svg:y2=\"4.8in\"><text:p/></draw:line>\
        <draw:path draw:style-name=\"gr3\" draw:name=\"Curve\" text:anchor-type=\"paragraph\" draw:z-index=\"5\" svg:x=\"1in\" svg:y=\"5.4in\" svg:width=\"2in\" svg:height=\"1.5in\" svg:viewBox=\"0 0 2000 1500\" svg:d=\"M0 1500C500 0 1500 0 2000 1500\"/>\
        <draw:polygon draw:style-name=\"gr5\" draw:name=\"Triangle\" text:anchor-type=\"paragraph\" draw:z-index=\"6\" svg:x=\"3.5in\" svg:y=\"5.4in\" svg:width=\"1.5in\" svg:height=\"1.3in\" svg:viewBox=\"0 0 1500 1300\" draw:points=\"750,0 1500,1300 0,1300\"/>\
        <draw:custom-shape draw:style-name=\"gr1\" draw:name=\"Turned Square\" text:anchor-type=\"paragraph\" draw:z-index=\"7\" svg:width=\"1in\" svg:height=\"1in\" draw:transform=\"rotate (-0.523598775598299) translate (5.933in 5.317in)\"><text:p/><draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"rectangle\"/></draw:custom-shape>";
    document(&format!(
        "<text:p>Running text. {objects}{FILLER}</text:p>{}{}",
        paragraph(FILLER),
        paragraph(FILLER)
    ))
}

fn groups() -> String {
    let group = "<draw:g draw:style-name=\"gr1\" draw:name=\"Outer Group\" text:anchor-type=\"paragraph\" draw:z-index=\"0\">\
        <draw:rect draw:style-name=\"gr1\" draw:name=\"Grouped Rectangle\" svg:x=\"1.4in\" svg:y=\"2.8in\" svg:width=\"1.67in\" svg:height=\"0.83in\"/>\
        <draw:ellipse draw:style-name=\"gr5\" draw:name=\"Grouped Oval\" svg:x=\"3.3in\" svg:y=\"2.9in\" svg:width=\"1.1in\" svg:height=\"1.1in\"/>\
        <draw:frame draw:style-name=\"fr1\" draw:name=\"Grouped Text\" svg:x=\"1.4in\" svg:y=\"4.2in\" svg:width=\"3in\" svg:height=\"0.7in\"><draw:text-box><text:p>Text inside a group</text:p></draw:text-box></draw:frame>\
        </draw:g>";
    document(&format!(
        "{}<text:p>{group}The group is anchored here. {FILLER}</text:p>",
        paragraph(&format!("Groups. {FILLER}"))
    ))
}

fn floating_table() -> String {
    let cell = |style: &str, text: &str, extra: &str| {
        format!("<table:table-cell table:style-name=\"{style}\" office:value-type=\"string\"{extra}><text:p>{text}</text:p></table:table-cell>")
    };
    let table = format!(
        "<table:table table:name=\"Layout\" table:style-name=\"Tab1\"><table:table-column table:number-columns-repeated=\"3\"/>\
         <table:table-row table:style-name=\"Row1\">{}{}{}</table:table-row>\
         <table:table-row table:style-name=\"Row1\">{}{}<table:covered-table-cell/></table:table-row>\
         <table:table-row table:style-name=\"Row1\">{}<table:covered-table-cell/>{}</table:table-row></table:table>",
        cell("Cell1", "Exact rows", ""),
        cell("Cell1", "Padded", ""),
        cell("Cell1", "Merged down", " table:number-rows-spanned=\"2\""),
        cell("Cell2", "Middle", ""),
        cell("Cell1", "Plain", ""),
        cell("Cell1", "Spans two", " table:number-columns-spanned=\"2\""),
        cell("Cell1", "End", ""),
    );
    let frame = format!(
        "<draw:frame draw:style-name=\"fr2\" draw:name=\"Table Frame\" text:anchor-type=\"paragraph\" svg:x=\"3.6in\" svg:y=\"4.2in\" svg:width=\"4in\" draw:z-index=\"0\"><draw:text-box fo:min-height=\"0in\">{table}</draw:text-box></draw:frame>"
    );
    document(&format!(
        "{}{}<text:p>{frame}</text:p>{}{}",
        paragraph(&format!("A floating table. {FILLER}")),
        paragraph(FILLER),
        paragraph(FILLER),
        paragraph(FILLER)
    ))
}

fn linked_boxes() -> String {
    let story: String = (1..=9)
        .map(|n| format!("<text:p>Paragraph {n} of the story flows from box to box without breaking the text apart.</text:p>"))
        .collect();
    let frame = |index: usize, content: &str| {
        let next = if index < 3 {
            format!(" draw:chain-next-name=\"Story Box {}\"", index + 1)
        } else {
            String::new()
        };
        format!(
            "<draw:frame draw:style-name=\"fr1\" draw:name=\"Story Box {index}\" text:anchor-type=\"paragraph\" svg:x=\"{}in\" svg:y=\"2.1in\" svg:width=\"2.08in\" svg:height=\"2.22in\" draw:z-index=\"{index}\"><draw:text-box{next}>{content}</draw:text-box></draw:frame>",
            1.0 + 2.29 * (index - 1) as f64
        )
    };
    let frames = format!("{}{}{}", frame(1, &story), frame(2, ""), frame(3, ""));
    document(&format!(
        "<text:p>Linked text boxes carry one story across three boxes.{frames}</text:p>{}",
        paragraph(FILLER)
    ))
}

fn columns() -> String {
    let column_text: String = (1..=6)
        .map(|n| paragraph(&format!("Column text {n}. {FILLER} {FILLER}")))
        .collect();
    document(&format!(
        "{}<text:section text:style-name=\"Sect1\" text:name=\"Columns\">{column_text}</text:section>{}",
        paragraph(&format!("One column at the top of the page. {FILLER}")),
        paragraph(&format!("Back to one column. {FILLER}"))
    ))
}

/// a page of objects anchored to the page itself, with a picture.
fn flyer(picture_base64: &str) -> String {
    let objects = format!(
        "<draw:rect draw:style-name=\"gr7\" draw:name=\"Banner\" text:anchor-type=\"page\" text:anchor-page-number=\"1\" draw:z-index=\"0\" svg:x=\"0in\" svg:y=\"0in\" svg:width=\"8.5in\" svg:height=\"2.2in\"/>\
         <draw:frame draw:style-name=\"fr2\" draw:name=\"Title\" text:anchor-type=\"page\" text:anchor-page-number=\"1\" draw:z-index=\"1\" svg:x=\"0.75in\" svg:y=\"0.55in\" svg:width=\"7in\" svg:height=\"1.1in\"><draw:text-box><text:p>Spring Science Fair</text:p></draw:text-box></draw:frame>\
         <draw:frame draw:style-name=\"fr2\" draw:name=\"Flyer Picture\" text:anchor-type=\"page\" text:anchor-page-number=\"1\" draw:z-index=\"2\" svg:x=\"0.75in\" svg:y=\"2.8in\" svg:width=\"3in\" svg:height=\"2.25in\"><draw:image><office:binary-data>{picture_base64}</office:binary-data></draw:image></draw:frame>\
         <draw:frame draw:style-name=\"fr1\" draw:name=\"Details\" text:anchor-type=\"page\" text:anchor-page-number=\"1\" draw:z-index=\"3\" svg:x=\"4.2in\" svg:y=\"2.8in\" svg:width=\"3.5in\" svg:height=\"3in\"><draw:text-box><text:p>Friday, 3 pm</text:p><text:p>Main hall</text:p><text:p>Bring a project, a poster and a friend.</text:p></draw:text-box></draw:frame>\
         <draw:custom-shape draw:style-name=\"gr6\" draw:name=\"Badge\" text:anchor-type=\"page\" text:anchor-page-number=\"1\" draw:z-index=\"4\" svg:x=\"6.1in\" svg:y=\"8.3in\" svg:width=\"1.5in\" svg:height=\"1.5in\"><text:p>Free</text:p><draw:enhanced-geometry svg:viewBox=\"0 0 21600 21600\" draw:type=\"star5\"/></draw:custom-shape>"
    );
    document(&format!("{objects}<text:p/>"))
}

/// every libreoffice corpus document as flat opendocument xml.
pub fn documents(picture_base64: &str) -> Vec<(&'static str, String)> {
    vec![
        ("lo_shapes", shapes()),
        ("lo_groups", groups()),
        ("lo_floating_table", floating_table()),
        ("lo_linked_boxes", linked_boxes()),
        ("lo_columns", columns()),
        ("lo_flyer", flyer(picture_base64)),
    ]
}
