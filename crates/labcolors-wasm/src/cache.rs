//! Контрактный кэш последнего резолва в каждом публичном слоте темы.
//!
//! Полный результат содержит весь набор ролей и его JSON-проекцию, поэтому
//! число произвольных фонов нельзя превращать в число одновременно живых
//! результатов. Здесь нет лимита «на глаз» и порога массовой очистки: структура
//! имеет ровно по одному слоту на каждый вариант [`Theme`]. Новый фон заменяет
//! предыдущий результат только своей темы, а остальные темы не охлаждаются.
//!
//! Ключ хранит все входы, способные изменить результат. При успешной загрузке
//! конфига движок дополнительно очищает все слоты, поэтому корректность при
//! смене контракта не зависит от отсутствия коллизий 64-битного отпечатка.
//!
//! WASM-движок однопоточный, поэтому `RefCell` даёт нужную внутреннюю
//! изменяемость без блокировок и ложной гарантии `Send`.

use std::cell::RefCell;

use crate::theme::Theme;

/// Стабильное пространство ключей для модульных тестов самого кэша.
#[cfg(test)]
pub(crate) const DEFAULT_TABLE_FINGERPRINT: u64 = 0;

/// Полный ключ резолва внутри одного загруженного контракта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    bg_hex: String,
    theme: Theme,
    table_fingerprint: u64,
}

impl CacheKey {
    /// Создать ключ из уже нормализованного фона, темы и отпечатка таблицы.
    /// Нормализация выполняется до обращения к кэшу, чтобы разные записи одного
    /// `#RRGGBB` не занимали разные состояния.
    pub fn new(bg_hex: String, theme: Theme, table_fingerprint: u64) -> Self {
        Self {
            bg_hex,
            theme,
            table_fingerprint,
        }
    }
}

struct CacheEntry<V> {
    key: CacheKey,
    value: V,
}

/// Именованные поля намеренно повторяют варианты `Theme`: так ограничение
/// памяти проверяется компилятором при каждом `match`, а не спрятано в числе.
struct ThemeSlots<V> {
    light: Option<CacheEntry<V>>,
    dark: Option<CacheEntry<V>>,
    light_ic: Option<CacheEntry<V>>,
    dark_ic: Option<CacheEntry<V>>,
}

impl<V> ThemeSlots<V> {
    fn empty() -> Self {
        Self {
            light: None,
            dark: None,
            light_ic: None,
            dark_ic: None,
        }
    }

    fn get(&self, theme: Theme) -> &Option<CacheEntry<V>> {
        match theme {
            Theme::Light => &self.light,
            Theme::Dark => &self.dark,
            Theme::LightIc => &self.light_ic,
            Theme::DarkIc => &self.dark_ic,
        }
    }

    fn get_mut(&mut self, theme: Theme) -> &mut Option<CacheEntry<V>> {
        match theme {
            Theme::Light => &mut self.light,
            Theme::Dark => &mut self.dark,
            Theme::LightIc => &mut self.light_ic,
            Theme::DarkIc => &mut self.dark_ic,
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        [
            self.light.is_some(),
            self.dark.is_some(),
            self.light_ic.is_some(),
            self.dark_ic.is_some(),
        ]
        .into_iter()
        .filter(|occupied| *occupied)
        .count()
    }
}

/// Однопоточный memo: один последний ключ и результат на каждую тему.
///
/// Объём служебной структуры постоянен, а число тяжёлых значений не может
/// превысить число вариантов `Theme`. Поэтому последовательность из миллионов
/// уникальных фонов не создаёт ни линейного роста, ни скачка очистки на N+1.
pub struct ContractCache<V> {
    slots: RefCell<ThemeSlots<V>>,
}

impl<V: Clone> ContractCache<V> {
    /// Создать пустые тематические слоты.
    pub fn new() -> Self {
        Self {
            slots: RefCell::new(ThemeSlots::empty()),
        }
    }

    /// Вернуть результат по ключу или вычислить и сохранить его при промахе.
    /// Ошибка построения не занимает слот и не вытесняет предыдущий корректный
    /// результат: это важно для атомарности невозможной/невалидной проекции.
    ///
    /// Замыкание выполняется без активного заимствования `RefCell`, поэтому
    /// может безопасно обращаться к другому слоту этого же кэша. Повторный вход
    /// с тем же ключом остаётся логической ошибкой вызывающего: значение ещё не
    /// построено, и рекурсия не сможет завершиться.
    pub fn get_or_try_insert_with<E>(
        &self,
        key: CacheKey,
        build: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        let theme = key.theme;
        if let Some(hit) = self
            .slots
            .borrow()
            .get(theme)
            .as_ref()
            .filter(|entry| entry.key == key)
        {
            return Ok(hit.value.clone());
        }

        let value = build()?;
        *self.slots.borrow_mut().get_mut(theme) = Some(CacheEntry {
            key,
            value: value.clone(),
        });
        Ok(value)
    }

    /// Очистить все тематические слоты после успешной смены контракта.
    pub fn clear(&self) {
        *self.slots.borrow_mut() = ThemeSlots::empty();
    }

    /// Число живых полных результатов; доступно только проверкам инварианта.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn key(bg: &str, theme: Theme, fingerprint: u64) -> CacheKey {
        CacheKey::new(bg.to_owned(), theme, fingerprint)
    }

    #[test]
    fn повторный_ключ_строится_ровно_один_раз() {
        let cache: ContractCache<u32> = ContractCache::new();
        let calls = Cell::new(0);

        let first = cache
            .get_or_try_insert_with::<()>(
                key("#FFFFFF", Theme::Light, DEFAULT_TABLE_FINGERPRINT),
                || {
                    calls.set(calls.get() + 1);
                    Ok(42)
                },
            )
            .unwrap();
        let second = cache
            .get_or_try_insert_with::<()>(
                key("#FFFFFF", Theme::Light, DEFAULT_TABLE_FINGERPRINT),
                || {
                    calls.set(calls.get() + 1);
                    Ok(99)
                },
            )
            .unwrap();

        assert_eq!((first, second), (42, 42));
        assert_eq!(calls.get(), 1, "попадание не должно запускать построитель");
    }

    #[test]
    fn тема_хранит_только_свой_последний_фон() {
        let cache: ContractCache<&str> = ContractCache::new();
        cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Light, 1), || Ok("white"))
            .unwrap();
        cache
            .get_or_try_insert_with::<()>(key("#000000", Theme::Light, 1), || Ok("black"))
            .unwrap();

        let rebuilt = Cell::new(false);
        let value = cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Light, 1), || {
                rebuilt.set(true);
                Ok("white-again")
            })
            .unwrap();
        assert!(
            rebuilt.get(),
            "первый фон обязан быть вытеснен новым фоном той же темы"
        );
        assert_eq!(value, "white-again");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn четыре_темы_не_вытесняют_друг_друга() {
        let cache: ContractCache<Theme> = ContractCache::new();
        let themes = [Theme::Light, Theme::Dark, Theme::LightIc, Theme::DarkIc];
        for theme in themes {
            cache
                .get_or_try_insert_with::<()>(key("#808080", theme, 7), || Ok(theme))
                .unwrap();
        }
        for theme in themes {
            let built = Cell::new(false);
            let value = cache
                .get_or_try_insert_with::<()>(key("#808080", theme, 7), || {
                    built.set(true);
                    Ok(theme)
                })
                .unwrap();
            assert!(
                !built.get(),
                "слот {theme:?} должен пережить обращения к другим темам"
            );
            assert_eq!(value, theme);
        }
        assert_eq!(cache.len(), themes.len());
    }

    #[test]
    fn произвольный_поток_ключей_не_увеличивает_число_тяжёлых_значений() {
        let cache: ContractCache<std::rc::Rc<usize>> = ContractCache::new();
        let themes = [Theme::Light, Theme::Dark, Theme::LightIc, Theme::DarkIc];
        let mut payloads = Vec::new();

        for i in 0..10_000 {
            let theme = themes[i % themes.len()];
            let payload = std::rc::Rc::new(i);
            payloads.push(std::rc::Rc::downgrade(&payload));
            cache
                .get_or_try_insert_with::<()>(key(&format!("#{i:06X}"), theme, 11), || {
                    Ok(std::rc::Rc::clone(&payload))
                })
                .unwrap();
        }

        assert_eq!(
            cache.len(),
            themes.len(),
            "длина определяется словарём тем, а не числом входных фонов"
        );
        assert_eq!(
            payloads
                .into_iter()
                .filter(|payload| payload.upgrade().is_some())
                .count(),
            themes.len(),
            "кэш обязан освободить вытесненные payload, а не только скрыть их из len()"
        );
    }

    #[test]
    fn другой_отпечаток_не_может_попасть_в_старую_запись() {
        let cache: ContractCache<&str> = ContractCache::new();
        cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Light, 1), || Ok("config-a"))
            .unwrap();
        let value = cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Light, 2), || Ok("config-b"))
            .unwrap();
        assert_eq!(value, "config-b");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn ошибка_не_вытесняет_последний_корректный_результат() {
        let cache: ContractCache<&str> = ContractCache::new();
        cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Dark, 3), || Ok("valid"))
            .unwrap();
        let failed = cache.get_or_try_insert_with(
            key("#000000", Theme::Dark, 3),
            || -> Result<&str, &'static str> { Err("projection failed") },
        );
        assert_eq!(failed, Err("projection failed"));

        let built = Cell::new(false);
        let old = cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Dark, 3), || {
                built.set(true);
                Ok("wrong")
            })
            .unwrap();
        assert!(!built.get());
        assert_eq!(old, "valid");
    }

    #[test]
    fn clear_сбрасывает_всё_пространство_контракта() {
        let cache: ContractCache<u8> = ContractCache::new();
        cache
            .get_or_try_insert_with::<()>(key("#FFFFFF", Theme::Light, 1), || Ok(1))
            .unwrap();
        cache
            .get_or_try_insert_with::<()>(key("#000000", Theme::Dark, 1), || Ok(2))
            .unwrap();
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
