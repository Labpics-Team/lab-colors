//! Воспроизводимая деривация M-12 `H_Y_DEG` — Oklab hue уникального жёлтого.
//!
//! Запуск: `cargo run -p labcolors-core --example unique_yellow_provenance`
//!
//! Метод (Zone B slice 5, 2026-07-03):
//!   1. λ = 578nm — инвариантная точка жёлтого Бецольда-Брюкке
//!      (Purdy 1937, Am. J. Psychol. 49, 313–315; Jacobs & Wascher 1967,
//!      J. Opt. Soc. Am. 57, 1155–1156).
//!   2. CMF при 578nm — линейная интерполяция официальной CIE 1931 2° 5нм-таблицы.
//!   3. Нормировка к Y=1 (hue инвариантен к радиансности: масштабирование LMS′
//!      множит a и b одинаково — отношение в atan2 не меняется; проверяется ниже).
//!   4. XYZ → Oklab НАПРЯМУЮ через M1/M2 (Ottosson 2020) — БЕЗ проекции в sRGB:
//!      покомпонентный гамут-клэмп не сохраняет оттенок (демонстрируется ниже).
//!   5. h = atan2(b, a).
//!
//! Печатает также golden-значения muddiness для characterization-тестов.

// CIE 1931 2° standard observer, 5nm table (x̄, ȳ, z̄)
const CMF_575: [f64; 3] = [0.8425, 0.9154, 0.0018];
const CMF_580: [f64; 3] = [0.9163, 0.8700, 0.00165];

/// XYZ (D65) → LMS, Ottosson 2020 M1. Не входит в продуктивное ядро
/// (ядро работает от sRGB), поэтому определена здесь, в скрипте провенанса.
#[rustfmt::skip]
const M1_XYZ_TO_LMS: [[f64; 3]; 3] = [
    [0.8189330101, 0.3618667424, -0.1288597137],
    [0.0329845436, 0.9293118715,  0.0361456387],
    [0.0482003018, 0.2643662691,  0.6338517070],
];

/// LMS′ → Oklab, Ottosson 2020 M2 (та же матрица, что в spaces/oklab.rs).
#[rustfmt::skip]
const M2_LMS_TO_OKLAB: [[f64; 3]; 3] = [
    [0.2104542553,  0.7936177850, -0.0040720468],
    [1.9779984951, -2.4285922050,  0.4505937099],
    [0.0259040371,  0.7827717662, -0.8086757660],
];

fn mat_vec(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn oklab_from_xyz(xyz: [f64; 3]) -> [f64; 3] {
    let lms = mat_vec(M1_XYZ_TO_LMS, xyz);
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    mat_vec(M2_LMS_TO_OKLAB, lms_)
}

fn hue_deg(lab: [f64; 3]) -> f64 {
    lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
}

fn main() {
    // Шаг 2: CMF при 578nm (t = 0.6 между 575 и 580)
    let t = 0.6;
    let cmf: [f64; 3] = [
        CMF_575[0] + t * (CMF_580[0] - CMF_575[0]),
        CMF_575[1] + t * (CMF_580[1] - CMF_575[1]),
        CMF_575[2] + t * (CMF_580[2] - CMF_575[2]),
    ];
    println!("CMF@578nm: x̄={:.6} ȳ={:.6} z̄={:.6}", cmf[0], cmf[1], cmf[2]);

    // Шаг 3: нормировка к Y=1
    let xyz = [cmf[0] / cmf[1], 1.0, cmf[2] / cmf[1]];
    println!("XYZ/Y = ({:.6}, 1.000000, {:.6})", xyz[0], xyz[2]);

    // Шаг 4-5: XYZ → Oklab напрямую, hue
    let lab = oklab_from_xyz(xyz);
    let h = hue_deg(lab);
    println!("Oklab a={:+.10} b={:+.10}", lab[1], lab[2]);
    println!(
        "H_Y = {h:.4}°  (константа M-12: {})",
        labcolors_core::cleanliness::H_Y_DEG
    );

    // Инвариантность hue к радиансности: тот же стимул на 20% радиансности
    let lab_dim = oklab_from_xyz([xyz[0] * 0.2, 0.2, xyz[2] * 0.2]);
    println!(
        "hue-инвариантность к радиансности: {:.6}° vs {:.6}°",
        h,
        hue_deg(lab_dim)
    );

    // Демонстрация НЕ-hue-сохранности sRGB-клэмпа (причина отзыва slice-4 значения):
    // спектральный стимул лежит вне гамута sRGB; клэмп сдвигает оттенок.
    // (slice 4: ложный XYZ=(1.207,1,0) давал 69.7° без клэмпа и 96.9° после.)
    let lab_wrong = oklab_from_xyz([1.207, 1.0, 0.0]);
    println!(
        "slice-4 XYZ=(1.207,1,0) без клэмпа: {:.4}° (после sRGB-клэмпа было 96.9°)",
        hue_deg(lab_wrong)
    );

    // Golden-значения muddiness для characterization-тестов (Fowler class B)
    println!("\n— mud goldens (muddiness_from_hex) —");
    for (label, hex) in [
        ("olive", "#6B6B2E"),
        ("babypoop", "#937C00"),
        ("puke", "#9AAE07"),
        ("gold1", "#9e6c00"),
        ("gold2", "#8f6424"),
        ("grey", "#808080"),
        ("teal", "#008080"),
        ("navy", "#000080"),
        ("red", "#FF0000"),
        ("blue", "#0000FF"),
    ] {
        let mud = labcolors_core::cleanliness::muddiness_from_hex(hex).unwrap();
        println!("{label:9} {hex}  mud={mud:.8}");
    }
}
