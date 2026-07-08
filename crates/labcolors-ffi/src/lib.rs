//! Нативный биндинг ядра `labcolors-core` через UniFFI — **доказательство
//! динамического рантайм-ядра** на Apple-платформах.
//!
//! # Архитектурный закон
//!
//! Рантайм-ядро Rust — обязательный слой на КАЖДОЙ платформе; запечённые
//! build-time токены — лишь опция, не архитектура. Этот крейт зовёт ядро В
//! РАНТАЙМЕ (через FFI), а не сериализует его выход на этапе сборки. Swift
//! получает те же перцептивные вычисления (CIECAM16 / LCS / LPC / WCAG), что и
//! WASM-граница — из одного и того же Rust-ядра.
//!
//! # Срез API — «рантайм-контраст-ядро»
//!
//! Экспортируется минимальный ОСМЫСЛЕННЫЙ срез — ровно примитивы, которые зовёт
//! реактивный рантайм, и ровно те, что покрывают семейства conformance-пака:
//!
//! | Экспорт | Семейство пака | Функция ядра |
//! |---------|----------------|--------------|
//! | [`contrast`] / [`recheck`] | `contrasts` | `recheck_against` |
//! | [`solve_contrast`] | `solve` | `solve` |
//! | [`ladder_alpha`] | `ladders` | `LadderPosition::alpha_pair` |
//! | [`composite`] / [`min_alpha`] | `alpha` | `alpha::composite_hex` / `alpha::min_alpha_hex` |
//! | [`muddiness`] | `muddiness` | `cleanliness::muddiness_from_hex` |
//! | [`core_version`] | `manifest` | версия ядра |
//!
//! Резолв полной темы из JSON-конфига (агностичный движок) СОЗНАТЕЛЬНО вне
//! среза: он требует serde-границы конфига (живёт в WASM-крейте) — перенос её
//! в нативный крейт либо дублировал бы её (риск дрейфа), либо тянул бы
//! wasm-bindgen в нативный граф. Это отдельный следующий шаг (см. PR).
//!
//! Хью/хрома у [`solve_contrast`] фиксированы нейтралью (серый) — резолв
//! хью-независим, срез детерминирован; параметризация хью — естественное
//! расширение.

use labcolors_core::alpha::{composite_hex, min_alpha_hex};
use labcolors_core::cleanliness::muddiness_from_hex;
use labcolors_core::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, LadderPosition, Theme as CoreTheme,
    ViewingConditions, recheck_against, solve,
};

// Регистрирует UniFFI-scaffolding под namespace = имя крейта (`labcolors`).
// Обязателен для чисто-proc-macro крейта.
uniffi::setup_scaffolding!();

// ─────────────────────────────────────────────────────────────────────────────
// Словарь границы
// ─────────────────────────────────────────────────────────────────────────────

/// Тема просмотра — стабильный словарь границы (`light` / `dark` / `light-ic` /
/// `dark-ic`). Отображается в канонические условия просмотра ядра.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Theme {
    /// Светлая: average surround.
    Light,
    /// Тёмная: dim surround.
    Dark,
    /// Светлая, повышенный контраст.
    LightIc,
    /// Тёмная, повышенный контраст.
    DarkIc,
}

impl Theme {
    fn to_core(self) -> CoreTheme {
        match self {
            Theme::Light => CoreTheme::Light,
            Theme::Dark => CoreTheme::Dark,
            Theme::LightIc => CoreTheme::LightIc,
            Theme::DarkIc => CoreTheme::DarkIc,
        }
    }

    fn vc(self) -> ViewingConditions {
        self.to_core().viewing_conditions()
    }
}

/// Контракт резолва — какой контраст обязан достичь передний план.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum ContractSpec {
    /// Текст: цель `Lc`, юридический пол WCAG AA-text (4.5:1).
    Text {
        /// Целевой перцептивный `Lc`.
        lc: f64,
    },
    /// UI-элемент: цель `Lc`, пол WCAG AA-UI (3:1).
    Ui {
        /// Целевой перцептивный `Lc`.
        lc: f64,
    },
    /// Декоративная полоса `[floor, ceiling]` без юридического пола.
    Range {
        /// Нижняя граница `Lc`.
        floor: f64,
        /// Верхняя граница `Lc`.
        ceiling: f64,
    },
}

impl ContractSpec {
    fn to_core(self) -> Contract {
        match self {
            ContractSpec::Text { lc } => Contract::text(lc),
            ContractSpec::Ui { lc } => Contract::ui(lc),
            ContractSpec::Range { floor, ceiling } => Contract::range(floor, ceiling),
        }
    }
}

/// Пара контрастов переднего плана на фоне: перцептивный `Lc` и юридический
/// WCAG-ratio. Отчитываются РАЗДЕЛЬНО (инвариант ядра — не смешивать).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Contrast {
    /// Знаковый перцептивный контраст (LPC `Lc`).
    pub lc: f64,
    /// Контраст-ratio WCAG 2.1 (1–21).
    pub wcag_ratio: f64,
}

/// Резолвнутый цвет и достигнутые им контрасты.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Solved {
    /// Резолвнутый цвет, `#RRGGBB`.
    pub hex: String,
    /// Знаковый перцептивный `Lc` на отданном hex.
    pub lc: f64,
    /// WCAG-ratio на отданном hex.
    pub wcag_ratio: f64,
    /// Юридический пол переопределил перцептивную цель.
    pub floor_override: bool,
}

/// Ошибки границы — сматчиваемые на Swift-стороне (бросаются как исключения).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ColorError {
    /// Невалидный hex-цвет на входе.
    #[error("невалидный цвет: {reason}")]
    InvalidColor {
        /// Человекочитаемая причина.
        reason: String,
    },
    /// Альфа вне `[0,1]` или не конечна.
    #[error("альфа вне [0,1]: {reason}")]
    InvalidAlpha {
        /// Человекочитаемая причина.
        reason: String,
    },
    /// Неизвестный ключ позиции лестницы.
    #[error("неизвестная позиция лестницы: {key}")]
    UnknownLadderPosition {
        /// Запрошенный ключ.
        key: String,
    },
    /// Контракт недостижим; `code` — стабильная причина (тождественна кодам
    /// WASM-границы: `floor_unreachable`, `exceeds_range`, …).
    #[error("контракт недостижим: {code}")]
    Unreachable {
        /// Стабильный машинный код причины.
        code: String,
    },
}

/// Стабильный код недостижимости — тождественен маппингу WASM-границы
/// (`labcolors-wasm/src/engine.rs`) и conformance-пака. Общий контракт имён для
/// ВСЕХ биндингов: одна причина → один код на любой платформе.
fn unreachable_code(err: &labcolors_core::Unreachable) -> &'static str {
    use labcolors_core::Unreachable as U;
    match err {
        U::BelowContrastFloor { .. } => "below_contrast_floor",
        U::ExceedsRange { .. } => "exceeds_range",
        U::QuantizationGap { .. } => "quantization_gap",
        U::FloorUnreachable { .. } => "floor_unreachable",
        U::PolarityMismatch { .. } => "polarity_mismatch",
        U::GamutUnsupported => "gamut_unsupported",
        U::InvalidInput(_) => "invalid_input",
        // `Unreachable` помечен `#[non_exhaustive]`; forward-compat-арм.
        _ => "unreachable",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Экспорты — рантайм-контраст-ядро
// ─────────────────────────────────────────────────────────────────────────────

/// Версия ядра, к которой привязан этот биндинг (и conformance-пак). Все крейты
/// воркспейса делят одну версию — привязка пак ⇄ ядро ⇄ биндинг однозначна.
#[uniffi::export]
#[must_use]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Контраст одного переднего плана на фоне под темой — `(Lc, WCAG)`. Дешёвый
/// пер-кадровый примитив: один forward перцептивной модели для фона плюс один
/// для переднего плана.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn contrast(fg: String, bg: String, theme: Theme) -> Result<Contrast, ColorError> {
    let vc = theme.vc();
    let mut pairs =
        recheck_against(&bg, &[&fg], &vc).map_err(|reason| ColorError::InvalidColor { reason })?;
    let (lc, wcag_ratio) = pairs.pop().expect("ровно один передний план");
    Ok(Contrast { lc, wcag_ratio })
}

/// Перепроверка контрастов многих передних планов на одном фоне под темой.
/// Фон платит свой forward один раз на весь батч — примитив реактивного
/// рантайма для решения «прошли ли уже резолвнутые цвета против сменившегося
/// фона».
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex (фон или любой передний план).
#[uniffi::export]
pub fn recheck(bg: String, fgs: Vec<String>, theme: Theme) -> Result<Vec<Contrast>, ColorError> {
    let vc = theme.vc();
    let fg_refs: Vec<&str> = fgs.iter().map(String::as_str).collect();
    let pairs = recheck_against(&bg, &fg_refs, &vc)
        .map_err(|reason| ColorError::InvalidColor { reason })?;
    Ok(pairs
        .into_iter()
        .map(|(lc, wcag_ratio)| Contrast { lc, wcag_ratio })
        .collect())
}

/// Резолв переднего плана, удовлетворяющего `contract` на фоне `bg` под темой.
/// Хрома нейтральна (серый) — атомарный резолв, хью-независимый.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex фона;
/// [`ColorError::Unreachable`] со стабильным кодом, если ни один цвет не
/// удовлетворяет контракт (честный отказ, не тихий клип).
#[uniffi::export]
pub fn solve_contrast(
    bg: String,
    contract: ContractSpec,
    theme: Theme,
) -> Result<Solved, ColorError> {
    let vc = theme.vc();
    let bg_input = BgInput::solid(&bg).map_err(|e| ColorError::InvalidColor {
        reason: e.to_string(),
    })?;
    match solve(
        bg_input,
        contract.to_core(),
        Hue::deg(0.0),
        ChromaPolicy::Neutral,
        &vc,
        Gamut::Srgb,
    ) {
        Ok(s) => Ok(Solved {
            hex: s.hex().to_string(),
            lc: s.lc(),
            wcag_ratio: s.wcag_ratio(),
            floor_override: s.floor_override(),
        }),
        Err(e) => Err(ColorError::Unreachable {
            code: unreachable_code(&e).to_string(),
        }),
    }
}

/// Альфа позиции лестницы под темой. `position` — стабильный kebab-ключ
/// (`label-secondary`, `fill-primary`, …).
///
/// # Errors
///
/// [`ColorError::UnknownLadderPosition`] на неизвестном ключе.
#[uniffi::export]
pub fn ladder_alpha(position: String, theme: Theme) -> Result<f64, ColorError> {
    let pos = LadderPosition::ALL
        .iter()
        .copied()
        .find(|p| p.key() == position)
        .ok_or(ColorError::UnknownLadderPosition {
            key: position.clone(),
        })?;
    Ok(pos.alpha_for_vc(&theme.vc()))
}

/// Прямой ход подложка→α: композит `α·tint + (1−α)·bg`, квантованный до
/// `#RRGGBB` (закон Figma/браузера в кодированном sRGB).
///
/// # Errors
///
/// [`ColorError::InvalidAlpha`] при α вне `[0,1]`; [`ColorError::InvalidColor`]
/// на невалидном hex.
#[uniffi::export]
pub fn composite(tint: String, alpha: f64, bg: String) -> Result<String, ColorError> {
    composite_hex(&tint, alpha, &bg).map_err(|reason| {
        if reason.starts_with("alpha") {
            ColorError::InvalidAlpha { reason }
        } else {
            ColorError::InvalidColor { reason }
        }
    })
}

/// Минимально разрешимая α: нижняя граница, при которой тинт остаётся в гамуте
/// над фоном (граница инверсии композита).
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn min_alpha(tint: String, bg: String) -> Result<f64, ColorError> {
    min_alpha_hex(&tint, &bg).map_err(|reason| ColorError::InvalidColor { reason })
}

/// Оценка мутности («грязи») цвета `[0,1]`: 0 — чистый, 1 — грязный.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn muddiness(hex: String) -> Result<f64, ColorError> {
    muddiness_from_hex(&hex).map_err(|reason| ColorError::InvalidColor { reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_black_on_white_is_21_wcag() {
        let c = contrast("#000000".into(), "#FFFFFF".into(), Theme::Light).unwrap();
        assert!((c.wcag_ratio - 21.0).abs() < 1e-6);
        assert!(c.lc > 0.0, "тёмное на светлом — положительный Lc");
    }

    #[test]
    fn invalid_hex_is_structured_error() {
        let err = contrast("nope".into(), "#FFFFFF".into(), Theme::Light).unwrap_err();
        assert!(matches!(err, ColorError::InvalidColor { .. }));
    }

    #[test]
    fn solve_text_on_white_meets_floor() {
        let s = solve_contrast(
            "#FFFFFF".into(),
            ContractSpec::Text { lc: 60.0 },
            Theme::Light,
        )
        .unwrap();
        assert!(s.wcag_ratio >= 4.5, "AA-text пол держится");
    }

    #[test]
    fn solve_unreachable_carries_stable_code() {
        let err = solve_contrast(
            "#FFFFFF".into(),
            ContractSpec::Text { lc: 150.0 },
            Theme::Light,
        )
        .unwrap_err();
        match err {
            ColorError::Unreachable { code } => assert_eq!(code, "exceeds_range"),
            other => panic!("ожидался Unreachable, получено {other:?}"),
        }
    }

    #[test]
    fn ladder_alpha_per_theme() {
        // NeutralFillPrimary: light @20, dark @36 — пер-темная пара.
        let light = ladder_alpha("neutral-fill-primary".into(), Theme::Light).unwrap();
        let dark = ladder_alpha("neutral-fill-primary".into(), Theme::Dark).unwrap();
        assert!((light - 0.2).abs() < 1e-9);
        assert!((dark - 0.361).abs() < 1e-9);
    }

    #[test]
    fn unknown_ladder_position_is_error() {
        let err = ladder_alpha("label-quinary".into(), Theme::Light).unwrap_err();
        assert!(matches!(err, ColorError::UnknownLadderPosition { .. }));
    }

    #[test]
    fn composite_and_min_alpha_are_wired() {
        let c = composite("#787880".into(), 0.2, "#FFFFFF".into()).unwrap();
        assert!(c.starts_with('#') && c.len() == 7);
        let m = min_alpha("#787880".into(), "#FFFFFF".into()).unwrap();
        assert!((0.0..=1.0).contains(&m));
    }

    #[test]
    fn muddiness_orders_olive_above_clean_gray() {
        // Значения — факт ядра (см. conformance/vectors/muddiness.json): серый
        // почти чист, олива заметно мутнее. Проверяем ПОРЯДОК и чистоту серого,
        // не магическое число.
        let olive = muddiness("#6B6B2E".into()).unwrap();
        let gray = muddiness("#808080".into()).unwrap();
        assert!(gray < 0.05, "серый должен быть чистым, получено {gray}");
        assert!(olive > gray, "олива должна быть мутнее серого");
    }
}
