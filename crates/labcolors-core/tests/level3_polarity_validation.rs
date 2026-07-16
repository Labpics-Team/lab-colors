//! ВАЛИДАЦИЯ (задача #64) — level-3 солвер полярности лейбла на замороженном
//! 49-якорном паспорте labui.
//!
//! # Что валидируется
//!
//! Level-3 солвер выбирает сторону лейбла как **максимизацию |Lc|** по
//! контраст-ядру APCA. Продакшн-предикат `pair::pair_side` — СМЕШАННО-ДОМЕННЫЙ
//! (глава #64, ADR-0003): чернильная сторона меряется от воспринимаемой яркости
//! якоря `Y_eff` (Гельмгольц–Кольрауш, `bg_luma`), белая — от чистого экранного
//! `Ys`. `Ink ⇔ |contrast_core(0, Y_eff)| > |contrast_core(1, Ys)|`. Насыщенный
//! фон выглядит светлее (`Y_eff > Ys`), поэтому раньше готов нести ЧЕРНИЛА.
//!
//! На АХРОМАТИЧЕСКОЙ оси H-K-член ≈ 0 (`Y_eff ≈ Ys`) и правило редуцируется к
//! чисто-люминансному перелому `PAIR_CROSSOVER_Y = 0.341955` — его ахроматический
//! терминал. На ХРОМАТИЧЕСКИХ фонах H-K поднимает якорь над порогом РАНЬШЕ:
//! продакшн отдаёт чернила там, где чистый порог ещё держит белый. Разница =
//! ровно те якоря, где level-3 H-K МЕНЯЕТ полярность (промоция Light→Ink).
//!
//! Тест гоняет ЧЕТЫРЕ величины на 49 РЕАЛЬНЫХ якорях и печатает таблицу главы:
//! 1. **Чисто-люминансный порог** (`Y < PAIR_CROSSOVER_Y`) — ахроматический
//!    терминал / level-2 референс.
//! 2. **H-K-домен** (продакшн `lpc`, кормит `Y_hk` ОБЕ стороны) — argmax |Lc|
//!    полной apparent-метрики, характеризация.
//! 3. **Домен `Ys`** (ADR-0003 вариант A, `lpc_readability_ys` ОБЕ стороны) —
//!    argmax |Lc| в домене, где откалиброваны константы APCA SAPC-8.
//! 4. **Продакшн level-3** (`pair::pair_side`) — реально отгружаемое решение.
//!
//! Расхождение продакшна с чистым порогом — ровно якоря H-K-промоции; тест это
//! считает, печатает и запирает НАПРАВЛЕНИЕ (только Light→Ink) + поимённый набор.
//!
//! # Воспроизведение
//! ```text
//! cargo test -p labcolors-core --test level3_polarity_validation -- --nocapture
//! ```
//! Все числа главы §4 whitepaper — из stdout `table_level3_polarity_over_49_anchors`.
//!
//! Источник паспорта: `crates/labcolors-wasm/tests/data/labui.config.json`,
//! замороженный дедуп-набор = `exposure_support::LABUI_ANCHORS` (cfg(test)-
//! внутренний, недоступен интеграционному тесту, потому список воспроизведён здесь
//! и сверяется по длине 49).

use labcolors_core::lpc::{lpc, lpc_readability_ys};
use labcolors_core::pair::{PairSide, pair_side};
use labcolors_core::srgb_encoded_from_hex;

/// 49 реальных якорей labui (роли/сентименты/акценты/нейтраль). Дословная копия
/// замороженного `exposure_support::LABUI_ANCHORS`.
const ANCHORS: [&str; 49] = [
    "#0040DD", "#0050CF", "#0071A4", "#007AFF", "#00C7BE", "#0C817B", "#101012", "#248A3D",
    "#30D158", "#30DB5B", "#34C759", "#3634A3", "#3C3C43", "#3E87FF", "#409CFF", "#4A8FFF",
    "#5696FF", "#5856D6", "#5AC8FA", "#5E5CE6", "#63E6E2", "#64D2FF", "#6CEBE7", "#70D7FF",
    "#787880", "#7D7AFF", "#8944AB", "#95C0FF", "#AF52DE", "#B0B0B9", "#B25000", "#BF5AF2",
    "#C93400", "#D30F45", "#D70015", "#DA8FFF", "#F6F8FA", "#FF2D55", "#FF3A3A", "#FF3B30",
    "#FF6161", "#FF6482", "#FF9008", "#FFA100", "#FFA940", "#FFD000", "#FFD426", "#FFD60A",
    "#FFFFFF",
];

/// Y-порог кроссовера чисто-люминансного контраст-ядра `contrast_core` — литерал
/// воспроизводит `pair::PAIR_CROSSOVER_Y` (`pub(crate)`, извне недоступен). НЕ
/// слепая копия: тест `pure_luminance_side_matches_production_pair_side` доказывает,
/// что `y < PAIR_CROSSOVER_Y` даёт РОВНО то же разбиение, что продакшн `pair_side`,
/// на всех 49 якорях — дрейф константы в ядре расщепил бы согласие.
const PAIR_CROSSOVER_Y: f64 = 0.341_955;

/// sRGB EOTF (IEC 61966-2-1 §6.4), неотрицательная ветвь — реплика
/// `spaces::srgb::srgb_gamma_inv` (в `pub(crate) mod spaces`, извне недоступен).
/// Стандарт, не политика: та же ветвь, что кормит `pair::wcag_y_encoded`.
fn srgb_eotf(v: f64) -> f64 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance (Rec.709 / BT.709 luma) кодированного якоря — домен
/// решения `pair_side`. Реплика `pair::wcag_y_encoded` (private).
fn wcag_y(hex: &str) -> f64 {
    let e = srgb_encoded_from_hex(hex).expect("passport hex valid");
    0.2126 * srgb_eotf(e[0]) + 0.7152 * srgb_eotf(e[1]) + 0.0722 * srgb_eotf(e[2])
}

/// Сторона лейбла: белый (светлая) или чёрный (чернильная).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    White,
    Black,
}

impl Label {
    fn tag(self) -> &'static str {
        match self {
            Label::White => "белый",
            Label::Black => "чёрный",
        }
    }
}

/// Из `PairSide` (чисто-люминансный домен) в сторону лейбла.
fn side_of(p: PairSide) -> Label {
    match p {
        PairSide::Light => Label::White,
        PairSide::Ink => Label::Black,
    }
}

/// Level-3 argmax |Lc| над произвольной контраст-функцией домена. Чёрный
/// выигрывает при `|Lc_чёрный| >= |Lc_белый|` — та же ориентация перелома, что
/// пиннит `PAIR_CROSSOVER_Y` (первый серый байт, где чёрный догоняет белый).
fn argmax_side(lc_white: f64, lc_black: f64) -> Label {
    if lc_black.abs() >= lc_white.abs() {
        Label::Black
    } else {
        Label::White
    }
}

/// Полный замер одного якоря во всех четырёх величинах.
struct Row {
    hex: &'static str,
    y: f64,
    // H-K домен (продакшн lpc, Y_hk обе стороны) — характеризация.
    lc_white_hk: f64,
    lc_black_hk: f64,
    p_hk: Label,
    // Ys домен (ADR-0003 вариант A, обе стороны Ys) — характеризация.
    lc_white_ys: f64,
    lc_black_ys: f64,
    p_ys: Label,
    // Чисто-люминансный порог `Y < PAIR_CROSSOVER_Y` — ахроматический терминал.
    p_thr: Label,
    // Продакшн level-3 `pair::pair_side` — реально отгружаемое решение.
    p_prod: Label,
}

fn measure(hex: &'static str) -> Row {
    let enc = srgb_encoded_from_hex(hex).expect("passport hex valid");
    let white_disp = srgb_encoded_from_hex("#FFFFFF").unwrap();
    let black_disp = srgb_encoded_from_hex("#000000").unwrap();

    let lc_white_hk = lpc("#FFFFFF", hex);
    let lc_black_hk = lpc("#000000", hex);
    let lc_white_ys = lpc_readability_ys(white_disp, enc);
    let lc_black_ys = lpc_readability_ys(black_disp, enc);
    let y = wcag_y(hex);

    Row {
        hex,
        y,
        lc_white_hk,
        lc_black_hk,
        p_hk: argmax_side(lc_white_hk, lc_black_hk),
        lc_white_ys,
        lc_black_ys,
        p_ys: argmax_side(lc_white_ys, lc_black_ys),
        p_thr: if y < PAIR_CROSSOVER_Y {
            Label::White
        } else {
            Label::Black
        },
        p_prod: side_of(pair_side(enc)),
    }
}

fn rows() -> Vec<Row> {
    ANCHORS.iter().map(|&h| measure(h)).collect()
}

/// Печать таблицы главы (§4) + сводки флипов. Это ИСТОЧНИК всех чисел главы.
#[test]
fn table_level3_polarity_over_49_anchors() {
    assert_eq!(ANCHORS.len(), 49, "паспорт обязан нести ровно 49 якорей");

    let rows = rows();
    let mut prod_vs_thr = Vec::new();
    let mut hk_vs_ys = Vec::new();

    eprintln!(
        "\n{:<9} {:>7} | {:>8} {:>8} {:>7} | {:>8} {:>8} {:>7} | {:>7} {:>7} | {:>5}",
        "anchor",
        "Y",
        "|Lc_w|hk",
        "|Lc_b|hk",
        "P_hk",
        "|Lc_w|ys",
        "|Lc_b|ys",
        "P_ys",
        "P_thr",
        "P_prod",
        "promo"
    );
    eprintln!("{}", "-".repeat(100));
    for r in &rows {
        let promo = if r.p_prod != r.p_thr { "L→I" } else { "" };
        eprintln!(
            "{:<9} {:>7.4} | {:>8.2} {:>8.2} {:>7} | {:>8.2} {:>8.2} {:>7} | {:>7} {:>7} | {:>5}",
            r.hex,
            r.y,
            r.lc_white_hk.abs(),
            r.lc_black_hk.abs(),
            r.p_hk.tag(),
            r.lc_white_ys.abs(),
            r.lc_black_ys.abs(),
            r.p_ys.tag(),
            r.p_thr.tag(),
            r.p_prod.tag(),
            promo
        );
        if r.p_prod != r.p_thr {
            prod_vs_thr.push(r.hex);
        }
        if r.p_hk != r.p_ys {
            hk_vs_ys.push(r.hex);
        }
    }
    eprintln!("{}", "-".repeat(100));
    eprintln!(
        "H-K-ПРОМОЦИИ продакшн level-3 vs чистый порог ({}): {:?}",
        prod_vs_thr.len(),
        prod_vs_thr
    );
    eprintln!(
        "ФЛИПЫ H-K-домен vs Ys-домен ({}) [диагностика ADR 15:0]: {:?}",
        hk_vs_ys.len(),
        hk_vs_ys
    );
    eprintln!(
        "Сводка: 49 якорей; продакшн чернильных {}, светлых {}",
        rows.iter().filter(|r| r.p_prod == Label::Black).count(),
        rows.iter().filter(|r| r.p_prod == Label::White).count()
    );
}

/// ЛОК АХРОМАТИЧЕСКОГО ТЕРМИНАЛА + H-K-ПРОМОЦИЙ: продакшн level-3 `pair::pair_side`
/// совпадает с чисто-люминансным порогом `Y < PAIR_CROSSOVER_Y` НА ВСЕХ 49 якорях,
/// КРОМЕ поимённого набора H-K-промоций, и каждая промоция — строго Light→Ink
/// (насыщенный фон выглядит светлее → раньше несёт чернила). Это доказывает: (1)
/// литерал `0.341955` = ахроматический терминал продакшн-правила; (2) level-3 H-K
/// только ДОБАВЛЯЕТ чернила поверх порога, поимённо и однонаправленно. Дрейф
/// константы ядра или направления H-K расщепил бы этот набор.
#[test]
fn production_diverges_from_pure_threshold_only_by_named_hk_promotions() {
    let mut promotions = Vec::new();
    for &hex in &ANCHORS {
        let enc = srgb_encoded_from_hex(hex).unwrap();
        let threshold = if wcag_y(hex) < PAIR_CROSSOVER_Y {
            PairSide::Light
        } else {
            PairSide::Ink
        };
        let prod = pair_side(enc);
        if prod != threshold {
            assert_eq!(
                (threshold, prod),
                (PairSide::Light, PairSide::Ink),
                "{hex}: расхождение обязано быть промоцией Light→Ink (Y={:.4}), \
                 получено порог={threshold:?} продакшн={prod:?}",
                wcag_y(hex)
            );
            promotions.push(hex);
        }
    }
    assert_eq!(
        promotions,
        vec!["#409CFF", "#4A8FFF", "#5696FF", "#FF6161", "#FF6482"],
        "поимённый набор H-K-промоций на 49-якорном паспорте"
    );
}

/// СВОЙСТВО НАПРАВЛЕНИЯ (ядро вывода ADR-0003, воспроизведено на паспорте): всюду,
/// где H-K-домен расходится с `Ys`-доменом, H-K уходит в ЧЁРНЫЙ там, где `Ys`
/// держит БЕЛЫЙ — H-K монотонно ТОПИТ белый на насыщенных фонах, обратного флипа
/// (`Ys`→чёрный, H-K→белый) нет НИ НА ОДНОМ якоре. Та же 15:0-направленность, что
/// измерил синтетический свип V3, но на реальном паспорте.
#[test]
fn hk_flips_are_monotone_white_to_black() {
    let mut flips = 0;
    for r in rows() {
        if r.p_hk != r.p_ys {
            flips += 1;
            assert_eq!(
                (r.p_ys, r.p_hk),
                (Label::White, Label::Black),
                "{}: флип обязан быть Ys=белый→H-K=чёрный, получено Ys={:?} H-K={:?}",
                r.hex,
                r.p_ys,
                r.p_hk
            );
        }
    }
    assert!(
        flips > 0,
        "на паспорте обязан быть хотя бы один H-K-флип, иначе валидация ничего не показывает"
    );
}

/// СВОЙСТВО НАПРАВЛЕНИЯ продакшна vs чисто-люминансный терминал: продакшн level-3
/// расходится с порогом `PAIR_CROSSOVER_Y` только В СТОРОНУ чернил (Light→Ink),
/// никогда обратно. Пиннит, что H-K-лифт только ДОБАВЛЯЕТ чернильные решения
/// относительно сырого перелома — дубликат-страж набора (см. поимённый лок выше).
#[test]
fn production_only_adds_ink_relative_to_pure_threshold() {
    for r in rows() {
        if r.p_prod != r.p_thr {
            assert_eq!(
                (r.p_thr, r.p_prod),
                (Label::White, Label::Black),
                "{}: продакшн обязан только ДОБАВЛЯТЬ чернила (порог={:?} продакшн={:?})",
                r.hex,
                r.p_thr,
                r.p_prod
            );
        }
    }
}

/// ЭНДПОИНТ: белый фон `#FFFFFF` (`Y_hk = Y = 1`) — чёрный лейбл во ВСЕХ трёх
/// доменах, и |Lc| канонический `≈106.04` (H-K не двигает люминансный эндпоинт;
/// домены `Y_hk` и `Ys` совпадают там побайтно). Якорь ratio-пола цел.
#[test]
fn white_anchor_is_black_label_at_canonical_contrast() {
    let r = measure("#FFFFFF");
    assert_eq!(r.p_hk, Label::Black);
    assert_eq!(r.p_ys, Label::Black);
    assert_eq!(r.p_thr, Label::Black);
    assert_eq!(r.p_prod, Label::Black);
    assert!(
        (r.lc_black_hk.abs() - 106.04).abs() < 0.5,
        "чёрный на белом обязан быть ≈106.04, получено {}",
        r.lc_black_hk.abs()
    );
    // Эндпоинты доменов совпадают: Y_hk(0/1) == Ys(0/1).
    assert!(
        (r.lc_black_hk.abs() - r.lc_black_ys.abs()).abs() < 1e-9,
        "на люминансном эндпоинте домены Y_hk и Ys обязаны совпасть: hk={} ys={}",
        r.lc_black_hk.abs(),
        r.lc_black_ys.abs()
    );
}
