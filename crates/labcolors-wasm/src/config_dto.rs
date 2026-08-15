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
//! ключей и пробелов входного JSON. Это детерминированный вероятностный
//! идентификатор и дополнительный компонент ключа; корректность reload не
//! зависит от уникальности, потому что успешная загрузка очищает прежний кэш.

use labcolors_core::Floor;
use labcolors_core::config::{
    Brand, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint, PaletteFamily,
    RoleRecipe, ThemeConfig, ThemesConfig, VcPreset,
};
use labcolors_core::{LadderPosition, ThemeAnchors};
use serde::{Deserialize, Serialize};

/// Пер-темная четвёрка якорных hex.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorsDto {
    pub light: String,
    pub dark: String,
    pub light_ic: String,
    pub dark_ic: String,
}

/// Тройка якорей нейтральной шкалы.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeutralAnchorsDto {
    pub light: String,
    pub mid: String,
    pub dark: String,
}

/// Ручки нейтрального подтона.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeutralTintDto {
    pub target_mp: f64,
    pub hue_stiffness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_override_deg: Option<f64>,
}

/// Нейтраль: якоря + подтон + опциональные пер-темные края.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct FamilyDto {
    pub key: String,
    pub anchors: AnchorsDto,
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
#[serde(deny_unknown_fields)]
pub struct ThemeEntryDto {
    pub name: String,
    pub preset: VcPresetDto,
}

/// Источник тинта лестницы/альфа-аналога.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LadderSourceDto {
    Brand {},
    Family { key: String },
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

/// Рецепт роли из закрытого физического меню текущего resolver-а.
///
/// Это граница сериализации совместимого API, не доменный IR и не extension point.
/// Новая физика не должна добавляться новым recipe variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RoleRecipeDto {
    TextAnchor {
        fraction: f64,
        floor: FloorDto,
        /// Опциональный источник физической цветовой идентичности; отсутствие
        /// выбирает neutral policy таблицы.
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
        /// Опциональный юридический пол UI для solid-позиции; у полупрозрачной
        /// позиции поле должно отсутствовать.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        floor: Option<FloorDto>,
    },
    Glow {
        source: LadderSourceDto,
        step: String,
        decision_profile: String,
    },
    AlphaAnalog {
        of: LadderSourceDto,
        alpha: f64,
    },
    /// Переходная двухслойная point-композиция: base на заданном |ΔJ'| и tint
    /// с вычисленной alpha. Не является моделью glass, blur или spatial field.
    Material {
        source: LadderSourceDto,
        tone_light: f64,
        tone_dark: f64,
        floor: FloorDto,
    },
    Zero {},
}

/// Роль: имя + рецепт.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDto {
    pub name: String,
    pub recipe: RoleRecipeDto,
}

/// Компонентный алиас.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AliasDto {
    pub alias: String,
    pub target: String,
}

/// Полный конфиг темы потребителя — JSON-форма [`ThemeConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigDto {
    pub brand: AnchorsDto,
    pub neutral: NeutralDto,
    pub palette: Vec<FamilyDto>,
    pub themes: Vec<ThemeEntryDto>,
    /// `#[serde(default)]` разрешает ОПУСТИТЬ словарь синтаксически, но конфиг
    /// обязан нести собственные роли: пустой контракт (без `roles` и `aliases`)
    /// отклоняется на загрузке (`ConfigError::EmptyContract`). Полный конфиг всегда
    /// сериализует `roles` (никогда не пусто), его байты и отпечаток неизменны.
    #[serde(default)]
    pub roles: Vec<RoleDto>,
    #[serde(default)]
    pub aliases: Vec<AliasDto>,
}

/// FNV-1a 64 над канонической JSON-сериализацией DTO — отпечаток конфига.
///
/// Детерминированный вероятностный идентификатор конфига. Конфиг несёт
/// собственный словарь ролей и хэшируется как есть — форма входа и есть форма,
/// что реально резолвится.
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
            LadderSourceDto::Brand {} => LadderSource::Brand,
            LadderSourceDto::Family { key } => LadderSource::Family(key),
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
            RoleRecipeDto::Glow {
                source,
                step,
                decision_profile,
            } => RoleRecipe::Glow {
                source: source.into(),
                step: labcolors_core::glow::GlowStep::parse(&step)
                    .map_err(|bad| format!("roles.*.step: неизвестная ступень glow `{bad}` (ожидается subtle|base|bloom)"))?,
                decision_profile: labcolors_core::GlowDecisionProfileV1::parse(&decision_profile)
                    .map_err(|bad| format!("roles.*.decision_profile: неизвестный профиль glow `{bad}` (ожидается stable-v1|legacy-platform-dependent-v1)"))?,
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
            RoleRecipeDto::AlphaAnalog { of, alpha } => RoleRecipe::AlphaAnalog {
                of: of.into(),
                alpha,
            },
            RoleRecipeDto::Material {
                source,
                tone_light,
                tone_dark,
                floor,
            } => RoleRecipe::Material {
                source: source.into(),
                tone_light,
                tone_dark,
                floor: floor.into(),
            },
            RoleRecipeDto::Zero {} => RoleRecipe::Zero,
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
        let brand = Brand {
            anchors: dto.brand.into(),
        };
        let neutral = NeutralConfig {
            anchors: NeutralAnchors {
                light: dto.neutral.anchors.light,
                mid: dto.neutral.anchors.mid,
                dark: dto.neutral.anchors.dark,
            },
            tint: NeutralTint {
                target_mp: dto.neutral.tint.target_mp,
                hue_stiffness: dto.neutral.tint.hue_stiffness,
                hue_override_deg: dto.neutral.tint.hue_override_deg,
            },
            edge: dto.neutral.edge.map(Into::into),
            inverted: dto.neutral.inverted.map(Into::into),
        };
        let palette = dto
            .palette
            .into_iter()
            .map(|f| PaletteFamily {
                key: f.key,
                anchors: f.anchors.into(),
            })
            .collect();
        let themes = ThemesConfig {
            entries: dto
                .themes
                .into_iter()
                .map(|t| (t.name, t.preset.into()))
                .collect(),
        };
        let aliases = dto
            .aliases
            .into_iter()
            .map(|a| (a.alias, a.target))
            .collect();

        // ThemeConfig — #[non_exhaustive]: сборка через конструктор ядра, не
        // struct-литералом (запрещён вне крейта ядра).
        let cfg = ThemeConfig::new(brand, neutral, palette, themes, roles, aliases);
        Ok(cfg)
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
            LadderSource::Brand => LadderSourceDto::Brand {},
            LadderSource::Family(key) => LadderSourceDto::Family { key: key.clone() },
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
            RoleRecipe::Glow {
                source,
                step,
                decision_profile,
            } => RoleRecipeDto::Glow {
                source: source.try_into()?,
                step: step.key().to_string(),
                decision_profile: decision_profile.key().to_string(),
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
            RoleRecipe::AlphaAnalog { of, alpha } => RoleRecipeDto::AlphaAnalog {
                of: of.try_into()?,
                alpha: *alpha,
            },
            RoleRecipe::Material {
                source,
                tone_light,
                tone_dark,
                floor,
            } => RoleRecipeDto::Material {
                source: source.try_into()?,
                tone_light: *tone_light,
                tone_dark: *tone_dark,
                floor: floor_to_dto(*floor)?,
            },
            RoleRecipe::Zero => RoleRecipeDto::Zero {},
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
    /// Даниила вынесено из прод-API ядра (ADR-0001), граница читает паспорт.
    fn labui_dto() -> ConfigDto {
        serde_json::from_str(include_str!("../tests/data/labui.config.json"))
            .expect("паспорт labui парсится")
    }

    /// Снапшот ПРОДАКШН-паспорта labui (`labui/packages/colors/labui.config.json`):
    /// ВОКАБУЛЯР синкнут со словарным каноном (labui#92 — роль `icon` снесена в
    /// алиас на label-tertiary, `border-ghost`→`border-none`), но РЕЦЕПТЫ цветных
    /// лейблов НАМЕРЕННО оставлены в ladder-стиле — ветки M1 text-anchor не
    /// активируются. Этим `.prod.json` и отличается от канонического `.json`:
    /// покрывает путь ladder-эпохи потребителя — класс «тестируем не тот стиль
    /// рецептов, что в проде». Обновлять при изменении паспорта.
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

    /// C6 RED: удалённая специальная sentiment-схема обязана стать неизвестной,
    /// а не тихо игнорироваться serde после удаления поля/варианта.
    #[test]
    fn retired_sentiment_schema_is_rejected() {
        let mut root: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/labui.config.json"))
                .expect("fixture JSON");
        root["sentiments"] = serde_json::json!({
            "categories": [],
            "hardness": 5.0,
            "chroma_fraction": 0.88
        });
        assert!(
            serde_json::from_value::<ConfigDto>(root).is_err(),
            "retired root `sentiments` must be rejected, never ignored"
        );

        assert!(
            serde_json::from_str::<LadderSourceDto>(r#"{"kind":"sentiment","name":"warning"}"#)
                .is_err(),
            "retired source kind `sentiment` must be rejected"
        );
    }

    /// Неизвестное поле не может тихо превратить опечатку или удалённую
    /// ручку в no-op. Гейт покрывает каждую объектную границу public config.
    #[test]
    fn unknown_fields_are_rejected_at_every_config_object_boundary() {
        macro_rules! rejects {
            ($ty:ty, $json:literal) => {
                assert!(
                    serde_json::from_str::<$ty>($json).is_err(),
                    "{} accepted unknown fields from {}",
                    stringify!($ty),
                    $json
                );
            };
        }

        rejects!(
            AnchorsDto,
            r##"{"light":"#000000","dark":"#000000","light_ic":"#000000","dark_ic":"#000000","retired":true}"##
        );
        rejects!(
            NeutralAnchorsDto,
            r##"{"light":"#FFFFFF","mid":"#808080","dark":"#000000","retired":true}"##
        );
        rejects!(
            NeutralTintDto,
            r#"{"target_mp":1.5,"hue_stiffness":9.0,"hue_override_deq":286.0}"#
        );
        rejects!(
            NeutralTintDto,
            r#"{"ratio":0.1,"target_mp":1.5,"hue_stiffness":9.0}"#
        );
        rejects!(
            NeutralDto,
            r##"{"anchors":{"light":"#FFFFFF","mid":"#808080","dark":"#000000"},"tint":{"target_mp":1.5,"hue_stiffness":9.0},"retired":true}"##
        );
        rejects!(
            FamilyDto,
            r##"{"key":"red","anchors":{"light":"#000000","dark":"#000000","light_ic":"#000000","dark_ic":"#000000"},"retired":true}"##
        );
        rejects!(
            ThemeEntryDto,
            r#"{"name":"light","preset":"srgb","retired":true}"#
        );
        rejects!(
            LadderSourceDto,
            r#"{"kind":"family","key":"red","name":"warning"}"#
        );
        rejects!(LadderSourceDto, r#"{"kind":"brand","name":"warning"}"#);
        rejects!(
            RoleRecipeDto,
            r#"{"kind":"ladder","source":{"kind":"brand"},"position":"fill-primary","retired":true}"#
        );
        rejects!(
            RoleRecipeDto,
            r#"{"kind":"zero","source":{"kind":"brand"}}"#
        );
        rejects!(
            RoleDto,
            r#"{"name":"none","recipe":{"kind":"zero"},"retired":true}"#
        );
        rejects!(AliasDto, r#"{"alias":"a","target":"b","retired":true}"#);
    }

    /// Рецепт `material` (whitepaper, «Точечные композиции») гоняется через JSON без потерь: kebab-тег
    /// `material`, источник/тон/пол целы туда-обратно (поля snake_case
    /// `tone_light`/`tone_dark`, как остальная config-схема). Закрывает класс
    /// «DTO-ветка компилируется, но круг-трип врёт».
    #[test]
    fn material_recipe_round_trips_through_json() {
        use labcolors_core::Floor;
        let json = r#"{"kind":"material","source":{"kind":"neutral","pick":"mid"},"tone_light":12.0,"tone_dark":18.0,"floor":"aa-text"}"#;
        let dto: RoleRecipeDto = serde_json::from_str(json).expect("material парсится");
        let core = RoleRecipe::try_from(dto).expect("DTO → RoleRecipe");
        assert!(
            matches!(
                &core,
                RoleRecipe::Material { tone_light, tone_dark, floor: Floor::AaText, .. }
                    if (*tone_light - 12.0).abs() < 1e-12 && (*tone_dark - 18.0).abs() < 1e-12
            ),
            "material конвертируется в ядро с целыми полями"
        );
        let back = RoleRecipeDto::try_from(&core).expect("RoleRecipe → DTO");
        let re = serde_json::to_string(&back).expect("сериализуем");
        assert!(re.contains(r#""kind":"material""#), "kebab-тег цел: {re}");
        assert!(re.contains(r#""tone_light":12"#), "tone_light цел: {re}");
        assert!(re.contains(r#""tone_dark":18"#), "tone_dark цел: {re}");
        assert!(re.contains(r#""floor":"aa-text""#), "пол цел: {re}");
    }

    #[test]
    fn glow_recipe_requires_and_fingerprints_explicit_decision_profile() {
        let missing = r#"{"kind":"glow","source":{"kind":"brand"},"step":"base"}"#;
        assert!(
            serde_json::from_str::<RoleRecipeDto>(missing).is_err(),
            "implicit legacy/default profile запрещён schema-границей"
        );

        let legacy = r#"{"kind":"glow","source":{"kind":"brand"},"step":"base","decision_profile":"legacy-platform-dependent-v1"}"#;
        let dto: RoleRecipeDto = serde_json::from_str(legacy).expect("explicit legacy парсится");
        let core = RoleRecipe::try_from(dto).expect("known profile компилируется");
        assert!(matches!(
            core,
            RoleRecipe::Glow {
                decision_profile: labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1,
                ..
            }
        ));

        let mut stable = labui_dto();
        let role = stable
            .roles
            .iter_mut()
            .find(|role| matches!(role.recipe, RoleRecipeDto::Glow { .. }))
            .expect("anti-vacuum: в паспорте есть glow");
        let RoleRecipeDto::Glow {
            decision_profile, ..
        } = &mut role.recipe
        else {
            unreachable!("find выше сузил variant")
        };
        *decision_profile = "stable-v1".to_string();
        assert_ne!(
            fingerprint(&labui_dto()),
            fingerprint(&stable),
            "decision profile обязан входить в canonical config identity"
        );
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

    // ── Слой 3: отпечаток полного паспорта закреплён (характеризационный пин) ──

    /// Характеризационный пин канонической JSON-формы текущего Lab UI-паспорта.
    /// Обновляется только вместе с проверенным изменением схемы или данных.
    /// C6 удалил корневой sentiment-объект, заменил специальные source-теги
    /// обычными family-ссылками и удалил неиспользуемый `neutral.tint.ratio`;
    /// именно эти изменения канонического JSON объясняют смену отпечатка.
    #[test]
    fn full_labui_fingerprint_pin_current_main() {
        let full = labui_dto();
        assert_eq!(
            format!("{:016x}", fingerprint(&full)),
            "bce14f09e43c705a",
            "пин паспорта main; при легитимной смене паспорта обнови это число"
        );
    }
}
