//! builders for small docx and odt packages, written by hand from xml
//! strings so each test shows exactly what the file contains.

#![allow(dead_code)]

pub mod fixtures;

use letters_office::package::{write_zip, Part};

pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// the namespaces a test document declares on its root.
pub fn story_namespaces() -> String {
    format!(
        "xmlns:w=\"{W}\" xmlns:r=\"{R}\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" \
         xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
         xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
         xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" \
         mc:Ignorable=\"w14\""
    )
}

pub fn zip(parts: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let parts: Vec<Part> = parts
        .iter()
        .map(|(name, bytes)| Part {
            name: name.to_string(),
            bytes: bytes.clone(),
        })
        .collect();
    write_zip(&parts, &["mimetype"]).unwrap()
}

/// a docx package put together part by part.
#[derive(Default)]
pub struct Docx {
    body: String,
    parts: Vec<(String, String, Vec<u8>)>,
    rels: Vec<(String, String, String, bool)>,
    defaults: Vec<(String, String)>,
}

impl Docx {
    pub fn new(body: &str) -> Docx {
        Docx {
            body: body.to_string(),
            ..Default::default()
        }
    }

    /// adds a part the main document refers to with a relationship.
    pub fn related(
        mut self,
        id: &str,
        kind: &str,
        name: &str,
        content_type: &str,
        xml: &str,
    ) -> Docx {
        self.rels.push((
            id.into(),
            format!("{REL}{kind}"),
            name.trim_start_matches("word/").into(),
            false,
        ));
        self.parts
            .push((name.into(), content_type.into(), xml.as_bytes().to_vec()));
        self
    }

    pub fn styles(self, styles: &str) -> Docx {
        let xml = format!("<w:styles {}>{styles}</w:styles>", story_namespaces());
        self.related(
            "rIdStyles",
            "styles",
            "word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            &xml,
        )
    }

    pub fn numbering(self, numbering: &str) -> Docx {
        let xml = format!(
            "<w:numbering {}>{numbering}</w:numbering>",
            story_namespaces()
        );
        self.related(
            "rIdNumbering",
            "numbering",
            "word/numbering.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
            &xml,
        )
    }

    pub fn footnotes(self, notes: &str) -> Docx {
        let xml = format!("<w:footnotes {}>{notes}</w:footnotes>", story_namespaces());
        self.related(
            "rIdFootnotes",
            "footnotes",
            "word/footnotes.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
            &xml,
        )
    }

    pub fn comments(self, comments: &str) -> Docx {
        let xml = format!("<w:comments {}>{comments}</w:comments>", story_namespaces());
        self.related(
            "rIdComments",
            "comments",
            "word/comments.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            &xml,
        )
    }

    pub fn header(self, id: &str, name: &str, content: &str) -> Docx {
        let xml = format!("<w:hdr {}>{content}</w:hdr>", story_namespaces());
        self.related(
            id,
            "header",
            name,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
            &xml,
        )
    }

    pub fn footer(self, id: &str, name: &str, content: &str) -> Docx {
        let xml = format!("<w:ftr {}>{content}</w:ftr>", story_namespaces());
        self.related(
            id,
            "footer",
            name,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
            &xml,
        )
    }

    /// a picture the document refers to.
    pub fn image(mut self, id: &str, name: &str, bytes: &[u8]) -> Docx {
        self.rels.push((
            id.into(),
            format!("{REL}image"),
            name.trim_start_matches("word/").into(),
            false,
        ));
        self.parts
            .push((name.into(), String::new(), bytes.to_vec()));
        self.defaults.push(("png".into(), "image/png".into()));
        self
    }

    pub fn link(mut self, id: &str, url: &str) -> Docx {
        self.rels
            .push((id.into(), format!("{REL}hyperlink"), url.into(), true));
        self
    }

    /// a part nobody refers to (custom xml, say), carried as it is.
    pub fn loose_part(mut self, name: &str, content_type: &str, bytes: &[u8]) -> Docx {
        self.parts
            .push((name.into(), content_type.into(), bytes.to_vec()));
        self
    }

    pub fn build(self) -> Vec<u8> {
        let document = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <w:document {}><w:body>{}</w:body></w:document>",
            story_namespaces(),
            self.body
        );
        let mut types = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
             <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
             <Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        );
        for (extension, kind) in &self.defaults {
            types.push_str(&format!(
                "<Default Extension=\"{extension}\" ContentType=\"{kind}\"/>"
            ));
        }
        types.push_str(
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
        );
        for (name, kind, _) in &self.parts {
            if !kind.is_empty() {
                types.push_str(&format!(
                    "<Override PartName=\"/{name}\" ContentType=\"{kind}\"/>"
                ));
            }
        }
        types.push_str("</Types>");
        let package_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
            <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
            </Relationships>";
        let mut document_rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (id, kind, target, external) in &self.rels {
            let mode = if *external {
                " TargetMode=\"External\""
            } else {
                ""
            };
            document_rels.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"{mode}/>"
            ));
        }
        document_rels.push_str("</Relationships>");
        let mut parts: Vec<(&str, Vec<u8>)> = vec![
            ("[Content_Types].xml", types.into_bytes()),
            ("_rels/.rels", package_rels.as_bytes().to_vec()),
            ("word/document.xml", document.into_bytes()),
            ("word/_rels/document.xml.rels", document_rels.into_bytes()),
        ];
        for (name, _, bytes) in &self.parts {
            parts.push((name.as_str(), bytes.clone()));
        }
        zip(&parts)
    }
}

/// a paragraph with plain text, as word writes it.
pub fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// the bytes of a tiny valid png (one transparent pixel).
pub fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// every part of a package by name, for comparing bytes.
pub fn parts_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    letters_office::package::Package::read(bytes)
        .unwrap()
        .parts
        .into_iter()
        .map(|part| (part.name, part.bytes))
        .collect()
}

pub fn part_text(bytes: &[u8], name: &str) -> String {
    let package = letters_office::package::Package::read(bytes).unwrap();
    String::from_utf8(package.bytes(name).unwrap().to_vec()).unwrap()
}

/// an odt package from its content.xml body and optional styles.
pub fn odt(content: &str, styles: &str) -> Vec<u8> {
    odt_with(content, styles, "", &[])
}

pub const ODT_NAMESPACES: &str =
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
    xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" \
    xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" \
    xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" \
    xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" \
    xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" \
    xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
    xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
    xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" \
    xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" \
    xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" \
    office:version=\"1.3\"";

/// an odt with content, styles, automatic styles for content.xml and
/// extra files (pictures).
pub fn odt_with(
    content: &str,
    styles: &str,
    automatic: &str,
    extra: &[(&str, Vec<u8>)],
) -> Vec<u8> {
    let content_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {ODT_NAMESPACES}>\
         <office:automatic-styles>{automatic}</office:automatic-styles>\
         <office:body><office:text>{content}</office:text></office:body></office:document-content>"
    );
    let styles_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles {ODT_NAMESPACES}>{styles}</office:document-styles>"
    );
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-meta {ODT_NAMESPACES}>\
         <office:meta><dc:title>Test</dc:title><meta:initial-creator>Ada</meta:initial-creator></office:meta></office:document-meta>"
    );
    let mut manifest = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">\
         <manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
         <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
         <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>",
    );
    for (name, _) in extra {
        manifest.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"image/png\"/>"
        ));
    }
    manifest.push_str("</manifest:manifest>");
    let mut parts: Vec<(&str, Vec<u8>)> = vec![
        (
            "mimetype",
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        ("content.xml", content_xml.into_bytes()),
        ("styles.xml", styles_xml.into_bytes()),
        ("meta.xml", meta.into_bytes()),
        ("META-INF/manifest.xml", manifest.into_bytes()),
    ];
    for (name, bytes) in extra {
        parts.push((name, bytes.clone()));
    }
    zip(&parts)
}
