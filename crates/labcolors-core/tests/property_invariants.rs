//! Property / fuzz инварианты ядра `labcolors-core` — Класс В (Фаулер): проверка
//! МАТЕМАТИЧЕСКИХ законов на всём пространстве входов, а не на выбранных примерах.
//!
//! Каждый тест закрывает КЛАСС багов (инвариант), не заплатку. Дополняет
//! существующие golden/characterization (byte-identity) тесты, которые пинят
//! КОНКРЕТНЫЕ выходы, но не доказывают законы на всём домене.
//!
//! # Детерминизм (per-PR гейт)
//!
//! Каждый прогон использует ФИКСИРОВАННЫЙ seed (`TestRng::deterministic()`), поэтому
//! последовательность входов побайтово одинакова между прогонами — воспроизводимость
//! без флака. Персистентность отключена (`failure_persistence: None`): фиксированный
//! seed сам воспроизводит контрпример, файл `proptest-regressions/` не нужен.
//! Истинный инвариант зелёный независимо от seed — вот почему исход детерминирован.
//!
//! # RED-доказательство (TDD)
//!
//! Каждый инвариант получен RED-first: временная инъекция нарушения (в код-под-тестом
//! или на границе вызова) роняет property с МИНИМИЗИРОВАННЫМ (shrunk) контрпримером.
//! Зелёный-с-рождения запрещён (был бы театром). Механика инъекции описана в PR.
//!
//! # ЗАМЕЧАНИЕ ПО monotonicity (задокументированный vs фактический инвариант)
//!
//! `muddiness_oklch` документирован как «строго монотонный в C при фиксированных L,h».
//! Это ВЕРНО только для ТЁПЛЫХ оттенков (`sin(h) ≥ 0`, h∈[0°,180°]): там оба зависящих
//! от C множителя — `neutral_gate(C)` и b-гейт `sigmoid((C·sin h − B0)/BW)` — растут в C.
//! Для ХОЛОДНЫХ оттенков (`sin(h) < 0`, h∈(180°,360°)) b-гейт УБЫВАЕТ в C (b = C·sin h < 0
//! уходит от B0), поэтому произведение НЕ монотонно — глобальное утверждение доки
//! переусиливает. Property ниже проверяет ИСТИННЫЙ инвариант (тёплая полуплоскость);
//! холодный контрпример зафиксирован тестом `muddiness_cool_hue_is_not_monotone_finding`
//! как characterization реального поведения (не одобрение — фиксация факта для владельца).

use labcolors_core::{
    BgInput, Brand, DefectContext, Floor, LadderPosition, LadderSource, NeutralAnchors,
    NeutralConfig, NeutralPick, NeutralTint, PaletteFamily, Resolved, RoleRecipe,
    SentimentCategory, SentimentsConfig, Theme, ThemeAnchors, ThemeConfig, ThemesConfig, VcPreset,
    ViewingConditions, muddiness_in_context, muddiness_oklch, oklch_from_hex, p3_from_hex,
    resolve_named_set, srgb_encoded_from_hex,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

// ─────────────────────────────────────────────────────────────────────────────
// Детерминированный раннер: фиксированный seed → воспроизводимая последовательность
// ─────────────────────────────────────────────────────────────────────────────

/// Прогнать `body` на `cases` входах из `strategy` с ФИКСИРОВАННЫМ seed.
/// На нарушении `runner.run` возвращает shrunk-контрпример; `.expect` роняет тест
/// с этим минимизированным входом (наглядный RED).
fn check<S: Strategy>(cases: u32, strategy: S, body: impl Fn(S::Value) -> Result<(), TestCaseError>)
where
    S::Value: std::fmt::Debug,
{
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    runner
        .run(&strategy, body)
        .expect("property нарушен — см. минимизированный контрпример выше");
}

// ─────────────────────────────────────────────────────────────────────────────
// Общие помощники
// ─────────────────────────────────────────────────────────────────────────────

/// `#RRGGBB` из трёх байтов.
fn hex_of(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Наименьшее угловое расстояние двух оттенков в градусах, `[0, 180]`.
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = ((a - b) % 360.0 + 360.0).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

/// Две канонические viewing-conditions пресета (srgb / dim) по индексу 0/1.
fn vc_of(i: usize) -> ViewingConditions {
    if i == 0 {
        ViewingConditions::srgb()
    } else {
        ViewingConditions::dim_surround()
    }
}

/// НЕЗАВИСИМАЯ транскрипция WCAG 2.1 контраста прямо из 8-битного hex — оракул,
/// не разделяющий кода с движком (differential-проверка законности пола). Формула
/// дословно из W3C: линеаризация каналов, относительная яркость, `(L↑+0.05)/(L↓+0.05)`.
fn wcag_ratio_from_hex(fg: &str, bg: &str) -> f64 {
    fn channel(byte: u8) -> f64 {
        let c = f64::from(byte) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(hex: &str) -> f64 {
        let s = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&s[0..2], 16).unwrap();
        let g = u8::from_str_radix(&s[2..4], 16).unwrap();
        let b = u8::from_str_radix(&s[4..6], 16).unwrap();
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }
    let (la, lb) = (luminance(fg), luminance(bg));
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// `#RRGGBB` — ровно 7 символов, `#` + 6 hex-цифр.
fn is_valid_solid_hex(hex: &str) -> bool {
    hex.len() == 7 && hex.starts_with('#') && hex[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Осветлить/затемнить базовый байт для light/dark якорей семьи (валидный hex,
/// оттенок примерно сохраняется — движку для компиляции нужны лишь валидные hex).
fn lighten(c: u8) -> u8 {
    128u8.saturating_add(c / 2)
}
fn darken(c: u8) -> u8 {
    (c / 3).max(8)
}

/// Четвёрка одинаковых light/dark (IC = не-IC), как в эталонных конфигах.
fn anchors_ld(light: String, dark: String) -> ThemeAnchors {
    ThemeAnchors {
        light: light.clone(),
        dark: dark.clone(),
        light_ic: light,
        dark_ic: dark,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 1 — muddiness_oklch монотонна в C (тёплая полуплоскость)
//
// КЛАСС: инвариант монотонности параметр-свободной цены грязи. Закрывает весь
// класс «муддинес немонотонен по хроме для тёплого оттенка», не один вход.
// БЬЁТ НА МУТАЦИИ: замена `neutral_gate` sigmoid на константу / переворот знака
// аргумента / отбрасывание C-зависимости → монотонность рушится → RED.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn muddiness_is_monotone_nondecreasing_in_chroma_for_warm_hues() {
    // Тёплая полуплоскость sin(h) ≥ 0 ⇒ h∈[0°,180°]. Произведение неубывающих
    // неотрицательных множителей неубывает; допуск 1e-12 гасит f64-шум насыщения.
    check(
        512,
        (0.0f64..=1.0, 0.0f64..=180.0, 0.0f64..=0.4, 0.0f64..=0.4),
        |(l, h, c1, delta)| {
            let lo = muddiness_oklch(l, c1, h);
            let hi = muddiness_oklch(l, c1 + delta, h);
            prop_assert!(
                hi + 1e-12 >= lo,
                "муддинес убыл по C при тёплом h={h}: L={l}, C:{c1}→{} дало {lo}→{hi}",
                c1 + delta
            );
            Ok(())
        },
    );
}

#[test]
fn muddiness_strictly_increases_in_chroma_in_the_steep_warm_band() {
    // Строгая версия в КРУТОЙ полосе обоих сигмоид (около C0=0.0395, B0=0.036),
    // где приращение f64-различимо. L ≤ 0.44 < min(cusp_L)=0.454 ⇒ depth_term>0;
    // h∈[10°,170°] ⇒ sin>0 и hue_weight>0. Оба C-множителя строго растут ⇒
    // произведение строго растёт.
    check(
        512,
        (
            0.15f64..=0.44,
            10.0f64..=170.0,
            0.02f64..=0.06,
            0.008f64..=0.03,
        ),
        |(l, h, c1, delta)| {
            let lo = muddiness_oklch(l, c1, h);
            let hi = muddiness_oklch(l, c1 + delta, h);
            prop_assert!(
                hi > lo,
                "муддинес не вырос строго в крутой полосе: L={l}, h={h}, C:{c1}→{} ({lo} vs {hi})",
                c1 + delta
            );
            Ok(())
        },
    );
}

/// Закрытие ДЫРЫ, найденной mutation-прогоном (survivor `cleanliness.rs:541:35
/// replace + with * in muddiness_in_context`): surround-aware путь обязан считать
/// РЕАЛЬНУЮ Oklab-хрому `√(a²+b²)`. Мутант `a²·b²` схлопывает хрому тёплых муддистых
/// цветов почти в ноль (ниже C0) → mud падает с ~0.65 до ~0.03. Тёплые муддистые
/// оливы обязаны читаться грязными (mud > 0.3) в контексте — это убивает мутанта.
/// Заземлено замером: #6B6B2E=0.651, #8A7A2E=0.622, #7A5A20=0.666, #B8860B=0.506.
#[test]
fn muddiness_in_context_reflects_real_oklab_chroma() {
    let ctx = DefectContext {
        bg_hex: "#FFFFFF",
        theme: Theme::Light,
    };
    for hex in ["#6B6B2E", "#8A7A2E", "#7A5A20", "#B8860B"] {
        let mud = muddiness_in_context(hex, ctx).unwrap();
        assert!(
            mud > 0.3,
            "тёплый мутный {hex} обязан читаться грязным в контексте (√(a²+b²)); \
             схлопнутая хрома дала бы ~0. Получено mud={mud}"
        );
    }
}

/// Characterization реального поведения: для ХОЛОДНОГО оттенка (h=270°, sin=−1)
/// муддинес НЕ монотонен по C — сперва растёт, потом падает. Это фиксирует
/// расхождение между глобальным утверждением доки и кодом (находка для владельца),
/// НЕ одобряя его. Числа заземлены прямым вычислением ядра.
#[test]
fn muddiness_cool_hue_is_not_monotone_finding() {
    let l = 0.3;
    let h = 270.0; // sin(270°) = −1 → b = −C, b-гейт убывает в C
    let at = |c: f64| muddiness_oklch(l, c, h);
    let (a, b, c) = (at(0.0), at(0.04), at(0.10));
    // Растёт 0→0.04, затем падает 0.04→0.10 — немонотонно (не строго-монотонная кривая).
    assert!(
        b > a && b > c,
        "ожидался немонотонный холодный профиль (пик у C≈0.04): {a} {b} {c}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 2 — законность WCAG-пола на КВАНТОВАННЫХ байтах (полярность/легальность)
//
// КЛАСС: «движок эмитит цвет текст/UI-роли, нарушающий свой легальный пол на
// байтах» — для ЛЮБОГО фона. Решённый цвет либо КЛИРИТ пол на квантованных
// байтах, либо роль честно Unreachable (не тихое нарушение). Оракул НЕЗАВИСИМ
// (своя транскрипция WCAG), не читает число движка — differential-проверка.
// БЬЁТ НА МУТАЦИИ: ослабление проверки пола в solve (`>=` → всегда-true) →
// эмитится под-пороговый цвет → независимый оракул ловит → RED.
// ─────────────────────────────────────────────────────────────────────────────

/// Небольшая витрина ролей с легальными полами: сильный/слабый текст + цветной
/// бренд-лейбл (все несут пол через `RoleSpec::legal_floor`).
fn legality_table() -> labcolors_core::NamedRoleTable {
    let cfg = base_config((0x30, 0x6A, 0xE0), (0x30, 0x6A, 0xE0), (0.968, 0.6, 0.3));
    cfg.compile_named_role_table()
        .expect("витрина законности обязана компилироваться")
}

#[test]
fn every_floored_role_clears_its_wcag_floor_on_quantised_bytes() {
    let table = legality_table();
    let vcs = [ViewingConditions::srgb(), ViewingConditions::dim_surround()];
    check(
        400,
        (any::<u8>(), any::<u8>(), any::<u8>(), 0usize..2),
        |(r, g, b, vc_i)| {
            let bg_hex = hex_of(r, g, b);
            let bg = BgInput::solid(&bg_hex).expect("valid #RRGGBB фон");
            let set = resolve_named_set(&bg, &table, &vcs[vc_i]);
            for (name, spec) in table.entries() {
                let Some(floor) = spec.legal_floor() else {
                    continue; // роль без легального пола — вне закона
                };
                let resolved = set.iter().find(|(n, _)| n == name).map(|(_, r)| r);
                // Unreachable/None/Translucent — честный не-solid исход, не нарушение.
                if let Some(Resolved::Color { solved, .. }) = resolved {
                    let ratio = wcag_ratio_from_hex(solved.hex(), &bg_hex);
                    prop_assert!(
                        ratio + 1e-6 >= floor,
                        "роль `{name}` на фоне {bg_hex}: WCAG {ratio} < пол {floor} (hex {})",
                        solved.hex()
                    );
                }
            }
            Ok(())
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 3 — тотальность/валидность resolve для ПРОИЗВОЛЬНОГО валидного конфига
//
// КЛАСС: «компиляция+резолв произвольного валидного ThemeConfig паникует / теряет
// роль / эмитит битый hex». Ноль паник, ровно N ролей в порядке декларации,
// каждый solid-цвет — валидный 7-символьный `#RRGGBB`.
// БЬЁТ НА МУТАЦИИ: срыв построения таблицы / потеря записи / порча форматтера hex.
// ─────────────────────────────────────────────────────────────────────────────

/// Построить ВАЛИДНЫЙ конфиг из варьируемых скаляров (структура/имена фиксированы,
/// оттенки бренда/семьи и текстовые доли — генерируемые). `hue_override_deg` задан
/// явно ⇒ ахроматический якорь никогда не роняет компиляцию.
fn base_config(
    brand: (u8, u8, u8),
    family: (u8, u8, u8),
    fractions: (f64, f64, f64),
) -> ThemeConfig {
    let (br, bg, bb) = brand;
    let (fr, fg, fb) = family;
    let (f_strong, f_weak, f_tertiary) = fractions;
    let brand_anchors = anchors_ld(
        hex_of(lighten(br), lighten(bg), lighten(bb)),
        hex_of(darken(br), darken(bg), darken(bb)),
    );
    let family_anchors = anchors_ld(
        hex_of(lighten(fr), lighten(fg), lighten(fb)),
        hex_of(darken(fr), darken(fg), darken(fb)),
    );
    // `ThemeConfig` помечен `#[non_exhaustive]` — снаружи крейта только `new`
    // (позиционный порядок = порядок объявления полей).
    ThemeConfig::new(
        Brand {
            anchors: brand_anchors,
        },
        NeutralConfig {
            anchors: NeutralAnchors {
                light: "#FBFBFD".to_string(),
                mid: "#8A8A8E".to_string(),
                dark: "#101012".to_string(),
            },
            tint: NeutralTint {
                ratio: 0.10,
                target_mp: 5.0,
                hue_stiffness: 8.0,
                hue_override_deg: Some(286.0),
            },
            edge: None,
            inverted: None,
        },
        vec![PaletteFamily {
            key: "fam".to_string(),
            anchors: family_anchors,
        }],
        SentimentsConfig {
            categories: vec![SentimentCategory {
                name: "alert".to_string(),
                family: "fam".to_string(),
            }],
        },
        ThemesConfig {
            entries: vec![
                ("day".to_string(), VcPreset::Srgb),
                ("night".to_string(), VcPreset::Dim),
            ],
        },
        vec![
            (
                "text-strong".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: f_strong,
                    floor: Floor::AaText,
                    hue: None,
                },
            ),
            (
                "text-weak".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: f_weak,
                    floor: Floor::AaUi,
                    hue: None,
                },
            ),
            (
                "text-tertiary".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: f_tertiary,
                    floor: Floor::None,
                    hue: None,
                },
            ),
            (
                "surface".to_string(),
                RoleRecipe::Ladder {
                    source: LadderSource::Neutral(NeutralPick::Mid),
                    position: LadderPosition::NeutralFillPrimary,
                    floor: None,
                },
            ),
            (
                "brand-fill".to_string(),
                RoleRecipe::Ladder {
                    source: LadderSource::Brand,
                    position: LadderPosition::FillPrimary,
                    floor: None,
                },
            ),
            (
                "brand-label".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: 0.968,
                    floor: Floor::AaText,
                    hue: Some(LadderSource::Brand),
                },
            ),
        ],
        // Цель алиаса ОБЯЗАНА существовать среди ролей (валидатор это проверяет).
        vec![("ring".to_string(), "brand-fill".to_string())],
    )
}

/// Стратегия произвольного ВАЛИДНОГО конфига: доли строго в (0,1], убывающая
/// текстовая лестница не требуется законом (иерархию движок сам сжимает).
fn arb_config() -> impl Strategy<Value = ThemeConfig> {
    (
        (any::<u8>(), any::<u8>(), any::<u8>()),
        (any::<u8>(), any::<u8>(), any::<u8>()),
        (0.5f64..=1.0, 0.2f64..=0.7, 0.05f64..=0.4),
    )
        .prop_map(|(brand, family, fr)| base_config(brand, family, fr))
}

#[test]
fn resolve_named_set_is_total_and_emits_valid_hex_for_any_valid_config() {
    check(
        200,
        (
            arb_config(),
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            0usize..2,
        ),
        |(cfg, r, g, b, vc_i)| {
            let table = cfg
                .compile_named_role_table()
                .expect("сгенерированный конфиг сконструирован валидным ⇒ Ok");
            let vc = vc_of(vc_i);
            let bg = BgInput::solid(&hex_of(r, g, b)).expect("valid фон");
            let set = resolve_named_set(&bg, &table, &vc);

            // Ровно N ролей, в порядке декларации.
            prop_assert_eq!(set.len(), table.entries().len(), "потеряна/добавлена роль");
            for ((got, _), (want, _)) in set.iter().zip(table.entries().iter()) {
                prop_assert_eq!(got, want, "порядок ролей поехал");
            }
            // Каждый solid-цвет — валидный 7-символьный hex; Unreachable — легальный исход.
            for (name, res) in &set {
                if let Some(solved) = res.solved() {
                    prop_assert!(
                        is_valid_solid_hex(solved.hex()),
                        "роль `{}` эмитила битый hex `{}`",
                        name,
                        solved.hex()
                    );
                }
            }
            Ok(())
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 4 — детерминизм (эмиссия воспроизводима)
//
// КЛАСС: «один вход даёт разные выходы» (скрытая недетерминированность — общий
// корень флака). Дважды резолвим один (bg, table, vc) ⇒ побайтово одинаково.
// БЬЁТ НА МУТАЦИИ: внесение зависимости от неупорядоченного обхода / кэш-состояния.
// ─────────────────────────────────────────────────────────────────────────────

/// Стабильная строковая форма решённой роли (как в agnostic-гейтах).
fn repr(res: &Resolved) -> String {
    match res {
        Resolved::Color { solved, .. } => solved.hex().to_string(),
        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
        Resolved::Glow(g) => format!("glow({},{},{:.4})", g.core_hex(), g.halo_hex(), g.alpha()),
        Resolved::None => "none".to_string(),
        Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
        // `Resolved` помечен `#[non_exhaustive]`: заглушка для будущих вариантов.
        _ => "other".to_string(),
    }
}

#[test]
fn resolve_named_set_is_deterministic() {
    check(
        300,
        (arb_config(), any::<u8>(), any::<u8>(), any::<u8>()),
        |(cfg, r, g, b)| {
            let t = cfg.compile_named_role_table().expect("валидный конфиг");
            let bg = BgInput::solid(&hex_of(r, g, b)).expect("valid фон");
            let vc = ViewingConditions::srgb();
            let a = resolve_named_set(&bg, &t, &vc);
            let b2 = resolve_named_set(&bg, &t, &vc);
            prop_assert_eq!(a.len(), b2.len());
            for ((n1, r1), (n2, r2)) in a.iter().zip(b2.iter()) {
                prop_assert_eq!(n1, n2, "имена ролей разошлись между прогонами");
                prop_assert_eq!(repr(r1), repr(r2), "цвет роли `{}` недетерминирован", n1);
            }
            Ok(())
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 5 — сохранение оттенка семьи цветным лейблом (M1)
//
// КЛАСС: «цветной лейбл дрейфует из своей цветовой семьи / стерильно сереет».
// Там, где решённый лейбл несёт РЕАЛЬНУЮ хрому (C > порога), его oklch-оттенок
// остаётся в полосе семьи. Порог по хроме обязателен: у светлотных экстремумов
// max_chroma схлопывается и оттенок численно шумит (как в dim_tinted_tests).
// БЬЁТ НА МУТАЦИИ: срыв прокидывания оттенка бренда в тинт-якорь (M1) → лейбл
// сереет/уходит в нейтральный подтон → C падает или дистанция растёт → RED.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn hued_brand_label_preserves_family_hue_where_it_has_chroma() {
    // Живой насыщенный бренд (яркий синий), лейбл держит его оттенок.
    let brand = (0x0Au8, 0x54u8, 0xF0u8);
    let cfg = base_config(brand, brand, (0.968, 0.6, 0.3));
    let table = cfg.compile_named_role_table().unwrap();
    // Оттенок семьи бренда — из ФАКТИЧЕСКИ построенных светлого и тёмного якорей
    // (та же lighten/darken, что в base_config), берём оба края семьи.
    let (br, bg_, bb) = brand;
    let brand_hue_light = oklch_from_hex(&hex_of(lighten(br), lighten(bg_), lighten(bb)))
        .unwrap()
        .h_deg
        .expect("цветной светлый якорь имеет hue");
    let brand_hue_dark = oklch_from_hex(&hex_of(darken(br), darken(bg_), darken(bb)))
        .unwrap()
        .h_deg
        .expect("цветной тёмный якорь имеет hue");

    check(
        400,
        (any::<u8>(), any::<u8>(), any::<u8>(), 0usize..2),
        move |(r, g, b, vc_i)| {
            let vc = vc_of(vc_i);
            let bg = BgInput::solid(&hex_of(r, g, b)).expect("valid фон");
            let set = resolve_named_set(&bg, &table, &vc);
            let label = set.iter().find(|(n, _)| n == "brand-label").map(|(_, r)| r);
            if let Some(Resolved::Color { solved, .. }) = label {
                let coordinates = oklch_from_hex(solved.hex()).unwrap();
                // Только там, где есть реальный цвет (иначе оттенок численно шумит).
                if coordinates.c > 0.03 {
                    let hue = coordinates.h_deg.expect("цвет выше порога имеет hue");
                    let dist =
                        hue_distance(brand_hue_light, hue).min(hue_distance(brand_hue_dark, hue));
                    prop_assert!(
                        dist <= 35.0,
                        "бренд-лейбл на {} ушёл {dist:.1}° от семьи (hue {hue:.1}°, C={:.3}, hex {})",
                        hex_of(r, g, b),
                        coordinates.c,
                        solved.hex()
                    );
                }
            }
            Ok(())
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// СВОЙСТВО 6 — fuzz-устойчивость парсеров цвета (ноль паник на мусоре)
//
// КЛАСС: «парсер hex падает/паникует на враждебном/битом входе». Любая строка на
// входе `oklch_from_hex` / `srgb_encoded_from_hex` / `p3_from_hex` / `BgInput::solid`
// → аккуратный `Result` (Ok или Err), НИКОГДА не паника (паника развернула бы стек
// и уронила бы тест). Класс-закрыватель для парсеров, читающих внешний ввод.
// БЬЁТ НА МУТАЦИИ: замена валидации на `unwrap`/индексацию без границ → паника → RED.
// ─────────────────────────────────────────────────────────────────────────────

/// hex-подобные строки (мусор рядом с валидным доменом ловит краевые случаи парсера).
fn hex_like() -> impl Strategy<Value = String> {
    use proptest::string::string_regex;
    prop_oneof![
        // Произвольный мусор.
        string_regex(".{0,16}").unwrap(),
        // Почти-hex: `#` + переменная длина hex-цифр (ловит длины 3/4/6/7 и странные).
        string_regex("#?[0-9a-fA-F]{0,12}").unwrap(),
        // Смешанный ввод с не-hex символами.
        string_regex("#[0-9a-fA-FgGzZ /]{1,10}").unwrap(),
    ]
}

#[test]
fn color_parsers_never_panic_on_arbitrary_input() {
    check(2000, hex_like(), |s| {
        // Все четыре парсера обязаны вернуть Result, не паниковать.
        let _ = oklch_from_hex(&s);
        let _ = srgb_encoded_from_hex(&s);
        let _ = p3_from_hex(&s);
        let _ = BgInput::solid(&s);
        Ok(())
    });
}

/// Прицельные враждебные литералы (детерминированный набор поверх fuzz).
///
/// КОНТРАКТ ПАРСЕРА (заземлён в `spaces::srgb::hex_bytes`): принимается ТОЛЬКО
/// `#RRGGBB` — ровно 6 hex-цифр, `#` необязателен, ASCII-only. Сокращение `#RGB`
/// (3 цифры) НЕ поддерживается (в отличие от намёка в доке `recheck_against`
/// «#RGB/#RRGGBB» — расхождение доки и кода, находка для владельца). Не-ASCII из
/// 6 «символов» отвергается (ASCII-гейт против slice-паники по границе кодпоинта).
#[test]
fn color_parsers_reject_malformed_and_accept_wellformed() {
    // Битые/неподдерживаемые входы → Err (не паника, не молчаливый Ok).
    // Включая сокращённый `#RGB` — закрываем класс «шорткат тихо принят».
    for bad in [
        "",
        "#",
        "#1",
        "#12",
        "#abc",
        "#0af",
        "#12345",
        "#1234567",
        "#GGGGGG",
        "zzzzzz",
        "#12 45 67",
        "rgb(1,2,3)",
        "①②③④⑤⑥",
        "#ffffff ",
        " #ffffff",
    ] {
        assert!(
            oklch_from_hex(bad).is_err(),
            "oklch_from_hex должен отвергнуть `{bad}`"
        );
        assert!(
            srgb_encoded_from_hex(bad).is_err(),
            "srgb_encoded_from_hex должен отвергнуть `{bad}`"
        );
    }
    // Валидные `#RRGGBB` (и «голый» 6-hex без `#`) → Ok.
    for good in ["#000000", "#FFFFFF", "#3478F6", "#abcdef", "FFFFFF"] {
        assert!(
            oklch_from_hex(good).is_ok(),
            "oklch_from_hex должен принять `{good}`"
        );
        assert!(
            srgb_encoded_from_hex(good).is_ok(),
            "srgb_encoded_from_hex должен принять `{good}`"
        );
    }
}
