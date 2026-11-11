//! the addresses the ui uses to ask for images.
//!
//! a tile address looks like `/3/0/1500/1/2`: document 3, page 0,
//! 1.5 pixels per point, column 1, row 2. a thumbnail address looks
//! like `/thumbnail/0123456789abcdef`.

use letters_pdf::{DocumentId, TileKey};

use crate::recents::is_thumbnail_name;

/// what an address is asking for.
#[derive(Debug, PartialEq)]
pub enum Wanted {
    Tile(TileKey),
    Thumbnail(String),
}

/// reads an address, or returns none if it is not one of ours.
///
/// anything unexpected is refused here, so a strange address can never
/// reach the file system.
pub fn parse(path: &str) -> Option<Wanted> {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    match parts.as_slice() {
        ["thumbnail", name] if is_thumbnail_name(name) => Some(Wanted::Thumbnail(name.to_string())),
        [document, page, scale, column, row] => Some(Wanted::Tile(TileKey {
            document: DocumentId(document.parse().ok()?),
            page: page.parse().ok()?,
            scale_milli: scale.parse().ok()?,
            column: column.parse().ok()?,
            row: row.parse().ok()?,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_tile_address() {
        let wanted = parse("/3/0/1500/1/2");
        assert_eq!(
            wanted,
            Some(Wanted::Tile(TileKey {
                document: DocumentId(3),
                page: 0,
                scale_milli: 1500,
                column: 1,
                row: 2,
            }))
        );
    }

    #[test]
    fn reads_a_thumbnail_address() {
        assert_eq!(
            parse("/thumbnail/0123456789abcdef"),
            Some(Wanted::Thumbnail("0123456789abcdef".into()))
        );
    }

    #[test]
    fn rejects_anything_else() {
        assert_eq!(parse("/"), None);
        assert_eq!(parse("/3/0/1500/1"), None);
        assert_eq!(parse("/3/0/1500/1/x"), None);
        assert_eq!(parse("/3/-1/1500/1/2"), None);
        assert_eq!(parse("/thumbnail/..%2F..%2Fsecret"), None);
        assert_eq!(parse("/thumbnail/../../etc/passwd"), None);
    }
}
