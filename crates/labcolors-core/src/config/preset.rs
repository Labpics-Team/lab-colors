//! Словарь эталонного пресета labui — СЕМАНТИКА ролей/алиасов (ADR-0001 PR-c,
//! план BL-007): имена (= `Role::key()` ядра), фракции, позиции лестницы,
//! полы — ни одного цветового значения. Модуль `#[cfg(test)]`-ONLY: labui-дерево
//! НЕ входит в ОТГРУЖАЕМЫЙ код ядра (строгая агностичность) — прод-скан
//! `tests/agnostic_cleanliness.rs` этот модуль ИСКЛЮЧАЕТ. Единственный
//! потребитель — цветоносная референс-фикстура (`labui_reference`,
//! `config/fixture.rs`), которая наполняет свой словарь ИЗ этого модуля: один
//! источник, ноль расхождения.
//!
//! Дерево Даниила С ЦВЕТАМИ (замеренные hex) живёт в фикстуре — hex не покидают
//! `#[cfg(test)]`; этот модуль несёт только имена/рецепты, без цвета.

use super::*;
use crate::ladder::LadderPosition;
use crate::solve::Floor;

/// Роли эталонного пресета labui в порядке объявления.
///
/// Общий источник для фикстуры `labui_reference` (`#[cfg(test)]`, потому не
/// линк): полный эталон несёт ТОТ ЖЕ словарь — один источник, ноль расхождения.
/// Словарь несёт семантику (имена = `Role::key()` ядра, рецепты 1:1 из
/// `RoleTable::default` — тоже `#[cfg(test)]`-only оракул (ADR-0001 PR-c), потому
/// не линк) — ни одного цветового значения: якоря/ручки остаются в конфиге клиента.
pub fn labui_preset_roles() -> Vec<(String, RoleRecipe)> {
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
        // Доли — Ys-перенос Figma-якорей (генезис Y_hk: 102.6/66.5/48.9/29.3),
        // инвариант переноса — цвет: см. semantic.rs «Доли текстовой иерархии».
        ("label-primary".to_string(), text(0.97335917, Floor::AaText)),
        (
            "label-secondary".to_string(),
            text(0.64359014, Floor::AaText),
        ),
        ("label-tertiary".to_string(), text(0.47572199, Floor::AaUi)),
        (
            "label-quaternary".to_string(),
            text(0.29335999, Floor::None),
        ),
        // Иконки владеют Labels: отдельной роли `icon` в словаре НЕТ — глиф
        // красится `label-*` (по умолчанию `label-tertiary`); `icon` живёт
        // deprecation-алиасом (labui_preset_aliases). Сепаратора тоже нет:
        // бордер и сепаратор — единое целое (так задумано в Figma),
        // компонент-сепаратор применяет бордер-токен.
        // Border ladder. Strong — РАЗЛИЧИМОСТЬ, не читаемость: та же доля
        // контраста, что у label-primary, но пол non-text 3:1 (WCAG 1.4.11 для
        // границ контролов) вместо текстового 4.5:1 — бордер не обязан читаться.
        // base/soft — лестница от нейтрали: полупрозрачный mid-тинт ложится на
        // ЛЮБУЮ поверхность (композитит браузер), пер-темные пары альф — данные.
        ("border-strong".to_string(), text(0.97335917, Floor::AaUi)),
        (
            "border-base".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderBase),
        ),
        (
            "border-soft".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderSoft),
        ),
        ("border-none".to_string(), RoleRecipe::Zero),
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
    // контракт лейбла (0.97335917/0.64359014/0.47572199/0.29335999,
    // AaText/AaText/AaUi/None) —
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
                hued_label(prefix, "primary", 0.97335917, Floor::AaText, &source),
                hued_label(prefix, "secondary", 0.64359014, Floor::AaText, &source),
                hued_label(prefix, "tertiary", 0.47572199, Floor::AaUi, &source),
                hued_label(prefix, "quaternary", 0.29335999, Floor::None, &source),
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
    // Словарный канон labui#92: `fill-accent`/`fill-danger` — НЕ роли, а
    // deprecation-алиасы на `badge-fill-brand`/`badge-fill-danger` (закон пары;
    // labui_preset_aliases). `-tinted` остаётся РОЛЬЮ: ЗАЛИВКА при низкой альфе
    // (тинт×альфа напрямую), то есть Ladder FillPrimary — тинт = якорь источника,
    // α = @12 (солид над белым дал бы α_min≈1 и «-tinted» перестал быть
    // полупрозрачным, поэтому Ladder, а не инверсия).
    // fill-neutral — солид-литерал стаба без engine-деривации; приближен
    // солидом Neutral(Mid) и потому исключён из точного value-теста.
    roles.push((
        "fill-neutral".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::LabelPrimary),
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

    roles
}

/// Компонентные алиасы эталонного пресета labui (имя → существующая роль),
/// в порядке `passport.aliases` labui (словарный канон #92).
///
/// Нейтральные компонент-роли, которые стаб алиасит через `var()` на
/// нейтральные core-роли (одна истина, ноль дублирования значений):
/// fill-neutral-tinted = var(--lab-fill-primary); border-neutral =
/// var(--lab-border-base). Плюс deprecation-алиасы канона #92: fill-accent/
/// fill-danger → badge-fill-brand/danger (закон пары), icon → label-tertiary
/// (глиф красится Labels), border-ghost → border-none (честный ноль). Пресет
/// наполняет роли И алиасы как единое целое.
pub fn labui_preset_aliases() -> Vec<(String, String)> {
    vec![
        (
            "fill-neutral-tinted".to_string(),
            "fill-primary".to_string(),
        ),
        ("border-neutral".to_string(), "border-base".to_string()),
        (
            "fx-skeleton-base".to_string(),
            "fill-quaternary".to_string(),
        ),
        // Словарный канон labui#92 (порядок = passport.aliases labui; отпечаток
        // тонкий==полный чувствителен к порядку). Акцент/данжер-заливки — закон
        // пары; icon — глиф (label-tertiary); border-ghost — честный ноль.
        ("fill-accent".to_string(), "badge-fill-brand".to_string()),
        ("fill-danger".to_string(), "badge-fill-danger".to_string()),
        ("icon".to_string(), "label-tertiary".to_string()),
        ("border-ghost".to_string(), "border-none".to_string()),
    ]
}
