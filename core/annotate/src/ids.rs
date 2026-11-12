//! the stable names (/NM) that identify annotations.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// every name Letters makes starts with this, which helps when reading a
/// file to see which marks came from Letters.
pub(crate) const NAME_PREFIX: &str = "letters-";

/// prefix of the ids given to annotations from other apps that have no
/// /NM yet. the rest is the object number and generation, e.g. "ref:12-0".
pub(crate) const REFERENCE_PREFIX: &str = "ref:";

/// a new name that won't clash with any other, in this file or another.
///
/// the time, a counter and the randomly seeded hasher of the standard
/// library are mixed together, so two names made in the same instant
/// still differ and no extra dependency is needed for randomness.
pub(crate) fn new_unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{NAME_PREFIX}{:016x}{:08x}", hasher.finish(), nanos as u32)
}

/// the id reported for an annotation with no /NM, from its object number.
pub(crate) fn reference_id((number, generation): (u32, u16)) -> String {
    format!("{REFERENCE_PREFIX}{number}-{generation}")
}

/// reads an id made by `reference_id` back into an object number.
pub(crate) fn parse_reference_id(id: &str) -> Option<(u32, u16)> {
    let rest = id.strip_prefix(REFERENCE_PREFIX)?;
    let (number, generation) = rest.split_once('-')?;
    Some((number.parse().ok()?, generation.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_do_not_repeat() {
        let names: HashSet<String> = (0..1000).map(|_| new_unique_name()).collect();
        assert_eq!(names.len(), 1000);
        assert!(names.iter().all(|name| name.starts_with(NAME_PREFIX)));
    }

    #[test]
    fn reference_ids_round_trip() {
        assert_eq!(reference_id((12, 0)), "ref:12-0");
        assert_eq!(parse_reference_id("ref:12-0"), Some((12, 0)));
        assert_eq!(parse_reference_id("letters-abc"), None);
    }
}
