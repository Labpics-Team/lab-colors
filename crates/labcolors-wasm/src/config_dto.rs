//! Сериализуемое зеркало [`ThemeConfig`] — JSON-контракт границы WASM.
//!
//! Ядро намеренно не знает сериализации (ноль runtime-зависимостей); JSON живёт
//! только здесь, на границе. DTO повторяет структуру ядра 1:1 (snake_case поля,
//! enum-ы — tagged `{"kind": …}` и kebab-строки), конверсия — в обе стороны:
//! `TryFrom<ConfigDto> for ThemeConfig` (вход `load_config`) и
//! `TryFrom<&ThemeConfig> for ConfigDto` (сериализация эталонов в тестах и
//! пакете). Обе стороны честно падают на неизвестном варианте — ядро несёт
//! `#[non_exhaustive]`-меню, и молчаливый пропуск варианта был бы тихой потерей
//! роли.
//!
//! Отпечаток конфига ([`fingerprint`]) — FNV-1a 64 над канонической
//! JSON-сериализацией DTO: порядок полей структур фиксирован serde, поэтому
//! один и тот же конфиг даёт один и тот же отпечаток независимо от порядка
//! ключей и пробелов входного JSON. Отпечаток — компонент ключа контракт-кэша:
//! два разных конфига обязаны давать разные ключи (кэш-коллизия = чужие цвета).

use labcolors_core::config::{
    Brand, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint, PaletteFamily,
    RoleRecipe, SentimentCategory, SentimentsConfig, ThemeConfig, ThemesConfig, VcPreset,
};
use labcolors_core::solve::Floor;
use labcolors_core::{LadderPosition, ThemeAnchors};
use serde::{Deserialize, Serialize};

/// Пер-темная четвёрка якорных hex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorsDto {
    pub light: String,
    pub dark: String,
    pub light_ic: String,
    pub dark_ic: String,
}

/// Тройка якорей нейтральной шкалы.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralAnchorsDto {
    pub light: String,
    pub mid: String,
    pub dark: String,
}

/// Ручки нейтрального подтона.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralTintDto {
    pub ratio: f64,
    pub target_mp: f64,
    pub hue_stiffness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_override_deg: Option<f64>,
}

/// Нейтраль: якоря + подтон + опциональные пер-темные края.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralDto {
    pub anchors: NeutralAnchorsDto,
    pub tint: NeutralTintDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<AnchorsDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverted: Option<AnchorsDto>,
}

/// Именованное семейство палитры.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyDto {
    pub key: String,
    pub anchors: AnchorsDto,
}

/// Одна сентимент-категория.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentCategoryDto {
    pub name: String,
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_floor_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_side: Option<i8>,
}

/// Сентимент-политика.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentsDto {
    pub categories: Vec<SentimentCategoryDto>,
    pub hardness: f64,
    pub chroma_fraction: f64,
}

/// VC-пресет закрытого меню.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VcPresetDto {
    Srgb,
    Dim,
    SrgbIc,
    DimIc,
}

/// Запись словаря тем.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeEntryDto {
    pub name: String,
    pub preset: VcPresetDto,
}

/// Источник тинта лестницы/альфа-аналога.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LadderSourceDto {
    Brand,
    Family { key: String },
    Sentiment { name: String },
    Neutral { pick: NeutralPickDto },
}

/// Выбор нейтрального якоря.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NeutralPickDto {
    Mid,
    Edge,
    Inverted,
    Light,
    Dark,
}

/// WCAG-пол текстового якоря.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FloorDto {
    AaText,
    AaUi,
    None,
}

/// Рецепт роли (физическое меню ядра).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RoleRecipeDto {
    TextAnchor {
        fraction: f64,
        floor: FloorDto,
        /// Опциональный источник оттенка семьи (M1 ch5c) — аддитивен: отсутствие
        /// = нейтральный лейбл (прежние конфиги читаются без изменений).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hue: Option<LadderSourceDto>,
    },
    DjAnchor {
        light: f64,
        dark: f64,
    },
    DecorativeLc {
        magnitude: f64,
    },
    Ladder {
        source: LadderSourceDto,
        position: String,
        /// Опциональный юр. пол UI для солидной семейной границы (M2 ch5c) —
        /// аддитивен: отсутствие = прежний путь без пола.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        floor: Option<FloorDto>,
    },
    Glow {
        source: LadderSourceDto,
        step: String,
    },
    PairFill {
        source: LadderSourceDto,
    },
    AlphaAnalog {
        of: LadderSourceDto,
        alpha: f64,
    },
    Zero,
}

/// Роль: имя + рецепт.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDto {
    pub name: String,
    pub recipe: RoleRecipeDto,
}

/// Компонентный алиас.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasDto {
    pub alias: String,
    pub target: String,
}

/// Полный конфиг темы потребителя — JSON-форма [`ThemeConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDto {
    pub brand: AnchorsDto,
    pub neutral: NeutralDto,
    pub palette: Vec<FamilyDto>,
    pub sentiments: SentimentsDto,
    pub themes: Vec<ThemeEntryDto>,
    pub roles: Vec<RoleDto>,
    #[serde(default)]
    pub aliases: Vec<AliasDto>,
}

/// FNV-1a 64 над канонической JSON-сериализацией DTO — отпечаток конфига.
///
/// Не криптографический: различение конфигов ВЕРОЯТНОСТНОЕ, поэтому оно не
/// несущая гарантия — корректность кэша держит очистка при загрузке (в кэше
/// одномоментно одно пространство ключей); отпечаток — идентичность конфига
/// наружу и belt-and-suspenders в ключе. Детерминизм даёт serde: порядок
/// полей структур фиксирован, вход нормализуется парсингом.
pub fn fingerprint(dto: &ConfigDto) -> u64 {
    let bytes = serde_json::to_vec(dto).expect("DTO без не-сериализуемых типов");
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

// ─────────────────────────────────────────────────────────────────────────────
// DTO → ядро (вход load_config).
// ─────────────────────────────────────────────────────────────────────────────

impl From<AnchorsDto> for ThemeAnchors {
    fn from(a: AnchorsDto) -> Self {
        ThemeAnchors {
            light: a.light,
            dark: a.dark,
            light_ic: a.light_ic,
            dark_ic: a.dark_ic,
        }
    }
}

impl From<VcPresetDto> for VcPreset {
    fn from(p: VcPresetDto) -> Self {
        match p {
            VcPresetDto::Srgb => VcPreset::Srgb,
            VcPresetDto::Dim => VcPreset::Dim,
            VcPresetDto::SrgbIc => VcPreset::SrgbIc,
            VcPresetDto::DimIc => VcPreset::DimIc,
        }
    }
}

impl From<NeutralPickDto> for NeutralPick {
    fn from(p: NeutralPickDto) -> Self {
        match p {
            NeutralPickDto::Mid => NeutralPick::Mid,
            NeutralPickDto::Edge => NeutralPick::Edge,
            NeutralPickDto::Inverted => NeutralPick::Inverted,
            NeutralPickDto::Light => NeutralPick::Light,
            NeutralPickDto::Dark => NeutralPick::Dark,
        }
    }
}

impl From<FloorDto> for Floor {
    fn from(f: FloorDto) -> Self {
        match f {
            FloorDto::AaText => Floor::AaText,
            FloorDto::AaUi => Floor::AaUi,
            FloorDto::None => Floor::None,
        }
    }
}

impl From<LadderSourceDto> for LadderSource {
    fn from(s: LadderSourceDto) -> Self {
        match s {
            LadderSourceDto::Brand => LadderSource::Brand,
            LadderSourceDto::Family { key } => LadderSource::Family(key),
            LadderSourceDto::Sentiment { name } => LadderSource::Sentiment(name),
            LadderSourceDto::Neutral { pick } => LadderSource::Neutral(pick.into()),
        }
    }
}

/// Позиция лестницы из стабильного kebab-ключа ([`LadderPosition::key`]).
fn position_from_key(key: &str) -> Result<LadderPosition, String> {
    LadderPosition::ALL
        .into_iter()
        .find(|p| p.key() == key)
        .ok_or_else(|| {
            format!(
                "неизвестная позиция лестницы `{key}` (меню: {})",
                LadderPosition::ALL.map(|p| p.key()).join(", ")
            )
        })
}

/// [`Floor`] → сериализуемый [`FloorDto`] (kebab). Разделяемо `TextAnchor` и
/// опциональным полом `Ladder` (M2 ch5c). `Floor` — `#[non_exhaustive]`:
/// неизвестный вариант — честный `Err`, не тихий дефолт.
fn floor_to_dto(f: Floor) -> Result<FloorDto, String> {
    Ok(match f {
        Floor::AaText => FloorDto::AaText,
        Floor::AaUi => FloorDto::AaUi,
        Floor::None => FloorDto::None,
        other => return Err(format!("несериализуемый Floor: {other:?}")),
    })
}

impl TryFrom<RoleRecipeDto> for RoleRecipe {
    type Error = String;

    fn try_from(r: RoleRecipeDto) -> Result<Self, String> {
        Ok(match r {
            RoleRecipeDto::Glow { source, step } => RoleRecipe::Glow {
                source: source.into(),
                step: labcolors_core::glow::GlowStep::parse(&step)
                    .map_err(|bad| format!("roles.*.step: неизвестная ступень glow `{bad}` (ожидается subtle|base|bloom)"))?,
            },
            RoleRecipeDto::TextAnchor {
                fraction,
                floor,
                hue,
            } => RoleRecipe::TextAnchor {
                fraction,
                floor: floor.into(),
                hue: hue.map(LadderSource::from),
            },
            RoleRecipeDto::DjAnchor { light, dark } => RoleRecipe::DjAnchor { light, dark },
            RoleRecipeDto::DecorativeLc { magnitude } => RoleRecipe::DecorativeLc { magnitude },
            RoleRecipeDto::Ladder {
                source,
                position,
                floor,
            } => RoleRecipe::Ladder {
                source: source.into(),
                position: position_from_key(&position)?,
                floor: floor.map(Floor::from),
            },
            RoleRecipeDto::PairFill { source } => RoleRecipe::PairFill {
                source: source.into(),
            },
            RoleRecipeDto::AlphaAnalog { of, alpha } => RoleRecipe::AlphaAnalog {
                of: of.into(),
                alpha,
            },
            RoleRecipeDto::Zero => RoleRecipe::Zero,
        })
    }
}

impl TryFrom<ConfigDto> for ThemeConfig {
    type Error = String;

    fn try_from(dto: ConfigDto) -> Result<Self, String> {
        let mut roles = Vec::with_capacity(dto.roles.len());
        for role in dto.roles {
            roles.push((role.name, RoleRecipe::try_from(role.recipe)?));
        }
        Ok(ThemeConfig {
            brand: Brand {
                anchors: dto.brand.into(),
            },
            neutral: NeutralConfig {
                anchors: NeutralAnchors {
                    light: dto.neutral.anchors.light,
                    mid: dto.neutral.anchors.mid,
                    dark: dto.neutral.anchors.dark,
                },
                tint: NeutralTint {
                    ratio: dto.neutral.tint.ratio,
                    target_mp: dto.neutral.tint.target_mp,
                    hue_stiffness: dto.neutral.tint.hue_stiffness,
                    hue_override_deg: dto.neutral.tint.hue_override_deg,
                },
                edge: dto.neutral.edge.map(Into::into),
                inverted: dto.neutral.inverted.map(Into::into),
            },
            palette: dto
                .palette
                .into_iter()
                .map(|f| PaletteFamily {
                    key: f.key,
                    anchors: f.anchors.into(),
                })
                .collect(),
            sentiments: SentimentsConfig {
                categories: dto
                    .sentiments
                    .categories
                    .into_iter()
                    .map(|c| SentimentCategory {
                        name: c.name,
                        family: c.family,
                        hue_floor_deg: c.hue_floor_deg,
                        preferred_side: c.preferred_side,
                    })
                    .collect(),
                hardness: dto.sentiments.hardness,
                chroma_fraction: dto.sentiments.chroma_fraction,
            },
            themes: ThemesConfig {
                entries: dto
                    .themes
                    .into_iter()
                    .map(|t| (t.name, t.preset.into()))
                    .collect(),
            },
            roles,
            aliases: dto
                .aliases
                .into_iter()
                .map(|a| (a.alias, a.target))
                .collect(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ядро → DTO (сериализация эталонов; честный Err на неизвестном варианте).
// ─────────────────────────────────────────────────────────────────────────────

impl From<&ThemeAnchors> for AnchorsDto {
    fn from(a: &ThemeAnchors) -> Self {
        AnchorsDto {
            light: a.light.clone(),
            dark: a.dark.clone(),
            light_ic: a.light_ic.clone(),
            dark_ic: a.dark_ic.clone(),
        }
    }
}

impl TryFrom<&LadderSource> for LadderSourceDto {
    type Error = String;

    fn try_from(s: &LadderSource) -> Result<Self, String> {
        Ok(match s {
            LadderSource::Brand => LadderSourceDto::Brand,
            LadderSource::Family(key) => LadderSourceDto::Family { key: key.clone() },
            LadderSource::Sentiment(name) => LadderSourceDto::Sentiment { name: name.clone() },
            LadderSource::Neutral(pick) => LadderSourceDto::Neutral {
                pick: match pick {
                    NeutralPick::Mid => NeutralPickDto::Mid,
                    NeutralPick::Edge => NeutralPickDto::Edge,
                    NeutralPick::Inverted => NeutralPickDto::Inverted,
                    NeutralPick::Light => NeutralPickDto::Light,
                    NeutralPick::Dark => NeutralPickDto::Dark,
                    other => return Err(format!("несериализуемый NeutralPick: {other:?}")),
                },
            },
            other => return Err(format!("несериализуемый LadderSource: {other:?}")),
        })
    }
}

impl TryFrom<&RoleRecipe> for RoleRecipeDto {
    type Error = String;

    fn try_from(r: &RoleRecipe) -> Result<Self, String> {
        Ok(match r {
            RoleRecipe::Glow { source, step } => RoleRecipeDto::Glow {
                source: source.try_into()?,
                step: step.key().to_string(),
            },
            RoleRecipe::TextAnchor {
                fraction,
                floor,
                hue,
            } => RoleRecipeDto::TextAnchor {
                fraction: *fraction,
                floor: floor_to_dto(*floor)?,
                hue: hue.as_ref().map(TryInto::try_into).transpose()?,
            },
            RoleRecipe::DjAnchor { light, dark } => RoleRecipeDto::DjAnchor {
                light: *light,
                dark: *dark,
            },
            RoleRecipe::DecorativeLc { magnitude } => RoleRecipeDto::DecorativeLc {
                magnitude: *magnitude,
            },
            RoleRecipe::Ladder {
                source,
                position,
                floor,
            } => RoleRecipeDto::Ladder {
                source: source.try_into()?,
                position: position.key().to_string(),
                floor: floor.map(floor_to_dto).transpose()?,
            },
            RoleRecipe::PairFill { source } => RoleRecipeDto::PairFill {
                source: source.try_into()?,
            },
            RoleRecipe::AlphaAnalog { of, alpha } => RoleRecipeDto::AlphaAnalog {
                of: of.try_into()?,
                alpha: *alpha,
            },
            RoleRecipe::Zero => RoleRecipeDto::Zero,
            other => return Err(format!("несериализуемый RoleRecipe: {other:?}")),
        })
    }
}

impl TryFrom<&ThemeConfig> for ConfigDto {
    type Error = String;

    fn try_from(cfg: &ThemeConfig) -> Result<Self, String> {
        let mut roles = Vec::with_capacity(cfg.roles.len());
        for (name, recipe) in &cfg.roles {
            roles.push(RoleDto {
                name: name.clone(),
                recipe: recipe.try_into()?,
            });
        }
        Ok(ConfigDto {
            brand: (&cfg.brand.anchors).into(),
            neutral: NeutralDto {
                anchors: NeutralAnchorsDto {
                    light: cfg.neutral.anchors.light.clone(),
                    mid: cfg.neutral.anchors.mid.clone(),
                    dark: cfg.neutral.anchors.dark.clone(),
                },
                tint: NeutralTintDto {
                    ratio: cfg.neutral.tint.ratio,
                    target_mp: cfg.neutral.tint.target_mp,
                    hue_stiffness: cfg.neutral.tint.hue_stiffness,
                    hue_override_deg: cfg.neutral.tint.hue_override_deg,
                },
                edge: cfg.neutral.edge.as_ref().map(Into::into),
                inverted: cfg.neutral.inverted.as_ref().map(Into::into),
            },
            palette: cfg
                .palette
                .iter()
                .map(|f| FamilyDto {
                    key: f.key.clone(),
                    anchors: (&f.anchors).into(),
                })
                .collect(),
            sentiments: SentimentsDto {
                categories: cfg
                    .sentiments
                    .categories
                    .iter()
                    .map(|c| SentimentCategoryDto {
                        name: c.name.clone(),
                        family: c.family.clone(),
                        hue_floor_deg: c.hue_floor_deg,
                        preferred_side: c.preferred_side,
                    })
                    .collect(),
                hardness: cfg.sentiments.hardness,
                chroma_fraction: cfg.sentiments.chroma_fraction,
            },
            themes: cfg
                .themes
                .entries
                .iter()
                .map(|(name, preset)| ThemeEntryDto {
                    name: name.clone(),
                    preset: match preset {
                        VcPreset::Srgb => VcPresetDto::Srgb,
                        VcPreset::Dim => VcPresetDto::Dim,
                        VcPreset::SrgbIc => VcPresetDto::SrgbIc,
                        VcPreset::DimIc => VcPresetDto::DimIc,
                    },
                })
                .collect(),
            roles,
            aliases: cfg
                .aliases
                .iter()
                .map(|(alias, target)| AliasDto {
                    alias: alias.clone(),
                    target: target.clone(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Паспорт labui как статический SSOT (`tests/data/labui.config.json`): дерево
    /// Даниила вынесено из прод-API ядра (ADR-0001 PR-c), граница читает паспорт.
    fn labui_dto() -> ConfigDto {
        serde_json::from_str(include_str!("../tests/data/labui.config.json"))
            .expect("паспорт labui парсится")
    }

    /// Снапшот ПРОДАКШН-паспорта labui (`labui/packages/colors/labui.config.json`,
    /// @ labui bd7b843 (#80), sha256 f9bbf7e4… — снапшот, обновлять при изменении паспорта): цветные лейблы там ещё в ladder-стиле, ветки
    /// M1 text-anchor не активируются. Покрывает путь, которым потребитель идёт
    /// СЕГОДНЯ, — класс «тестируем не тот стиль рецептов, что в проде».
    fn labui_prod_dto() -> ConfigDto {
        serde_json::from_str(include_str!("../tests/data/labui.config.prod.json"))
            .expect("прод-паспорт labui парсится")
    }

    /// Прод-снапшот гоняется тем же путём без потерь и компилируется — паритет
    /// гейта для обоих стилей паспорта (канонический M1 + прод-ladder).
    #[test]
    fn labui_prod_passport_round_trips_and_compiles() {
        let cfg = ThemeConfig::try_from(labui_prod_dto()).expect("прод-паспорт → ThemeConfig");
        let dto = ConfigDto::try_from(&cfg).expect("сериализуем");
        let json = serde_json::to_string(&dto).expect("JSON");
        let back: ConfigDto = serde_json::from_str(&json).expect("парсится");
        let restored = ThemeConfig::try_from(back).expect("конвертируется");
        assert_eq!(cfg, restored, "JSON-путь без потерь (прод-стиль)");
        restored
            .compile_named_role_table()
            .expect("прод-паспорт компилируется");
    }

    /// Канонический конфиг гоняется через JSON туда-обратно без потерь:
    /// паспорт → ядро → DTO → JSON → DTO → ядро даёт РАВНЫЙ конфиг (PartialEq ядра).
    #[test]
    fn labui_passport_round_trips_through_json() {
        let cfg = ThemeConfig::try_from(labui_dto()).expect("паспорт → ThemeConfig");
        let dto = ConfigDto::try_from(&cfg).expect("эталон сериализуем");
        let json = serde_json::to_string(&dto).expect("JSON");
        let back: ConfigDto = serde_json::from_str(&json).expect("парсится");
        let restored = ThemeConfig::try_from(back).expect("конвертируется");
        assert_eq!(cfg, restored, "JSON-путь без потерь");
        restored
            .compile_named_role_table()
            .expect("восстановленный конфиг компилируется");
    }

    /// Отпечаток: детерминирован для одного конфига (включая нормализацию
    /// пробелов/порядка через парсинг) и различает разные конфиги.
    #[test]
    fn fingerprint_is_deterministic_and_discriminating() {
        let dto = labui_dto();
        let fp1 = fingerprint(&dto);
        // Реконструкция из JSON — тот же отпечаток.
        let json = serde_json::to_string_pretty(&dto).unwrap();
        let re: ConfigDto = serde_json::from_str(&json).unwrap();
        assert_eq!(fp1, fingerprint(&re), "детерминизм через JSON-нормализацию");

        // Минимальная мутация (один якорь бренда) — другой отпечаток.
        let mut other = labui_dto();
        other.brand.light = "#007AFE".to_string();
        assert_ne!(fp1, fingerprint(&other), "разные конфиги различимы");
    }

    /// Неизвестная позиция лестницы — честная ошибка с перечнем меню.
    #[test]
    fn unknown_ladder_position_is_rejected_with_menu() {
        let err = position_from_key("label-quinary").unwrap_err();
        assert!(err.contains("label-quinary") && err.contains("label-primary"));
    }
}
