//! what a font's os/2 `fsType` allows (recipe 3.7).

const RESTRICTED: u16 = 0x0002;
const PREVIEW_AND_PRINT: u16 = 0x0004;
const EDITABLE: u16 = 0x0008;

/// what fsType allows for a font embedded in an editable document.
pub fn embedding_permission(fs_type: Option<u16>) -> &'static str {
    match fs_type {
        None => "not stated (the program has no os/2 table)",
        Some(t) if t & EDITABLE != 0 => "editable embedding",
        Some(t) if t & PREVIEW_AND_PRINT != 0 => {
            "preview and print only: Word may open the file read-only"
        }
        Some(t) if t & RESTRICTED != 0 => "restricted: must not be embedded",
        Some(_) => "installable",
    }
}

/// a restricted font (bit 1 set, bits 2 and 3 clear) is not embedded; the
/// stand-in rule applies and the report says so. everything else may be.
pub fn may_embed(fs_type: Option<u16>) -> bool {
    match fs_type {
        None => true,
        Some(t) => !(t & RESTRICTED != 0 && t & (PREVIEW_AND_PRINT | EDITABLE) == 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_type_rules() {
        // the prototype's cases (test_prototype.py)
        assert!(may_embed(Some(0)));
        assert!(may_embed(Some(0x0008)));
        assert!(may_embed(Some(0x0004)));
        assert!(!may_embed(Some(0x0002)));
        assert!(may_embed(None));
        assert!(embedding_permission(Some(0x0004)).contains("read-only"));
        assert!(embedding_permission(None).contains("not stated"));
    }
}
