//! Палитра акцентов как ДАННЫЕ: 10 именованных цветовых семейств, каждое —
//! якорный hex из Figma-примитивов, а не копия кода.
//!
//! Где [`sentiment`](crate::sentiment) отвечает на семантический вопрос «какой
//! оттенок у *Danger* при данном бренде, чтобы он не слился с ним», этот модуль
//! отвечает на более низкий вопрос — «каковы якорные цвета палитры акцентов
//! дизайн-системы». [`Accent`] — это перечисление 10 семейств (Red … Pink);
//! каждое несёт единственный измеренный якорный hex ([`anchor_hex`](Accent::anchor_hex)),
//! из которого выводится якорный Oklab-оттенок ([`prototype_hue`](Accent::prototype_hue))
//! и полная рампа через [`curve`](Accent::curve) (та же [`AccentCurve`], что
//! строит ремень акцента на общей нейтральной лестнице).
//!
//! # Почему данные, а не 10 копий кода
//!
//! Рампа акцента строится одним законом ([`AccentCurve`]): общая
//! perceived-lightness лестница нейтрали + фиксированная доля граничной хромы
//! гамута на резолвленном оттенке. Отличие семейства от семейства — ровно один
//! якорный оттенок. Поэтому семейство — это *строка данных* (`anchor_hex`), а не
//! отдельный блок логики: добавить семейство = добавить измеренный hex, ноль
//! нового кода рампы. Это же устраняет дублирование с [`sentiment`](crate::sentiment):
//! четыре сентимента (Danger/Warning/Success/Info) больше не хранят собственные копии
//! hex — они ссылаются на семейство палитры
//! ([`Sentiment::accent`](crate::sentiment::Sentiment::accent)).
//!
//! # Провенанс якорей — измерение, не выдумка
//!
//! Все 10 hex сняты с живой Figma-коллекции «🔵 4.1 Primitives» (переменные
//! `Accent/*`, режим Light-mode) через figma-console MCP, 2026-07-02. Протокол
//! замера, все режимы (Light / Dark / IC) и вычисленные Oklab-оттенки —
//! `reference/labui-accent-primitives.md`; воспроизводимый скрипт —
//! `cargo run -p labcolors-core --example accent_provenance`. Якорный hex — это
//! *измеренное* значение (не выведенное): его правильность фиксирует тест
//! `accent_anchor_hex_matches_figma_primitives_light_mode` против буквальных
//! значений Figma.
//!
//! # Только оттенок якоря — прототип
//!
//! Как и в [`sentiment`](crate::sentiment), из якоря берётся *только Oklab-оттенок*. Светлота и
//! хрома якоря не применяются: рампа кладёт оттенок на общую
//! perceived-lightness лестницу при фиксированной доле граничной хромы (см.
//! [`AccentCurve::at`](crate::scale::AccentCurve::at)). Поэтому тёмный/IC-вариант
//! Figma-якоря не участвует в построении рампы — он задокументирован в
//! reference как часть замера, но прототипом служит светлый якорь.

use crate::neutral::NeutralCurve;
use crate::scale::AccentCurve;
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::srgb_from_hex;

/// Именованное цветовое семейство палитры акцентов.
///
/// Ровно 10 семейств — фиксированная палитра дизайн-системы (Figma
/// `Accent/{Red,Orange,Yellow,Green,Teal,Mint,Blue,Indigo,Purple,Pink}`).
/// `Accent/Brand` намеренно НЕ входит: бренд — это *вход* (настраиваемый
/// пользователем оттенок, по умолчанию `#007AFF`), а не фиксированное семейство
/// палитры, ровно как фон — вход движка, а не роль.
///
/// Порядок вариантов повторяет порядок семейств в коллекции Figma-примитивов
/// (семейства системных цветов HIG). Это НЕ монотонный Oklab-оттенок: Teal
/// (≈230.8°) идёт раньше Mint (≈189.0°), а Pink заворачивается к ≈17.9°. Порядок
/// — это порядок источника (Figma), а не сортировка по оттенку; единственный
/// инвариант, который проверяется тестом, — попарная различимость оттенков
/// (см. `all_ten_families_are_distinct_hues`), не их монотонность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Accent {
    /// Красный. Figma `Accent/Red` = `#FF3B30`. Прототип сентимента Danger.
    Red,
    /// Оранжевый. Figma `Accent/Orange` = `#FFA100`. Прототип сентимента Warning.
    Orange,
    /// Жёлтый. Figma `Accent/Yellow` = `#FFD000`.
    Yellow,
    /// Зелёный. Figma `Accent/Green` = `#34C759`. Прототип сентимента Success.
    Green,
    /// Бирюзовый (голубой). Figma `Accent/Teal` = `#5AC8FA`.
    Teal,
    /// Мятный. Figma `Accent/Mint` = `#00C7BE`.
    Mint,
    /// Синий. Figma `Accent/Blue` = `#3E87FF`. Прототип сентимента Info.
    Blue,
    /// Индиго. Figma `Accent/Indigo` = `#5856D6`.
    Indigo,
    /// Фиолетовый. Figma `Accent/Purple` = `#AF52DE`.
    Purple,
    /// Розовый. Figma `Accent/Pink` = `#FF2D55`.
    Pink,
}

impl Accent {
    /// Все 10 семейств палитры в порядке коллекции Figma-примитивов (Red → Pink,
    /// порядок семейств HIG — не сортировка по оттенку) — поверхность для
    /// property-свипов в тестах и для будущей роль-таблицы акцентов (#59).
    pub const ALL: [Accent; 10] = [
        Accent::Red,
        Accent::Orange,
        Accent::Yellow,
        Accent::Green,
        Accent::Teal,
        Accent::Mint,
        Accent::Blue,
        Accent::Indigo,
        Accent::Purple,
        Accent::Pink,
    ];

    /// Якорный hex семейства — измеренное значение Figma-примитива `Accent/*`
    /// (коллекция «🔵 4.1 Primitives», режим **Light-mode**, замер 2026-07-02).
    ///
    /// Это *измеренное*, а не выведенное значение: единственный вход, из которого
    /// строится всё семейство. Провенанс и остальные режимы (Dark / IC) —
    /// `reference/labui-accent-primitives.md`. Тёмный/IC-вариант не участвует в
    /// построении рампы (прототип — светлый якорь, см. модульную документацию).
    pub fn anchor_hex(self) -> &'static str {
        match self {
            Accent::Red => "#FF3B30",
            Accent::Orange => "#FFA100",
            Accent::Yellow => "#FFD000",
            Accent::Green => "#34C759",
            Accent::Teal => "#5AC8FA",
            Accent::Mint => "#00C7BE",
            Accent::Blue => "#3E87FF",
            Accent::Indigo => "#5856D6",
            Accent::Purple => "#AF52DE",
            Accent::Pink => "#FF2D55",
        }
    }

    /// Стабильный kebab-ключ семейства (`"red"`, `"orange"`, …) — часть будущего
    /// контракта имён ролей (`--lab-*-{key}`), фиксируется здесь, чтобы имена
    /// семейств не расходились между модулями.
    pub fn key(self) -> &'static str {
        match self {
            Accent::Red => "red",
            Accent::Orange => "orange",
            Accent::Yellow => "yellow",
            Accent::Green => "green",
            Accent::Teal => "teal",
            Accent::Mint => "mint",
            Accent::Blue => "blue",
            Accent::Indigo => "indigo",
            Accent::Purple => "purple",
            Accent::Pink => "pink",
        }
    }

    /// Якорный **Oklab-оттенок** (градусы `[0, 360)`) семейства — оттенок его
    /// якорного цвета, из которого строится рампа. Выводится из
    /// [`anchor_hex`](Accent::anchor_hex), а не вводится вручную (то же правило,
    /// что в [`Sentiment::prototype_hue`](crate::sentiment)): это исключает дрейф
    /// между цветовыми моделями (HSB-оттенок отличается от Oklab на 12–46°).
    pub fn prototype_hue(self) -> f64 {
        oklab_hue_of(self.anchor_hex())
    }

    /// Рампа этого семейства на нейтральной лестнице `neutral` — та же
    /// [`AccentCurve`], которой строится любой акцент: общая
    /// perceived-lightness лестница + фиксированная доля граничной хромы на
    /// резолвленном оттенке. Единственный вход, отличающий семейства, — якорный
    /// hex.
    ///
    /// # Errors
    ///
    /// Возвращает `Err`, если [`AccentCurve::new`] не смогла разобрать якорный hex
    /// (для встроенных якорей не наступает — все 10 валидны; ошибка возможна лишь
    /// при будущем изменении константы на невалидную).
    pub fn curve(self, neutral: &NeutralCurve) -> Result<AccentCurve, String> {
        AccentCurve::new(self.anchor_hex(), neutral)
    }
}

/// Oklab-оттенок (градусы, `[0, 360)`) hex-цвета — ЕДИНСТВЕННАЯ реализация
/// формулы якорного оттенка в движке: `pub(crate)`, чтобы [`sentiment`](crate::sentiment)
/// потреблял её, а не держал вторую копию физики.
///
/// # Panics
///
/// Паникует на невалидном hex — вызывается только с встроенными якорями палитры,
/// корректность которых фиксируют тесты обоих модулей.
pub(crate) fn oklab_hue_of(hex: &str) -> f64 {
    let lab = srgb_linear_to_oklab(srgb_from_hex(hex).expect("valid anchor hex"));
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012")
            .expect("канонические нейтральные якоря валидны — ошибка означает регресс парсера hex")
    }

    /// Якорные hex, kebab-ключи и Oklab-оттенки заземлены в живой Figma
    /// (коллекция «🔵 4.1 Primitives», переменные `Accent/*`, режим Light-mode,
    /// обход через figma-console MCP, 2026-07-02 MSK). Тест фиксирует ЛЮБОЙ дрейф
    /// константы от измеренного примитива, опечатку в `key()` (часть будущего
    /// контракта имён `--lab-*-{key}`) и — числовым пином h° из reference §3 —
    /// поломку самой цепочки srgb→Oklab→atan2 (нетавтологичное ожидание: числа
    /// взяты из документа, не из вызова той же функции).
    ///
    /// Доказательство из Figma (Light-mode):
    ///   Accent/Red=#FF3B30 · Accent/Orange=#FFA100 · Accent/Yellow=#FFD000
    ///   Accent/Green=#34C759 · Accent/Teal=#5AC8FA · Accent/Mint=#00C7BE
    ///   Accent/Blue=#3E87FF · Accent/Indigo=#5856D6 · Accent/Purple=#AF52DE
    ///   Accent/Pink=#FF2D55
    #[test]
    fn accent_anchor_hex_matches_figma_primitives_light_mode() {
        // (Accent, якорный hex Figma, kebab-ключ, Oklab h° из reference §3).
        let expected: &[(Accent, &str, &str, f64)] = &[
            (Accent::Red, "#FF3B30", "red", 28.6592),
            (Accent::Orange, "#FFA100", "orange", 68.6070),
            (Accent::Yellow, "#FFD000", "yellow", 92.2265),
            (Accent::Green, "#34C759", "green", 147.4439),
            (Accent::Teal, "#5AC8FA", "teal", 230.8271),
            (Accent::Mint, "#00C7BE", "mint", 189.0284),
            (Accent::Blue, "#3E87FF", "blue", 259.8918),
            (Accent::Indigo, "#5856D6", "indigo", 278.3368),
            (Accent::Purple, "#AF52DE", "purple", 312.4106),
            (Accent::Pink, "#FF2D55", "pink", 17.8982),
        ];
        for (accent, hex, key, hue) in expected {
            assert_eq!(
                accent.anchor_hex(),
                *hex,
                "{accent:?}: anchor_hex() дрейфанул от заземлённого Figma-примитива"
            );
            assert_eq!(
                accent.key(),
                *key,
                "{accent:?}: key() разошёлся с контрактом имён семейств"
            );
            // Допуск 5e-5: значения §3 напечатаны с 4 знаками после запятой.
            assert!(
                (accent.prototype_hue() - hue).abs() < 5e-5,
                "{accent:?}: prototype_hue {} != {hue} (reference §3)",
                accent.prototype_hue()
            );
        }
        // ALL покрывает ровно перечисленные семейства — сравнение по СОДЕРЖИМОМУ
        // (дубликат в ALL при сохранении длины 10 тоже упадёт).
        let all: Vec<Accent> = Accent::ALL.to_vec();
        let listed: Vec<Accent> = expected.iter().map(|(a, ..)| *a).collect();
        assert_eq!(
            all, listed,
            "Accent::ALL разошёлся с заземлённым списком семейств"
        );
    }

    /// Деривационная идентичность: якорный оттенок читается из Oklab-оттенка
    /// якорного цвета, а не вводится вручную — исключает дрейф между цветовыми
    /// моделями. Допуск < 1e-9 (точное равенство одной и той же функции);
    /// нетавтологичный числовой пин h° живёт в табличном тесте выше.
    #[test]
    fn prototype_hue_is_the_anchor_oklab_hue() {
        for a in Accent::ALL {
            let want = oklab_hue_of(a.anchor_hex());
            assert!(
                (a.prototype_hue() - want).abs() < 1e-9,
                "{a:?}: prototype_hue {} != anchor Oklab hue {want}",
                a.prototype_hue()
            );
        }
    }

    /// Все 10 якорных оттенков различимы (перцептивно разные семейства): любые
    /// два отстоят более чем на 8° Oklab. Ловит опечатку в hex, из-за которой
    /// два семейства схлопнулись бы в один оттенок.
    #[test]
    fn all_ten_families_are_distinct_hues() {
        let hues: Vec<f64> = Accent::ALL.iter().map(|a| a.prototype_hue()).collect();
        for i in 0..hues.len() {
            for j in (i + 1)..hues.len() {
                let d = {
                    let raw = (hues[i] - hues[j]).rem_euclid(360.0);
                    raw.min(360.0 - raw)
                };
                assert!(
                    d > 8.0,
                    "{:?} и {:?} слились по оттенку ({:.2}° < 8°)",
                    Accent::ALL[i],
                    Accent::ALL[j],
                    d
                );
            }
        }
    }

    /// Каждое семейство строит валидную рампу той же [`AccentCurve`]: якорь
    /// разбирается, hex-сэмплы валидны. Доказывает, что семейство — это данные
    /// поверх одного закона рампы, а не 10 отдельных реализаций.
    #[test]
    fn every_family_builds_a_valid_ramp() {
        let n = neutral();
        for a in Accent::ALL {
            let curve = a.curve(&n).unwrap_or_else(|e| panic!("{a:?}: {e}"));
            for hex in curve.sample_hex(13) {
                assert!(
                    srgb_from_hex(&hex).is_ok(),
                    "{a:?}: невалидный hex в рампе {hex}"
                );
            }
        }
    }
}
