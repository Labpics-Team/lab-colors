//! Лестница акцента/сентимента/бренда/нейтрали как ДАННЫЕ: закрытое меню позиций
//! (каждая несёт свою альфу Figma-рампы) + физика тинта источника по теме.
//!
//! # Закон лестницы (заземление 2026-07-02)
//!
//! Акцентная лестница labui устроена КАК нейтральная: **один тинт (якорный цвет
//! источника, пер-темно) × закрытая рампа альф** — Figma-переменная
//! `Accent/Derivable/<Family>/<Family>@NN`, где `NN` — процент прозрачности.
//! Labui-контракт эмитит `rgba(tint, α)` НАПРЯМУЮ (композитит браузер), а НЕ
//! солид-эквивалент. Поэтому позиция лестницы = `{тинт = якорь источника по теме,
//! α из меню}`; резолв несёт rgba (см. [`crate::semantic::Resolved::Rgba`]).
//! Солид-эквивалент (композит тинта на фоне резолва,
//! [`crate::alpha::composite_over_encoded`]) — для честного замера dJ'/WCAG
//! контраст-корректности на подложке (фаза 1 AA: контраст меряется на композите).
//! Заземление: `reference/labui-accent-primitives.md` §2 (пер-темные якоря),
//! стаб labui `packages/colors-stub/contract.css` (@NN-рампа),
//! `.agents/epics/ds-config-train/chapters/ch02-engine-config-input/grounding-accent-roles-2026-07-02.md`.
//!
//! # Провенанс альф позиций
//!
//! Альфы — ДАННЫЕ рампы Figma `@NN` (float32-квантование процентов из имён
//! переменных), не выведенные величины: `@72 → 0.722`, `@52 → 0.522`,
//! `@32 → 0.322`, `@20 → 0.2`, `@12 → 0.122`, `@8 → 0.078`, `@4 → 0.039`,
//! `@2 → 0.02`; `primary`/`border-strong`/`focus-ring` — солид (α = 1.0).
//! Единый паттерн проверен на brand/danger/info/success (grounding §Закон
//! лестницы). Это данные позиций, а не POLICY-константы перцептивных модулей,
//! поэтому провенанс держится этой doc-строкой + тестом лестницы, а не строкой
//! реестра (как якорные hex палитры, `accent.rs`).

use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::{srgb_encoded_from_hex, srgb_gamma_inv};
use crate::spaces::vc::ViewingConditions;

/// Пер-темная четвёрка якорных hex (`light` / `dark` / `light-ic` / `dark-ic`).
///
/// Источник лестницы (семейство палитры или бренд) несёт свой якорь отдельно для
/// каждого режима — тёмная тема и режим повышенного контраста (IC) не выводятся
/// из светлого якоря, а замеряются (`reference/labui-accent-primitives.md` §2:
/// Red light `#FF3B30` / dark `#FF3A3A` / light-ic `#D70015` / dark-ic `#FF6161`).
/// Выбор режима — по условиям просмотра резолва ([`ThemeAnchors::for_vc`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeAnchors {
    /// Светлая тема (average surround).
    pub light: String,
    /// Тёмная тема (dim surround).
    pub dark: String,
    /// Светлая тема, повышенный контраст (IC).
    pub light_ic: String,
    /// Тёмная тема, повышенный контраст (IC).
    pub dark_ic: String,
}

impl ThemeAnchors {
    /// Якорный hex под условия просмотра резолва: IC-режим (`vc.high_contrast`)
    /// выбирает `*_ic`, тёмный сурраунд (`vc.is_dark_theme()`) — тёмную ветку.
    /// Четыре VC-пресета движка ([`crate::config::VcPreset`]) отображаются ровно
    /// на четыре якоря — иных режимов у лестницы нет.
    pub fn for_vc(&self, vc: &ViewingConditions) -> &str {
        match (vc.is_dark_theme(), vc.high_contrast) {
            (false, false) => &self.light,
            (true, false) => &self.dark,
            (false, true) => &self.light_ic,
            (true, true) => &self.dark_ic,
        }
    }

    /// Кодированные (byte/255) RGB четырёх якорей — компилятор лестницы
    /// раскладывает пер-темную четвёрку в [`LadderTint`] один раз, чтобы
    /// [`crate::semantic::RoleSpec`] остался `Copy` (без строк в горячем резолве).
    ///
    /// # Errors
    ///
    /// `Err`, если любой из четырёх hex невалиден (валидатор конфига ловит это
    /// раньше — здесь защита компиляции).
    pub fn encoded_quad(&self) -> Result<[[f64; 3]; 4], String> {
        Ok([
            srgb_encoded_from_hex(&self.light)?,
            srgb_encoded_from_hex(&self.dark)?,
            srgb_encoded_from_hex(&self.light_ic)?,
            srgb_encoded_from_hex(&self.dark_ic)?,
        ])
    }
}

/// Кодированный (byte/255) тинт лестницы, разложенный по четырём режимам —
/// `Copy`-полезная нагрузка [`crate::semantic::RoleSpec::Ladder`].
///
/// Индексация повторяет [`ThemeAnchors::for_vc`]:
/// `0 = light`, `1 = dark`, `2 = light-ic`, `3 = dark-ic`. Тинт bg-независим
/// (это якорь источника по теме), поэтому раскладывается на этапе компиляции,
/// а не в резолве.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LadderTint {
    /// Кодированные RGB якорей `[light, dark, light-ic, dark-ic]`.
    quad: [[f64; 3]; 4],
}

impl LadderTint {
    /// Собрать тинт из кодированной четвёрки режимов.
    pub fn new(quad: [[f64; 3]; 4]) -> Self {
        Self { quad }
    }

    /// Кодированный тинт под условия просмотра резолва (тот же выбор режима, что
    /// [`ThemeAnchors::for_vc`]).
    pub fn for_vc(&self, vc: &ViewingConditions) -> [f64; 3] {
        let idx = match (vc.is_dark_theme(), vc.high_contrast) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        };
        self.quad[idx]
    }

    /// Oklab-хрома светлого якоря тинта — вход в пересчёт `S_PERC_MIN`
    /// (среднее по четырём сентимент-якорям конфига, [`crate::sentiment`]).
    pub fn light_oklab_chroma(&self) -> f64 {
        // Кодированный тинт → линейный свет (per-channel gamma-декод, тот же, что
        // в srgb_from_hex), затем Oklab-хрома = |(a, b)|.
        let e = self.quad[0];
        let lin = [
            srgb_gamma_inv(e[0]),
            srgb_gamma_inv(e[1]),
            srgb_gamma_inv(e[2]),
        ];
        let lab = srgb_linear_to_oklab(lin);
        (lab[1] * lab[1] + lab[2] * lab[2]).sqrt()
    }
}

/// Закрытое меню позиций лестницы: каждая позиция несёт свою альфу Figma-рампы.
///
/// Перечень зафиксирован приложением A к ADR-0001 (заземление 2026-07-02).
/// Солидные позиции (`α = 1.0`) — `LabelPrimary`, `BorderStrong`, `FocusRing`;
/// остальные несут альфу `@NN` из рампы (см. провенанс в документации модуля).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LadderPosition {
    /// Метка первичная — солид (α = 1.0).
    LabelPrimary,
    /// Метка вторичная — `@72`.
    LabelSecondary,
    /// Метка третичная — `@52`.
    LabelTertiary,
    /// Метка четвертичная — `@32`.
    LabelQuaternary,
    /// Заливка первичная — `@12`.
    FillPrimary,
    /// Заливка вторичная — `@8`.
    FillSecondary,
    /// Заливка третичная — `@4`.
    FillTertiary,
    /// Заливка четвертичная — `@2`.
    FillQuaternary,
    /// Граница базовая — `@20`.
    BorderBase,
    /// Граница мягкая — `@12`.
    BorderSoft,
    /// Граница сильная — солид (α = 1.0).
    BorderStrong,
    /// Кольцо фокуса — солид (α = 1.0).
    FocusRing,
    /// Свечение — `@52`.
    Glow,
}

impl LadderPosition {
    /// Все позиции меню — поверхность для property-свипов и генерации ролей.
    pub const ALL: [LadderPosition; 13] = [
        LadderPosition::LabelPrimary,
        LadderPosition::LabelSecondary,
        LadderPosition::LabelTertiary,
        LadderPosition::LabelQuaternary,
        LadderPosition::FillPrimary,
        LadderPosition::FillSecondary,
        LadderPosition::FillTertiary,
        LadderPosition::FillQuaternary,
        LadderPosition::BorderBase,
        LadderPosition::BorderSoft,
        LadderPosition::BorderStrong,
        LadderPosition::FocusRing,
        LadderPosition::Glow,
    ];

    /// Альфа позиции — ДАННЫЕ рампы Figma `@NN` (провенанс — документация модуля).
    /// Мутация любого значения роняет тест лестницы (RED-proof).
    pub fn alpha(self) -> f64 {
        match self {
            LadderPosition::LabelPrimary => 1.0,
            LadderPosition::LabelSecondary => 0.722,
            LadderPosition::LabelTertiary => 0.522,
            LadderPosition::LabelQuaternary => 0.322,
            LadderPosition::FillPrimary => 0.122,
            LadderPosition::FillSecondary => 0.078,
            LadderPosition::FillTertiary => 0.039,
            LadderPosition::FillQuaternary => 0.02,
            LadderPosition::BorderBase => 0.2,
            LadderPosition::BorderSoft => 0.122,
            LadderPosition::BorderStrong => 1.0,
            LadderPosition::FocusRing => 1.0,
            LadderPosition::Glow => 0.522,
        }
    }

    /// Стабильный kebab-ключ позиции — для разбора рецепта из конфига (t3 JSON)
    /// и для приложения A к ADR. Часть контракта имён; опечатка ловится тестом.
    pub fn key(self) -> &'static str {
        match self {
            LadderPosition::LabelPrimary => "label-primary",
            LadderPosition::LabelSecondary => "label-secondary",
            LadderPosition::LabelTertiary => "label-tertiary",
            LadderPosition::LabelQuaternary => "label-quaternary",
            LadderPosition::FillPrimary => "fill-primary",
            LadderPosition::FillSecondary => "fill-secondary",
            LadderPosition::FillTertiary => "fill-tertiary",
            LadderPosition::FillQuaternary => "fill-quaternary",
            LadderPosition::BorderBase => "border-base",
            LadderPosition::BorderSoft => "border-soft",
            LadderPosition::BorderStrong => "border-strong",
            LadderPosition::FocusRing => "focus-ring",
            LadderPosition::Glow => "glow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Меню позиций закрыто и его альфы — заземлённая Figma-рампа `@NN`.
    /// Нетавтологичный пин: числа взяты из grounding-документа, а мутация
    /// [`LadderPosition::alpha`] роняет тест (RED-proof альф позиций).
    #[test]
    fn position_alphas_match_grounded_figma_ramp() {
        let expected: &[(LadderPosition, f64, &str)] = &[
            (LadderPosition::LabelPrimary, 1.0, "label-primary"),
            (LadderPosition::LabelSecondary, 0.722, "label-secondary"),
            (LadderPosition::LabelTertiary, 0.522, "label-tertiary"),
            (LadderPosition::LabelQuaternary, 0.322, "label-quaternary"),
            (LadderPosition::FillPrimary, 0.122, "fill-primary"),
            (LadderPosition::FillSecondary, 0.078, "fill-secondary"),
            (LadderPosition::FillTertiary, 0.039, "fill-tertiary"),
            (LadderPosition::FillQuaternary, 0.02, "fill-quaternary"),
            (LadderPosition::BorderBase, 0.2, "border-base"),
            (LadderPosition::BorderSoft, 0.122, "border-soft"),
            (LadderPosition::BorderStrong, 1.0, "border-strong"),
            (LadderPosition::FocusRing, 1.0, "focus-ring"),
            (LadderPosition::Glow, 0.522, "glow"),
        ];
        for (pos, alpha, key) in expected {
            assert_eq!(
                pos.alpha(),
                *alpha,
                "{pos:?}: альфа дрейфанула от Figma-рампы"
            );
            assert_eq!(pos.key(), *key, "{pos:?}: ключ разошёлся с контрактом имён");
        }
        let all: Vec<LadderPosition> = LadderPosition::ALL.to_vec();
        let listed: Vec<LadderPosition> = expected.iter().map(|(p, ..)| *p).collect();
        assert_eq!(all, listed, "LadderPosition::ALL разошёлся с меню");
    }

    /// Выбор якоря по режиму: четыре VC-пресета движка → четыре разных якоря.
    #[test]
    fn theme_anchors_select_by_vc() {
        let anchors = ThemeAnchors {
            light: "#FF3B30".to_string(),
            dark: "#FF3A3A".to_string(),
            light_ic: "#D70015".to_string(),
            dark_ic: "#FF6161".to_string(),
        };
        assert_eq!(anchors.for_vc(&ViewingConditions::srgb()), "#FF3B30");
        assert_eq!(
            anchors.for_vc(&ViewingConditions::dim_surround()),
            "#FF3A3A"
        );
        assert_eq!(
            anchors.for_vc(&ViewingConditions::srgb_high_contrast()),
            "#D70015"
        );
        assert_eq!(
            anchors.for_vc(&ViewingConditions::dim_surround_high_contrast()),
            "#FF6161"
        );
    }

    /// Тинт `for_vc` повторяет выбор режима якоря побайтно (кодированный RGB).
    #[test]
    fn ladder_tint_for_vc_mirrors_anchor_selection() {
        let anchors = ThemeAnchors {
            light: "#FF3B30".to_string(),
            dark: "#FF3A3A".to_string(),
            light_ic: "#D70015".to_string(),
            dark_ic: "#FF6161".to_string(),
        };
        let tint = LadderTint::new(anchors.encoded_quad().unwrap());
        for vc in [
            ViewingConditions::srgb(),
            ViewingConditions::dim_surround(),
            ViewingConditions::srgb_high_contrast(),
            ViewingConditions::dim_surround_high_contrast(),
        ] {
            let want = srgb_encoded_from_hex(anchors.for_vc(&vc)).unwrap();
            assert_eq!(tint.for_vc(&vc), want, "тинт для vc разошёлся с якорем");
        }
    }
}
