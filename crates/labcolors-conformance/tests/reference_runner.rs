//! Раннер-референс conformance-пака: **ядро само себя проходит**, и это CI-гейт.
//!
//! Четыре независимых слоя, каждый ловит свой класс дрейфа:
//!
//! 1. **Гейт дрейфа (толерантный)** — ядро воспроизводит КАЖДЫЙ закоммиченный
//!    вектор: числовые поля в пределах [`DRIFT_TOL`] (`1e-6`, SSOT пака —
//!    `labcolors-conformance/src/lib.rs`), hex/строки/enum/bool — точно. Байт-точность
//!    f64 кросс-платформенно НЕВОЗМОЖНА: `powf`/`atan2`/`ln` расходятся на
//!    несколько ULP между libm разных ОС (векторы генерятся на одной, CI бежит
//!    на другой). Реальный дрейф (не тот surround, опечатка в матрице) сдвигает
//!    значения на целые единицы — на десять порядков выше толерантности.
//!    Та же толерантность гасит и неточность парсера f64 у `serde_json`.
//!
//! 2. **Метаданные манифеста** — версии и счётчики точны (в манифесте нет f64).
//!
//! 3. **Согласованность дайджеста** — `manifest.packDigest` равен FNV-1a-32 над
//!    СЫРЫМИ байтами закоммиченных семейств (порядок [`FAMILY_FILES`]). Без
//!    регенерации — чистая самосогласованность закоммиченного артефакта.
//!
//! 4. **Внешние якоря WCAG** — опубликованные значения (чёрное/белое = 21:1,
//!    граница AA-текста `#767676` ≈ 4.54:1, шаг ниже `#777777` < 4.5:1)
//!    сверяются с закоммиченными контраст-векторами. Замыкает цепочку на правду
//!    ВНЕ репозитория.
//!
//! Вместе: закоммичено == ядро (в пределах толерантности) == опубликованный
//! стандарт, а манифест честно описывает лежащие рядом байты.

use std::path::PathBuf;

use labcolors_conformance::{
    AlphaVector, ContrastVector, DRIFT_TOL, FAMILY_FILES, LadderVector, MANIFEST_FILE, Manifest,
    MuddinessVector, Pack, SolveOutcome, SolveVector, generate_alpha, generate_contrasts,
    generate_ladders, generate_muddiness, generate_solve,
};
use labcolors_core::fnv1a_32;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("conformance")
        .join("vectors")
}

/// Прочитать закоммиченный файл как строку с LF-переводами (нормализуем CRLF на
/// случай чекаута с `core.autocrlf`, хотя `.gitattributes` пинит к LF).
fn read_lf(name: &str) -> String {
    let path = vectors_dir().join(name);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "не прочитать {} — сгенерируй пак `cargo run -p labcolors-conformance --bin gen`: {e}",
            path.display()
        )
    });
    raw.replace("\r\n", "\n")
}

fn parse<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_str(&read_lf(name))
        .unwrap_or_else(|e| panic!("{name} не парсится как ожидаемый тип: {e}"))
}

/// f64 в пределах кросс-платформенной толерантности.
#[track_caller]
fn approx(a: f64, b: f64, ctx: &str) {
    assert!(
        (a - b).abs() <= DRIFT_TOL,
        "{ctx}: |{a} − {b}| = {} > DRIFT_TOL ({DRIFT_TOL})",
        (a - b).abs()
    );
}

// ── Слой 1: толерантный гейт дрейфа, по семействам ───────────────────────────

#[test]
fn core_reproduces_committed_contrasts() {
    let committed: Vec<ContrastVector> = parse("contrasts.json");
    let fresh = generate_contrasts();
    assert_eq!(
        committed.len(),
        fresh.len(),
        "contrasts.json: изменился состав — перегенерируй пак"
    );
    for (c, f) in committed.iter().zip(&fresh) {
        assert_eq!(
            (&c.fg, &c.bg, &c.theme),
            (&f.fg, &f.bg, &f.theme),
            "ключ вектора"
        );
        approx(c.lc, f.lc, &format!("contrasts lc {}/{}", c.fg, c.bg));
        approx(
            c.wcag_ratio,
            f.wcag_ratio,
            &format!("contrasts wcag {}/{}", c.fg, c.bg),
        );
    }
}

#[test]
fn core_reproduces_committed_ladders() {
    let committed: Vec<LadderVector> = parse("ladders.json");
    let fresh = generate_ladders();
    assert_eq!(
        committed.len(),
        fresh.len(),
        "ladders.json: изменился состав"
    );
    for (c, f) in committed.iter().zip(&fresh) {
        assert_eq!(c.position, f.position, "ключ позиции");
        approx(
            c.alpha_light,
            f.alpha_light,
            &format!("ladder α_light {}", c.position),
        );
        approx(
            c.alpha_dark,
            f.alpha_dark,
            &format!("ladder α_dark {}", c.position),
        );
    }
}

#[test]
fn core_reproduces_committed_alpha() {
    let committed: Vec<AlphaVector> = parse("alpha.json");
    let fresh = generate_alpha();
    assert_eq!(committed.len(), fresh.len(), "alpha.json: изменился состав");
    for (c, f) in committed.iter().zip(&fresh) {
        assert_eq!((&c.tint, &c.bg), (&f.tint, &f.bg), "ключ альфа-вектора");
        approx(c.alpha, f.alpha, "alpha α");
        // Композит — чистая алгебра кодированного sRGB + квантование: бит-точен
        // на всех IEEE-платформах.
        assert_eq!(c.composite, f.composite, "композит {}@{}", c.tint, c.alpha);
        approx(
            c.min_alpha,
            f.min_alpha,
            &format!("min_alpha {}/{}", c.tint, c.bg),
        );
    }
}

#[test]
fn core_reproduces_committed_solve() {
    let committed: Vec<SolveVector> = parse("solve.json");
    let fresh = generate_solve();
    assert_eq!(committed.len(), fresh.len(), "solve.json: изменился состав");
    for (c, f) in committed.iter().zip(&fresh) {
        assert_eq!(
            (&c.bg, c.contract, &c.theme),
            (&f.bg, f.contract, &f.theme),
            "ключ резолв-вектора"
        );
        match (&c.outcome, &f.outcome) {
            (
                SolveOutcome::Solved {
                    hex: ch,
                    lc: cl,
                    wcag_ratio: cw,
                    floor_override: cf,
                },
                SolveOutcome::Solved {
                    hex: fh,
                    lc: fl,
                    wcag_ratio: fw,
                    floor_override: ff,
                },
            ) => {
                assert_eq!(ch, fh, "solve hex на {}", c.bg);
                assert_eq!(cf, ff, "floor_override на {}", c.bg);
                approx(*cl, *fl, &format!("solve lc на {}", c.bg));
                approx(*cw, *fw, &format!("solve wcag на {}", c.bg));
            }
            (SolveOutcome::Unreachable { code: cc }, SolveOutcome::Unreachable { code: fc }) => {
                assert_eq!(cc, fc, "код недостижимости на {}", c.bg)
            }
            (a, b) => panic!("исход резолва разошёлся на {}: {a:?} vs {b:?}", c.bg),
        }
    }
}

#[test]
fn core_reproduces_committed_muddiness() {
    let committed: Vec<MuddinessVector> = parse("muddiness.json");
    let fresh = generate_muddiness();
    assert_eq!(
        committed.len(),
        fresh.len(),
        "muddiness.json: изменился состав"
    );
    for (c, f) in committed.iter().zip(&fresh) {
        assert_eq!(c.hex, f.hex, "ключ мутности");
        approx(c.score, f.score, &format!("muddiness {}", c.hex));
    }
}

// ── Слой 2: метаданные манифеста ──────────────────────────────────────────────

#[test]
fn manifest_metadata_matches_core() {
    let committed: Manifest = parse(MANIFEST_FILE);
    let fresh = Pack::generate().manifest();
    assert_eq!(committed.pack_version, fresh.pack_version, "версия пака");
    assert_eq!(committed.core_version, fresh.core_version, "версия ядра");
    assert_eq!(committed.counts, fresh.counts, "счётчики семейств");
    assert_eq!(
        committed.numerical_sites, fresh.numerical_sites,
        "numerical registry обязан быть exact copy core SSOT"
    );
}

// ── Слой 3: согласованность дайджеста над сырыми байтами ──────────────────────

#[test]
fn manifest_digest_matches_raw_committed_bytes() {
    // Дайджест пересчитывается из СЫРЫХ байтов закоммиченных семейств (порядок
    // FAMILY_FILES) — никакой регенерации и никакого parse f64. Ловит правку
    // семейства без обновления манифеста.
    let mut concat = String::new();
    for name in FAMILY_FILES {
        concat.push_str(&read_lf(name));
    }
    let digest = format!("{:08x}", fnv1a_32(concat.as_bytes()));
    let manifest: Manifest = parse(MANIFEST_FILE);
    assert_eq!(
        digest, manifest.pack_digest,
        "packDigest не сходится с сырыми байтами семейств — файлы рассогласованы"
    );
}

// ── Слой 4: внешние опубликованные якоря WCAG ────────────────────────────────

fn light_contrast(fg: &str, bg: &str) -> ContrastVector {
    let vs: Vec<ContrastVector> = parse("contrasts.json");
    vs.into_iter()
        .find(|v| v.fg == fg && v.bg == bg && v.theme == "light")
        .unwrap_or_else(|| panic!("в паке нет вектора {fg} на {bg} (light)"))
}

#[test]
fn published_wcag_anchors_hold_in_pack() {
    let bw = light_contrast("#000000", "#FFFFFF");
    assert!(
        (bw.wcag_ratio - 21.0).abs() < 1e-6,
        "чёрное/белое должно быть 21:1, в паке {}",
        bw.wcag_ratio
    );

    let boundary = light_contrast("#767676", "#FFFFFF");
    assert!(
        (boundary.wcag_ratio - 4.5422).abs() < 1e-3,
        "#767676/белое должно быть ≈4.5422:1, в паке {}",
        boundary.wcag_ratio
    );
    assert!(
        boundary.wcag_ratio >= 4.5,
        "#767676 обязан пройти AA-текст (4.5:1), в паке {}",
        boundary.wcag_ratio
    );

    let below = light_contrast("#777777", "#FFFFFF");
    assert!(
        below.wcag_ratio < 4.5,
        "#777777/белое должно быть < 4.5:1, в паке {}",
        below.wcag_ratio
    );
    assert!(
        (below.wcag_ratio - 4.4781).abs() < 1e-3,
        "#777777 опубликовано ≈4.4781, в паке {}",
        below.wcag_ratio
    );
}

// ── Санитарная проверка формы ────────────────────────────────────────────────

#[test]
fn committed_files_are_lf_only() {
    for name in FAMILY_FILES.iter().chain(std::iter::once(&MANIFEST_FILE)) {
        let raw = std::fs::read(vectors_dir().join(name)).unwrap();
        assert!(
            !raw.contains(&b'\r'),
            "{name} содержит CR — должен быть LF-only"
        );
    }
}
