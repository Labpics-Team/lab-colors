//! Каноническая референс-фикстура labui — дерево Даниила ЦЕЛИКОМ (якоря,
//! ручки, палитра, замер Figma 2026-07-02): тестовый оракул, `#[cfg(test)]`-only.
//! Прод-ядро агностично (CH-3, ADR-0001 PR-c): брендовые hex не покидают тестов —
//! гейт `tests/agnostic_cleanliness.rs`; wasm-граница читает замороженный
//! JSON-паспорт (`labcolors-wasm/tests/data/labui.config.json`), не эту фикстуру.
//!
//! Словарь ролей/алиасов фикстура тянет из ПРОДОВОГО модуля пресета
//! (`config/preset.rs`) — один источник; байт-в-байт гейт эмиссии
//! (`crate::agnostic_gates`) замораживает контракт тонкий == полный.
//!
//! Собирается изнутри крейта: тянет `pub(crate)` SSOT-константы подтона из
//! `semantic`.

use super::preset::{labui_preset_aliases, labui_preset_roles};
use super::*;
use crate::ladder::ThemeAnchors;
use crate::semantic;

/// Каноническая референс-фикстура labui (см. док модуля).
pub fn labui_reference() -> ThemeConfig {
    ThemeConfig {
        brand: Brand {
            // Пер-темный бренд labui (reference/labui-accent-primitives.md §2,
            // Figma `Accent/Brand`): light/dark/light-ic/dark-ic — дословно.
            anchors: anchors("#007AFF", "#4A8FFF", "#0040DD", "#409CFF"),
        },
        neutral: NeutralConfig {
            anchors: NeutralAnchors {
                light: "#FFFFFF".to_string(),
                mid: "#787880".to_string(),
                dark: "#101012".to_string(),
            },
            tint: NeutralTint {
                // Ручки подтона — из констант semantic.rs (единый источник истины).
                ratio: semantic::NEUTRAL_TINT_RATIO,
                target_mp: semantic::TINT_TARGET_MP,
                hue_stiffness: semantic::TINT_HUE_STIFFNESS,
                // Явный измеренный оттенок (SSOT NEUTRAL_HUE_DEG): labui несёт
                // замер, деривация из тёмного якоря — путь клиентов без замера.
                hue_override_deg: Some(semantic::NEUTRAL_HUE_DEG),
            },
            // Пер-темные края (стаб labui дословно; IC = дубль базовых — стаб
            // без ic-скоупов, наследование как у альф):
            // контур — light #101012 / dark #F6F8FA; инверт — #B0B0B9 / #3C3C43.
            edge: Some(crate::ladder::ThemeAnchors {
                light: "#101012".to_string(),
                dark: "#F6F8FA".to_string(),
                light_ic: "#101012".to_string(),
                dark_ic: "#F6F8FA".to_string(),
            }),
            inverted: Some(crate::ladder::ThemeAnchors {
                light: "#B0B0B9".to_string(),
                dark: "#3C3C43".to_string(),
                light_ic: "#B0B0B9".to_string(),
                dark_ic: "#3C3C43".to_string(),
            }),
        },
        // Палитра labui — 10 замеренных семейств, ПЕР-ТЕМНО ДОСЛОВНО из
        // reference/labui-accent-primitives.md §2 (Figma `Accent/*`, все 4 режима,
        // замер 2026-07-02). Светлый якорь совпадает с accent.rs::anchor_hex.
        palette: vec![
            fam("red", "#FF3B30", "#FF3A3A", "#D70015", "#FF6161"),
            fam("orange", "#FFA100", "#FF9008", "#C93400", "#FFA940"),
            fam("yellow", "#FFD000", "#FFD60A", "#B25000", "#FFD426"),
            fam("green", "#34C759", "#30D158", "#248A3D", "#30DB5B"),
            fam("teal", "#5AC8FA", "#64D2FF", "#0071A4", "#70D7FF"),
            fam("mint", "#00C7BE", "#63E6E2", "#0C817B", "#6CEBE7"),
            fam("blue", "#3E87FF", "#5696FF", "#0050CF", "#95C0FF"),
            fam("indigo", "#5856D6", "#5E5CE6", "#3634A3", "#7D7AFF"),
            fam("purple", "#AF52DE", "#BF5AF2", "#8944AB", "#DA8FFF"),
            fam("pink", "#FF2D55", "#FF2D55", "#D30F45", "#FF6482"),
        ],
        sentiments: SentimentsConfig {
            categories: vec![
                sentiment("danger", "red", None, None),
                sentiment(
                    "warning",
                    "orange",
                    Some(crate::sentiment::WARNING_HUE_FLOOR_DEG),
                    Some(1),
                ),
                sentiment("success", "green", None, None),
                sentiment("info", "blue", None, None),
            ],
            hardness: 5.0,
            // 1.0 = потолок на чистой стене гамута: якоря labui — авторитет
            // идентичности (Figma-калибровка, danger #FF3B30 сидит ВЫШЕ
            // 0.88·C_max — доля 0.88 съедала бы клиентский красный).
            // Реестровый дефолт для клиентов без якорной калибровки — 0.88.
            chroma_fraction: 1.0,
        },
        themes: ThemesConfig {
            entries: vec![
                ("light".to_string(), VcPreset::Srgb),
                ("dark".to_string(), VcPreset::Dim),
                ("light-ic".to_string(), VcPreset::SrgbIc),
                ("dark-ic".to_string(), VcPreset::DimIc),
            ],
        },
        roles: labui_preset_roles(),
        aliases: labui_preset_aliases(),
    }
}

/// Краткий конструктор пер-темной четвёрки якорей.
fn anchors(light: &str, dark: &str, light_ic: &str, dark_ic: &str) -> ThemeAnchors {
    ThemeAnchors {
        light: light.to_string(),
        dark: dark.to_string(),
        light_ic: light_ic.to_string(),
        dark_ic: dark_ic.to_string(),
    }
}

/// Краткий конструктор семейства палитры для фикстуры (пер-темно).
fn fam(key: &str, light: &str, dark: &str, light_ic: &str, dark_ic: &str) -> PaletteFamily {
    PaletteFamily {
        key: key.to_string(),
        anchors: anchors(light, dark, light_ic, dark_ic),
    }
}

/// Краткий конструктор сентимент-категории для фикстуры.
fn sentiment(
    name: &str,
    family: &str,
    hue_floor_deg: Option<f64>,
    preferred_side: Option<i8>,
) -> SentimentCategory {
    SentimentCategory {
        name: name.to_string(),
        family: family.to_string(),
        hue_floor_deg,
        preferred_side,
    }
}
