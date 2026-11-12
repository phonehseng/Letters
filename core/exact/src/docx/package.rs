//! the docx package: every part written by hand, so every byte word reads
//! is one this crate chose (recipe 10: settings, font table, embedded fonts).

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

use super::xml::{escape, NAMESPACES};
use crate::fonts::embed::obfuscate;

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAIN: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";
const OBFUSCATED_FONT_TYPE: &str = "application/vnd.openxmlformats-officedocument.obfuscatedFont";

/// a font embedded in the docx.
#[derive(Clone, Debug)]
pub struct EmbeddedFont {
    pub family: String,
    pub bytes: Vec<u8>,
    /// the guid word uses to undo the obfuscation
    pub font_key: String,
}

/// everything that goes into one docx.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub body: Vec<String>,
    pub final_section: String,
    pub fonts: Vec<EmbeddedFont>,
    pub media: Vec<(String, Vec<u8>)>,
    pub(crate) next_shape_id: u32,
}

impl Package {
    /// a fresh drawing id.
    pub fn shape_id(&mut self) -> u32 {
        self.next_shape_id += 1;
        self.next_shape_id + 1
    }

    /// stores a picture and returns its relationship id.
    pub fn add_media(&mut self, extension: &str, data: Vec<u8>) -> String {
        self.media
            .push((format!("image{}.{extension}", self.media.len() + 1), data));
        format!("rIdImage{}", self.media.len())
    }

    /// the zip bytes of the package.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let family = self
            .fonts
            .first()
            .map(|f| f.family.as_str())
            .unwrap_or("Arial");
        let (font_table, font_rels) = font_table_xml(family, &self.fonts);
        let mut parts: Vec<(String, Vec<u8>)> = vec![
            (
                "[Content_Types].xml".into(),
                self.content_types().into_bytes(),
            ),
            ("_rels/.rels".into(), package_rels().into_bytes()),
            ("word/document.xml".into(), self.document_xml().into_bytes()),
            (
                "word/_rels/document.xml.rels".into(),
                self.document_rels().into_bytes(),
            ),
            ("word/styles.xml".into(), styles_xml(family).into_bytes()),
            (
                "word/settings.xml".into(),
                settings_xml(!self.fonts.is_empty()).into_bytes(),
            ),
            ("word/fontTable.xml".into(), font_table.into_bytes()),
        ];
        if !self.fonts.is_empty() {
            parts.push((
                "word/_rels/fontTable.xml.rels".into(),
                font_rels.into_bytes(),
            ));
        }
        for (number, font) in self.fonts.iter().enumerate() {
            parts.push((
                format!("word/fonts/font{}.odttf", number + 1),
                obfuscate(&font.bytes, &font.font_key),
            ));
        }
        for (name, data) in &self.media {
            parts.push((format!("word/media/{name}"), data.clone()));
        }
        for (name, data) in parts {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
        zip.finish()
            .map(Cursor::into_inner)
            .map_err(|e| e.to_string())
    }

    fn document_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NAMESPACES}><w:body>{}{}</w:body></w:document>"#,
            self.body.concat(),
            self.final_section
        )
    }

    fn content_types(&self) -> String {
        format!(
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
                r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
                r#"<Default Extension="xml" ContentType="application/xml"/>"#,
                r#"<Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/>"#,
                r#"<Default Extension="odttf" ContentType="{font}"/>"#,
                r#"<Override PartName="/word/document.xml" ContentType="{main}.document.main+xml"/>"#,
                r#"<Override PartName="/word/styles.xml" ContentType="{main}.styles+xml"/>"#,
                r#"<Override PartName="/word/settings.xml" ContentType="{main}.settings+xml"/>"#,
                r#"<Override PartName="/word/fontTable.xml" ContentType="{main}.fontTable+xml"/>"#,
                "</Types>"
            ),
            font = OBFUSCATED_FONT_TYPE,
            main = MAIN
        )
    }

    fn document_rels(&self) -> String {
        let mut rels = vec![
            format!(r#"<Relationship Id="rId1" Type="{REL}/styles" Target="styles.xml"/>"#),
            format!(r#"<Relationship Id="rId2" Type="{REL}/settings" Target="settings.xml"/>"#),
            format!(r#"<Relationship Id="rId3" Type="{REL}/fontTable" Target="fontTable.xml"/>"#),
        ];
        for (index, (name, _)) in self.media.iter().enumerate() {
            rels.push(format!(
                r#"<Relationship Id="rIdImage{}" Type="{REL}/image" Target="media/{name}"/>"#,
                index + 1
            ));
        }
        relationships(&rels.concat())
    }
}

fn relationships(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{inner}</Relationships>"#
    )
}

fn package_rels() -> String {
    relationships(&format!(
        r#"<Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/>"#
    ))
}

/// document defaults that change nothing about placed text.
fn styles_xml(family: &str) -> String {
    let family = escape(family);
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {ns}>"#,
            "<w:docDefaults><w:rPrDefault><w:rPr>",
            r#"<w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:cs="{f}" w:eastAsia="{f}"/>"#,
            r#"<w:sz w:val="20"/><w:szCs w:val="20"/>"#,
            r#"<w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/><w14:ligatures w14:val="none"/>"#,
            "</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>{neutral}",
            r#"<w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>"#,
            "</w:pPr></w:pPrDefault></w:docDefaults>",
            r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#,
            "</w:styles>"
        ),
        ns = NAMESPACES,
        f = family,
        neutral = super::xml::LAYOUT_NEUTRAL,
    )
}

/// `w:embedTrueTypeFonts`, compatibility mode 15 and
/// `characterSpacingControl doNotCompress` (recipe 10).
fn settings_xml(embed_fonts: bool) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {ns}>"#,
            r#"{embed}<w:defaultTabStop w:val="720"/><w:autoHyphenation w:val="false"/>"#,
            r#"<w:characterSpacingControl w:val="doNotCompress"/>"#,
            r#"<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>"#,
            "</w:settings>"
        ),
        ns = NAMESPACES,
        embed = if embed_fonts {
            "<w:embedTrueTypeFonts/>"
        } else {
            ""
        },
    )
}

/// one `w:font` per family; embedded ones point at their own obfuscated part.
fn font_table_xml(family: &str, fonts: &[EmbeddedFont]) -> (String, String) {
    let mut names: Vec<&str> = fonts.iter().map(|f| f.family.as_str()).collect();
    if !names.contains(&family) {
        names.insert(0, family);
    }
    let mut entries = String::new();
    let mut rels = String::new();
    for name in names {
        let mut embed = String::new();
        for (number, font) in fonts.iter().enumerate() {
            if font.family == name {
                let id = number + 1;
                embed = format!(
                    r#"<w:embedRegular r:id="rIdFont{id}" w:fontKey="{}"/>"#,
                    font.font_key
                );
                rels.push_str(&format!(r#"<Relationship Id="rIdFont{id}" Type="{REL}/font" Target="fonts/font{id}.odttf"/>"#));
            }
        }
        entries.push_str(&format!(
            r#"<w:font w:name="{}"><w:charset w:val="00"/><w:family w:val="roman"/><w:pitch w:val="variable"/>{embed}</w:font>"#,
            escape(name)
        ));
    }
    let table = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts {NAMESPACES}>{entries}</w:fonts>"#
    );
    (table, relationships(&rels))
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn a_package_holds_every_part() {
        let mut package = Package::default();
        package.body.push("<w:p/>".into());
        package.final_section = super::super::xml::section_properties(612.0, 792.0);
        package.fonts.push(EmbeddedFont {
            family: "Ltr 1 12".into(),
            bytes: vec![1; 40],
            font_key: "{00010203-0405-0607-0809-0A0B0C0D0E0F}".into(),
        });
        let relationship = package.add_media("png", vec![0x89, b'P']);
        assert_eq!(relationship, "rIdImage1");
        let bytes = package.to_bytes().unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        for name in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/settings.xml",
            "word/fontTable.xml",
            "word/_rels/fontTable.xml.rels",
            "word/fonts/font1.odttf",
            "word/media/image1.png",
        ] {
            assert!(archive.by_name(name).is_ok(), "{name}");
        }
        let mut settings = String::new();
        archive
            .by_name("word/settings.xml")
            .unwrap()
            .read_to_string(&mut settings)
            .unwrap();
        assert!(settings.contains("<w:embedTrueTypeFonts/>"));
        assert!(settings.contains(r#"w:val="doNotCompress""#));
        let mut font = Vec::new();
        archive
            .by_name("word/fonts/font1.odttf")
            .unwrap()
            .read_to_end(&mut font)
            .unwrap();
        assert_eq!(
            obfuscate(&font, "{00010203-0405-0607-0809-0A0B0C0D0E0F}"),
            vec![1; 40]
        );
    }
}
