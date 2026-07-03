//! Каноническая референс-фикстура labui (ADR-0001 PR-c: вынесена из прод-API в
//! `#[cfg(test)]`). Дерево Даниила — семьи, сентименты, роли, нейтраль-тройка —
//! живёт ТОЛЬКО в тестах: движок агностичен, витрина его не касается. Байт-в-байт
//! гейт эмиссии этой фикстуры (`crate::agnostic_gates`) — гарант ядра.
//!
//! Собирается изнутри крейта: тянет `pub(crate)` SSOT-константы подтона из
//! `semantic` и приватные хелперы конфига — потому и `#[cfg(test)]`, не публичный
//! конфиг. Клиенты подключают свою ДС через `ThemeConfig`/`compile_named_role_table`.

use super::*;
use crate::ladder::{LadderPosition, ThemeAnchors};
use crate::semantic;
use crate::solve::Floor;

/// Каноническая референс-фикстура labui (см. док модуля).
pub(crate) fn labui_reference() -> ThemeConfig {
    // Фракции и полы — 1:1 из RoleTable::default (semantic.rs), включая border-strong
    // = контракт label-primary. Рецепты собраны так, чтобы имя роли совпадало с
    // Role::key(), а RoleSpec был идентичен дефолтному.
    let text = |fraction, floor| RoleRecipe::TextAnchor {
        fraction,
        floor,
        hue: None,
    };
    // Конструктор нейтрального источника (стаб: `Neutral/Derivable` тинтуется
    // краями нейтральной шкалы, НЕ семейством палитры).
    let neutral_pos = |pick, position| RoleRecipe::Ladder {
        source: LadderSource::Neutral(pick),
        position,
        floor: None,
    };

    // Конструкторы лестницы: источник × позиция → рецепт полупрозрачной эмиссии.
    let brand_pos = |position| RoleRecipe::Ladder {
        source: LadderSource::Brand,
        position,
        floor: None,
    };
    let sent_pos = |name: &str, position| RoleRecipe::Ladder {
        source: LadderSource::Sentiment(name.to_string()),
        position,
        floor: None,
    };

    let mut roles = vec![
        // Backgrounds — тона лестницы фонов (labui ADR-0002 §1, волна 1).
        // Асимметрия тем — закон владельца: светлая — 2 тона × 3 применения
        // (elevation тенями), тёмная — 3 тона × 2 (elevation осветлением).
        // Тон-1 = сам фон резолва (не эмитится); тона 2-3 — dJ'-шаги от него,
        // направление даёт полярность (светлая → темнее, тёмная → светлее).
        // Величины ИЗМЕРЕНЫ движком по СОБСТВЕННЫМ Figma-якорям labui
        // (examples/bg_ladder_anchors; методология HIG там же как референс):
        // «еле отличимо» светлой = 2.03 (#FFFFFF↔#F7F8FA); тёмная лестница
        // замедляется — данные: тон-2 = 5.78 (#101012→#1C1C1E), тон-3 = 9.60
        // от базы (#101012→#242426). Светлой темой тон-3 не используется —
        // маппинг ролей bg-primary/…/grouped-* на тона живёт в потребителе
        // (чередование 2×3); light-значение тона-3 = тону-2, чтобы шкала
        // оставалась честной, если потребитель его прочтёт.
        (
            "bg-tone-2".to_string(),
            RoleRecipe::DjAnchor {
                light: 2.03,
                dark: 5.78,
            },
        ),
        (
            "bg-tone-3".to_string(),
            RoleRecipe::DjAnchor {
                light: 2.03,
                dark: 9.6,
            },
        ),
        // Labels.
        ("label-primary".to_string(), text(0.968, Floor::AaText)),
        ("label-secondary".to_string(), text(0.627, Floor::AaText)),
        ("label-tertiary".to_string(), text(0.461, Floor::AaUi)),
        ("label-quaternary".to_string(), text(0.276, Floor::None)),
        // Icon.
        ("icon".to_string(), text(0.461, Floor::AaUi)),
        // Сепаратора в словаре НЕТ: бордер и сепаратор — единое целое (так
        // задумано в Figma), компонент-сепаратор применяет бордер-токен.
        // Border ladder. Strong — РАЗЛИЧИМОСТЬ, не читаемость: та же доля
        // контраста, что у label-primary, но пол non-text 3:1 (WCAG 1.4.11 для
        // границ контролов) вместо текстового 4.5:1 — бордер не обязан читаться.
        // base/soft — лестница от нейтрали: полупрозрачный mid-тинт ложится на
        // ЛЮБУЮ поверхность (композитит браузер), пер-темные пары альф — данные.
        ("border-strong".to_string(), text(0.968, Floor::AaUi)),
        (
            "border-base".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderBase),
        ),
        (
            "border-soft".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderSoft),
        ),
        ("border-ghost".to_string(), RoleRecipe::Zero),
        // Fill ladder — лестница от нейтрали (та же форма, что стаб labui:
        // rgba(mid, α) с пер-темной парой — заливка обязана красиво ложиться
        // на любой фон, солвер-солид терял полупрозрачность).
        (
            "fill-primary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillPrimary),
        ),
        (
            "fill-secondary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillSecondary),
        ),
        (
            "fill-tertiary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillTertiary),
        ),
        (
            "fill-quaternary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillQuaternary),
        ),
        ("fill-none".to_string(), RoleRecipe::Zero),
        // Тени — полупрозрачность by design (солид над картинкой/стеклом закрывал бы контент
        // пятном): тёмный якорь нейтрали в ОБЕИХ темах × пер-темная пара альф
        // ступени. Имена = стаб-контракт fx-shadow-*.
        (
            "fx-shadow-minor".to_string(),
            neutral_pos(NeutralPick::Dark, LadderPosition::ShadowMinor),
        ),
        (
            "fx-shadow-ambient".to_string(),
            neutral_pos(NeutralPick::Dark, LadderPosition::ShadowAmbient),
        ),
        (
            "fx-shadow-penumbra".to_string(),
            neutral_pos(NeutralPick::Dark, LadderPosition::ShadowPenumbra),
        ),
        (
            "fx-shadow-major".to_string(),
            neutral_pos(NeutralPick::Dark, LadderPosition::ShadowMajor),
        ),
        // Универсальный ноль.
        ("none".to_string(), RoleRecipe::Zero),
    ];

    // ── Акцентная/сентимент/FX/альфа-лестница (поглощает GAP #59) ─────────────
    // Имена = consumedRoles labui (roles.json) без префикса `--lab-`, минус
    // удаляемые по коллапсу (static-*/inverted-*/on-*/material-*, роли-от-фона).
    // Каждая семья (brand + 4 сентимента) несёт label×4 · fill×4 · border(strong/
    // base/soft). FX focus-ring/glow — солид/@52. `-tinted` — альфа-аналог солида
    // соответствующего fill-*-primary. Все альфы — из меню LadderPosition (Figma).
    // Цветной лейбл (ратификация ch5c, M1): доля/пол КАЖДОГО уровня = нейтральный
    // контракт лейбла (0.968/0.627/0.461/0.276, AaText/AaText/AaUi/None) —
    // одноуровневость поперёк характеров ПО ПОСТРОЕНИЮ; оттенок = источник семьи
    // (чистый цвет, светлота выводится контрактом на кривой семьи). Заменяет
    // прежнюю α-рампу @72/@52/@32 поверх тинта (40/40 нарушений одноуровневости,
    // нелегальность light-темы) — см. scratchpad/ch5c-ratification.md §2.
    let hued_label =
        |prefix: &str, level: &str, fraction: f64, floor: Floor, source: &LadderSource| {
            (
                format!("label-{prefix}-{level}"),
                RoleRecipe::TextAnchor {
                    fraction,
                    floor,
                    hue: Some(source.clone()),
                },
            )
        };
    let ladder_family =
        |prefix: &str, source: LadderSource, mk: &dyn Fn(LadderPosition) -> RoleRecipe| {
            use LadderPosition::*;
            vec![
                hued_label(prefix, "primary", 0.968, Floor::AaText, &source),
                hued_label(prefix, "secondary", 0.627, Floor::AaText, &source),
                hued_label(prefix, "tertiary", 0.461, Floor::AaUi, &source),
                hued_label(prefix, "quaternary", 0.276, Floor::None, &source),
                (format!("fill-{prefix}-primary"), mk(FillPrimary)),
                (format!("fill-{prefix}-secondary"), mk(FillSecondary)),
                (format!("fill-{prefix}-tertiary"), mk(FillTertiary)),
                (format!("fill-{prefix}-quaternary"), mk(FillQuaternary)),
                // M2 ch5c: солидная семейная граница обязана держать юр. пол UI
                // (3:1, WCAG 1.4.11). Солид эмитится как есть, если легален
                // (Figma-тинт цел); иначе минимальный сдвиг по кривой семьи.
                (
                    format!("border-{prefix}-strong"),
                    RoleRecipe::Ladder {
                        source: source.clone(),
                        position: BorderStrong,
                        floor: Some(Floor::AaUi),
                    },
                ),
                (format!("border-{prefix}-base"), mk(BorderBase)),
                (format!("border-{prefix}-soft"), mk(BorderSoft)),
            ]
        };

    // Brand-семья: источник = бренд.
    roles.extend(ladder_family("brand", LadderSource::Brand, &brand_pos));
    // Сентимент-семьи: источник = сентимент-категория (разводится с брендом).
    for (prefix, sname) in [
        ("danger", "danger"),
        ("warning", "warning"),
        ("success", "success"),
        ("info", "info"),
    ] {
        let mk = move |pos| sent_pos(sname, pos);
        roles.extend(ladder_family(
            prefix,
            LadderSource::Sentiment(sname.to_string()),
            &mk,
        ));
    }

    // FX focus-ring (солид) и glow (@52). Сентимент/бренд-источники — акцентные;
    // `*-neutral`/`inverted` — НЕЙТРАЛЬНЫЕ (стаб: rgb(255 255 255 / .522) и т.п.,
    // НЕ бренд).
    roles.push((
        "fx-focus-ring-brand".to_string(),
        brand_pos(LadderPosition::FocusRing),
    ));
    roles.push((
        "fx-focus-ring-danger".to_string(),
        sent_pos("danger", LadderPosition::FocusRing),
    ));
    roles.push((
        "fx-focus-ring-warning".to_string(),
        sent_pos("warning", LadderPosition::FocusRing),
    ));
    // Нейтральный фокус: тёмный край нейтрали, солид (стаб light rgb(16 16 18) =
    // Контур нейтрали ПЕР-ТЕМНЫЙ (стаб: light #101012 / dark #F6F8FA) — едет
    // на neutral.edge (дублирование одного края дало бы невидимое кольцо
    // фокуса на тёмной теме). В точном value-тесте — обе темы.
    roles.push((
        "fx-focus-ring-neutral".to_string(),
        neutral_pos(NeutralPick::Edge, LadderPosition::FocusRing),
    ));
    // Свечения — новый kind glow (labui ADR-0002 §5, 2026-07-03): screen-слои
    // цвета источника, интенсивность решается под контрактную ступень base
    // (зеркало fx-shadow-ambient) на фактическом фоне. Прежние Ladder@52
    // (фикс-альфа, нормальная композиция) вырождались на одноимённых фонах;
    // physics свечения — добавление света, не наложение краски.
    let glow = |source: LadderSource| RoleRecipe::Glow {
        source,
        step: crate::glow::GlowStep::Base,
    };
    roles.push(("fx-glow-brand".to_string(), glow(LadderSource::Brand)));
    roles.push((
        "fx-glow-danger".to_string(),
        glow(LadderSource::Sentiment("danger".to_string())),
    ));
    roles.push((
        "fx-glow-warning".to_string(),
        glow(LadderSource::Sentiment("warning".to_string())),
    ));
    roles.push((
        "fx-glow-neutral".to_string(),
        glow(LadderSource::Neutral(NeutralPick::Light)),
    ));
    // Инвертированное свечение — на neutral.inverted (пер-темная пара стаба
    // #B0B0B9 / #3C3C43 дословно). В точном value-тесте — обе темы.
    roles.push((
        "fx-glow-inverted".to_string(),
        neutral_pos(NeutralPick::Inverted, LadderPosition::Glow),
    ));
    // Skeleton — нейтральный тинт #787880 (стаб rgb(120 120 128 / …)), ПЕР-ТЕМНАЯ
    // альфа: base light @8 / dark @12, highlight @4. Источник = Neutral(Mid).
    // Skeleton-base НАСЛЕДУЕТ fill-quaternary (алиас, см. aliases ниже):
    // четверичная заливка = disabled-уровень, скелетон = будущая форма — то же
    // семейство слабых заливок, отдельной позиции не заслуживает.
    roles.push((
        "fx-skeleton-highlight".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::SkeletonHighlight),
    ));

    // Компонентные роли. accent = бренд, danger = danger-сентимент, neutral —
    // НЕЙТРАЛЬНЫЙ (стаб: fill-neutral солид-литерал; fill-neutral-tinted и
    // border-neutral алиасят нейтральные core-роли fill-primary/border-base).
    //
    // Солид-роль (`fill-accent`) = лестница LabelPrimary (солид, α=1). `-tinted` —
    // ЗАЛИВКА при низкой альфе (тинт×альфа напрямую), то есть Ladder FillPrimary: тинт
    // = якорь источника, α = @12. (AlphaAnalog-рецепт — для инверсии УЖЕ
    // РЕШЁННОГО контраст-солида, отдельный случай #119; здесь тинт-якорь эмитится
    // напрямую, поэтому Ladder, а не инверсия — иначе солид над белым дал бы
    // α_min≈1 и «-tinted» перестал быть полупрозрачным.)
    roles.push((
        "fill-accent".to_string(),
        brand_pos(LadderPosition::LabelPrimary),
    ));
    // fill-neutral — солид-литерал стаба без engine-деривации; приближен
    // солидом Neutral(Mid) и потому исключён из точного value-теста.
    roles.push((
        "fill-neutral".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::LabelPrimary),
    ));
    roles.push((
        "fill-danger".to_string(),
        sent_pos("danger", LadderPosition::LabelPrimary),
    ));
    roles.push((
        "fill-accent-tinted".to_string(),
        brand_pos(LadderPosition::FillPrimary),
    ));
    // fill-neutral-tinted = var(fill-primary) → алиас на нейтральную core-заливку.
    // border-neutral = var(border-base) → алиас (см. aliases ниже).
    roles.push((
        "fill-danger-tinted".to_string(),
        sent_pos("danger", LadderPosition::FillPrimary),
    ));
    roles.push((
        "label-accent".to_string(),
        brand_pos(LadderPosition::LabelPrimary),
    ));
    roles.push((
        "label-danger".to_string(),
        sent_pos("danger", LadderPosition::LabelPrimary),
    ));
    roles.push((
        "border-accent".to_string(),
        brand_pos(LadderPosition::BorderBase),
    ));
    // border-neutral = var(border-base): алиас на нейтральную dJ' границу core.
    roles.push((
        "border-danger".to_string(),
        sent_pos("danger", LadderPosition::BorderBase),
    ));
    roles.push((
        "border-focus".to_string(),
        brand_pos(LadderPosition::FocusRing),
    ));

    // Пары «заливка × лейбл» бейджа (crate::pair): якорь источника, минимально
    // сдвинутый до победы перцептивной стороны лейбла в штатной полярности;
    // лейбл на такой заливке — обычный nested resolve потребителя. Статики
    // покрываются тем же законом (белый/чёрный якоря нейтрали).
    let pair = |source| RoleRecipe::PairFill { source };
    roles.push(("badge-fill-brand".to_string(), pair(LadderSource::Brand)));
    for sname in ["danger", "warning", "success", "info"] {
        roles.push((
            format!("badge-fill-{sname}"),
            pair(LadderSource::Sentiment(sname.to_string())),
        ));
    }
    roles.push((
        "badge-fill-static-dark".to_string(),
        pair(LadderSource::Neutral(NeutralPick::Dark)),
    ));
    roles.push((
        "badge-fill-static-light".to_string(),
        pair(LadderSource::Neutral(NeutralPick::Light)),
    ));

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
        roles,
        // Компонентные нейтральные роли, которые стаб алиасит через var() на
        // нейтральные core-роли (одна истина, ноль дублирования значений):
        // fill-neutral-tinted = var(--lab-fill-primary); border-neutral = var(--lab-border-base).
        aliases: vec![
            (
                "fill-neutral-tinted".to_string(),
                "fill-primary".to_string(),
            ),
            ("border-neutral".to_string(), "border-base".to_string()),
            (
                "fx-skeleton-base".to_string(),
                "fill-quaternary".to_string(),
            ),
        ],
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
