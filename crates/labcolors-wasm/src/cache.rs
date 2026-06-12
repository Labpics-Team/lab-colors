//! A contract cache for resolved theme sets, keyed by `(bgHex, theme, table
//! fingerprint)`.
//!
//! Re-solving the same background under the same theme is the common case while
//! a tool tweaks a colour, and a resolve sweep is real work. The cache returns
//! the byte-identical prior result for a repeated key. It is *contractual*: the
//! key carries every input that can change the output, so a hit is always
//! correct, never stale.
//!
//! The table fingerprint is the third key component. v1 ships only the default
//! [`RoleTable`](labcolors_core::RoleTable), so the fingerprint is a constant
//! today — but it is a real key slot, not a hard-coded omission. When a future
//! engine carries an overridden table, the fingerprint changes with it and the
//! cache invalidates the affected entries automatically (no entry built under
//! one table can alias another). See [`DEFAULT_TABLE_FINGERPRINT`].
//!
//! Single-threaded by design: WASM has no threads, so a `RefCell` interior is
//! the right shared-mutability tool — no lock, no contention, no `Send` bound.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::theme::Theme;

/// The fingerprint of the default role table.
///
/// SEAM: a constant while only [`RoleTable::default`](labcolors_core::RoleTable)
/// is reachable through the public engine. The moment a table override lands,
/// this becomes a hash of the table's specs + chroma, computed where the table
/// is built, and threaded into [`CacheKey`]. Until then it is one stable value
/// so every default-table resolve shares a cache namespace.
pub const DEFAULT_TABLE_FINGERPRINT: u64 = 0;

/// The full key of a cached resolve: every input that can change the output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    bg_hex: String,
    theme: &'static str,
    table_fingerprint: u64,
}

impl CacheKey {
    /// Build a key from a normalised background hex, a theme, and a table
    /// fingerprint. The hex is normalised by the caller (uppercased, `#`-led)
    /// so `#fff` and `#FFFFFF` never split the cache once expanded upstream.
    pub fn new(bg_hex: String, theme: Theme, table_fingerprint: u64) -> Self {
        Self {
            bg_hex,
            theme: theme.key(),
            table_fingerprint,
        }
    }
}

/// A bounded, single-threaded memo from [`CacheKey`] to a cached value `V`.
///
/// Bounded memory is a correctness property under sustained load (ZERO
/// SURPRISES): an unbounded map keyed on arbitrary backgrounds could grow
/// without limit. At capacity the cache is cleared wholesale — a cold rebuild,
/// never a wrong answer. `V` is cloned on a hit, so callers pass a cheaply
/// cloneable value (e.g. an `Rc`-backed or already-serialised result).
pub struct ContractCache<V> {
    entries: RefCell<HashMap<CacheKey, V>>,
    capacity: usize,
}

impl<V: Clone> ContractCache<V> {
    /// A cache holding up to `capacity` distinct keys before a wholesale clear.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: RefCell::new(HashMap::new()),
            capacity,
        }
    }

    /// Return the cached value for `key`, computing and storing it with `build`
    /// on a miss. `build` runs at most once per distinct key between clears.
    pub fn get_or_insert_with(&self, key: CacheKey, build: impl FnOnce() -> V) -> V {
        if let Some(hit) = self.entries.borrow().get(&key) {
            return hit.clone();
        }
        let value = build();
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= self.capacity {
            entries.clear();
        }
        entries.insert(key, value.clone());
        value
    }

    /// Number of live entries — for tests and introspection.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn builds_once_then_serves_from_cache() {
        let cache: ContractCache<u32> = ContractCache::new(8);
        let calls = Cell::new(0);
        let key = || CacheKey::new("#FFFFFF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT);

        let first = cache.get_or_insert_with(key(), || {
            calls.set(calls.get() + 1);
            42
        });
        let second = cache.get_or_insert_with(key(), || {
            calls.set(calls.get() + 1);
            99
        });

        assert_eq!(first, 42);
        assert_eq!(second, 42, "second call must hit the cache, not rebuild");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn distinct_keys_do_not_collide() {
        let cache: ContractCache<&str> = ContractCache::new(8);
        let light = cache.get_or_insert_with(
            CacheKey::new("#FFFFFF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT),
            || "light",
        );
        let dark = cache.get_or_insert_with(
            CacheKey::new("#FFFFFF".into(), Theme::Dark, DEFAULT_TABLE_FINGERPRINT),
            || "dark",
        );
        assert_eq!(light, "light");
        assert_eq!(dark, "dark");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn clears_wholesale_at_capacity() {
        let cache: ContractCache<u32> = ContractCache::new(2);
        for i in 0..2 {
            cache.get_or_insert_with(
                CacheKey::new(
                    format!("#00000{i}"),
                    Theme::Light,
                    DEFAULT_TABLE_FINGERPRINT,
                ),
                || i,
            );
        }
        assert_eq!(cache.len(), 2);
        // The third distinct key trips the cap → wholesale clear, then insert.
        cache.get_or_insert_with(
            CacheKey::new("#0000FF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT),
            || 3,
        );
        assert_eq!(
            cache.len(),
            1,
            "cap trips a wholesale clear, never unbounded growth"
        );
    }
}
