//! A contract cache for resolved theme sets, keyed by `(bgHex, theme, table
//! fingerprint)`.
//!
//! Re-solving the same background under the same theme is the common case while
//! a tool tweaks a colour, and a resolve sweep is real work. The cache returns
//! the byte-identical prior result for a repeated key. It is *contractual*: the
//! key carries every input that can change the output, so a hit is always
//! correct, never stale.
//!
//! The table fingerprint is the third key component. A loaded config
//! (`loadConfig`) carries a real fingerprint — an FNV-1a over its canonical DTO,
//! computed in the engine and threaded into the key. Correctness across a config
//! switch does not rest on that fingerprint being unique, though: `loadConfig`
//! wholesale-clears the cache (see [`ContractCache::clear`]), so exactly one key
//! namespace is ever live and a stale entry from another config cannot be served.
//!
//! Каждый `Engine` живёт в одном JavaScript-агенте, поэтому `RefCell` даёт
//! нужную внутреннюю изменяемость без локов и `Send`-обязательств.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::theme::Theme;

/// A stable, arbitrary fingerprint used by the cache's own unit tests as a
/// single key namespace. Production keys always carry a real config fingerprint
/// (an FNV-1a over the canonical DTO, computed in the engine); this constant
/// exists only so the cache tests can key on a fixed value.
#[cfg(test)]
pub(crate) const DEFAULT_TABLE_FINGERPRINT: u64 = 0;

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
    /// Ключи, чей `build` выполняется прямо сейчас: жёсткий страж
    /// same-key-реентерабельности (см. `get_or_try_insert_with`).
    building: RefCell<HashSet<CacheKey>>,
    capacity: usize,
}

impl<V: Clone> ContractCache<V> {
    /// A cache holding up to `capacity` distinct keys before a wholesale clear.
    ///
    /// `capacity` must be at least 1; a zero-capacity cache is a configuration
    /// error (it could never hold the entry it just built), so it is rejected
    /// up front rather than degrading silently.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ContractCache capacity must be at least 1");
        Self {
            entries: RefCell::new(HashMap::new()),
            building: RefCell::new(HashSet::new()),
            capacity,
        }
    }

    /// Вернуть закэшированное значение для `key`, при промахе вычислив и
    /// сохранив его через `build`. Кэш мутирует только успешная сборка: ошибка
    /// ничего не вставляет и не вытесняет существующее успешное значение.
    ///
    /// # Реентерабельность
    /// `build` не смеет звать `get_or_try_insert_with` этого же кэша с тем же
    /// `key`: запись появляется только после возврата `build`, так что
    /// same-key-реентерабельность зациклилась бы. Это не предусловие на
    /// честном слове, а ЖЁСТКИЙ страж: повторный вход по строящемуся ключу —
    /// детерминированная паника с внятным сообщением (fail-closed contract
    /// drift), а не переполнение стека. Другой ключ безопасен; страж
    /// снимается и при ошибке `build` (drop-guard).
    pub fn get_or_try_insert_with<E>(
        &self,
        key: CacheKey,
        build: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        if let Some(hit) = self.entries.borrow().get(&key) {
            return Ok(hit.clone());
        }
        assert!(
            self.building.borrow_mut().insert(key.clone()),
            "ContractCache: реентерабельный build по тому же ключу — контрактный дрейф"
        );
        struct BuildingGuard<'a> {
            building: &'a RefCell<HashSet<CacheKey>>,
            key: &'a CacheKey,
        }
        impl Drop for BuildingGuard<'_> {
            fn drop(&mut self) {
                self.building.borrow_mut().remove(self.key);
            }
        }
        let guard = BuildingGuard {
            building: &self.building,
            key: &key,
        };
        let value = build()?;
        drop(guard);
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= self.capacity {
            entries.clear();
        }
        entries.insert(key, value.clone());
        Ok(value)
    }

    /// Очистить кэш целиком. Смена таблицы (загрузка конфига) обязана снести
    /// прошлое пространство записей: одновременно в кэше живёт ровно ОДНО
    /// пространство ключей, и корректность не опирается на вероятностную
    /// уникальность отпечатка.
    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
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
    use std::rc::Rc;

    #[test]
    fn failed_build_is_not_cached_and_a_later_success_is_shared() {
        let cache: ContractCache<Rc<u32>> = ContractCache::new(8);
        let calls = Cell::new(0);
        let key = || CacheKey::new("#FFFFFF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT);

        let failed: Result<Rc<u32>, &'static str> = cache.get_or_try_insert_with(key(), || {
            calls.set(calls.get() + 1);
            Err("injected failure")
        });
        assert_eq!(failed, Err("injected failure"));
        assert_eq!(cache.len(), 0, "a failed build must not mutate the cache");

        let inserted = cache
            .get_or_try_insert_with(key(), || {
                calls.set(calls.get() + 1);
                Ok::<_, &'static str>(Rc::new(42))
            })
            .unwrap();
        let hit = cache
            .get_or_try_insert_with(key(), || -> Result<Rc<u32>, &'static str> {
                panic!("cache hit must not run the builder")
            })
            .unwrap();
        assert_eq!(calls.get(), 2, "the failed key is retried exactly once");
        assert!(Rc::ptr_eq(&inserted, &hit), "a hit reuses the same theme");
    }

    #[test]
    fn failed_miss_at_capacity_preserves_every_successful_entry() {
        let cache: ContractCache<Rc<u32>> = ContractCache::new(2);
        let first_key = CacheKey::new("#000000".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT);
        let second_key = CacheKey::new("#111111".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT);
        let first = cache
            .get_or_try_insert_with(first_key.clone(), || Ok::<_, &'static str>(Rc::new(1)))
            .unwrap();
        cache
            .get_or_try_insert_with(second_key, || Ok::<_, &'static str>(Rc::new(2)))
            .unwrap();
        assert_eq!(cache.len(), 2);

        let failed: Result<Rc<u32>, &'static str> = cache.get_or_try_insert_with(
            CacheKey::new("#222222".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT),
            || Err("injected failure"),
        );
        assert_eq!(failed, Err("injected failure"));
        assert_eq!(cache.len(), 2, "an error cannot trigger wholesale eviction");

        let first_hit = cache
            .get_or_try_insert_with(first_key, || -> Result<Rc<u32>, &'static str> {
                panic!("preserved successful entry must still hit")
            })
            .unwrap();
        assert!(Rc::ptr_eq(&first, &first_hit));
    }

    #[test]
    fn builds_once_then_serves_from_cache() {
        let cache: ContractCache<u32> = ContractCache::new(8);
        let calls = Cell::new(0);
        let key = || CacheKey::new("#FFFFFF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT);

        let first = cache
            .get_or_try_insert_with(key(), || {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(42)
            })
            .unwrap();
        let second = cache
            .get_or_try_insert_with(key(), || {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(99)
            })
            .unwrap();

        assert_eq!(first, 42);
        assert_eq!(second, 42, "second call must hit the cache, not rebuild");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn distinct_keys_do_not_collide() {
        let cache: ContractCache<&str> = ContractCache::new(8);
        let light = cache
            .get_or_try_insert_with(
                CacheKey::new("#FFFFFF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT),
                || Ok::<_, ()>("light"),
            )
            .unwrap();
        let dark = cache
            .get_or_try_insert_with(
                CacheKey::new("#FFFFFF".into(), Theme::Dark, DEFAULT_TABLE_FINGERPRINT),
                || Ok::<_, ()>("dark"),
            )
            .unwrap();
        assert_eq!(light, "light");
        assert_eq!(dark, "dark");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn clears_wholesale_at_capacity() {
        let cache: ContractCache<u32> = ContractCache::new(2);
        for i in 0..2 {
            cache
                .get_or_try_insert_with(
                    CacheKey::new(
                        format!("#00000{i}"),
                        Theme::Light,
                        DEFAULT_TABLE_FINGERPRINT,
                    ),
                    || Ok::<_, ()>(i),
                )
                .unwrap();
        }
        assert_eq!(cache.len(), 2);
        // The third distinct key trips the cap → wholesale clear, then insert.
        cache
            .get_or_try_insert_with(
                CacheKey::new("#0000FF".into(), Theme::Light, DEFAULT_TABLE_FINGERPRINT),
                || Ok::<_, ()>(3),
            )
            .unwrap();
        assert_eq!(
            cache.len(),
            1,
            "cap trips a wholesale clear, never unbounded growth"
        );
    }
}

#[cfg(test)]
mod reentrancy_tests {
    use super::*;

    /// Страж реентерабельности — жёсткий: same-key build падает детерминированной
    /// паникой (контрактный дрейф), а не переполнением стека.
    #[test]
    #[should_panic(expected = "реентерабельный build")]
    fn same_key_reentrant_build_panics_deterministically() {
        let cache: ContractCache<u32> = ContractCache::new(4);
        let key = CacheKey::new(
            "#FFFFFF".to_string(),
            Theme::Light,
            DEFAULT_TABLE_FINGERPRINT,
        );
        let key_inner = key.clone();
        let _ = cache.get_or_try_insert_with::<()>(key, || {
            // Тот же ключ изнутри build — обязан паниковать, не рекурсировать.
            cache
                .get_or_try_insert_with::<()>(key_inner.clone(), || Ok(1))
                .map(|_| 2)
        });
    }

    /// Другой ключ изнутри build безопасен, а страж снимается и после ошибки:
    /// повторная сборка того же ключа после Err проходит.
    #[test]
    fn different_key_nested_build_is_safe_and_guard_lifts_on_error() {
        let cache: ContractCache<u32> = ContractCache::new(4);
        let a = CacheKey::new(
            "#FFFFFF".to_string(),
            Theme::Light,
            DEFAULT_TABLE_FINGERPRINT,
        );
        let b = CacheKey::new(
            "#000000".to_string(),
            Theme::Light,
            DEFAULT_TABLE_FINGERPRINT,
        );
        let b_inner = b.clone();
        let nested = cache
            .get_or_try_insert_with::<()>(a.clone(), || {
                cache
                    .get_or_try_insert_with::<()>(b_inner.clone(), || Ok(7))
                    .map(|inner| inner + 1)
            })
            .unwrap();
        assert_eq!(nested, 8);

        let c = CacheKey::new(
            "#123456".to_string(),
            Theme::Dark,
            DEFAULT_TABLE_FINGERPRINT,
        );
        assert!(
            cache
                .get_or_try_insert_with(c.clone(), || Err::<u32, _>("boom"))
                .is_err()
        );
        // Страж снят drop-guard'ом — та же клетка строится снова.
        assert_eq!(
            cache.get_or_try_insert_with::<()>(c, || Ok(42)).unwrap(),
            42
        );
    }
}
