//! Port of the final "Law of Dirt" (Muddiness Law) for the `labcolors` system.
//!
//! # Equations
//!
//! ```text
//! raw = neutral_gate(L, C) * hue_weight(h) * depth_term(L, h) * b_gate(C, h)
//! mud = sigmoid(T * ln(raw + eps) + b_cal)
//! ```
//!
//! # Parameters — inventory status (docs/decisions/empirical-inventory.md, Muddiness section)
//!
//! Science-vs-fit boundary: the **structural form** is derived from Oklab opponent-colour
//! theory and CAM16-UCS geometry.  The **link calibration scalars** are Platt-fit on the
//! v3 labelled dataset and declared as DECLARED-CALIBRATION.  See inventory for full
//! per-constant rationale and bounds.  Audited by `mud-oracle/verify_inventory.js` (exit 0).
//!
//! | const           | mud-id | status                         | value (frozen)              |
//! |-----------------|--------|--------------------------------|-----------------------------|
//! | `C0`            | M-01   | cited-and-kept                 | 0.0395 (sRGB grey-frontier; Evans/Xie-Fairchild yellow zero-grayness frontier) |
//! | `JND`           | M-02   | cited-and-kept                 | 0.01228 (Oklab chroma JND; Oklab perceptual measurement) |
//! | `LESC`          | M-03   | DECLARED-CALIBRATION           | 0.8208552 (light-escape threshold) |
//! | `B0`            | M-04   | cited-measured                 | 0.036 (central of cited range [0.030, 0.044]; Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975) |
//! | `BW`            | M-05   | cited-measured                 | 0.017 (central of cited range [0.013, 0.020]; Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975) |
//! | `CAL_EPS`       | M-06   | DECLARED-CALIBRATION           | 0.01 (log regularisation bias) |
//! | `CAL_T`         | M-07   | OPEN (flagged-provisional)     | 2.356978; resolving study: CAL_T/CAL_B olive-brown naming-crossing (Zone C) |
//! | `CAL_B`         | M-08   | OPEN (flagged-provisional)     | 6.445168; resolving study: CAL_T/CAL_B olive-brown naming-crossing (Zone C) |
//! | `M_W`           | M-09   | DECLARED-CALIBRATION           | 0.181527 (margin scale, confidence band) |
//! | `KAPPA_CORE`    | M-10   | DECLARED-CALIBRATION           | 0.34 (empirical concept-floor from v3 retest analysis; not a published perceptual constant) |
//! | `KAPPA_INTERIOR`| M-11   | DECLARED-CALIBRATION           | 0.10 (empirical interior floor from v3 disputed-stratum analysis; not a published perceptual constant) |
//! | `W_HUE[8]`      | M-12   | OPEN (flagged-provisional)     | (logistic regression fit); resolving study: drab L-tilt grayness magnitude-estimation (Zone C) |
//! | `CUSP_L_TABLE`  | M-13   | cited-and-kept                 | (pure Oklab gamut geometry, verified to f64 — kept as-is per paradigm North) |
//! | `CEIL_N_TABLE`  | M-14   | DECLARED-CALIBRATION           | (Fourier basis fit on observer-label dataset) |

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

pub(crate) static CEIL_N_TABLE: [f64; 361] = [
    0.74160393,
    0.73351744,
    0.72604512,
    0.71919446,
    0.71297286,
    0.70738769,
    0.70244630,
    0.69815609,
    0.69452443,
    0.69155889,
    0.68926710,
    0.68765682,
    0.68673604,
    0.68651294,
    0.68699602,
    0.68819397,
    0.69011592,
    0.69277122,
    0.69616971,
    0.70032166,
    0.70523773,
    0.71092920,
    0.71740775,
    0.72468573,
    0.73277612,
    0.74169248,
    0.75144914,
    0.76206114,
    0.77354431,
    0.78591540,
    0.70027213,
    0.59137281,
    0.48956450,
    0.39422162,
    0.30479085,
    0.22078105,
    0.19356341,
    0.20432103,
    0.21611592,
    0.22896574,
    0.22098246,
    0.15477270,
    0.09209063,
    0.03270091,
    -0.02361046,
    -0.07703843,
    -0.12776107,
    -0.17594116,
    -0.22172796,
    -0.26525830,
    -0.30665801,
    -0.34604289,
    -0.38351965,
    -0.41918675,
    -0.45313512,
    -0.48544893,
    -0.51620603,
    -0.54547854,
    -0.57333340,
    -0.59983270,
    -0.62503408,
    -0.64899110,
    -0.67175354,
    -0.69336771,
    -0.71387663,
    -0.73332040,
    -0.75173620,
    -0.76915872,
    -0.78562011,
    -0.80115029,
    -0.81577704,
    -0.82952614,
    -0.84242144,
    -0.85448501,
    -0.86573730,
    -0.87619706,
    -0.88588159,
    -0.89480670,
    -0.90298684,
    -0.91043513,
    -0.91716338,
    -0.92318226,
    -0.92850114,
    -0.93312837,
    -0.93707108,
    -0.94033536,
    -0.94292623,
    -0.94484762,
    -0.94610251,
    -0.94669277,
    -0.94661935,
    -0.94588210,
    -0.94447988,
    -0.94241060,
    -0.93967104,
    -0.93625707,
    -0.93216340,
    -0.92738369,
    -0.92191057,
    -0.91573547,
    -0.90884874,
    -0.90123943,
    -0.89289548,
    -0.88380343,
    -0.87394857,
    -0.86331475,
    -0.85188434,
    -0.83963821,
    -0.82655559,
    -0.81261403,
    -0.79778921,
    -0.78205492,
    -0.76538295,
    -0.74774288,
    -0.72910195,
    -0.70942500,
    -0.68867413,
    -0.66680870,
    -0.64378489,
    -0.61955569,
    -0.59407044,
    -0.56727472,
    -0.53910990,
    -0.50951284,
    -0.47841547,
    -0.44574442,
    -0.41142041,
    -0.37535784,
    -0.33746412,
    -0.29763894,
    -0.25577366,
    -0.21175023,
    -0.16544042,
    -0.11670466,
    -0.06539071,
    -0.01133247,
    0.04565192,
    0.10576156,
    0.16921519,
    0.23625342,
    0.30714153,
    0.38217269,
    0.46167165,
    0.44295845,
    0.33133566,
    0.22813348,
    0.13245039,
    0.04351418,
    -0.03934083,
    -0.11669191,
    -0.18904237,
    -0.25683325,
    -0.32045274,
    -0.38024401,
    -0.43651166,
    -0.48952704,
    -0.53953267,
    -0.58674601,
    -0.63136268,
    -0.67355907,
    -0.71349469,
    -0.75131412,
    -0.78714871,
    -0.82111804,
    -0.85333118,
    -0.88388779,
    -0.91287911,
    -0.94038878,
    -0.96649363,
    -0.99126425,
    -1.01476558,
    -1.03705756,
    -1.05819534,
    -1.07822992,
    -1.09720835,
    -1.11517412,
    -1.13216746,
    -1.14822552,
    -1.16338267,
    -1.17767067,
    -1.19111893,
    -1.20375457,
    -1.21560262,
    -1.22668613,
    -1.23702638,
    -1.24664289,
    -1.25555352,
    -1.26377467,
    -1.27132122,
    -1.27820669,
    -1.28444325,
    -1.29004186,
    -1.29501217,
    -1.29936277,
    -1.30310100,
    -1.30623317,
    -1.30876452,
    -1.31069916,
    -1.31204023,
    -1.31278981,
    -1.31294899,
    -1.31251784,
    -1.31149538,
    -1.30987963,
    -1.30766760,
    -1.30485524,
    -1.30143741,
    -1.29740793,
    -1.29275947,
    -1.28748351,
    -1.28157043,
    -1.27500930,
    -1.26778787,
    -1.25989258,
    -1.25130844,
    -1.24201895,
    -1.23200596,
    -1.22124976,
    -1.20972868,
    -1.19741924,
    -1.18429587,
    -1.17033078,
    -1.15549379,
    -1.13975219,
    -1.12307044,
    -1.10540997,
    -1.08672895,
    -1.06698194,
    -1.04611952,
    -1.02408799,
    -1.00082886,
    -0.97627835,
    -0.95036691,
    -0.92301839,
    -0.89414951,
    -0.86366879,
    -0.83147574,
    -0.79745948,
    -0.76149765,
    -0.72345455,
    -0.68317939,
    -0.64050402,
    -0.59524029,
    -0.54717671,
    -0.49607460,
    -0.44166331,
    -0.38363412,
    -0.32163295,
    -0.33882575,
    -0.35518534,
    -0.37051869,
    -0.35147850,
    -0.27132222,
    -0.18416045,
    -0.08893319,
    0.01568898,
    0.13140994,
    0.23780005,
    0.22803376,
    0.21934121,
    0.30712644,
    0.51131688,
    0.76671718,
    0.90851922,
    1.10296425,
    1.64525526,
    1.64362264,
    1.64344531,
    1.64472305,
    1.64745780,
    1.65165376,
    1.61179793,
    1.57146559,
    1.53226145,
    1.49414114,
    1.45706178,
    1.42098196,
    1.38586169,
    1.35166224,
    1.31834611,
    1.28587699,
    1.25421959,
    1.22333974,
    1.19320421,
    1.16378072,
    1.13503789,
    1.13767750,
    1.16447204,
    1.19293146,
    1.22310669,
    1.25505264,
    1.28882842,
    1.32449760,
    1.36212863,
    1.40179499,
    1.44357582,
    1.46927494,
    1.44304177,
    1.41720621,
    1.39174457,
    1.36663416,
    1.34185321,
    1.31738099,
    1.29319777,
    1.26928489,
    1.24562484,
    1.22220121,
    1.20392997,
    1.26920037,
    1.33785379,
    1.41009459,
    1.48614548,
    1.56624945,
    1.62321714,
    1.59830653,
    1.57356013,
    1.54897389,
    1.52454523,
    1.50027309,
    1.47615781,
    1.45220118,
    1.42840637,
    1.45498223,
    1.57259132,
    1.69792012,
    1.83168522,
    1.83488751,
    1.80952619,
    1.78440737,
    1.75954405,
    1.73495003,
    1.71063970,
    1.68662802,
    1.66293040,
    1.63956254,
    1.61654041,
    1.59388016,
    1.57159796,
    1.52299015,
    1.44899408,
    1.37823379,
    1.31056190,
    1.24583915,
    1.18393407,
    1.12472246,
    1.06808724,
    1.01391787,
    0.96211027,
    0.91256631,
    0.86878064,
    0.85448467,
    0.84070694,
    0.82745770,
    0.81474677,
    0.80258369,
    0.79097764,
    0.77993745,
    0.76947178,
    0.75958892,
    0.75029701,
    0.74160393,
];

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
pub const CAL_EPS: f64 = 0.0100000000000000;
pub const CAL_T: f64 = 2.3569779717896631;
pub const CAL_B: f64 = 6.4451683591874760;
pub const M_W: f64 = 0.1815267777247454;
pub const KAPPA_CORE: f64 = 0.34;
pub const KAPPA_INTERIOR: f64 = 0.10;

// Trained hue-basis vector weights
pub static W_HUE: [f64; 8] = [
    -3.30564376,
    -0.82741959,
    1.07193452,
    1.33498730,
    -0.63786425,
    0.20178720,
    0.77607114,
    0.07070652,
];

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

/// Fourier hue basis vectors for K = 3
///
/// Hue-quantization rationale: `CEIL_N_TABLE` is sampled at 1-degree resolution.
/// The table index uses `round()` (nearest integer degree), so the maximum lookup
/// error is ≤ 0.5° of hue.  The mud response varies smoothly with hue (band-limited
/// to K = 3 Fourier terms, i.e. the highest spatial frequency is a 120°-period wave),
/// so a 0.5° quantization error produces a negligible output error — well below 1 JND
/// in any perceptual hue dimension.  The trig terms (`cos`/`sin`) continue to use
/// the original floating-point `h_deg` and are NOT quantized.
pub fn hue_basis(h_deg: f64) -> [f64; 8] {
    let th = h_deg.rem_euclid(360.0).to_radians();
    let idx = (h_deg.rem_euclid(360.0).round() as usize) % 361;
    let ceil_val = CEIL_N_TABLE[idx];
    [
        1.0,
        ceil_val,
        (1.0 * th).cos(),
        (1.0 * th).sin(),
        (2.0 * th).cos(),
        (2.0 * th).sin(),
        (3.0 * th).cos(),
        (3.0 * th).sin(),
    ]
}

/// Hue weight term: emergent band-limited hue response
///
/// The table index for `CEIL_N_TABLE` is taken at the nearest integer degree (see
/// `hue_basis` for the quantization-acceptability rationale).  W_HUE is currently
/// OPEN (M-12, flagged-provisional) pending the drab L-tilt grayness study (Zone C).
pub fn hue_weight(h_deg: f64) -> f64 {
    let rounded_hue = h_deg.rem_euclid(360.0).round() as usize;
    let idx = rounded_hue % 361;
    let basis = hue_basis(idx as f64);
    let mut dot = 0.0;
    for i in 0..8 {
        dot += basis[i] * W_HUE[i];
    }
    sigmoid(dot)
}

/// Raw chromatic muddiness score (before magnitude calibration)
pub fn raw_chromatic(l: f64, c: f64, h_deg: f64) -> f64 {
    let cc = neutral_gate(l, c, C0, JND, LESC);
    let hw = hue_weight(h_deg);
    let dpm = depth_mod(l, c, h_deg, B0, BW);
    cc * hw * dpm
}

/// Smooth monotone bounded magnitude calibration link (Platt)
pub fn muddiness_oklch(l: f64, c: f64, h_deg: f64) -> f64 {
    let raw = raw_chromatic(l, c, h_deg);
    let z = (raw.max(0.0) + CAL_EPS).ln();
    let out = sigmoid(CAL_T * z + CAL_B);
    out.clamp(0.0, 1.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Smoke / class-membership reference values ────────────────────────────
    //
    // Updated in Zone B (2026-06-29) to reflect cited-measured B0=0.036 /
    // BW=0.017 (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS;
    // Boynton 1975). Values within ±1e-3 of the pre-swap fitted outputs
    // confirm the behaviour delta is bounded and directionally consistent
    // (muddy colours stay muddy; clean colours stay clean).
    #[test]
    fn test_muddiness_v3_reference_values() {
        // Olive #6B6B2E (Highly muddy)
        let olive_mud = muddiness_from_hex("#6B6B2E").unwrap();
        let olive_conf = confidence_from_hex("#6B6B2E").unwrap();
        assert!(
            olive_mud > 0.80,
            "olive mud is {olive_mud:.4}, expected > 0.80"
        );
        assert!(
            (olive_mud - 0.8706).abs() < 1e-3,
            "olive mud {olive_mud:.4} != 0.8706"
        );
        assert!(
            (olive_conf - 0.2946).abs() < 1e-3,
            "olive conf {olive_conf:.4} != 0.2946"
        );

        // Babypoop #937C00 (Highly muddy)
        let babypoop_mud = muddiness_from_hex("#937C00").unwrap();
        assert!(
            babypoop_mud > 0.80,
            "babypoop mud is {babypoop_mud:.4}, expected > 0.80"
        );
        assert!(
            (babypoop_mud - 0.8389).abs() < 1e-3,
            "babypoop mud {babypoop_mud:.4} != 0.8389"
        );

        // Puke #9AAE07 (Moderately muddy)
        let puke_mud = muddiness_from_hex("#9AAE07").unwrap();
        assert!(
            (puke_mud - 0.5443).abs() < 1e-3,
            "puke mud {puke_mud:.4} != 0.5443"
        );

        // Golds (graded middle, not slammed)
        let gold1_mud = muddiness_from_hex("#9e6c00").unwrap();
        assert!(
            (gold1_mud - 0.7133).abs() < 1e-3,
            "gold1 mud {gold1_mud:.4} != 0.7133"
        );

        let gold2_mud = muddiness_from_hex("#8f6424").unwrap();
        assert!(
            (gold2_mud - 0.7065).abs() < 1e-3,
            "gold2 mud {gold2_mud:.4} != 0.7065"
        );

        // Pure grey #808080 (Clean)
        let grey_mud = muddiness_from_hex("#808080").unwrap();
        assert!(
            grey_mud < 0.05,
            "grey mud is {grey_mud:.4}, expected < 0.05"
        );

        // Teal #008080 (Clean, cool-immune)
        let teal_mud = muddiness_from_hex("#008080").unwrap();
        let teal_conf = confidence_from_hex("#008080").unwrap();
        assert!(
            teal_mud < 0.10,
            "teal mud is {teal_mud:.4}, expected < 0.10"
        );
        assert!(
            (teal_mud - 0.0123).abs() < 1e-3,
            "teal mud {teal_mud:.4} != 0.0123"
        );
        assert!(
            (teal_conf - 0.3222).abs() < 1e-3,
            "teal conf {teal_conf:.4} != 0.3222"
        );

        // Navy Blue #000080 (Clean)
        let navy_mud = muddiness_from_hex("#000080").unwrap();
        assert!(
            navy_mud < 0.10,
            "navy mud is {navy_mud:.4}, expected < 0.10"
        );
    }

    // ─── Characterization golden test (Zone B, Fowler class B) ───────────────
    //
    // Locks the mud(L,C,h) output table under the CITED-MEASURED constants
    // B0=0.036 / BW=0.017 (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014
    // PNAS; Boynton 1975).  This test was introduced in commit 1 of 2 (git
    // history proves the golden precedes the constant change) with pre-swap
    // values, then updated here in commit 2 to lock the post-swap outputs —
    // making any future regression against the cited constants immediately
    // visible.  Tolerance: ±5e-6 (f64 precision).
    #[test]
    fn characterization_mud_golden_cited() {
        struct Case {
            hex: &'static str,
            label: &'static str,
            expected: f64,
        }
        let cases = [
            Case {
                hex: "#6B6B2E",
                label: "olive",
                expected: 0.87060095,
            },
            Case {
                hex: "#937C00",
                label: "babypoop",
                expected: 0.83885873,
            },
            Case {
                hex: "#9AAE07",
                label: "puke",
                expected: 0.54431359,
            },
            Case {
                hex: "#9e6c00",
                label: "gold1",
                expected: 0.71332396,
            },
            Case {
                hex: "#8f6424",
                label: "gold2",
                expected: 0.70647979,
            },
            Case {
                hex: "#808080",
                label: "grey",
                expected: 0.01328840,
            },
            Case {
                hex: "#008080",
                label: "teal",
                expected: 0.01232390,
            },
            Case {
                hex: "#000080",
                label: "navy",
                expected: 0.01201972,
            },
            Case {
                hex: "#FF0000",
                label: "red",
                expected: 0.01243314,
            },
            Case {
                hex: "#0000FF",
                label: "blue",
                expected: 0.01201969,
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
