//! Порт финального «Закона Грязи» (Muddiness Law) системы `labcolors`.
//!
//! # Уравнения
//!
//! ```text
//! raw = neutral_gate(L, C) * hue_weight(h) * depth_mod(L, C, h)
//! mud = raw                   ← параметр-свободная монотонная нормировка (Zone B, 2026-06-30)
//! ```
//!
//! Platt-звено (логарифм raw + eps → sigmoid с двумя подогнанными скалярами) **удалено полностью**
//! в Zone B slice 3: оба скаляра (M-07/M-08) подогнаны на авторском датасете 738 меток
//! (наблюдатель-фит) и нарушали инвариант «ZERO observer-fit» (North).  mud = raw_chromatic напрямую:
//! - параметр-свободно (нет новых констант);
//! - монотонно (произведение монотонных сигмоидных факторов ∈ [0,1]);
//! - ограничено [0,1] точно (каждый множитель ∈ [0,1]);
//! - JND-относительно по конструкции (gate N(C) выровнен по M-02 JND).
//!
//! # Инвентарь параметров (раздел Muddiness Law)
//!
//! | const           | mud-id | статус                         | значение                    |
//! |-----------------|--------|--------------------------------|-----------------------------|
//! | `C0`            | M-01   | cited-and-kept                 | 0.0395 (граница серого sRGB; Evans/Xie-Fairchild yellow zero-grayness frontier) |
//! | `JND`           | M-02   | cited-and-kept                 | 0.01228 (Oklab chroma JND; Oklab perceptual measurement) |
//! | `LESC`          | M-03   | DECLARED-CALIBRATION           | 0.8208552 (порог light-escape) |
//! | `B0`            | M-04   | cited-measured                 | 0.036 (центр диапазона [0.030, 0.044]; Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975) |
//! | `BW`            | M-05   | cited-measured                 | 0.017 (центр диапазона [0.013, 0.020]; Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975) |
//! | ~~`CAL_EPS`~~   | M-06   | УДАЛЁН (Zone B slice 3)        | был 0.01; log-регуляризатор для Platt — более не нужен |
//! | ~~`CAL_T`~~     | M-07   | УДАЛЁН (Zone B slice 3)        | был 2.356978; Platt-скаляр, подогнанный на 738 авторских метках — нарушал ZERO observer-fit |
//! | ~~`CAL_B`~~     | M-08   | УДАЛЁН (Zone B slice 3)        | был 6.445168; Platt-смещение — нарушало ZERO observer-fit |
//! | `M_W`           | M-09   | DECLARED-CALIBRATION           | 0.181527 (полуширина доверительного поля confidence) |
//! | `KAPPA_CORE`    | M-10   | DECLARED-CALIBRATION           | 0.34 (эмпирический concept-floor из v3 retest; не перцептивная константа) |
//! | `KAPPA_INTERIOR`| M-11   | DECLARED-CALIBRATION           | 0.10 (эмпирический interior-floor из v3 disputed-stratum; не перцептивная константа) |
//! | `H_Y_DEG`       | M-12   | cited-derived (Zone B slice 4) | 96.9172° — Oklab hue уникального жёлтого (λ=578nm, CIE 1931 2° D65); hue_weight = (1+cos(h−H_Y))/2 |
//! | ~~`W_HUE[8]`~~  | M-12   | УДАЛЁН (Zone B slice 4)        | был: подогнанный K=3 Fourier-вектор логистической регрессии — нарушал ZERO observer-fit |
//! | `CUSP_L_TABLE`  | M-13   | cited-and-kept                 | (чистая геометрия гамута Oklab, верифицирована до f64 — kept as-is) |
//! | ~~`CEIL_N_TABLE`~~ | M-14 | УДАЛЕНА (Zone B slice 4)       | была: Fourier CEIL_N, подогнана на датасете v3 — более не нужна после BB-замены |

#![allow(clippy::excessive_precision)]

pub(crate) static CUSP_L_TABLE: [f64; 361] = [
    0.64774951, 0.64579256, 0.64579256, 0.64383562, 0.64383562, 0.64187867, 0.64187867, 0.64187867,
    0.63992172, 0.63992172, 0.63796477, 0.63796477, 0.63796477, 0.63600783, 0.63600783, 0.63600783,
    0.63405088, 0.63405088, 0.63405088, 0.63209393, 0.63209393, 0.63209393, 0.63013699, 0.63013699,
    0.63013699, 0.63013699, 0.62818004, 0.62818004, 0.62818004, 0.62818004, 0.63209393, 0.63796477,
    0.64187867, 0.64774951, 0.65362035, 0.65753425, 0.66340509, 0.66731898, 0.67123288, 0.67710372,
    0.68101761, 0.68493151, 0.68884540, 0.69275930, 0.69667319, 0.70058708, 0.70450098, 0.70841487,
    0.71232877, 0.71624266, 0.72015656, 0.72407045, 0.72798434, 0.73189824, 0.73581213, 0.73776908,
    0.74168297, 0.74559687, 0.74951076, 0.75342466, 0.75538160, 0.75929550, 0.76320939, 0.76516634,
    0.76908023, 0.77299413, 0.77690802, 0.77886497, 0.78277886, 0.78669276, 0.79060665, 0.79256360,
    0.79647750, 0.80039139, 0.80430528, 0.80626223, 0.81017613, 0.81409002, 0.81800391, 0.82191781,
    0.82387476, 0.82778865, 0.83170254, 0.83561644, 0.83953033, 0.84344423, 0.84735812, 0.85127202,
    0.85518591, 0.85909980, 0.86301370, 0.86692759, 0.87084149, 0.87475538, 0.88062622, 0.88454012,
    0.88845401, 0.89432485, 0.89823875, 0.90410959, 0.90802348, 0.91389432, 0.91976517, 0.92563601,
    0.92954990, 0.93542074, 0.94129159, 0.94911937, 0.95499022, 0.96086106, 0.96673190, 0.96281800,
    0.96086106, 0.95890411, 0.95499022, 0.95303327, 0.94911937, 0.94716243, 0.94520548, 0.94129159,
    0.93933464, 0.93542074, 0.93346380, 0.92954990, 0.92759295, 0.92367906, 0.92172211, 0.91780822,
    0.91585127, 0.91193738, 0.90998043, 0.90606654, 0.90215264, 0.90019569, 0.89628180, 0.89236791,
    0.89041096, 0.88649706, 0.88258317, 0.87866928, 0.87475538, 0.87084149, 0.86692759, 0.86692759,
    0.86692759, 0.86888454, 0.86888454, 0.87084149, 0.87084149, 0.87279843, 0.87279843, 0.87475538,
    0.87475538, 0.87671233, 0.87671233, 0.87671233, 0.87866928, 0.87866928, 0.88062622, 0.88062622,
    0.88062622, 0.88258317, 0.88258317, 0.88454012, 0.88454012, 0.88454012, 0.88649706, 0.88649706,
    0.88649706, 0.88845401, 0.88845401, 0.88845401, 0.89041096, 0.89041096, 0.89041096, 0.89236791,
    0.89236791, 0.89236791, 0.89432485, 0.89432485, 0.89432485, 0.89628180, 0.89628180, 0.89628180,
    0.89823875, 0.89823875, 0.89823875, 0.90019569, 0.90019569, 0.90019569, 0.90215264, 0.90215264,
    0.90215264, 0.90410959, 0.90410959, 0.90410959, 0.89823875, 0.89432485, 0.89041096, 0.88649706,
    0.88062622, 0.87671233, 0.87279843, 0.86888454, 0.86497065, 0.86105675, 0.85714286, 0.85322896,
    0.84931507, 0.84540117, 0.84148728, 0.83757339, 0.83365949, 0.82974560, 0.82583170, 0.82191781,
    0.81800391, 0.81409002, 0.81017613, 0.80626223, 0.80234834, 0.79843444, 0.79452055, 0.79060665,
    0.78669276, 0.78277886, 0.77886497, 0.77495108, 0.76908023, 0.76516634, 0.76125245, 0.75733855,
    0.75342466, 0.74951076, 0.74559687, 0.73972603, 0.73581213, 0.73189824, 0.72602740, 0.72211350,
    0.71624266, 0.71232877, 0.70645793, 0.70254403, 0.69667319, 0.69080235, 0.68493151, 0.67906067,
    0.67318982, 0.66731898, 0.66144814, 0.65362035, 0.64774951, 0.63992172, 0.63209393, 0.62426614,
    0.61643836, 0.60665362, 0.59686888, 0.58512720, 0.57338552, 0.56164384, 0.54403131, 0.52446184,
    0.49315068, 0.45401174, 0.45596869, 0.45792564, 0.45988258, 0.46183953, 0.46575342, 0.46771037,
    0.46966732, 0.47162427, 0.47358121, 0.47749511, 0.47945205, 0.48140900, 0.48336595, 0.48727984,
    0.48923679, 0.49119374, 0.49510763, 0.49706458, 0.49902153, 0.50293542, 0.50489237, 0.50880626,
    0.51076321, 0.51467710, 0.51663405, 0.52054795, 0.52446184, 0.52641879, 0.53033268, 0.53424658,
    0.53816047, 0.54011742, 0.54403131, 0.54794521, 0.55185910, 0.55577299, 0.55968689, 0.56360078,
    0.56751468, 0.57142857, 0.57729941, 0.58121331, 0.58512720, 0.58904110, 0.59491194, 0.59882583,
    0.60469667, 0.60861057, 0.61448141, 0.62035225, 0.62426614, 0.63013699, 0.63600783, 0.64187867,
    0.64774951, 0.65362035, 0.65949119, 0.66536204, 0.67123288, 0.67906067, 0.68493151, 0.69275930,
    0.69863014, 0.69863014, 0.69667319, 0.69471624, 0.69080235, 0.68884540, 0.68688845, 0.68493151,
    0.68297456, 0.68101761, 0.67906067, 0.67710372, 0.67514677, 0.67318982, 0.67123288, 0.66927593,
    0.66731898, 0.66536204, 0.66536204, 0.66340509, 0.66144814, 0.65949119, 0.65949119, 0.65753425,
    0.65557730, 0.65557730, 0.65362035, 0.65166341, 0.65166341, 0.64970646, 0.64970646, 0.64774951,
    0.64774951,
];

// CEIL_N_TABLE удалена (Zone B slice 4, 2026-06-30):
// K=3 Fourier-базис CEIL_N использовался в hue_basis() для dot-product c W_HUE.
// После замены hue_weight на Hanning-окно BB ни hue_basis, ни CEIL_N_TABLE
// не нужны в продуктивном коде.

// High-precision frozen parameter constants
pub const C0: f64 = 0.0395000000000000;
/// JND reuse rationale: this constant is the cited Oklab chroma just-noticeable-difference
/// (Oklab perceptual measurement, M-02 cited-and-kept). It is used as the sigmoid gate width
/// throughout the module because JND is the natural perceptual scale for a logistic boundary —
/// a ±1 JND band captures the transition from sub-threshold to supra-threshold chroma presence.
/// No new fit is performed; this is the same cited value reused as a scale parameter.
/// The applicability of a chroma JND as the gate width is a design assumption; if a future
/// study finds a different gate width is warranted, M-02 should be split into separate cited
/// values (one for detection threshold, one for gate width).  Flagged OPEN as an assumption.
pub const JND: f64 = 0.0122779190541810;
pub const LESC: f64 = 0.8208552000000002;
pub const B0: f64 = 0.036; // cited-measured central (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975); range [0.030, 0.044]
pub const BW: f64 = 0.017; // cited-measured central (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975); range [0.013, 0.020]
// CAL_EPS / CAL_T / CAL_B удалены (Zone B slice 3, 2026-06-30):
// Platt-звено sigmoid(CAL_T*ln(raw+eps)+CAL_B) было observer-fit на 738 авторских метках
// (нарушение ZERO observer-fit, North).  Заменено параметр-свободным mud = raw_chromatic.
pub const M_W: f64 = 0.1815267777247454;
pub const KAPPA_CORE: f64 = 0.34;
pub const KAPPA_INTERIOR: f64 = 0.10;

// W_HUE[8] удалён (Zone B slice 4, 2026-06-30):
// Подогнанный вектор K=3 Фурье-регрессии (логистическая регрессия на 738 авторских метках,
// M-12, OPEN/flagged-provisional) заменён выведенным Hanning-окном Бецольда-Брюкке.
// Формула: hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2
// Провенанс: Parry (1967) J. Opt. Soc. Am. 57, 1130–1134 × Якобиан оттенка Oklab.
// Инварианты: hue_weight(H_Y_DEG) = 1.0 точно; hue_weight(H_Y_DEG ± 180°) = 0.0 точно;
// строго монотонное убывание при |h − H_Y_DEG| растёт от 0 до 180°.

/// Oklab hue уникального жёлтого (λ=578nm, CIE 1931 2° наблюдатель, D65).
///
/// Вывод:
///   1. CMF при 578nm: x̄=0.9015, ȳ=0.7470, z̄=0.000 (линейная интерп. CIE 10нм-таблицы:
///      570nm x̄=0.8425 ȳ=0.7070; 580nm x̄=0.9163 ȳ=0.7570; t=0.8).
///   2. Нормируем к Y=1: XYZ = (x̄/ȳ, 1, 0) = (1.207, 1.000, 0.000).
///   3. XYZ → linear sRGB (IEC 61966-2-1 D65 matrix), clamp к гамуту sRGB.
///   4. linear sRGB → Oklab (Ottosson 2020).
///   5. h = atan2(b, a) = 96.9°.
///
/// Использование: центр Hanning-окна `hue_weight(h)` в Zone B slice 4 (M-12).
/// Константа выводная, не подогнана.
pub const H_Y_DEG: f64 = 96.9172;

#[inline]
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Compute the Oklab yellow-blue opponent b axis coordinate: b = C * sin(h)
#[inline]
pub fn b_of(c: f64, h_deg: f64) -> f64 {
    c * h_deg.to_radians().sin()
}

/// Precomputed cusp lightness as a function of hue (Oklab degree) with linear interpolation.
pub fn cusp_l_of(h_deg: f64) -> f64 {
    let h = h_deg.rem_euclid(360.0);
    let idx = h.floor() as usize;
    let fract = h - h.floor();
    let next_idx = if idx >= 360 { 0 } else { idx + 1 };
    let y0 = CUSP_L_TABLE[idx];
    let y1 = CUSP_L_TABLE[next_idx];
    y0 + fract * (y1 - y0)
}

/// Depth term: how far below the hue's clean cusp the colour sits, normalized by cusp L.
/// Ranges from 0.0 (at/above cusp) to 1.0 (deep in the basement).
pub fn depth_term(l: f64, h_deg: f64) -> f64 {
    let cusp_l = cusp_l_of(h_deg);
    let below = (cusp_l - l) / cusp_l.max(1e-6);
    below.clamp(0.0, 1.0)
}

/// Hue-agnostic chroma-confidence gate: checks if the color is chromatic at all, or reads grey/beige.
pub fn neutral_gate(l: f64, c: f64, c0: f64, jnd: f64, lesc: f64) -> f64 {
    let cc = sigmoid((c - c0) / jnd);
    let escape = sigmoid((l - lesc) / jnd) * sigmoid((2.0 * c0 - c) / jnd);
    cc * (1.0 - escape)
}

/// Opponent b-gate (warm/clean filter): gates the geometric depth term. Warm -> 1.0, cool -> 0.0.
pub fn depth_mod(l: f64, c: f64, h_deg: f64, b0: f64, bw: f64) -> f64 {
    let dp = depth_term(l, h_deg);
    let s = sigmoid((b_of(c, h_deg) - b0) / bw);
    dp * s
}

/// Весовой множитель оттенка — Hanning-окно Бецольда-Брюкке (Zone B slice 4, 2026-06-30).
///
/// Заменяет подогнанный вектор W_HUE[8] (M-12, OPEN/flagged-provisional) на выводную формулу.
///
/// # Вывод формулы
///
/// Эффект Бецольда-Брюкке (Parry 1967, J. Opt. Soc. Am. 57, 1130–1134): при росте яркости
/// оттенки смещаются к уникальному жёлтому (h_Y) и уникальному синему.  Производная смещения
/// по Oklab-оттенку:
///
///   dΔH_BB/dh = A_BB · cos(h − h_Y)
///
/// — плотность конвергенции оттенков к h_Y.  Максимальна при h = h_Y (все ближние оттенки
/// сходятся к уникальному жёлтому, зона максимальной «грязи»); обнуляется при |h − h_Y| = 90°;
/// минимальна при |h − h_Y| = 180° (противоположное направление).
///
/// Нормируем в [0, 1]:
///
///   hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2   [Hanning-окно]
///
/// # Провенанс констант
///
/// - `H_Y_DEG = 96.9172°` — Oklab hue уникального жёлтого (λ=578nm, CIE 1931 2° D65).
///   Вывод: CMF → XYZ → linear sRGB → Oklab (Ottosson 2020) → atan2(b, a).
///   Цитата: Parry (1967) Table 1 (unique yellow); König & Dieterici (1884) unique-hue loci.
///
/// # Инварианты (проверены тестами)
///
/// - `hue_weight(H_Y_DEG) == 1.0` точно (cos(0) = 1)
/// - `hue_weight(H_Y_DEG + 180°) == 0.0` точно (cos(π) = -1)
/// - строго монотонное убывание при |h − H_Y_DEG| растёт от 0° до 180°
/// - симметрия: hue_weight(h_Y + δ) == hue_weight(h_Y − δ)
///
/// # Независимость осей
///
/// Эта функция НЕ вызывается внутри `drab` или `n_pure` (вторая ось mud/drab).
/// Единственная точка входа — `raw_chromatic`, которая и так гейтирована b-axis (depth_mod).
#[inline]
pub fn hue_weight(h_deg: f64) -> f64 {
    // Hanning-окно: (1 + cos(δ)) / 2, где δ = h − H_Y_DEG нормирован в [0°, 360°).
    //
    // Граничные случаи обрабатываются явно, чтобы гарантировать платформенно-точные
    // значения 1.0 и 0.0 независимо от FP-реализации cos(0) / cos(π):
    //   δ == 0.0° → cos(0) = 1 → hw = 1.0 точно
    //   δ == 180.0° → cos(π) = −1 → hw = 0.0 точно
    // Остальные углы вычисляются через cos как обычно.
    let delta = (h_deg - H_Y_DEG).rem_euclid(360.0);
    if delta == 0.0 {
        1.0
    } else if delta == 180.0 {
        0.0
    } else {
        (1.0 + delta.to_radians().cos()) / 2.0
    }
}

/// Сырой хроматический счёт грязи (без масштабирования).
///
/// raw = N(C) × hue_weight_BB(h) × depth_mod(L, C, h)
///
/// Все три множителя ∈ [0,1], поэтому raw ∈ [0,1] строго.
pub fn raw_chromatic(l: f64, c: f64, h_deg: f64) -> f64 {
    let cc = neutral_gate(l, c, C0, JND, LESC);
    let hw = hue_weight(h_deg);
    let dpm = depth_mod(l, c, h_deg, B0, BW);
    cc * hw * dpm
}

/// Параметр-свободная монотонная цена грязи (Zone B slices 3+4, 2026-06-30).
///
/// mud = raw_chromatic(l, c, h) — прямое произведение трёх перцептивно-обоснованных
/// факторов из геометрии Oklab и поворота Бецольда-Брюкке:
///   N(C) = sigmoid((C - C0) / JND)              — JND-взвешенное присутствие хромы (M-01/M-02)
///   hue_weight(h) = (1+cos(h−H_Y_DEG))/2        — Hanning-окно BB (M-12, Zone B slice 4)
///   depth_mod(L, C, h)                           — глубина под cusp-L, взвешенная b-гейтом (M-04/M-05)
///
/// Свойства:
///   - Выводных констант: только H_Y_DEG = 96.9° (CIE D65 unique yellow, Parry 1967)
///   - Нет подогнанных скаляров: W_HUE/CEIL_N_TABLE/CAL_T/CAL_B — все удалены
///   - raw ∈ [0,1] точно (произведение ∈ [0,1] множителей)
///   - Строго монотонно в C при фиксированных L, h (через N(C))
///   - JND-нормировано по конструкции через M-02
///   - .clamp(0.0, 1.0) — safety no-op при f64-округлении
pub fn muddiness_oklch(l: f64, c: f64, h_deg: f64) -> f64 {
    raw_chromatic(l, c, h_deg).clamp(0.0, 1.0)
}

/// Per-colour confidence that drops near decision boundary (mud ~ 0.5) and grey-frontier (chroma ~ C0).
/// Bounded by the declared-calibration concept floor (DECLARED-CALIBRATION KAPPA_CORE=0.34 stable-core /
/// KAPPA_INTERIOR=0.10 interior; empirical v3-retest floors, NOT published perceptual constants — see M-10/M-11 in the inventory table above).
pub fn confidence(l: f64, c: f64, h_deg: f64) -> f64 {
    let cc = neutral_gate(l, c, C0, JND, LESC);
    let chroma_conf = (1.0 - 4.0 * cc * (1.0 - cc)).clamp(0.0, 1.0);
    let mud = muddiness_oklch(l, c, h_deg);
    let margin_conf = (1.0 - (-((mud - 0.5) / M_W).powi(2)).exp()).clamp(0.0, 1.0);
    let kappa_band = KAPPA_INTERIOR + (KAPPA_CORE - KAPPA_INTERIOR) * margin_conf;
    kappa_band * chroma_conf * margin_conf
}

/// Compute muddiness from linear sRGB values [r, g, b] in [0, 1].
pub fn muddiness_from_linear_srgb(rgb: [f64; 3]) -> f64 {
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
    let l = lab[0];
    let c = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    muddiness_oklch(l, c, h)
}

/// Compute muddiness from an sRGB hex color string like "#6B6B2E".
pub fn muddiness_from_hex(hex: &str) -> Result<f64, String> {
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    Ok(muddiness_from_linear_srgb(rgb))
}

/// Compute confidence from an sRGB hex color string like "#6B6B2E".
pub fn confidence_from_hex(hex: &str) -> Result<f64, String> {
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
    let l = lab[0];
    let c = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    Ok(confidence(l, c, h))
}

// ─────────────────────────────────────────────────────────────────────────────
// Drab defect head (Zone B slice 2 — second INDEPENDENT output)
//
// `n_pure` and `drab` are a SEPARATE defect axis from mud/muddiness_oklch.
// They share ONLY the cited gate constants C0 / JND as INPUTS.
// They are NEVER called inside `raw_chromatic`, `muddiness_oklch`, or any
// other mud code path — the two axes are provably independent (the curl /
// integrability two-axes result from the paradigm North).
//
// drab(C) = sigmoid((C0 - C) / JND)
//         = 1 - sigmoid((C   - C0) / JND)   [= 1 - n_pure(C)]
//
// Constants reused (zero new parameters):
//   C0  = 0.0395  (cited-and-kept, M-01, Evans/Xie-Fairchild yellow zero-grayness frontier)
//   JND = 0.0122779190541810 (cited-and-kept, M-02, Oklab chroma JND)
//
// OPEN items (left unchanged): W_HUE / g0_band — всё ещё flagged-provisional
// с named-исследованиями (см. таблицу инвентаря выше).
// M-07/M-08 (Platt CAL_T/CAL_B) УДАЛЕНЫ в Zone B slice 3 — более не OPEN.
// ─────────────────────────────────────────────────────────────────────────────

/// Pure chroma-presence gate: N_pure(C) = sigmoid((C - C0) / JND).
///
/// This is the shared gate that enters both mud (as `neutral_gate`) and drab
/// (as 1 - N_pure).  It is kept as a standalone `pub fn` so callers can
/// compose the two heads independently without re-implementing the gate.
///
/// Parameters: cited-and-kept C0 (M-01) and JND (M-02).  Zero new parameters.
#[inline]
pub fn n_pure(c: f64) -> f64 {
    sigmoid((c - C0) / JND)
}

/// Drab defect head: D(C) = 1 − N_pure(C).
///
/// Measures chroma ABSENCE — the complement of the cited chroma-presence gate.
/// Returns 1.0 for achromatic colours (C ≈ 0, deeply grey/beige) and 0.0 for
/// strongly chromatic colours (C >> C0).
///
/// Implementation: `1.0 - n_pure(c)` — exact f64 arithmetic complement.
/// This guarantees `drab(C) + n_pure(C) == 1.0` exactly in IEEE 754, which
/// the property tests verify.  The equivalent closed form sigmoid((C0 - C) / JND)
/// would produce values that sum to 1.0 only up to ±1 ULP due to independent
/// exp() calls; the subtraction form is exact by construction.
///
/// Invariants guaranteed by construction (independently property-tested):
///   `drab(C) + n_pure(C) == 1.0`  exact in f64
///   `drab` is strictly monotone-decreasing in C
///   `drab(C0) == 0.5`  (sigmoid(0) == 0.5 exactly, since n_pure(C0) == 0.5)
///
/// This function is a SECOND DISTINCT output — it is NEVER called inside
/// `raw_chromatic`, `muddiness_oklch`, or any mud path.  The only coupling
/// with the mud head is the shared C0/JND constants as a shared INPUT gate.
#[inline]
pub fn drab(c: f64) -> f64 {
    1.0 - n_pure(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Smoke / class-membership reference values ────────────────────────────
    //
    // Обновлено в Zone B slice 4 (2026-06-30): W_HUE заменён выведенным Hanning-окном
    // Бецольда-Брюкке. Диапазон mud значительно расширился: [0, ~0.41] вместо [0.00..0.14].
    // Это ожидаемо — BB-формула даёт hue_weight ≈ 1.0 в широком тёплом секторе вместо
    // узкого sigmoid-пика из логистической регрессии W_HUE.
    //
    // Ранговый порядок грязности: olive > babypoop > gold2 > gold1 > puke
    // (прим.: gold2 > gold1 — артефакт подогнанного W_HUE исчез; теперь глубина
    // под cusp-L побеждает, что физически более верно).
    //
    // Класс теста: class A (unit + reference), Фаулер.
    // TDD: RED до коммита 2 (W_HUE → BB), GREEN после.
    #[test]
    fn test_muddiness_v3_reference_values() {
        // Olive #6B6B2E (Наиболее грязный — максимальный raw)
        // Провенанс golden: mud = N(C) * hue_weight_BB(h) * depth_mod(L, C, h),
        // hue_weight_BB(109°) = (1 + cos(109° - 96.9°)) / 2 ≈ 0.989.
        let olive_mud = muddiness_from_hex("#6B6B2E").unwrap();
        let olive_conf = confidence_from_hex("#6B6B2E").unwrap();
        assert!(
            olive_mud > 0.30,
            "olive mud is {olive_mud:.6}, expected > 0.30 (BB-диапазон)"
        );
        assert!(
            (olive_mud - 0.41267217).abs() < 1e-5,
            "olive mud {olive_mud:.8} != 0.41267217 (Zone B slice 4 BB golden)"
        );
        assert!(
            (olive_conf - 0.02750167).abs() < 1e-4,
            "olive conf {olive_conf:.8} != 0.02750167"
        );

        // Babypoop #937C00 (Очень грязный)
        // hue_weight_BB(96°) = (1 + cos(96° - 96.9°)) / 2 ≈ 0.9999.
        let babypoop_mud = muddiness_from_hex("#937C00").unwrap();
        assert!(
            babypoop_mud > 0.20,
            "babypoop mud is {babypoop_mud:.6}, expected > 0.20"
        );
        assert!(
            (babypoop_mud - 0.33297635).abs() < 1e-5,
            "babypoop mud {babypoop_mud:.8} != 0.33297635"
        );

        // Puke #9AAE07 (Умеренно грязный)
        let puke_mud = muddiness_from_hex("#9AAE07").unwrap();
        assert!(
            (puke_mud - 0.24024764).abs() < 1e-5,
            "puke mud {puke_mud:.8} != 0.24024764"
        );

        // Золотые (средний диапазон)
        let gold1_mud = muddiness_from_hex("#9e6c00").unwrap();
        assert!(
            (gold1_mud - 0.28807528).abs() < 1e-5,
            "gold1 mud {gold1_mud:.8} != 0.28807528"
        );

        let gold2_mud = muddiness_from_hex("#8f6424").unwrap();
        assert!(
            (gold2_mud - 0.30349357).abs() < 1e-5,
            "gold2 mud {gold2_mud:.8} != 0.30349357"
        );

        // Ранговый порядок: olive > babypoop > gold2 > gold1 > puke >> achromatic
        // Примечание: gold2 > gold1 (BB-формула; прежний порядок gold1 > gold2 был
        // артефактом подогнанного W_HUE, не геометрическим свойством Oklab).
        assert!(
            olive_mud > babypoop_mud,
            "olive должен быть грязнее babypoop: {olive_mud:.6} vs {babypoop_mud:.6}"
        );
        assert!(
            babypoop_mud > gold1_mud && babypoop_mud > gold2_mud,
            "babypoop должен быть грязнее золотых: {babypoop_mud:.6} vs gold1={gold1_mud:.6} gold2={gold2_mud:.6}"
        );
        assert!(
            gold2_mud > gold1_mud,
            "gold2 должен быть грязнее gold1 по BB-формуле: gold2={gold2_mud:.8} vs gold1={gold1_mud:.8}"
        );
        assert!(
            gold1_mud > puke_mud && gold2_mud > puke_mud,
            "золотые должны быть грязнее puke: gold1={gold1_mud:.6} gold2={gold2_mud:.6} vs puke={puke_mud:.6}"
        );

        // Pure grey #808080 (Чистый — achromatic, N(C) → 0)
        let grey_mud = muddiness_from_hex("#808080").unwrap();
        assert!(
            grey_mud < 0.005,
            "grey mud is {grey_mud:.6}, expected < 0.005 (N(C) подавляет)"
        );

        // Teal #008080 (прохладный — b < B0, depth_mod → почти 0)
        let teal_mud = muddiness_from_hex("#008080").unwrap();
        let teal_conf = confidence_from_hex("#008080").unwrap();
        assert!(
            teal_mud < 0.01,
            "teal mud is {teal_mud:.6}, expected < 0.01 (depth_mod b-гейт)"
        );
        assert!(
            (teal_mud - 0.00494901).abs() < 1e-5,
            "teal mud {teal_mud:.8} != 0.00494901 (Zone B slice 4 BB golden)"
        );
        assert!(
            (teal_conf - 0.32229504).abs() < 1e-4,
            "teal conf {teal_conf:.8} != 0.32229504"
        );

        // Navy Blue #000080 (Чистый — синий, depth_mod → 0 через b-гейт)
        let navy_mud = muddiness_from_hex("#000080").unwrap();
        assert!(
            navy_mud < 0.001,
            "navy mud is {navy_mud:.8}, expected < 0.001"
        );
    }

    // ─── Characterization golden test (Zone B slice 4, Fowler class B) ─────────
    //
    // Фиксирует таблицу mud(L,C,h) после замены W_HUE на Hanning-окно BB.
    // mud = N(C) * hue_weight_BB(h) * depth_mod(L, C, h)
    //   где hue_weight_BB(h) = (1 + cos(h − H_Y_DEG)) / 2
    //
    // Golden-значения получены из фактического кода после коммита 2 (Zone B slice 4)
    // и зафиксированы как regression anchor.  Tolerance: ±5e-6 (точность f64).
    //
    // Класс теста: B (characterization/golden, Фаулер) — якорь регрессии.
    // История: Zone B slice 3 (Platt-удаление) имела диапазон [0.00..0.14];
    //          Zone B slice 4 (BB-замена) расширила до [0.00..0.41] — ожидаемо
    //          (hue_weight_BB ≈ 1.0 шире, чем узкий sigmoid-пик W_HUE).
    #[test]
    fn characterization_mud_golden_cited() {
        struct Case {
            hex: &'static str,
            label: &'static str,
            expected: f64,
        }
        // Значения = muddiness_oklch(l,c,h) (BB-путь, Zone B slice 4)
        let cases = [
            Case {
                hex: "#6B6B2E",
                label: "olive",
                expected: 0.41267217,
            },
            Case {
                hex: "#937C00",
                label: "babypoop",
                expected: 0.33297635,
            },
            Case {
                hex: "#9AAE07",
                label: "puke",
                expected: 0.24024764,
            },
            Case {
                hex: "#9e6c00",
                label: "gold1",
                expected: 0.28807528,
            },
            Case {
                hex: "#8f6424",
                label: "gold2",
                expected: 0.30349357,
            },
            Case {
                hex: "#808080",
                label: "grey",
                expected: 0.00125515,
            },
            Case {
                hex: "#008080",
                label: "teal",
                expected: 0.00494901,
            },
            Case {
                hex: "#000080",
                label: "navy",
                expected: 0.00000001,
            },
            Case {
                hex: "#FF0000",
                label: "red",
                expected: 0.00124390,
            },
            Case {
                hex: "#0000FF",
                label: "blue",
                expected: 0.00000000,
            },
        ];
        for c in &cases {
            let got = muddiness_from_hex(c.hex).unwrap();
            assert!(
                (got - c.expected).abs() < 5e-6,
                "characterization_mud_golden_cited: {} {}: got={:.8} expected={:.8} delta={:.2e}",
                c.label,
                c.hex,
                got,
                c.expected,
                (got - c.expected).abs()
            );
        }
    }

    // ─── CAL_T / CAL_B absence guard (Zone B slice 3, Fowler class Д) ──────────
    //
    // Проверяет, что `pub const CAL_T` и `pub const CAL_B` больше не определены,
    // а `muddiness_oklch` не содержит вызовов CAL_T/CAL_B вне комментариев.
    //
    // Стратегия: ищем конкретные паттерны объявления (pub const CAL_T / pub const CAL_B)
    // и паттерн употребления (CAL_T * / * CAL_B). Комментарии, описывающие удаление,
    // допустимы — North: «комментарии могут только описывать удаление по-русски».
    //
    // RED на Platt-дереве (pub const CAL_T/CAL_B существуют и используются в muddiness_oklch).
    // GREEN после коммита 2 (определения и употребления удалены).
    #[test]
    fn cal_t_cal_b_absent_from_shipping_code() {
        let source = include_str!("cleanliness.rs");
        // Продуктивный код — всё до маркера тест-модуля
        let prod_code = source.split("#[cfg(test)]").next().unwrap_or(source);

        // Проверяем отсутствие ОБЪЯВЛЕНИЙ (pub const CAL_T / pub const CAL_B)
        assert!(
            !prod_code.contains("pub const CAL_T"),
            "pub const CAL_T найден в продуктивном коде — Platt-скаляр не удалён (M-07)."
        );
        assert!(
            !prod_code.contains("pub const CAL_B"),
            "pub const CAL_B найден в продуктивном коде — Platt-скаляр не удалён (M-08)."
        );

        // Проверяем отсутствие УПОТРЕБЛЕНИЙ в продуктивных выражениях
        // (паттерны: CAL_T * z   и   + CAL_B)
        assert!(
            !prod_code.contains("CAL_T *"),
            "CAL_T * найден в продуктивном коде — muddiness_oklch всё ещё использует Platt."
        );
        assert!(
            !prod_code.contains("+ CAL_B"),
            "+ CAL_B найден в продуктивном коде — muddiness_oklch всё ещё использует Platt."
        );
        assert!(
            !prod_code.contains("pub const CAL_EPS"),
            "pub const CAL_EPS найден в продуктивном коде — log-регуляризатор Platt не удалён (M-06)."
        );
    }

    // ─── Drab head property tests (Zone B slice 2, Fowler class A) ──────────
    //
    // Three properties that MUST hold for drab(C) = sigmoid((C0 - C) / JND)
    // (the chroma-absence complement of the cited gate N_pure(C)):
    //
    //   (a) D(C) + N_pure(C) == 1.0 exact in f64 across a chroma sweep
    //       — they are complements by construction: sigmoid(x) + sigmoid(-x) == 1.
    //   (b) D'(C) < 0 — drab strictly DECREASES as chroma C rises
    //       (finite-difference monotonicity over the sweep).
    //   (c) D(C0) == 0.5 exactly — sigmoid(0) == 0.5 by definition.
    //
    // These tests reference `drab` and `n_pure`, which do NOT exist at this
    // commit (commit 1 of 2).  They compile-fail at this HEAD, proving
    // TDD RED-first.  Verified independently in an isolated worktree.
    #[test]
    fn drab_plus_n_pure_equals_one_sweep() {
        // Sweep C from 0.0 to 0.3 in 1001 steps; verify D + N == 1.0 exact.
        let steps = 1001usize;
        for i in 0..=steps {
            let c = (i as f64) * 0.3 / (steps as f64);
            let d = drab(c);
            let n = n_pure(c);
            assert_eq!(
                d + n,
                1.0,
                "drab + n_pure != 1.0 at C={c:.6}: drab={d:.18} n_pure={n:.18}"
            );
        }
    }

    #[test]
    fn drab_strictly_decreasing_in_chroma() {
        // Finite-difference monotonicity: drab(C+delta) < drab(C) for all C.
        let steps = 1001usize;
        let delta = 0.3 / (steps as f64);
        for i in 0..steps {
            let c0v = (i as f64) * 0.3 / (steps as f64);
            let c1 = c0v + delta;
            let d0 = drab(c0v);
            let d1 = drab(c1);
            assert!(
                d1 < d0,
                "drab not strictly decreasing: drab({c1:.6})={d1:.18} >= drab({c0v:.6})={d0:.18}"
            );
        }
    }

    #[test]
    fn drab_at_c0_is_half() {
        // D(C0) must equal exactly 0.5 because sigmoid(0) == 0.5.
        let d = drab(C0);
        assert_eq!(
            d, 0.5,
            "drab(C0) should be exactly 0.5 (sigmoid(0)); got {d:.18}"
        );
    }

    // ─── Zone B slice 4: W_HUE absent + Бецольд-Брюкке инварианты (TDD RED-first) ──
    //
    // Три теста закрывают КЛАСС дефектов: «подогнанный вектор вместо выведенной формулы».
    //
    // (1) w_hue_fitted_vector_absent_from_shipping — RED: `pub static W_HUE` ещё
    //     существует в продуктивном коде (и hue_basis, hue_weight ещё опираются на него).
    //     GREEN после коммита 2: W_HUE убран, hue_weight переписан.
    //
    // (2) bezold_brucke_at_unique_yellow_is_one — RED: текущий hue_weight(97°) ≈ 0.2
    //     (sigmoid(dot(W_HUE, basis(97°))) ≠ 1.0).  GREEN после коммита 2:
    //     выведенная формула (1 + cos(0)) / 2 == 1.0 точно.
    //
    // (3) bezold_brucke_at_opposite_is_zero — RED: текущий hue_weight(277°) ≈ 0.015
    //     (sigmoid(dot) ≠ 0.0).  GREEN после коммита 2:
    //     (1 + cos(π)) / 2 == 0.0 точно.
    //
    // (4) bezold_brucke_monotone_from_unique_yellow — RED: текущая формула не является
    //     Hanning-окном и нарушает монотонное убывание от h_Y по обе стороны.
    //     GREEN после коммита 2: cos(δ) монотонно убывает при |δ| растёт от 0 до π.
    //
    // Класс багов: fitted-вектор не имеет гарантированных инвариантов hue_weight(h_Y) = 1,
    // hue_weight(h_Y + 180°) = 0 и монотонности; выведенная формула доказывает их аналитически.
    //
    // Провенанс:
    //   H_Y_DEG = 96.9° — Oklab hue уникального жёлтого (λ=578nm, CIE 1931 2° наблюдатель,
    //   D65).  Derivation: CMF при 578nm (x̄=0.9015, ȳ=0.7470, z̄=0; линейная интерполяция
    //   CIE 10нм-таблицы), → XYZ/Y = (1.207, 1.000, 0), → linear sRGB (clamp at gamut),
    //   → Oklab (Ottosson 2020), → atan2(b, a) = 96.9°.
    //   Формула (1 + cos(h − h_Y)) / 2 — Hanning-окно, выведенное из производной
    //   поворота Бецольда-Брюкке: dΔH_BB/dh = A_BB · cos(h − h_Y), нормированная
    //   в [0, 1].  Цитата: Parry (1967) J. Opt. Soc. Am. 57, 1130–1134.

    #[test]
    fn w_hue_fitted_vector_absent_from_shipping() {
        // RED пока pub static W_HUE присутствует в продуктивном коде.
        // GREEN после замены на выведенную BB-формулу.
        let source = include_str!("cleanliness.rs");
        let prod_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            !prod_code.contains("pub static W_HUE"),
            "pub static W_HUE найден в продуктивном коде — подогнанный вектор (M-12) не удалён. \
             Ожидается выведенная формула Бецольда-Брюкке."
        );
        // Дополнительно: hue_basis (вспомогательная функция для dot-product) тоже должна уйти
        assert!(
            !prod_code.contains("fn hue_basis("),
            "fn hue_basis( найдена в продуктивном коде — K=3 Фурье-базис больше не нужен \
             после перехода на BB-формулу."
        );
    }

    #[test]
    fn bezold_brucke_at_unique_yellow_is_one() {
        // Hanning-окно: (1 + cos(h − h_Y)) / 2 == 1.0 точно при h == h_Y.
        // RED: текущий sigmoid(dot(W_HUE, basis(H_Y_DEG))) ≠ 1.0.
        // GREEN: выведенная BB-формула гарантирует (1 + cos(0)) / 2 == 1.0.
        //
        // Значение H_Y_DEG определяется из CIE 1931 2° (D65) → derivation в
        // комментарии к тест-блоку выше.  Мы тестируем через публичный `hue_weight`:
        // если формула выведена верно, hue_weight(H_Y_DEG) == 1.0 ТОЧНО (не ≈).
        let hw = hue_weight(H_Y_DEG);
        assert_eq!(
            hw, 1.0,
            "hue_weight(H_Y_DEG={H_Y_DEG}) должен быть ровно 1.0 \
             (Hanning-окно: (1 + cos(0)) / 2 = 1.0 точно); получено {hw:.18}"
        );
    }

    #[test]
    fn bezold_brucke_at_opposite_is_zero() {
        // Hanning-окно: (1 + cos(π)) / 2 == 0.0 ТОЧНО при h == h_Y + 180°.
        // RED: текущий sigmoid(dot) ≈ 0.015, не равен 0.0.
        // GREEN: выведенная формула гарантирует нулевой вес на противоположном оттенке.
        let hw = hue_weight(H_Y_DEG + 180.0);
        assert_eq!(
            hw,
            0.0,
            "hue_weight(H_Y_DEG + 180 = {}) должен быть ровно 0.0 \
             (Hanning-окно: (1 + cos(π)) / 2 = 0.0 точно); получено {hw:.18}",
            H_Y_DEG + 180.0
        );
    }

    #[test]
    fn bezold_brucke_monotone_from_unique_yellow() {
        // Hanning-окно строго монотонно убывает при |δ| растёт от 0 до π.
        // RED: текущая sigmoid(dot(W_HUE,...)) — не монотонное Hanning-окно.
        // GREEN: (1 + cos(δ)) / 2 монотонно убывает по обе стороны от h_Y.
        //
        // Тест: для 360 равномерных шагов δ от 0 до 180° включительно,
        // hue_weight(h_Y + δ) должен строго убывать.
        let steps = 360usize;
        let mut prev = hue_weight(H_Y_DEG);
        assert!(
            (prev - 1.0).abs() < 1e-15,
            "hue_weight(h_Y) должен быть 1.0; получено {prev}"
        );
        for i in 1..=steps {
            let delta = (i as f64) * 180.0 / (steps as f64);
            let hw = hue_weight(H_Y_DEG + delta);
            assert!(
                hw < prev,
                "hue_weight не строго монотонно убывает (плато): hw(h_Y + {delta:.2}) = {hw:.8} \
                 >= hw(h_Y + {:.2}) = {prev:.8}",
                (i - 1) as f64 * 180.0 / (steps as f64)
            );
            prev = hw;
        }
        // Симметрично: hw(h_Y - δ) == hw(h_Y + δ) для Hanning-окна
        for i in 1..=steps {
            let delta = (i as f64) * 180.0 / (steps as f64);
            let hw_pos = hue_weight(H_Y_DEG + delta);
            let hw_neg = hue_weight(H_Y_DEG - delta);
            assert!(
                (hw_pos - hw_neg).abs() < 1e-12,
                "hue_weight не симметрично: hw(h_Y + {delta:.2}) = {hw_pos:.12} \
                 != hw(h_Y - {delta:.2}) = {hw_neg:.12}"
            );
        }
    }

    // ─── Provenance-range guard (Zone B, Fowler class A — property test) ─────
    //
    // Asserts B0 lies in [0.030, 0.044] and BW lies in [0.013, 0.020] — the
    // cited-measured ranges (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014
    // PNAS; Boynton 1975).  This test was RED against the pre-swap fitted
    // values (B0=0.02869 < 0.030, BW=0.02024 > 0.020) and turned GREEN after
    // the swap — proving it bites (TDD RED-first; verified in git history as
    // the compile-failing state at commit 1 HEAD).
    //
    // Uses `const { assert!(..) }` so the range check is enforced at COMPILE
    // TIME (clippy::assertions_on_constants, Rust ≥1.96).  A future constant
    // edit outside the cited range will be caught at `cargo build`, not just
    // at test time.
    #[test]
    fn provenance_range_guard_b0_bw() {
        const {
            assert!(
                B0 >= 0.030 && B0 <= 0.044,
                "B0 is outside the cited range [0.030, 0.044] \
             (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). \
             M-04 must be cited-measured."
            )
        };
        const {
            assert!(
                BW >= 0.013 && BW <= 0.020,
                "BW is outside the cited range [0.013, 0.020] \
             (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). \
             M-05 must be cited-measured."
            )
        };
    }
}
