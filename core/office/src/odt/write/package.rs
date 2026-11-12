//! meta.xml and the manifest, the parts that describe the package.

use super::*;

impl OdtWriter<'_> {
    // meta and manifest ------------------------------------------------------------

    pub(super) fn meta_xml(&self) -> String {
        let metadata = &self.doc.metadata;
        let mut xml = XmlWriter::with_declaration();
        xml.raw(&self.root("meta.xml", "office:document-meta"));
        xml.raw("<office:meta>");
        for (name, value) in [
            ("dc:title", &metadata.title),
            ("dc:subject", &metadata.subject),
            ("dc:description", &metadata.description),
            ("meta:keyword", &metadata.keywords),
            ("meta:initial-creator", &metadata.author),
            ("dc:creator", &metadata.last_modified_by),
            ("meta:creation-date", &metadata.created),
            ("dc:date", &metadata.modified),
            ("dc:language", &metadata.language),
        ] {
            if let Some(value) = value {
                xml.raw(&format!("<{name}>{}</{name}>", escape_text(value)));
            }
        }
        if let Some(odt) = self.odt_carry() {
            for extra in &odt.meta_extras {
                xml.raw(extra);
            }
        } else {
            xml.raw("<meta:generator>Letters</meta:generator>");
        }
        xml.raw("</office:meta></office:document-meta>");
        xml.finish()
    }

    pub(super) fn manifest_xml(&self, parts: &[Part]) -> String {
        let carried: HashMap<&str, &str> = self
            .odt_carry()
            .map(|odt| {
                odt.manifest
                    .iter()
                    .map(|e| (e.name.as_str(), e.value.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let mut xml = XmlWriter::with_declaration();
        // libreoffice refuses a package whose manifest names another odf
        // version than its parts do, and the parts keep a native file's own
        // version (1.4 from libreoffice 25), so the manifest says the same
        let version = escape_attribute(&self.odf_version());
        xml.raw(&format!(
            "<manifest:manifest xmlns:manifest=\"{}\" manifest:version=\"{version}\">",
            namespace_uri("manifest")
        ));
        xml.raw(&format!(
            "<manifest:file-entry manifest:full-path=\"/\" manifest:version=\"{version}\" manifest:media-type=\"{}\"/>",
            escape_attribute(&self.mimetype())
        ));
        for part in parts.iter().filter(|p| p.name != "mimetype") {
            let media_type = match part.name.as_str() {
                "content.xml" | "styles.xml" | "meta.xml" | "settings.xml" => {
                    "text/xml".to_string()
                }
                name => carried
                    .get(name)
                    .map(|t| t.to_string())
                    .or_else(|| {
                        self.doc
                            .media
                            .iter()
                            .find(|m| self.media_paths[&m.id] == name)
                            .map(|m| m.content_type.clone())
                    })
                    .unwrap_or_default(),
            };
            xml.raw(&format!(
                "<manifest:file-entry manifest:full-path=\"{}\" manifest:media-type=\"{}\"/>",
                escape_attribute(&part.name),
                escape_attribute(&media_type)
            ));
        }
        xml.raw("</manifest:manifest>");
        xml.finish()
    }
}
