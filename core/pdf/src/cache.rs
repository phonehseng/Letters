//! a tile cache with a hard limit on how many bytes it may hold.
//!
//! when the limit is reached the tiles that were used longest ago are
//! dropped first. this is what stops a 500 page document, or ten open
//! windows, from slowly eating all the memory.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::TileKey;

/// finished tiles are shared, not copied, when they are handed out.
pub type TileBytes = Arc<Vec<u8>>;

/// a tile together with the moment it was last used.
struct Entry {
    tile: TileBytes,
    last_used: u64,
}

pub struct TileCache {
    budget_bytes: usize,
    used_bytes: usize,
    tiles: HashMap<TileKey, Entry>,
    /// the same tiles ordered by when they were last used, oldest first.
    /// an ordered map makes "touch a tile" and "find the oldest" quick
    /// however many tiles there are, where a plain list had to be
    /// searched from end to end on every use.
    by_age: BTreeMap<u64, TileKey>,
    /// counts uses. every use gets the next number, so numbers never repeat.
    clock: u64,
}

impl TileCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            tiles: HashMap::new(),
            by_age: BTreeMap::new(),
            clock: 0,
        }
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// returns the tile if it is cached, and marks it as just used.
    pub fn get(&mut self, key: &TileKey) -> Option<TileBytes> {
        let now = self.tick();
        let entry = self.tiles.get_mut(key)?;
        self.by_age.remove(&entry.last_used);
        entry.last_used = now;
        self.by_age.insert(now, *key);
        Some(entry.tile.clone())
    }

    /// stores a tile, then drops old tiles until the budget is met again.
    ///
    /// a tile bigger than the whole budget is simply not kept.
    pub fn insert(&mut self, key: TileKey, tile: TileBytes) {
        self.remove(&key);
        if tile.len() > self.budget_bytes {
            return;
        }
        let now = self.tick();
        self.used_bytes += tile.len();
        self.tiles.insert(
            key,
            Entry {
                tile,
                last_used: now,
            },
        );
        self.by_age.insert(now, key);
        self.drop_old_tiles_until_within_budget();
    }

    /// forgets every tile that matches, for example all tiles of a closed document.
    pub fn remove_where(&mut self, matches: impl Fn(&TileKey) -> bool) {
        let doomed: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|key| matches(key))
            .copied()
            .collect();
        for key in doomed {
            self.remove(&key);
        }
    }

    fn remove(&mut self, key: &TileKey) {
        if let Some(entry) = self.tiles.remove(key) {
            self.used_bytes -= entry.tile.len();
            self.by_age.remove(&entry.last_used);
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn drop_old_tiles_until_within_budget(&mut self) {
        while self.used_bytes > self.budget_bytes {
            let Some((_, oldest)) = self.by_age.pop_first() else {
                break;
            };
            if let Some(entry) = self.tiles.remove(&oldest) {
                self.used_bytes -= entry.tile.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentId;

    fn key(document: u64, page: u32) -> TileKey {
        TileKey {
            document: DocumentId(document),
            page,
            scale_milli: 1000,
            column: 0,
            row: 0,
        }
    }

    fn tile(bytes: usize) -> TileBytes {
        Arc::new(vec![0; bytes])
    }

    #[test]
    fn keeps_tiles_within_budget() {
        let mut cache = TileCache::new(100);
        cache.insert(key(1, 0), tile(40));
        cache.insert(key(1, 1), tile(40));
        assert_eq!(cache.used_bytes(), 80);
        assert!(cache.get(&key(1, 0)).is_some());
    }

    #[test]
    fn drops_the_least_recently_used_tile_first() {
        let mut cache = TileCache::new(100);
        cache.insert(key(1, 0), tile(40));
        cache.insert(key(1, 1), tile(40));
        // touching page 0 makes page 1 the oldest
        cache.get(&key(1, 0));
        cache.insert(key(1, 2), tile(40));

        assert!(cache.get(&key(1, 1)).is_none());
        assert!(cache.get(&key(1, 0)).is_some());
        assert!(cache.get(&key(1, 2)).is_some());
        assert!(cache.used_bytes() <= 100);
    }

    #[test]
    fn never_exceeds_the_budget() {
        let mut cache = TileCache::new(1000);
        for page in 0..500 {
            cache.insert(key(1, page), tile(64));
            assert!(cache.used_bytes() <= 1000);
        }
        assert_eq!(cache.len(), 1000 / 64);
    }

    #[test]
    fn ignores_a_tile_larger_than_the_budget() {
        let mut cache = TileCache::new(10);
        cache.insert(key(1, 0), tile(11));
        assert!(cache.is_empty());
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn replacing_a_tile_does_not_double_count_it() {
        let mut cache = TileCache::new(100);
        cache.insert(key(1, 0), tile(30));
        cache.insert(key(1, 0), tile(50));
        assert_eq!(cache.used_bytes(), 50);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn removes_all_tiles_of_one_document() {
        let mut cache = TileCache::new(1000);
        cache.insert(key(1, 0), tile(10));
        cache.insert(key(1, 1), tile(10));
        cache.insert(key(2, 0), tile(10));
        cache.remove_where(|key| key.document == DocumentId(1));

        assert_eq!(cache.len(), 1);
        assert_eq!(cache.used_bytes(), 10);
        assert!(cache.get(&key(2, 0)).is_some());
    }

    #[test]
    fn heavy_mixed_use_keeps_the_count_and_the_bytes_right() {
        let mut cache = TileCache::new(640);
        for step in 0..5_000u32 {
            let page = (step * 7) % 40;
            if step % 3 == 0 {
                cache.get(&key(1, page));
            } else {
                cache.insert(key(1, page), tile(64));
            }
            assert!(cache.used_bytes() <= 640);
            assert_eq!(cache.used_bytes(), cache.len() * 64);
        }
        cache.remove_where(|_| true);
        assert!(cache.is_empty());
        assert_eq!(cache.used_bytes(), 0);
    }
}
