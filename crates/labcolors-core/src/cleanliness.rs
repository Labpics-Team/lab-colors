//! Замороженная legacy research-coordinate системы `labcolors`.
//!
//! Исторические имена модуля и функций (`cleanliness`, `muddiness`, `drab`,
//! `n_pure`) — только идентификаторы API. Результат — **experimental compatibility proxy**,
//! а не observer-validated cleanliness/murkiness,
//! эстетическая оценка, human verdict или product contract. Компилятор и
//! runtime-resolver токенов эту координату не используют.
//!
//! Формула и frozen conformance corpus воспроизводимы; это не валидирует
//! человеческий смысл названий. `C0`, `JND`, `B0` и `BW` остаются legacy
//! compatibility constants: опубликованный вывод их точных Oklab-значений и
//! универсальная применимость не установлены. Hanning-форма `hue_weight` —
//! эвристика; эффект Бецольда—Брюкке её не выводит. `H_Y_DEG` воспроизводит
//! арифметическое преобразование выбранного 578 nm reference в Oklab, но не
//! является универсальным unique-yellow наблюдателя.
//!
//! Полный эпистемический статус и владельцы миграции: `docs/empirical-inventory.md`
//! (M-01…M-14) и Issue #231.
//!
//! # Уравнения
//!
//! ```text
//! raw = neutral_gate(C) * hue_weight(h) * depth_mod(L, C, h)
//! mud = raw                   ← замороженный legacy-идентификатор результата
//! ```
//!
//! Platt-звено (логарифм raw + eps → sigmoid с двумя подогнанными скалярами) **удалено полностью**
//! в Zone B slice 3: оба скаляра (M-07/M-08) подогнаны на авторском датасете 738 меток
//! (наблюдатель-фит) и нарушали инвариант «ZERO observer-fit» (North).  mud = raw_chromatic напрямую:
//! - параметр-свободно (нет новых констант);
//! - ограничено \[0,1\] точно (каждый множитель ∈ \[0,1\]);
//! - масштаб logistic gate задаётся замороженной шириной M-02; это не заявление
//!   о perceptual threshold или универсальном JND.
//!
//! Бывший light-escape порог M-03 (DECLARED-CALIBRATION скаляр) и его escape-член
//! в `neutral_gate` **удалены целиком** (Zone B, 2026-07-01): escape-член протекал
//! ось L в функцию, документированную как чисто-хроматический hue-agnostic гейт —
//! нарушение независимости осей. `neutral_gate(c, c0, jnd) = sigmoid((c - c0) / jnd)`.
//!
//! # Инвентарь legacy-параметров
//!
//! | const           | mud-id | статус                         | значение                    |
//! |-----------------|--------|--------------------------------|-----------------------------|
//! | `C0`            | M-01   | Indeterminate provenance       | 0.0395 frozen compatibility value; exact Oklab derivation not established |
//! | `JND`           | M-02   | universal JND Rejected; value Indeterminate | 0.01228 frozen gate width; reference point/domain not specified |
//! | `B0`            | M-04   | Rejected provenance; Indeterminate value | 0.036 frozen positive-b gate centre; cited Oklab range was not derived by the named sources |
//! | `BW`            | M-05   | Rejected provenance; Indeterminate value | 0.017 frozen positive-b gate width; cited Oklab range was not derived by the named sources |
//! | ~~`CAL_EPS`~~   | M-06   | УДАЛЁН (Zone B slice 3)        | был 0.01; log-регуляризатор для Platt — более не нужен |
//! | ~~`CAL_T`~~     | M-07   | УДАЛЁН (Zone B slice 3)        | был 2.356978; Platt-скаляр, подогнанный на 738 авторских метках — нарушал ZERO observer-fit |
//! | ~~`CAL_B`~~     | M-08   | УДАЛЁН (Zone B slice 3)        | был 6.445168; Platt-смещение — нарушало ZERO observer-fit |
//! | ~~`M_W`~~       | M-09   | УДАЛЁН (Front B, 2026-07-04)    | был 0.181527; полуширина доверительного поля confidence-слоя — субъективная калибровка владельца без модели надёжности, вне пути эмиссии — удалён целиком |
//! | ~~`KAPPA_CORE`~~ | M-10  | УДАЛЁН (Front B, 2026-07-04)    | был 0.34; потолок уверенности ядра — та же субъективная калибровка — удалён вместе с confidence-слоем |
//! | ~~`KAPPA_INTERIOR`~~ | M-11 | УДАЛЁН (Front B, 2026-07-04)  | был 0.10; пол уверенности спорной полосы — удалён вместе с confidence-слоем |
//! | `H_Y_DEG`       | M-12   | arithmetic admitted; observer claim Rejected | 90.4011° — выбранный 578 nm reference → Oklab; не universal unique yellow и не вывод Hanning-семантики |
//! | ~~`W_HUE[8]`~~  | M-12   | УДАЛЁН (Zone B slice 4)        | был: подогнанный K=3 Fourier-вектор логистической регрессии — нарушал ZERO observer-fit |
//! | `CUSP_L_TABLE`  | M-13   | geometry admitted              | геометрия гамута Oklab; не человеческая граница cleanliness |
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
// После замены hue_weight на замороженное Hanning-окно ни hue_basis, ни CEIL_N_TABLE
// не нужны в продуктивном коде.

// High-precision frozen parameter constants
pub const C0: f64 = 0.0395000000000000;
/// Frozen width of the legacy sigmoid gate (M-02).
///
/// The historical name is retained for API compatibility. A universal Oklab
/// just-noticeable difference is rejected: no reference point, direction,
/// viewing protocol, or observer population establishes this exact value.
/// Therefore this constant must not be interpreted as a detection threshold.
/// See `docs/empirical-inventory.md` and Issue #242.
pub const JND: f64 = 0.0122779190541810;
// M-03 (former light-escape calibration threshold, DECLARED-CALIBRATION) removed
// entirely (Zone B, 2026-07-01): its escape term leaked the lightness axis into
// what is documented as a hue-agnostic, chroma-only gate (see `neutral_gate` above).
/// Frozen centre of the legacy positive-b gate (M-04).
/// The former cited-measured provenance claim is rejected; this is compatibility state.
pub const B0: f64 = 0.036;
/// Frozen width of the legacy positive-b gate (M-05).
/// The former cited-measured provenance claim is rejected; this is compatibility state.
pub const BW: f64 = 0.017;
// CAL_EPS / CAL_T / CAL_B удалены (Zone B slice 3, 2026-06-30):
// Platt-звено sigmoid(CAL_T*ln(raw+eps)+CAL_B) было observer-fit на 738 авторских метках
// (нарушение ZERO observer-fit, North).  Заменено параметр-свободным mud = raw_chromatic.
// Confidence-слой (M_W / KAPPA_CORE / KAPPA_INTERIOR) удалён целиком (Front B, 2026-07-04):
// это была субъективная калибровка владельца без модели надёжности — вне пути эмиссии
// (solve/semantic/resolve его не звали), вне агностичного контракта движка. Снос не
// двигает ни байта эмиссии. См. removed-строки M-09/M-10/M-11 в docs/empirical-inventory.md.

// W_HUE[8] удалён (Zone B slice 4, 2026-06-30):
// Подогнанный вектор K=3 Фурье-регрессии (логистическая регрессия на 738 авторских
// метках, M-12) заменён замороженной параметр-свободной Hanning-эвристикой.
// Формула: hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2
// H_Y_DEG воспроизводит арифметическое преобразование выбранного 578nm reference
// в Oklab. Эффект Бецольда—Брюкке не выводит эту Hanning-форму и не придаёт ей
// универсальный observer meaning.
// Инварианты: hue_weight(H_Y_DEG) = 1.0 точно; hue_weight(H_Y_DEG ± 180°) = 0.0 точно;
// строго монотонное убывание при |h − H_Y_DEG| растёт от 0 до 180°.

/// Oklab hue выбранного спектрального reference λ=578nm (CIE 1931 2° CMF).
///
/// Вывод (Zone B slice 5, 2026-07-03 — исправление ошибочной деривации slice 4):
///   1. CMF при 578nm — линейная интерп. официальной CIE 1931 2° 5нм-таблицы:
///      575nm (x̄=0.8425, ȳ=0.9154, z̄=0.0018); 580nm (x̄=0.9163, ȳ=0.8700, z̄=0.00165);
///      t=0.6 → (x̄=0.886780, ȳ=0.888160, z̄=0.001710).
///   2. Нормируем к Y=1: XYZ = (0.998446, 1.000000, 0.001925).
///   3. XYZ → Oklab НАПРЯМУЮ через M1/M2 (Ottosson 2020). Проекция в sRGB не нужна:
///      hue инвариантен к абсолютной радиансности стимула — равномерное
///      масштабирование LMS′ множит a и b ОДИНАКОВО (линейность M2), отношение
///      в atan2 не меняется; отдельный верный факт: нулевые суммы строк a,b
///      матрицы M2 дают a = b = 0 на ахромате (L′=M′=S′).
///   4. h = atan2(b, a) = 90.4011°.
///
/// История slice 4 (значение 96.9172 — ОТОЗВАНО): деривация содержала две ошибки.
/// (а) CMF-значения были спутаны между строками таблицы (заявленные ȳ=0.7070/0.7570
///     для 570/580нм — на деле ȳ(570)=0.9520, ȳ(580)=0.8700; 0.7570 — это ȳ(590нм)),
///     что давало ложный XYZ=(1.207, 1, 0) вместо (0.998, 1, 0.002).
/// (б) Перед взятием hue цвет клэмпился к гамуту sRGB — покомпонентный клэмп НЕ
///     сохраняет оттенок: без клэмпа тот же XYZ давал h=69.7°, после клэмпа 96.9°.
///     Значение 96.9172° было артефактом клэмпа поверх неверного XYZ.
///
/// Арифметика выбранного λ=578nm допускается и воспроизводима. Заявление, что
/// это универсальный unique-yellow наблюдателя, отвергнуто: результат зависит
/// от наблюдателя, яркости и метода. Прежняя цитата «Parry (1967) JOSA 57,
/// 1130–1134» не существует в литературе и удалена.
///
/// Использование: центр Hanning-окна `hue_weight(h)` (M-12).
/// Число не подогнано; воспроизводится скриптом арифметического преобразования CMF.
pub const H_Y_DEG: f64 = 90.4011;

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

/// Geometric depth below the hue-dependent sRGB/Oklab gamut cusp, normalized by cusp L.
/// Ranges from 0.0 (at/above cusp) to 1.0 (furthest below it); no human meaning follows.
pub fn depth_term(l: f64, h_deg: f64) -> f64 {
    let cusp_l = cusp_l_of(h_deg);
    let below = (cusp_l - l) / cusp_l.max(1e-6);
    below.clamp(0.0, 1.0)
}

/// Frozen hue-agnostic logistic coordinate over Oklab chroma.
///
/// `neutral_gate(c, c0, jnd) = sigmoid((c - c0) / jnd)`. The parameter named
/// `jnd` is a compatibility width, not a detection probability or universal
/// perceptual threshold. The former light-escape term (a product of two
/// sigmoids gated on lightness and chroma) depended on a DECLARED-CALIBRATION
/// threshold (M-03, a Platt-fit scalar on the v3 dataset) and leaked the
/// lightness axis into a function documented as hue-agnostic and chroma-only —
/// removed entirely (Zone B, 2026-07-01) per North's ZERO observer-fit invariant.
pub fn neutral_gate(c: f64, c0: f64, jnd: f64) -> f64 {
    sigmoid((c - c0) / jnd)
}

/// Frozen positive-b logistic gate multiplied by the geometric depth term.
/// Its historical parameter values have no admitted human clean/dirty semantics.
pub fn depth_mod(l: f64, c: f64, h_deg: f64, b0: f64, bw: f64) -> f64 {
    let dp = depth_term(l, h_deg);
    let s = sigmoid((b_of(c, h_deg) - b0) / bw);
    dp * s
}

/// Замороженная Hanning-эвристика над Oklab hue (Zone B slice 4, 2026-06-30).
///
/// Заменяет подогнанный вектор W_HUE\[8\] (M-12, удалён в Zone B slice 4)
/// параметр-свободной формулой.
///
/// # Эпистемический статус
///
/// Эффект Бецольда—Брюкке реален, но не выводит эту косинусную функцию,
/// её центр или human cleanliness semantics. Формула ниже — frozen heuristic;
/// доказаны только её математические свойства. `H_Y_DEG` — арифметика выбранного
/// 578nm reference, не универсальный unique-yellow наблюдателя.
///
///   hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2   [Hanning-окно]
///
/// # Провенанс констант
///
/// - `H_Y_DEG = 90.4011°` — Oklab hue выбранного λ=578nm reference по CIE 1931 2° CMF.
///   Вывод: CMF → XYZ → Oklab напрямую (Ottosson 2020) → atan2(b, a); без sRGB-клэмпа
///   (клэмп не сохраняет hue — см. историю slice 4 в доке H_Y_DEG).
///   Это не observer-validated invariant; см. M-12 в empirical inventory.
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

/// Сырая experimental compatibility proxy coordinate (legacy API identifier).
///
/// raw = N(C) × hue_weight_BB(h) × depth_mod(L, C, h)
///
/// Все три множителя ∈ \[0,1\], поэтому raw ∈ \[0,1\] строго.
pub fn raw_chromatic(l: f64, c: f64, h_deg: f64) -> f64 {
    let cc = neutral_gate(c, C0, JND);
    let hw = hue_weight(h_deg);
    let dpm = depth_mod(l, c, h_deg, B0, BW);
    cc * hw * dpm
}

/// Frozen experimental compatibility proxy (Zone B slices 3+4, 2026-06-30).
///
/// `mud = raw_chromatic(l, c, h)` is a deterministic product of three legacy factors:
///   `N(C) = sigmoid((C - C0) / JND)`             — frozen logistic gate (M-01/M-02)
///   `hue_weight(h) = (1+cos(h-H_Y_DEG))/2`       — Hanning heuristic (M-12)
///   `depth_mod(L, C, h)`                          — gamut geometry × positive-b gate (M-04/M-05)
///
/// The historical name does not make the output an observer-validated cleanliness,
/// murkiness, aesthetic, or production decision. The compiler/resolver does not use it.
///
/// Свойства:
///   - `H_Y_DEG` reproduces the selected 578nm-to-Oklab arithmetic
///   - Нет подогнанных скаляров: W_HUE/CEIL_N_TABLE/CAL_T/CAL_B — все удалены
///   - raw ∈ \[0,1\] точно (произведение ∈ \[0,1\] множителей)
///   - Монотонно в C только на ТЁПЛОЙ полуплоскости (h∈[0°,180°], sin h ≥ 0):
///     N(C) растёт, b-гейт depth_mod не убывает. На холодных (sin h < 0) b-гейт
///     `sigmoid((c·sin h − B0)/BW)` убывает в C → НЕ глобально монотонно
///     (запинено `property_invariants`: warm-инвариант + cool-characterization)
///   - logistic width is the frozen M-02 compatibility value
///   - .clamp(0.0, 1.0) — safety no-op при f64-округлении
pub fn muddiness_oklch(l: f64, c: f64, h_deg: f64) -> f64 {
    raw_chromatic(l, c, h_deg).clamp(0.0, 1.0)
}

/// Compute the legacy experimental compatibility proxy from linear sRGB in [0, 1].
pub fn muddiness_from_linear_srgb(rgb: [f64; 3]) -> f64 {
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
    let l = lab[0];
    let c = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    muddiness_oklch(l, c, h)
}

/// Compute the legacy experimental compatibility proxy from an sRGB hex string.
pub fn muddiness_from_hex(hex: &str) -> Result<f64, String> {
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    Ok(muddiness_from_linear_srgb(rgb))
}

// ─────────────────────────────────────────────────────────────────────────────
// Legacy `drab` coordinate (Zone B slice 2 — second INDEPENDENT output)
//
// `n_pure` and `drab` are a SEPARATE defect axis from mud/muddiness_oklch.
// They share ONLY the frozen compatibility constants C0 / JND as INPUTS.
// They are NEVER called inside `raw_chromatic`, `muddiness_oklch`, or any
// other mud code path — the two axes are provably independent (the curl /
// integrability two-axes result from the paradigm North).
//
// drab(C) = sigmoid((C0 - C) / JND)
//         = 1 - sigmoid((C   - C0) / JND)   [= 1 - n_pure(C)]
//
// Constants reused (zero new parameters):
//   C0  = 0.0395 (M-01, provenance Indeterminate)
//   JND = 0.0122779190541810 (M-02, universal JND Rejected)
//
// Статус (актуально на 2026-07-11): W_HUE[8] удалён и заменён замороженной
// Hanning-эвристикой (Zone B slice 4, M-12, см. таблицу инвентаря выше);
// Platt CAL_T/CAL_B удалены (Zone B slice 3). Открытых наблюдатель-фит параметров
// в этом блоке не осталось.
// ─────────────────────────────────────────────────────────────────────────────

/// Historical `n_pure` coordinate: `sigmoid((C - C0) / JND)`.
///
/// This is the shared gate that enters both mud (as `neutral_gate`) and drab
/// (as 1 - N_pure).  It is kept as a standalone `pub fn` so callers can
/// compose the two heads independently without re-implementing the gate.
///
/// The name is a compatibility identifier, not an observer estimate of purity.
/// Parameters are frozen C0 (M-01) and JND (M-02); zero new parameters.
#[inline]
pub fn n_pure(c: f64) -> f64 {
    sigmoid((c - C0) / JND)
}

/// Historical `drab` coordinate: `D(C) = 1 - N_pure(C)`.
///
/// This is the arithmetic complement of the frozen `n_pure` logistic coordinate.
/// Returns 1.0 for achromatic colours (C ≈ 0, deeply grey/beige) and 0.0 for
/// strongly chromatic colours (C >> C0), but it is not an observer-validated
/// estimate of dullness, drabness, purity, or any other human judgement.
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

// ─────────────────────────────────────────────────────────────────────────────
// Context-parameterized legacy coordinates (Zone G)
//
// Тема снаружи (light / dark / light-ic / dark-ic) маппируется на ViewingConditions
// по таблице CIECAM16 (Li et al. 2017, Table 1):
//   light      → average surround: F=1.0, c=0.69, Nc=1.0
//   dark       → dim surround:     F=0.9, c=0.59, Nc=0.9
//   light-ic   → average + increased contrast flag
//   dark-ic    → dim + increased contrast flag
//
// Фон bg_hex даёт яркость Yb (Y-компонент фона в % от D65-белого), которая
// подставляется в ViewingConditions::srgb_with_yb / dim_surround_with_yb.
// Это единственный канал влияния фона на legacy coordinate — через CAM16 J
// цвета под данным Yb+surround. `bg_hex + theme` не описывают полный visual context:
// отсутствуют spatial distribution, surround variance, geometry, observer и adaptation history.
//
// legacy `mud` и `drab` считаются на (l_app=J/100, C_oklab, h_oklab):
//   - C_oklab, h_oklab — Oklab-координаты исходного цвета (не меняются от surround)
//   - l_app = J/100 — CAM16 apparent lightness под фоном+темой, нормирован в [0,1]
//
// Это заменяет Oklab L в depth_term(l, h) (определяет положение цвета под cusp),
// делая координату чувствительной к локальным Yb+theme inputs. Это не полная
// appearance model и не observer-validated оценка; полный context owner — Issue #230.
//
// Ноль новых параметров: все VC-параметры — таблица CIECAM16 (Li et al. 2017).
// ─────────────────────────────────────────────────────────────────────────────

/// Compatibility theme input used to select the frozen CAM16 viewing conditions.
///
/// Соответствие CIECAM16 (Li et al. 2017, Table 1):
/// - `Light` → average surround (F=1.0, c=0.69, Nc=1.0)
/// - `Dark`  → dim surround    (F=0.9, c=0.59, Nc=0.9)
/// - `LightIc` / `DarkIc` — то же, но с флагом повышенного контраста
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// Light compatibility context: average surround (CIECAM16 Table 1).
    Light,
    /// Dark compatibility context: dim surround (CIECAM16 Table 1).
    Dark,
    /// Light compatibility context with the increased-contrast flag (IC).
    LightIc,
    /// Dark compatibility context with the increased-contrast flag (IC).
    DarkIc,
}

impl Theme {
    /// Разобрать стабильный kebab-контракт границы (`"light"` / `"dark"` /
    /// `"light-ic"` / `"dark-ic"`). Неизвестная строка — ошибка вызывающего,
    /// возвращается как есть (граница оборачивает в свой тип ошибки), никогда
    /// не коэрсится в тему по умолчанию.
    ///
    /// # Errors
    ///
    /// `Err` с непринятой строкой.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "light" => Ok(Theme::Light),
            "dark" => Ok(Theme::Dark),
            "light-ic" => Ok(Theme::LightIc),
            "dark-ic" => Ok(Theme::DarkIc),
            other => Err(other.to_string()),
        }
    }

    /// Стабильный kebab-ключ темы — обратная к [`parse`](Self::parse).
    pub fn key(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::LightIc => "light-ic",
            Theme::DarkIc => "dark-ic",
        }
    }

    /// Условия просмотра, под которыми ядро резолвит эту тему: та же карта
    /// surround-ов, что у `vc_for_context` (Light → average, Dark → dim,
    /// IC-темы → high-contrast двойники), но с дефолтным Yb — вход границы,
    /// где фон ещё неизвестен.
    pub fn viewing_conditions(self) -> crate::spaces::vc::ViewingConditions {
        self.vc_by(
            crate::spaces::vc::ViewingConditions::srgb,
            crate::spaces::vc::ViewingConditions::dim_surround,
        )
    }

    /// ЕДИНАЯ карта тема → surround: light-темы берут average-конструктор,
    /// dark-темы — dim, IC-темы поднимают флаг повышенного контраста. Обе
    /// точки входа (дефолтный Yb на границе, Yb-от-фона в контексте дефектов)
    /// обязаны выбирать surround здесь — вторая копия карты в цветовом коде
    /// расходилась бы тихо при добавлении темы. Эта карта не утверждает полноту
    /// perceptual context; она только сохраняет legacy boundary contract.
    fn vc_by(
        self,
        srgb: impl FnOnce() -> crate::spaces::vc::ViewingConditions,
        dim: impl FnOnce() -> crate::spaces::vc::ViewingConditions,
    ) -> crate::spaces::vc::ViewingConditions {
        let (mut vc, ic) = match self {
            Theme::Light => (srgb(), false),
            Theme::Dark => (dim(), false),
            Theme::LightIc => (srgb(), true),
            Theme::DarkIc => (dim(), true),
        };
        vc.high_contrast = vc.high_contrast || ic;
        vc
    }
}

/// Partial context input for the legacy experimental compatibility proxy.
///
/// Сочетает один фон (hex-строка, задаёт mean Yb) и тему (выбирает CAM16 surround).
/// Это не полный appearance context: variance, geometry, adaptation history and
/// observer state отсутствуют. Не использовать как human cleanliness verdict.
/// Передаётся в `muddiness_in_context` / `drab_in_context`.
#[derive(Debug, Clone, Copy)]
pub struct DefectContext<'a> {
    /// Фоновый цвет в hex (`#RRGGBB`). Задаёт яркость фона Yb для CIECAM16.
    pub bg_hex: &'a str,
    /// Тема просмотра.
    pub theme: Theme,
}

/// Y-компонент (относительная яркость) hex-цвета в % от D65-белого.
///
/// Формула: Y = 0.2126 R_lin + 0.7152 G_lin + 0.0722 B_lin (IEC 61966-2-1 D65),
/// затем умножаем на 100 для CIECAM16 (где Yb задаётся в %).
///
/// Диапазон результата: [0.0, 100.0].
fn y_pct_from_hex(hex: &str) -> Result<f64, String> {
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    // Строка srgb_from_hex возвращает ЛИНЕЙНЫЙ sRGB (без гаммы).
    // Y (IEC 61966-2-1, D65): Y = 0.2126 R + 0.7152 G + 0.0722 B
    let y = 0.212_639_005_871_510_27 * rgb[0]
        + 0.715_168_678_767_756 * rgb[1]
        + 0.072_192_315_360_733_71 * rgb[2];
    Ok(y * 100.0)
}

/// Viewing conditions для заданной темы и яркости фона Yb (в %).
///
/// Параметры surround — CIECAM16 Table 1 (Li et al. 2017). Ноль новых констант.
fn vc_for_context(theme: Theme, y_b_pct: f64) -> crate::spaces::vc::ViewingConditions {
    theme.vc_by(
        || crate::spaces::vc::ViewingConditions::srgb_with_yb(y_b_pct),
        || crate::spaces::vc::ViewingConditions::dim_surround_with_yb(y_b_pct),
    )
}

/// Compute the legacy experimental compatibility proxy with local Yb+theme inputs.
///
/// This is context-parameterized, not fully surround-aware: it does not model
/// spatial distribution, surround variance, geometry, adaptation history, or observers.
///
/// # Алгоритм
///
/// 1. Из `hex` получаем Oklab `(L, C, h)` для геометрии (C, h не зависят от surround).
/// 2. Из `ctx.bg_hex` получаем Yb — яркость фона в % от D65-белого.
/// 3. Строим `ViewingConditions` для темы `ctx.theme` с данным Yb (CIECAM16 Table 1).
/// 4. Из `hex` → XYZ → CAM16 `J` (apparent lightness под фоном+temой).
/// 5. `l_app = J / 100` — нормированный apparent lightness ∈ [0, 1].
/// 6. legacy proxy = `raw_chromatic(l_app, C_oklab, h_oklab)` — формула без изменений,
///    но `l_app` учитывает surround и фон вместо Oklab L.
///
/// # Провенанс
///
/// VC-параметры: CIECAM16, Li et al. 2017, DOI 10.1002/col.22131, Table 1.
/// Legacy formula is unchanged; provenance statuses are recorded in the empirical inventory.
pub fn muddiness_in_context(hex: &str, ctx: DefectContext<'_>) -> Result<f64, String> {
    // Oklab-координаты: C и h не зависят от viewing conditions
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
    let c_oklab = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
    let h_oklab = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);

    // Apparent lightness J через CAM16 под фоном+surround
    let y_b_pct = y_pct_from_hex(ctx.bg_hex)?;
    let vc = vc_for_context(ctx.theme, y_b_pct);
    let xyz = crate::spaces::srgb::srgb_to_xyz(rgb);
    let (j, _m, _h_cam) = crate::spaces::cam16::forward(xyz, &vc);
    let l_app = (j / 100.0).clamp(0.0, 1.0);

    Ok(raw_chromatic(l_app, c_oklab, h_oklab).clamp(0.0, 1.0))
}

/// Compute the historical `drab` compatibility coordinate while accepting the same context shape.
///
/// `drab(C) = 1 - N_pure(C)` depends only on Oklab chroma C. It is not an
/// observer-validated dullness estimate. Context is accepted only for API symmetry.
///
/// Returns the same value as `drab(C_oklab)`; local context inputs are ignored explicitly.
pub fn drab_in_context(hex: &str, ctx: DefectContext<'_>) -> Result<f64, String> {
    // Historical formula depends only on C_oklab; context is not part of this coordinate.
    let _ = ctx; // принимается для симметрии API
    let rgb = crate::spaces::srgb::srgb_from_hex(hex)?;
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
    let c_oklab = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
    Ok(drab(c_oklab))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Characterization golden test (Zone B slice 4, Fowler class B) ─────────
    //
    // Фиксирует таблицу legacy coordinate после замены W_HUE на Hanning-эвристику.
    // mud = N(C) * hue_weight(h) * depth_mod(L, C, h)
    //   где hue_weight(h) = (1 + cos(h - H_Y_DEG)) / 2
    //
    // Golden-значения получены из фактического кода после коммита 2 (Zone B slice 4)
    // и зафиксированы как regression anchor.  Tolerance: ±5e-6 (точность f64).
    //
    // Класс теста: B (characterization/golden, Фаулер) — якорь регрессии.
    // История: Zone B slice 3 (Platt-удаление) имела диапазон [0.00..0.14];
    //          Zone B slice 4 (Hanning-замена) расширила до [0.00..0.41].
    #[test]
    fn characterization_proxy_golden() {
        struct Case {
            hex: &'static str,
            expected: f64,
        }
        // Значения = muddiness_oklch(l,c,h) (frozen compatibility path).
        let cases = [
            Case {
                hex: "#6B6B2E",
                expected: 0.40640072,
            },
            Case {
                hex: "#937C00",
                expected: 0.33220155,
            },
            Case {
                hex: "#9AAE07",
                expected: 0.23449461,
            },
            Case {
                hex: "#9e6c00",
                expected: 0.29296999,
            },
            Case {
                hex: "#8f6424",
                expected: 0.30971375,
            },
            Case {
                hex: "#808080",
                expected: 0.00125988,
            },
            Case {
                hex: "#008080",
                expected: 0.00430967,
            },
            Case {
                hex: "#000080",
                expected: 0.00000000,
            },
            Case {
                hex: "#FF0000",
                expected: 0.00133634,
            },
            Case {
                hex: "#0000FF",
                expected: 0.00000000,
            },
        ];
        for c in &cases {
            let got = muddiness_from_hex(c.hex).unwrap();
            assert!(
                (got - c.expected).abs() < 5e-6,
                "characterization_proxy_golden: {}: got={:.8} expected={:.8} delta={:.2e}",
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

    // ─── Light-escape term removal guard (Zone B, Fowler class Д) ──────────────
    //
    // Проверяет, что `neutral_gate` — теперь 3-арная функция от (c, c0, jnd) —
    // не содержит скрытого escape-члена, зависящего от L (former M-03
    // DECLARED-CALIBRATION threshold). Закрывает класс «скрытый калибровочный
    // порог + протечка оси L в чисто-хроматический гейт».
    //
    // Стратегия (сильнее grep-по-имени): арность сигнатуры проверяется на
    // этапе КОМПИЛЯЦИИ (вызов ровно с 3 аргументами не скомпилируется, если
    // функция всё ещё принимает L/escape-параметр — RED был подтверждён до
    // правки: 5-арный вызов не собирался с 3 аргументами). Плюс узкий
    // grep-guard на паттерн употребления (`let escape`) и property-sweep,
    // доказывающий БИТОВОЕ равенство sigmoid((c-c0)/jnd) на широком диапазоне
    // C — если бы escape-член вернулся под другим именем, sweep поймал бы
    // отклонение на большинстве точек.
    //
    // RED на дереве с escape-членом: 3-арный вызов не компилировался (см.
    // историю коммита). GREEN после удаления: точное битовое равенство.
    #[test]
    fn light_escape_term_absent_from_shipping_code() {
        let source = include_str!("cleanliness.rs");
        let prod_code = source.split("#[cfg(test)]").next().unwrap_or(source);

        assert!(
            !prod_code.contains("let escape"),
            "`let escape` найден в продуктивном коде — escape-член neutral_gate не удалён."
        );

        // neutral_gate теперь чистый sigmoid gate: sigmoid((c-c0)/jnd), без L и без escape.
        // Sweep C от 0.0 до 1.0 (включая бледный тёплый диапазон, где раньше срабатывал
        // escape-член и подавлял cc) — каждая точка должна ТОЧНО совпадать с sigmoid.
        let steps = 1001usize;
        for i in 0..=steps {
            let c = (i as f64) / (steps as f64);
            let expected = sigmoid((c - C0) / JND);
            let got = neutral_gate(c, C0, JND);
            assert_eq!(
                got, expected,
                "neutral_gate(c={c:.6}, C0, JND) должен быть РОВНО sigmoid((c-C0)/JND) без \
                 escape-члена; got={got:.18} expected={expected:.18}"
            );
        }
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

    // ─── Zone B slice 4: W_HUE absent + Hanning invariants (TDD RED-first) ──
    //
    // Тесты закрывают класс дефектов: fitted vector вместо frozen parameter-free formula.
    //
    // (1) w_hue_fitted_vector_absent_from_shipping — RED: `pub static W_HUE` ещё
    //     существует в продуктивном коде (и hue_basis, hue_weight ещё опираются на него).
    //     GREEN после коммита 2: W_HUE убран, hue_weight переписан.
    //
    // (2) hanning_at_reference_hue_is_one — formula equals 1 at its frozen centre.
    //
    // (3) hanning_at_opposite_hue_is_zero — formula equals 0 at the opposite hue.
    //
    // (4) hanning_monotone_from_reference_hue — cosine decreases as |delta| grows.
    //
    // The invariants are mathematical only; they do not validate observer semantics.
    //
    // Arithmetic provenance:
    //   H_Y_DEG = 90.4011° — Oklab hue of the selected λ=578nm reference using
    //   CIE 1931 2° CMFs. Derivation (slice 5): linear interpolation of the 5nm table
    //   (575nm: 0.8425/0.9154/0.0018; 580nm: 0.9163/0.8700/0.00165) → (0.886780, 0.888160,
    //   0.001710), → XYZ/Y = (0.998446, 1, 0.001925), → Oklab напрямую (Ottosson 2020, M1/M2;
    //   hue инвариантен к радиансности, sRGB-клэмп не нужен и не hue-сохраняющ),
    //   → atan2(b, a) = 90.4011°. Неопределённость 5нм-интерполяции: ±0.01°
    //   (1нм-таблица CIE даёт 90.3925° — расхождение 0.009°, на 4 порядка ниже
    //   ширины Hanning-окна; центр объявлен по официальной 5нм-таблице).
    //   This arithmetic does not establish a universal unique-yellow observer point.
    //   The Hanning formula is a heuristic and is not derived from Bezold-Brucke.

    #[test]
    fn w_hue_fitted_vector_absent_from_shipping() {
        // RED пока pub static W_HUE присутствует в продуктивном коде.
        // GREEN после замены на frozen Hanning formula.
        let source = include_str!("cleanliness.rs");
        let prod_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(
            !prod_code.contains("pub static W_HUE"),
            "pub static W_HUE найден в продуктивном коде — подогнанный вектор (M-12) не удалён. \
             Ожидается frozen Hanning formula."
        );
        // Дополнительно: hue_basis (вспомогательная функция для dot-product) тоже должна уйти
        assert!(
            !prod_code.contains("fn hue_basis("),
            "fn hue_basis( найдена в продуктивном коде — K=3 Фурье-базис больше не нужен \
             после перехода на Hanning formula."
        );
    }

    #[test]
    fn hanning_at_reference_hue_is_one() {
        // Hanning-окно: (1 + cos(h − h_Y)) / 2 == 1.0 точно при h == h_Y.
        // The frozen formula guarantees (1 + cos(0)) / 2 == 1.0.
        //
        // H_Y_DEG is the selected arithmetic reference documented above.
        let hw = hue_weight(H_Y_DEG);
        assert_eq!(
            hw, 1.0,
            "hue_weight(H_Y_DEG={H_Y_DEG}) должен быть ровно 1.0 \
             (Hanning-окно: (1 + cos(0)) / 2 = 1.0 точно); получено {hw:.18}"
        );
    }

    #[test]
    fn hanning_at_opposite_hue_is_zero() {
        // Hanning-окно: (1 + cos(π)) / 2 == 0.0 ТОЧНО при h == h_Y + 180°.
        // The frozen formula guarantees zero at the opposite hue.
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
    fn hanning_monotone_from_reference_hue() {
        // Hanning-окно строго монотонно убывает при |δ| растёт от 0 до π.
        // (1 + cos(delta)) / 2 decreases on both sides of its reference hue.
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

    // ─── Zone G: local-context sensitivity tests (Fowler class A) ─────────────
    //
    // Characterization contract: changing the supplied Yb+theme inputs changes
    // CAM16 J and therefore the frozen proxy in the recorded directions.
    // This proves wiring and regression sensitivity, not human cleanliness truth
    // or completeness of the visual context model.
    //
    // Почему тест кусается (mutation-bite):
    //   Если заменить l_app = J/100 на l_app = Oklab-L (убрать local-context path),
    //   оба теста провалятся: без учёта Yb фона CAM16 J не меняется при смене bg_hex,
    //   поэтому muddiness_in_context вернёт одинаковое значение для обоих фонов.
    //
    // TDD RED-first: до добавления `muddiness_in_context` в файл эти тесты не
    // компилировались (функция не существовала) — RED доказан структурно.
    //
    // CAM16 viewing-condition parameters: Li et al. 2017 Table 1. Legacy proxy
    // parameter statuses are separate and recorded in the empirical inventory.

    use super::{DefectContext, Theme, drab_in_context, muddiness_in_context};

    /// The frozen proxy is higher for #808080 with the supplied pastel background.
    ///
    /// In this partial model, #FFE4E1 supplies a higher Yb than #808080, changing
    /// CAM16 J and the geometric depth input. No observer judgement is asserted.
    ///
    /// Направление дельты: mud_on_pastel > mud_on_neutral — строго.
    #[test]
    fn proxy_for_grey_is_higher_with_pastel_yb_than_neutral_yb() {
        let grey = "#808080";
        let pastel_bg = "#FFE4E1"; // розово-белёсый, Yb≈82%
        let neutral_bg = "#808080"; // нейтральный серый, Yb≈22%

        let mud_on_pastel = muddiness_in_context(
            grey,
            DefectContext {
                bg_hex: pastel_bg,
                theme: Theme::Light,
            },
        )
        .unwrap();

        let mud_on_neutral = muddiness_in_context(
            grey,
            DefectContext {
                bg_hex: neutral_bg,
                theme: Theme::Light,
            },
        )
        .unwrap();

        assert!(
            mud_on_pastel > mud_on_neutral,
            "local-context proxy ordering changed: pastel={mud_on_pastel:.6} \
             must be > neutral={mud_on_neutral:.6}; equality means Yb no longer reaches CAM16 J"
        );
    }

    /// The frozen proxy is lower for #C2185B with black than with white inputs.
    ///
    /// In this partial model the two Yb+theme inputs produce different CAM16 J
    /// and geometric depth values. No human clean/dirty direction is asserted.
    ///
    /// Направление дельты: mud_on_black < mud_on_white — строго.
    #[test]
    fn proxy_for_dark_pink_is_lower_with_black_input_than_white_input() {
        let dark_pink = "#C2185B"; // тёмно-розовый (Material Design Pink 800)
        let black_bg = "#000000";
        let white_bg = "#FFFFFF";

        let mud_on_black = muddiness_in_context(
            dark_pink,
            DefectContext {
                bg_hex: black_bg,
                theme: Theme::Dark,
            },
        )
        .unwrap();

        let mud_on_white = muddiness_in_context(
            dark_pink,
            DefectContext {
                bg_hex: white_bg,
                theme: Theme::Light,
            },
        )
        .unwrap();

        assert!(
            mud_on_black < mud_on_white,
            "local-context proxy ordering changed: black={mud_on_black:.6} \
             must be < white={mud_on_white:.6}; equality means context no longer reaches the formula"
        );
    }

    /// Mutation-bite: replacing l_app with a fixed value removes local-context sensitivity
    /// должна РОНЯТЬ первый кейс-тест. Проверяем здесь что тест различает два значения.
    ///
    /// Реализация: вычисляем mud дважды — с пастельным и нейтральным фоном.
    /// Разница строго ненулевая → epsilon-тест подтверждает укус.
    #[test]
    fn compatibility_proxy_differs_across_local_background_inputs() {
        let grey = "#808080";
        let pastel_bg = "#FFE4E1";
        let neutral_bg = "#808080";

        let mud_pastel = muddiness_in_context(
            grey,
            DefectContext {
                bg_hex: pastel_bg,
                theme: Theme::Light,
            },
        )
        .unwrap();
        let mud_neutral = muddiness_in_context(
            grey,
            DefectContext {
                bg_hex: neutral_bg,
                theme: Theme::Light,
            },
        )
        .unwrap();

        // Разница должна быть не менее 1e-4 — иначе тест не кусается
        assert!(
            (mud_pastel - mud_neutral).abs() > 1e-4,
            "local-context proxy delta < 1e-4: pastel={mud_pastel:.8} neutral={mud_neutral:.8} \
             delta={:.2e} — тест не кусается (mutation-bite провален).",
            (mud_pastel - mud_neutral).abs()
        );
    }

    /// API-test: `drab_in_context` preserves the context-independent legacy coordinate.
    ///
    /// The boundary accepts context for compatibility, ignores it explicitly, and must not
    /// fabricate zero instead of returning the historical arithmetic result.
    #[test]
    fn drab_in_context_matches_bare_drab() {
        use super::n_pure;
        let hex = "#937C00"; // frozen conformance fixture

        let ctx_light = DefectContext {
            bg_hex: "#FFFFFF",
            theme: Theme::Light,
        };
        let ctx_dark = DefectContext {
            bg_hex: "#000000",
            theme: Theme::Dark,
        };

        let d_light = drab_in_context(hex, ctx_light).unwrap();
        let d_dark = drab_in_context(hex, ctx_dark).unwrap();

        // drab зависит только от C_oklab — должно быть идентично для обоих контекстов
        assert_eq!(
            d_light, d_dark,
            "drab_in_context должен возвращать одинаковый результат \
             независимо от контекста (drab зависит только от C_oklab): \
             light={d_light:.8} dark={d_dark:.8}"
        );

        // Проверяем, что D + N = 1 выполняется и через context-путь
        let rgb = crate::spaces::srgb::srgb_from_hex(hex).unwrap();
        let lab = crate::spaces::oklab::srgb_linear_to_oklab(rgb);
        let c_oklab = (lab[1].powi(2) + lab[2].powi(2)).sqrt();
        assert_eq!(
            d_light + n_pure(c_oklab),
            1.0,
            "drab_in_context({hex}) + n_pure должно быть ровно 1.0 (D+N=1 инвариант)"
        );
        // For this fixture C >> C0, so the frozen arithmetic complement is < 0.1.
        assert!(
            d_light < 0.1,
            "drab_in_context({hex}) = {d_light:.6} ожидается < 0.1 \
             (fixture C >> C0, so the compatibility coordinate approaches 0)"
        );
    }

    /// IC-тема компилируется и возвращает корректный результат.
    #[test]
    fn ic_themes_compile_and_return_finite_value() {
        let hex = "#6B6B2E"; // frozen conformance fixture
        let ctx_lic = DefectContext {
            bg_hex: "#FFFFFF",
            theme: Theme::LightIc,
        };
        let ctx_dic = DefectContext {
            bg_hex: "#000000",
            theme: Theme::DarkIc,
        };

        let m_lic = muddiness_in_context(hex, ctx_lic).unwrap();
        let m_dic = muddiness_in_context(hex, ctx_dic).unwrap();

        assert!(
            m_lic.is_finite() && (0.0..=1.0).contains(&m_lic),
            "muddiness_in_context(LightIc) = {m_lic} вне [0,1]"
        );
        assert!(
            m_dic.is_finite() && (0.0..=1.0).contains(&m_dic),
            "muddiness_in_context(DarkIc) = {m_dic} вне [0,1]"
        );
    }
}
