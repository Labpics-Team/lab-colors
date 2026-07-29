//! Viewing conditions (surround parameters) for the CIECAM16 forward pass.
//!
//! Holds model viewing-condition inputs — adapting luminance, background factor,
//! and the surround triplet `(F, c, N_c)` — that [`crate::spaces::cam16`]
//! consumes. The surround presets are CIECAM16 Table 1 (average
//! `[1.0, 0.69, 1.0]`, dim `[0.9, 0.59, 0.9]`), as tabulated in Li et al.
//! 2017, DOI [10.1002/col.22131](https://doi.org/10.1002/col.22131) and
//! CIE 248:2022. Values are transcribed directly into the source; there is no
//! runtime dependency on a colour-science crate.

use crate::spaces::srgb::D65_WHITE;
// `OnceLock` кеширует фингерпринты пресетов только внутри `preset_index`, который
// с главы #64 стал test-only (единственный продакшн-потребитель, grey-axis LUT,
// удалён) — потому импорт под тем же `#[cfg(test)]`.
#[cfg(test)]
use std::sync::OnceLock;

use super::{cam16::adapt, cat16::xyz_to_cone};

// Именованные коэффициенты модели общие для production-математики и test-only
// exact-real binding ниже. Константы компилируются без runtime-цены, а имена не
// позволяют копии формулы получить второй независимо изменяемый набор чисел.
const ZERO: f64 = 0.0;
const ONE: f64 = 1.0;
const TWO: f64 = 2.0;
const FIVE: f64 = 5.0;
const TWENTY: f64 = 20.0;
const FORTY_TWO: f64 = 42.0;
const NINETY_TWO: f64 = 92.0;
const HUNDRED: f64 = 100.0;
const FL_INDIRECT_SCALE: f64 = 0.1;
const NBB_EXPONENT_MAGNITUDE: f64 = 0.2;
const FL_QUARTER_EXPONENT: f64 = 0.25;
const T_INNER_POWER_BASE: f64 = 0.29;
const SURROUND_DARK_C: f64 = 0.525;
const SURROUND_DIM_C: f64 = 0.59;
const SURROUND_AVERAGE_C: f64 = 0.69;
const NBB_SCALE: f64 = 0.725;
const T_INNER_EXPONENT: f64 = 0.73;
const SURROUND_DARK_F_NC: f64 = 0.8;
const SURROUND_DIM_F_NC: f64 = 0.9;
const Z_OFFSET: f64 = 1.48;
const T_INNER_OFFSET: f64 = 1.64;
const ADAPTATION_DECAY_DIVISOR: f64 = 3.6;

/// Точные числовые владельцы, используемые artifact-ом contextual-region.
///
/// Повторяющиеся имена намеренно сверяют общие с CAM16 коэффициенты: потребитель
/// registry отвергает владельцев, которые разошлись хотя бы одним битом.
#[cfg(test)]
pub(crate) fn contextual_region_formula_literals_v1() -> &'static [(&'static str, f64)] {
    &[
        ("one", ONE),
        ("p0_1", FL_INDIRECT_SCALE),
        ("p0_2", NBB_EXPONENT_MAGNITUDE),
        ("p0_25", FL_QUARTER_EXPONENT),
        ("p0_29", T_INNER_POWER_BASE),
        ("p0_525", SURROUND_DARK_C),
        ("p0_59", SURROUND_DIM_C),
        ("p0_69", SURROUND_AVERAGE_C),
        ("p0_725", NBB_SCALE),
        ("p0_73", T_INNER_EXPONENT),
        ("p0_8", SURROUND_DARK_F_NC),
        ("p0_9", SURROUND_DIM_F_NC),
        ("p1_48", Z_OFFSET),
        ("p1_64", T_INNER_OFFSET),
        ("two", TWO),
        ("p3_6", ADAPTATION_DECAY_DIVISOR),
        ("five", FIVE),
        ("twenty", TWENTY),
        ("forty_two", FORTY_TWO),
        ("ninety_two", NINETY_TWO),
        ("hundred", HUNDRED),
    ]
}

/// Closed CIECAM16 surround tuple admitted by the F0 occurrence context.
///
/// Keeping the triplets behind variants prevents callers from independently
/// combining `F`, `c` and `N_c` into a context the release never registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cam16SurroundV1 {
    Average,
    Dim,
    Dark,
}

/// Условия просмотра для модели цветового восприятия CIECAM16.
///
/// Значения по умолчанию соответствуют sRGB: D65, серый фон 20 %, среднее
/// окружение, без discounting.
///
/// Производные поля образуют единое проверенное состояние и не изменяются
/// клиентом по отдельности:
///
/// ```compile_fail
/// use labcolors_core::ViewingConditions;
/// let mut vc = ViewingConditions::srgb();
/// vc.fl = f64::NAN;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ViewingConditions {
    /// Background luminance factor (Yb / Yw).
    pub(crate) n: f64,
    /// Achromatic response to the reference white.
    pub(crate) aw: f64,
    /// Chromatic induction factor.
    pub(crate) nbb: f64,
    pub(crate) ncb: f64,
    /// Luminance-level adaptation factor.
    pub(crate) fl: f64,
    /// Base exponential nonlinearity.
    pub(crate) z: f64,
    /// Degree of chromatic adaptation.
    pub(crate) c: f64,
    /// Chromatic induction factor.
    pub(crate) nc: f64,
    /// RGB discounting factors.
    pub(crate) rgb_d: [f64; 3],
    /// Whether these conditions enforce increased contrast (IC).
    pub(crate) high_contrast: bool,
    /// Предвычисленный `F_L^0.25`. Пер-VC константа, которую прямой ход CIECAM16
    /// (множитель колорфулнесс `M`) и H-K-хрома иначе пересчитывали бы на КАЖДЫЙ
    /// цвет, хотя она зависит только от условий просмотра — фиксированных на все
    /// сотни forward'ов одного резолва. Вынесена сюда единожды (в `build`).
    /// Байт-идентична инлайновому `fl.powf(0.25)`, который заменяет: тот же
    /// libm-вызов на том же операнде, под гейтом bit-identity оракула
    /// `cam16::forward`. Производное состояние (чистая функция `fl`),
    /// синхронизируется только через `build`.
    pub(crate) fl_pow_025: f64,
    /// Предвычисленный `(1.64 − 0.29^n)^0.73` — пер-VC префактор колорфулнесс,
    /// общий для прямого `M` и обратного `t`. Хранится, а не перепечатывается на
    /// каждый цвет, по той же причине и под тем же гейтом bit-identity, что и
    /// [`fl_pow_025`](Self::fl_pow_025). Производное состояние (чистая функция
    /// `n`), синхронизируется только через `build`.
    pub(crate) t_inner: f64,
}

impl Default for ViewingConditions {
    fn default() -> Self {
        Self::srgb()
    }
}

impl ViewingConditions {
    /// Build CAM16 derived constants from immutable semantic context inputs.
    ///
    /// `background_luminance_ratio` is `Y_b / Y_w`, not a percentage. The
    /// existing builder consumes percent, so the conversion remains here at
    /// the legacy-kernel boundary rather than leaking into the occurrence
    /// domain. Admission of the numeric inputs is owned by `AppearanceContext`.
    pub(crate) fn from_semantic_inputs_v1(
        adapting_luminance_cd_m2: f64,
        background_luminance_ratio: f64,
        surround: Cam16SurroundV1,
    ) -> Self {
        let (f, c, nc) = match surround {
            Cam16SurroundV1::Average => (ONE, SURROUND_AVERAGE_C, ONE),
            Cam16SurroundV1::Dim => (SURROUND_DIM_F_NC, SURROUND_DIM_C, SURROUND_DIM_F_NC),
            Cam16SurroundV1::Dark => (SURROUND_DARK_F_NC, SURROUND_DARK_C, SURROUND_DARK_F_NC),
        };
        Self::build(
            adapting_luminance_cd_m2,
            background_luminance_ratio * HUNDRED,
            f,
            c,
            nc,
        )
    }

    /// Коэффициент фоновой яркости CAM16: `n = Y_b / Y_w`.
    pub fn n(&self) -> f64 {
        self.n
    }

    /// Ахроматический отклик эталонного белого.
    pub fn aw(&self) -> f64 {
        self.aw
    }

    /// Коэффициент хроматической индукции `N_bb`.
    pub fn nbb(&self) -> f64 {
        self.nbb
    }

    /// Коэффициент адаптации к уровню яркости `F_L`.
    pub fn fl(&self) -> f64 {
        self.fl
    }

    /// Базовый экспоненциальный член CAM16 `z`.
    pub fn z(&self) -> f64 {
        self.z
    }

    /// Коэффициент хроматической адаптации окружения `c`.
    pub fn c(&self) -> f64 {
        self.c
    }

    /// Коэффициент хроматической индукции окружения `N_c`.
    pub fn nc(&self) -> f64 {
        self.nc
    }

    /// RGB-коэффициенты discounting, выведенные из полных условий просмотра.
    pub fn rgb_d(&self) -> [f64; 3] {
        self.rgb_d
    }

    /// Требует ли пресет контракты повышенного контраста.
    pub fn is_high_contrast(&self) -> bool {
        self.high_contrast
    }

    /// Standard sRGB viewing conditions (average surround).
    ///
    /// Parameters: D65 illuminant, L_A = 64 cd/m², Y_b = 20 %,
    /// average surround (F = 1.0, c = 0.69, N_c = 1.0).
    ///
    /// The surround triplet `(F, c, N_c)` is CIE 159:2004 Table 1 (carried
    /// unchanged into CAM16; CIE 248:2022) and matches colorjs.io
    /// `surroundMap["average"]`. The adapting luminance does NOT match
    /// colorjs.io, whose default is `(64/π)·0.2 ≈ 4.07 cd/m²`: lab-colors
    /// deliberately uses L_A = 64. That choice is DECLARED, not canonical — the
    /// standards fix no single adapting luminance for display viewing; deriving
    /// the value or sweeping its sensitivity is left to a separate PR. The
    /// forward path at these exact parameters is cross-validated against
    /// colour-science in `golden_tests`.
    pub fn srgb() -> Self {
        // colour-science / colorjs.io surroundMap["average"] = [1.0, 0.69, 1.0]
        Self::build(64.0, TWENTY, ONE, SURROUND_AVERAGE_C, ONE)
    }

    /// Standard sRGB viewing conditions with Increased Contrast (IC).
    pub fn srgb_high_contrast() -> Self {
        let mut vc = Self::srgb();
        vc.high_contrast = true;
        vc
    }

    /// Dim surround viewing conditions for dark-theme colour resolution.
    ///
    /// Same illuminant (D65) and adapting luminance as sRGB average,
    /// but with reduced surround contrast per CIECAM16 Table 1:
    /// F = 0.9, c = 0.59, N_c = 0.9.
    ///
    /// Produces lower model J for the same stimulus than the average-surround preset.
    ///
    /// # Why dim (F = 0.9), not dark (F = 0.8), for a dark-theme UI?
    ///
    /// A CSS/product theme label does not determine the user's measured surround.
    /// Lab Colors therefore freezes *dim* (F = 0.9) as an engine compatibility
    /// preset, not as a universal observer claim or CIE mandate. The test-only
    /// `dark_surround` constructor keeps the F = 0.8 endpoint available for
    /// comparisons and tests.
    ///
    /// Doctest проверяет классификатор пресета и ловит тихий возврат к среднему
    /// окружению:
    ///
    /// ```
    /// use labcolors_core::ViewingConditions;
    /// let dim = ViewingConditions::dim_surround();
    /// assert!(dim.is_dark_theme());
    /// ```
    pub fn dim_surround() -> Self {
        // colour-science / colorjs.io surroundMap["dim"] = [0.9, 0.59, 0.9]
        Self::build(
            64.0,
            TWENTY,
            SURROUND_DIM_F_NC,
            SURROUND_DIM_C,
            SURROUND_DIM_F_NC,
        )
    }

    /// Dim surround viewing conditions with Increased Contrast (IC).
    pub fn dim_surround_high_contrast() -> Self {
        let mut vc = Self::dim_surround();
        vc.high_contrast = true;
        vc
    }

    /// Dark surround viewing conditions (CIECAM16 Table 1: F = 0.8, c = 0.525,
    /// N_c = 0.8). Not a precompiled LUT target — used in tests to exercise the
    /// grey-axis LUT's fall-back-to-bisection path for an unsupported VC.
    #[cfg(test)]
    pub(crate) fn dark_surround() -> Self {
        Self::build(
            64.0,
            TWENTY,
            SURROUND_DARK_F_NC,
            SURROUND_DARK_C,
            SURROUND_DARK_F_NC,
        )
    }

    /// Core constructor shared by all surround presets.
    ///
    /// * `la`  — adapting field luminance (cd/m²), typically 64.
    /// * `y_b` — background luminance factor (%), typically 20.
    /// * `f`   — surround factor (1.0 average, 0.9 dim, 0.8 dark).
    /// * `c`   — chromatic adaptation induction factor from surround table.
    /// * `nc`  — chromatic induction factor from surround table.
    fn build(la: f64, y_b: f64, f: f64, c: f64, nc: f64) -> Self {
        let k = ONE / (FIVE * la + ONE);
        let k4 = k * k * k * k;
        let fl = k4 * la + FL_INDIRECT_SCALE * (ONE - k4).powi(2) * (FIVE * la).cbrt();

        let n = y_b / HUNDRED;
        let nbb = NBB_SCALE * n.powf(-NBB_EXPONENT_MAGNITUDE);
        let z = Z_OFFSET + n.sqrt();

        let xyz_w = [
            D65_WHITE[0] * HUNDRED,
            D65_WHITE[1] * HUNDRED,
            D65_WHITE[2] * HUNDRED,
        ];
        let rgb_w = xyz_to_cone(xyz_w);
        let d = (f
            * (ONE - (ONE / ADAPTATION_DECAY_DIVISOR) * ((-la - FORTY_TWO) / NINETY_TWO).exp()))
        .clamp(ZERO, ONE);
        let rgb_d = [
            d * (HUNDRED / rgb_w[0]) + ONE - d,
            d * (HUNDRED / rgb_w[1]) + ONE - d,
            d * (HUNDRED / rgb_w[2]) + ONE - d,
        ];

        let rgb_w_adapted = [
            rgb_w[0] * rgb_d[0],
            rgb_w[1] * rgb_d[1],
            rgb_w[2] * rgb_d[2],
        ];
        let rgb_aw = [
            adapt(rgb_w_adapted[0], fl),
            adapt(rgb_w_adapted[1], fl),
            adapt(rgb_w_adapted[2], fl),
        ];
        let aw = (TWO * rgb_aw[0] + rgb_aw[1] + rgb_aw[2] / TWENTY) * nbb;

        // Пер-VC константы колорфулнесс, вынесенные из пер-цветового прямого и
        // обратного хода. Считаются здесь ровно на тех операндах, что использовали
        // инлайн-места (`fl`, `n`), поэтому сохранённые биты равны пересчитанным —
        // это устранение общего подвыражения (CSE), а не численное изменение
        // (пинится `derived_constants_are_bit_identical_to_inline_recompute` и,
        // ниже по потоку, bit-identity оракулом `cam16::forward`).
        let fl_pow_025 = fl.powf(FL_QUARTER_EXPONENT);
        let t_inner = (T_INNER_OFFSET - T_INNER_POWER_BASE.powf(n)).powf(T_INNER_EXPONENT);

        Self {
            n,
            aw,
            nbb,
            ncb: nbb,
            fl,
            z,
            c,
            nc,
            rgb_d,
            high_contrast: false,
            fl_pow_025,
            t_inner,
        }
    }

    /// Определяет, описывают ли условия приглушённое или тёмное окружение,
    /// используемое тёмной темой, в отличие от среднего окружения светлой темы.
    ///
    /// Дискриминатор — коэффициент хроматической адаптации окружения `c`:
    /// средний пресет фиксирует `0.69`, а `dim` и `dark` — `0.59` и `0.525`.
    /// Порог посередине (`0.64`) оставляет запас для округления. Условия
    /// просмотра остаются единственным источником режима: контракт роли с
    /// заданными по темам смещениями `J′` читает сторону из VC, а не из
    /// дублирующего флага.
    pub fn is_dark_theme(&self) -> bool {
        const AVERAGE_DIM_MIDPOINT_C: f64 = 0.64;
        self.c < AVERAGE_DIM_MIDPOINT_C
    }

    /// Slot index for the two precompiled viewing conditions (`0` = sRGB,
    /// `1` = dim surround), or `None` for any other VC.
    ///
    /// С главы #64 (level-3) единственный продакшн-потребитель — grey-axis LUT
    /// (`crate::lut`) — удалён (солвер решает лестницу напрямую, без сид-брекета);
    /// метод остаётся как проверяемый инвариант кеша фингерпринтов
    /// (`preset_index_is_stable_across_repeated_calls_and_cached_correctly`),
    /// потому `#[cfg(test)]`.
    ///
    /// Used by the grey and chroma fast paths and the grey-axis LUT to share a
    /// single canonical slot assignment — no duplicated fingerprint comparisons
    /// across callers. Matching on the full
    /// [`fingerprint`](Self::fingerprint) (not just the surround pair `(c, nc)`)
    /// ensures a caller-built VC that aliases a preset's `(c, nc)` but differs
    /// in adaptation still returns `None` and falls back to the live solver.
    ///
    /// Preset fingerprints are computed once at first call and cached in two
    /// `OnceLock<u64>` statics — `build()` + `fingerprint()` are never called
    /// again on the hot path, eliminating the transcendental-op rebuild that was
    /// paid on every `preset_index` invocation.
    #[cfg(test)]
    pub(crate) fn preset_index(&self) -> Option<usize> {
        // Cached preset fingerprints: computed once, reused forever.
        // Invariant: these statics hold exactly `ViewingConditions::srgb().fingerprint()`
        // and `ViewingConditions::dim_surround().fingerprint()` respectively.
        // They are the only place the presets are constructed at runtime; callers
        // do not need to build a VC to compare against — they compare a single u64.
        static SRGB_FP: OnceLock<u64> = OnceLock::new();
        static DIM_FP: OnceLock<u64> = OnceLock::new();

        let fp = self.fingerprint();
        if fp == *SRGB_FP.get_or_init(|| ViewingConditions::srgb().fingerprint()) {
            Some(0)
        } else if fp == *DIM_FP.get_or_init(|| ViewingConditions::dim_surround().fingerprint()) {
            Some(1)
        } else {
            None
        }
    }

    /// Exact identity fingerprint over **every** field that affects a resolved
    /// colour. Two viewing conditions with equal fingerprints produce
    /// bit-identical output, so a fast-path cache may key on it: any difference
    /// — even in a field the surround pair `(c, nc)` does not capture — forces a
    /// distinct slot (a cold rebuild), never a wrong-colour memo collision. This
    /// is why the grey/chroma fast paths match a VC on the full fingerprint, not
    /// just `(c, nc)`: a caller-built VC that aliases the surround pair but
    /// differs in adaptation (`aw`/`fl`/`n`/…) must fall through to the live
    /// solver, not be served another condition's cached set.
    pub(crate) fn fingerprint(&self) -> u64 {
        // Destructure rather than list `self.field`s: with no `..`, a field
        // added to `ViewingConditions` is an E0027 compile error here until it is
        // bound in this pattern — forcing whoever adds it to fold it into the hash
        // below (a bound-but-unhashed field then warns as unused, which CI treats
        // as an error). The fingerprint can no longer silently omit a field and
        // revive the subset-aliasing bug (#73): the compiler guards completeness,
        // not a comment.
        let &ViewingConditions {
            n,
            aw,
            nbb,
            ncb,
            fl,
            z,
            c,
            nc,
            rgb_d: [d0, d1, d2],
            high_contrast,
            fl_pow_025,
            t_inner,
        } = self;
        // `fl_pow_025` и `t_inner` — чистые функции уже хешируемых полей (`fl`,
        // `n`), так что для разделимости их вклад избыточен. Но деструктуризация
        // без `..` выше обязывает связать каждое поле, а дисциплина крейта —
        // хешировать каждое связанное поле, а не сбрасывать его через `: _` (чтобы
        // по-настоящему независимое новое поле нельзя было молча потерять). Равные
        // исходные поля ⇒ равные производные ⇒ равный отпечаток, поэтому контракт
        // «равный отпечаток ⇒ байт-идентичный выход» сохраняется.
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for f in [
            n, aw, nbb, ncb, fl, z, c, nc, d0, d1, d2, fl_pow_025, t_inner,
        ] {
            h ^= f.to_bits();
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Shift by 1 so the boolean's only possible values (0u64 / 1u64) map to
        // distinct bit positions from the FNV accumulator's LSB, avoiding a trivial
        // XOR cancellation when `high_contrast` is folded into the running hash.
        const HIGH_CONTRAST_FINGERPRINT_SALT: u64 = 1;
        h ^= (high_contrast as u64) << HIGH_CONTRAST_FINGERPRINT_SALT;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_constants_are_bit_identical_to_inline_recompute() {
        // BIT-IDENTITY ГЕЙТ для хоистинга VC-констант: сохранённые `fl_pow_025`
        // и `t_inner` обязаны равняться инлайн-выражениям, которые они заменили в
        // прямом ходе (`cam16`), обратном (`lcs::to_xyz`) и H-K-хроме (`lpc`) — до
        // последнего ULP, не «в пределах допуска». Любой дрейф здесь молча сдвинул
        // бы все нижележащие golden'ы; тест ловит это в источнике.
        for vc in [
            ViewingConditions::srgb(),
            ViewingConditions::dim_surround(),
            ViewingConditions::srgb_high_contrast(),
            ViewingConditions::dim_surround_high_contrast(),
            ViewingConditions::dark_surround(),
        ] {
            assert_eq!(
                vc.fl_pow_025.to_bits(),
                vc.fl.powf(0.25).to_bits(),
                "fl_pow_025 drifted from the inline fl.powf(0.25)"
            );
            assert_eq!(
                vc.t_inner.to_bits(),
                (1.64 - 0.29_f64.powf(vc.n)).powf(0.73).to_bits(),
                "t_inner drifted from the inline (1.64 - 0.29^n)^0.73"
            );
        }
    }

    #[test]
    fn srgb_c_is_069() {
        let vc = ViewingConditions::srgb();
        assert!(
            (vc.c - 0.69).abs() < 1e-10,
            "srgb c = {}, expected 0.69",
            vc.c
        );
    }

    #[test]
    fn dim_surround_c_is_059() {
        let vc = ViewingConditions::dim_surround();
        assert!(
            (vc.c - 0.59).abs() < 1e-10,
            "dim c = {}, expected 0.59",
            vc.c
        );
    }

    #[test]
    fn dim_surround_nc_is_09() {
        let vc = ViewingConditions::dim_surround();
        assert!(
            (vc.nc - 0.9).abs() < 1e-10,
            "dim nc = {}, expected 0.9",
            vc.nc
        );
    }

    #[test]
    fn dim_has_lower_aw_than_average() {
        // Dim surround reduces adaptation → lower achromatic response
        let avg = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        assert!(
            dim.aw < avg.aw,
            "dim aw ({}) should be < average aw ({})",
            dim.aw,
            avg.aw
        );
    }

    #[test]
    fn dim_has_different_rgb_d() {
        let avg = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        assert_ne!(
            avg.rgb_d, dim.rgb_d,
            "different surround → different discounting factors"
        );
    }

    #[test]
    fn is_dark_theme_classifies_presets() {
        // The 0.64 midpoint must land the average (light) surround above it and
        // both dimmed surrounds below it — the contract role resolution relies on.
        assert!(
            !ViewingConditions::srgb().is_dark_theme(),
            "srgb (average surround, c≈0.69) is a light theme"
        );
        assert!(
            ViewingConditions::dim_surround().is_dark_theme(),
            "dim_surround (c≈0.59) is a dark theme"
        );
        assert!(
            ViewingConditions::dark_surround().is_dark_theme(),
            "dark_surround (c≈0.525) is a dark theme"
        );
    }

    #[test]
    fn fingerprint_separates_presets_and_surround_pair_aliases() {
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        // The two precompiled conditions are distinct.
        assert_ne!(srgb.fingerprint(), dim.fingerprint());
        // Stable: a fresh construction fingerprints identically (so the fast-path
        // exact match recognises the live preset).
        assert_eq!(srgb.fingerprint(), ViewingConditions::srgb().fingerprint());

        // The whole point: a VC that ALIASES sRGB's surround pair (c, nc) but
        // differs in an adaptation field must fingerprint differently, so a
        // fingerprint-keyed cache can never serve it sRGB's set.
        let mut alias = srgb;
        alias.aw += 1.0;
        assert_eq!(alias.c, srgb.c);
        assert_eq!(alias.nc, srgb.nc);
        assert_ne!(
            alias.fingerprint(),
            srgb.fingerprint(),
            "an aw-perturbed VC must not collide with sRGB's fingerprint"
        );
    }

    /// Class fix: preset_index must return the correct slot on repeated calls
    /// (cached fingerprints must agree with a fresh construction every time).
    /// Bites (mutation proof): change either OnceLock initialiser to use the
    /// wrong preset → preset_index returns the wrong slot → assertions below fail.
    #[test]
    fn preset_index_is_stable_across_repeated_calls_and_cached_correctly() {
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        let dark = ViewingConditions::dark_surround();

        // First call primes the OnceLock; subsequent calls must agree.
        for _ in 0..4 {
            assert_eq!(
                srgb.preset_index(),
                Some(0),
                "sRGB must always be slot 0 (OnceLock cache must not drift)"
            );
            assert_eq!(
                dim.preset_index(),
                Some(1),
                "dim must always be slot 1 (OnceLock cache must not drift)"
            );
            assert_eq!(
                dark.preset_index(),
                None,
                "dark surround is not a compiled preset and must return None"
            );
        }

        // A surround-pair alias (same c/nc as sRGB, different aw) must NOT match
        // the cached sRGB fingerprint — the full-field fingerprint guards this.
        let mut alias = srgb;
        alias.aw += 1.0;
        assert_eq!(
            alias.preset_index(),
            None,
            "a VC that aliases sRGB's (c, nc) but differs in aw must not match the \
             cached sRGB fingerprint — subset-aliasing class is still open"
        );
    }
}
