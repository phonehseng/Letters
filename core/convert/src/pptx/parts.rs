//! the parts every presentation needs besides its slides: the content
//! type list, relationships, one blank slide master and layout, and a
//! theme. powerpoint refuses a file without a complete theme, so the
//! theme here has every colour, font and effect list the format asks for.

use crate::package::{xml_escape, XML_HEADER};

pub(super) const NAMESPACES: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";

/// the empty group every shape tree starts with
pub(super) const GROUP_START: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(super) fn content_types(slides: usize) -> String {
    let mut xml = format!(
        "{XML_HEADER}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/><Override PartName=\"/ppt/presProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presProps+xml\"/><Override PartName=\"/ppt/viewProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml\"/><Override PartName=\"/ppt/tableStyles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>"
    );
    for index in 1..=slides {
        xml.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
    }
    xml.push_str("</Types>");
    xml
}

pub(super) fn root_relationships() -> String {
    format!(
        "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"ppt/presentation.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"{REL}/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>"
    )
}

pub(super) fn app_properties(slides: usize) -> String {
    format!(
        "{XML_HEADER}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Letters</Application><Slides>{slides}</Slides></Properties>"
    )
}

/// slide sizes are in emu: 914 400 to the inch, 12 700 to the point.
pub(super) fn presentation(slides: usize, width_emu: i64, height_emu: i64) -> String {
    let mut ids = String::new();
    for index in 0..slides {
        ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
            256 + index,
            index + 2
        ));
    }
    format!(
        "{XML_HEADER}<p:presentation {NAMESPACES} saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{width_emu}\" cy=\"{height_emu}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    )
}

pub(super) fn presentation_relationships(slides: usize) -> String {
    let mut xml = format!(
        "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>"
    );
    for index in 1..=slides {
        xml.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"{REL}/slide\" Target=\"slides/slide{index}.xml\"/>",
            index + 1
        ));
    }
    let next = slides + 2;
    xml.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"{REL}/theme\" Target=\"theme/theme1.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/presProps\" Target=\"presProps.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/viewProps\" Target=\"viewProps.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/tableStyles\" Target=\"tableStyles.xml\"/></Relationships>",
        next,
        next + 1,
        next + 2,
        next + 3
    ));
    xml
}

pub(super) fn pres_props() -> String {
    format!("{XML_HEADER}<p:presentationPr {NAMESPACES}/>")
}

pub(super) fn view_props() -> String {
    format!(
        "{XML_HEADER}<p:viewPr {NAMESPACES}><p:normalViewPr><p:restoredLeft sz=\"15620\"/><p:restoredTop sz=\"94660\"/></p:normalViewPr><p:gridSpacing cx=\"76200\" cy=\"76200\"/></p:viewPr>"
    )
}

pub(super) fn table_styles() -> String {
    format!(
        "{XML_HEADER}<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>"
    )
}

pub(super) fn slide_master() -> String {
    format!(
        "{XML_HEADER}<p:sldMaster {NAMESPACES}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{GROUP_START}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"
    )
}

pub(super) fn slide_master_relationships() -> String {
    format!(
        "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/theme\" Target=\"../theme/theme1.xml\"/></Relationships>"
    )
}

pub(super) fn slide_layout() -> String {
    format!(
        "{XML_HEADER}<p:sldLayout {NAMESPACES} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{GROUP_START}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
    )
}

pub(super) fn slide_layout_relationships() -> String {
    format!(
        "{XML_HEADER}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>"
    )
}

pub(super) fn core_properties(title: Option<&str>) -> String {
    format!(
        "{XML_HEADER}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>Letters</dc:creator></cp:coreProperties>",
        xml_escape(title.unwrap_or(""))
    )
}

/// a plain office-like theme: its colours, fonts and the three fills,
/// lines, effects and background fills the format requires.
pub(super) fn theme() -> String {
    let colors = [
        ("dk1", "<a:sysClr val=\"windowText\" lastClr=\"000000\"/>"),
        ("lt1", "<a:sysClr val=\"window\" lastClr=\"FFFFFF\"/>"),
        ("dk2", "<a:srgbClr val=\"44546A\"/>"),
        ("lt2", "<a:srgbClr val=\"E7E6E6\"/>"),
        ("accent1", "<a:srgbClr val=\"4472C4\"/>"),
        ("accent2", "<a:srgbClr val=\"ED7D31\"/>"),
        ("accent3", "<a:srgbClr val=\"A5A5A5\"/>"),
        ("accent4", "<a:srgbClr val=\"FFC000\"/>"),
        ("accent5", "<a:srgbClr val=\"5B9BD5\"/>"),
        ("accent6", "<a:srgbClr val=\"70AD47\"/>"),
        ("hlink", "<a:srgbClr val=\"0563C1\"/>"),
        ("folHlink", "<a:srgbClr val=\"954F72\"/>"),
    ];
    let scheme: String = colors
        .iter()
        .map(|(name, color)| format!("<a:{name}>{color}</a:{name}>"))
        .collect();
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let line = |width: u32| {
        format!(
            "<a:ln w=\"{width}\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>"
        )
    };
    let effect = "<a:effectStyle><a:effectLst/></a:effectStyle>";
    format!(
        "{XML_HEADER}<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Letters\"><a:themeElements><a:clrScheme name=\"Letters\">{scheme}</a:clrScheme><a:fontScheme name=\"Letters\"><a:majorFont><a:latin typeface=\"Calibri Light\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"Letters\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{}{}{}</a:lnStyleLst><a:effectStyleLst>{effect}{effect}{effect}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
        line(6350),
        line(12700),
        line(19050)
    )
}
