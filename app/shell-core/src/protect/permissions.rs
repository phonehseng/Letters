//! reading the permissions a locked file asks for, in the engine's terms.
//!
//! a pdf stores them as bits of a number (/P). the engine writes them
//! from `PermissionOptions`; here the bits of a file someone else locked
//! are turned back into the same options, so unlocking a file and saving
//! it again keeps what its author asked for.

use letters_redact::lopdf::Permissions;
use letters_redact::{PermissionOptions, PrintPermission};

/// the options a file's permission bits stand for. `revision` is the
/// security handler's revision: revision 2 files have no separate bits
/// for high quality printing, filling forms or assembling pages.
pub fn options_from_bits(bits: Permissions, revision: i64) -> PermissionOptions {
    let allows = |flag: Permissions| bits.contains(flag);
    let modern = revision >= 3;
    let printing = if !allows(Permissions::PRINTABLE) {
        PrintPermission::None
    } else if !modern || allows(Permissions::PRINTABLE_IN_HIGH_QUALITY) {
        PrintPermission::Full
    } else {
        PrintPermission::LowQuality
    };
    PermissionOptions {
        printing,
        copying: allows(Permissions::COPYABLE),
        editing: allows(Permissions::MODIFIABLE),
        commenting: allows(Permissions::ANNOTABLE),
        filling_forms: allows(Permissions::FILLABLE) || (!modern && allows(Permissions::ANNOTABLE)),
        assembling: if modern {
            allows(Permissions::ASSEMBLABLE)
        } else {
            allows(Permissions::MODIFIABLE)
        },
    }
}

/// the bits as stored in /P: a signed 32 bit number in the file.
pub fn bits_from_p(p: i64) -> Permissions {
    Permissions::from_bits_truncate(u64::from(p as u32))
}

/// true when anything at all is restricted.
pub fn restricts_anything(options: &PermissionOptions) -> bool {
    *options != PermissionOptions::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_allowed_reads_as_the_default() {
        let all = Permissions::all();
        assert_eq!(options_from_bits(all, 6), PermissionOptions::default());
        assert!(!restricts_anything(&options_from_bits(all, 6)));
    }

    #[test]
    fn printing_has_three_levels() {
        let low = Permissions::PRINTABLE;
        assert_eq!(
            options_from_bits(low, 4).printing,
            PrintPermission::LowQuality
        );
        assert_eq!(options_from_bits(low, 2).printing, PrintPermission::Full);
        assert_eq!(
            options_from_bits(Permissions::empty(), 6).printing,
            PrintPermission::None
        );
    }

    #[test]
    fn a_negative_p_value_keeps_its_low_bits() {
        // -3904 is the common /P that allows nothing; -1028 allows all but
        // assembling (bit 11, 1 << 10, is the only one cleared)
        let nothing = options_from_bits(bits_from_p(-3904), 4);
        assert_eq!(nothing.printing, PrintPermission::None);
        assert!(!nothing.copying && !nothing.editing && !nothing.commenting);
        assert!(!nothing.filling_forms && !nothing.assembling);
        let no_assembly = options_from_bits(bits_from_p(-1028), 4);
        assert_eq!(no_assembly.printing, PrintPermission::Full);
        assert!(no_assembly.copying && no_assembly.editing && no_assembly.filling_forms);
        assert!(!no_assembly.assembling);
    }
}
