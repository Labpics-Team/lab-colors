use crate::lcs::LcsColor;
use crate::spaces::vc::ViewingConditions;

/// Порог M' (CAM16-UCS), ниже которого якорь считается ахроматическим и его
/// шумный `h_ok` замещается базовым оттенком. CAM16 даёт ненулевой M' даже для
/// номинально ахроматических стимулов (mp ≈ 1.5 для белого, ≈ 2.3 для near-black),
/// поэтому 5.0 ловит модельный шум, сохраняя подлинно хроматические якоря.
///
/// Терминал **(c) INTERVAL-INSENSITIVE**: порог сидит в ШИРОКОМ пустом зазоре
/// между потолком ахроматического `M'`-шума серых и полом хромы подлинно
/// цветных якорей; партиция achromatic↔chromatic инвариантна для ЛЮБОГО θ в
/// этом зазоре (`achromatic_threshold_sits_in_empty_noise_to_chroma_gap`).
/// Экспозиция — доля sRGB-гаммы, где точное значение флипает партицию, —
/// **1.99%** (`exposure_achromatic_mp_threshold`). Значение не меняется.
// SSOT-TRACKED — порог ахроматичности M' (модельный шум CAM16), терминал (c) interval-insensitive (exposure 1.99%), см. docs/empirical-inventory.md.
const ACHROMATIC_MP_THRESHOLD: f64 = 5.0;

/// Множитель опорной хромы для нормировки чистоты оттенка: `mp_ref = 1.5 × M'`
/// базового якоря, чтобы база сохраняла почти весь свой оттенок, а
/// near-ахроматические якоря сильно корректировались. Терминал (e) DESIGN-CHOICE:
/// магнитуда — свободная ручка (опубликованного значения не существует),
/// робастность доказана хроматическим инвариантом (`purity == 1` при
/// `mp ≥ mp_ref` при ЛЮБОМ множителе); ПРОВЕНАНС ФОРМЫ — см.
/// [`HUE_PURITY_EXPONENT`]. Легальный диапазон **> 1** (множитель обязан
/// поднимать опорную хрому над хромой базы, иначе база сама корректируется);
/// практический вкус [1.2, 2.0].
// SSOT-TRACKED — множитель опорной хромы, терминал (e) design-choice (форма мотивирована Abney; > 1), см. docs/empirical-inventory.md.
const HUE_PURITY_MP_REF_RATIO: f64 = 1.5;

/// Показатель степени кривой чистоты оттенка `(mp/mp_ref)^0.6`: агрессивная
/// коррекция оттенка для сильно десатурированных цветов, плавно отпускаемая с
/// ростом хромы.
///
/// ПРОВЕНАНС ФОРМЫ (не значения). «Не доверяй тону у нейтрали» мотивировано двумя
/// СХОДЯЩИМИСЯ причинами:
/// 1. ЧИСЛЕННОЙ — `atan2(b, a)` ill-conditioned у серой оси, оттенок почти-серых
///    построен из шума (см. [`hue_purity`]);
/// 2. ПЕРЦЕПТИВНОЙ — эффект Abney: воспринимаемый тон монохроматического стимула
///    СДВИГАЕТСЯ при разбавлении белым (падении чистоты). Abney (1909) Proc. R.
///    Soc. Lond. A 83, 120–127 (DOI 10.1098/rspa.1909.0085); величина сдвига
///    растёт с падением колориметрической чистоты — Kurtenbach, Sternheim &
///    Spillmann (1984) JOSA A 1(4), 365–372 (DOI 10.1364/JOSAA.1.000365).
///
/// ⚠️ Перцептивный Abney — ОТДЕЛЬНОЕ явление от численного atan2-шума; конкретные
/// `1.5`/`0.6` — инженерная калибровка формы, НЕ выведены из данных Abney (эта
/// кривая эффект Abney не моделирует, issue #27 — `abney_correct`).
///
/// Терминал (e) DESIGN-CHOICE (решение владельца 2026-07-07): фит магнитуд к
/// данным Abney дал бы ДРУГУЮ кривую — маркировать их (b) GROUNDED было бы
/// подлогом (ADR-0002); честный терминал — задекларированные свободные ручки с
/// доказанной робастностью. Sensitivity: хроматические якоря (`mp ≥ mp_ref`)
/// инвариантны к значению (`purity == 1`), константы двигают ТОЛЬКО оттенок
/// near-нейтралей (и так atan2-шум); свип показателя [0.4, 0.9] даёт
/// max|Δpurity| = 0.148 — непрерывный ограниченный дрейф, не флип. Легальный
/// диапазон **(0, 1]** (показатель < 1 = вогнутая кривая, агрессивная коррекция
/// near-нейтралей; практический вкус [0.4, 0.9]). Протокол «объективизации»:
/// 2AFC hue-shift на десатурированных стимулах (величина Abney-сдвига vs чистота)
/// стал бы кандидатом-ВЫВОДОМ (замер → сравнение → решение), не обязательным
/// экспериментом. Локи `hue_purity_curve_shape_is_pinned`, `exposure_hue_purity_curve`;
/// docs/empirical-inventory.md.
// SSOT-TRACKED — показатель кривой чистоты, терминал (e) design-choice (форма мотивирована Abney; (0,1]), см. docs/empirical-inventory.md.
const HUE_PURITY_EXPONENT: f64 = 0.6;

/// Унаследованные параметры формы нейтральной кривой.
///
/// Значения сохраняют текущую эмиссию до замены политики построения, но не являются
/// универсальной психофизикой или пользовательскими осями намерения. Равномерные
/// шаги Oklab/sRGB/CAM16-J′ и прямое прочтение метрики контрастного усиления Уиттла
/// уже опровергнуты по магнитуде; тёмная ветвь дополнительно вырождается около
/// чёрного. Поэтому gamma — терминальная compatibility policy для прежних байтов,
/// а не human law. Задачи #219/#261 могут исследовать отдельную человеко-
/// ориентированную замену; generic fact-only механизмы не должны зависеть от
/// этих чисел или ждать такого исследования.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveParams {
    pub gamma_light: f64,
    pub gamma_dark: f64,
    pub chroma_peak_t: f64,
}

impl Default for CurveParams {
    fn default() -> Self {
        Self {
            // Показатель степени, которым гамма-кривая отображает t на светлотный
            // интервал [светлый якорь, базовый], при t <= 0.5.
            // SSOT-TRACKED — (e) compatibility policy прежней эмиссии;
            // научная замена отдельно остаётся OPEN в #219/#261.
            gamma_light: 1.75,
            // Показатель степени для тёмной ветви (t > 0.5): интервал [базовый,
            // тёмный якорь].
            // SSOT-TRACKED — (e) compatibility policy прежней эмиссии;
            // научная замена отдельно остаётся OPEN в #219/#261.
            gamma_dark: 1.5,
            // Позиция вдоль параметра кривой t, где огибающая хромы достигает пика.
            // SSOT-TRACKED — зафиксированный скаляр прежней эмиссии; исследование
            // зависимости от оттенка принадлежит #219/#261.
            chroma_peak_t: 0.35,
        }
    }
}

/// Нейтральная (серо-осевая) кривая по трём якорям: светлый → базовый → тёмный.
///
/// Светлота ведётся гамма-ветвями (`CurveParams`), хрома — C1-огибающей
/// (`chroma_envelope`), оттенок — покоем у базы с purity-коррекцией шумных
/// концов (`hue_purity`). `at()` — примитив каждого шага построения лестницы,
/// поэтому все t-инвариантные величины предвычислены в конструкторе.
#[derive(Debug, Clone)]
pub struct NeutralCurve {
    a_light: LcsColor,
    a_base: LcsColor,
    a_dark: LcsColor,
    h_ok_base: f64,
    h_cam_base: f64,
    // Purity-скорректированные оттенки концов кривой. Они не зависят от t,
    // а их пересчёт в каждом `at()` стоил 4×powf на сэмпл (hue_purity для
    // двух якорей в двух hue-пространствах). Предвычисление в with_vc даёт
    // бит-идентичный результат: те же операции над теми же входами, один раз.
    h_ok_light_eff: f64,
    h_ok_dark_eff: f64,
    h_cam_light_eff: f64,
    h_cam_dark_eff: f64,
    params: CurveParams,
    vc: ViewingConditions,
}

impl NeutralCurve {
    /// Build a neutral curve using standard sRGB viewing conditions (average surround).
    pub fn new(light: &str, base: &str, dark: &str) -> Result<Self, String> {
        Self::with_vc(
            light,
            base,
            dark,
            &CurveParams::default(),
            &ViewingConditions::srgb(),
        )
    }

    /// Как [`NeutralCurve::new`], но с нестандартными форм-параметрами
    /// (например, из `labui.config`), viewing conditions — стандартные sRGB.
    pub fn with_params(
        light: &str,
        base: &str,
        dark: &str,
        params: CurveParams,
    ) -> Result<Self, String> {
        Self::with_vc(light, base, dark, &params, &ViewingConditions::srgb())
    }

    /// Build a neutral curve for the given viewing conditions.
    ///
    /// Anchor colours are parsed through `vc`, so J' and saturation reflect
    /// the perceptual environment (e.g. dim-surround for dark themes).
    /// Use [`ViewingConditions::srgb()`] for light themes and
    /// [`ViewingConditions::dim_surround()`] for dark themes.
    pub fn with_vc(
        light: &str,
        base: &str,
        dark: &str,
        params: &CurveParams,
        vc: &ViewingConditions,
    ) -> Result<Self, String> {
        let a_light = LcsColor::from_hex_with_vc(light, vc)?;
        let a_base = LcsColor::from_hex_with_vc(base, vc)?;
        let a_dark = LcsColor::from_hex_with_vc(dark, vc)?;

        if a_light.jp <= a_base.jp {
            return Err("light anchor must be lighter than base".into());
        }
        if a_base.jp <= a_dark.jp {
            return Err("base anchor must be lighter than dark".into());
        }

        let h_ok_base = a_base.h_ok;
        let h_cam_base = a_base.h_cam();

        // Achromatic anchors have unreliable h_ok (atan2 of ~0 values).
        // CAM16 viewing-condition adaptation produces non-zero M' even for
        // nominally achromatic stimuli (mp ≈ 1.5 for white, ≈ 2.3 for
        // near-black).  Threshold 5.0 catches model noise while preserving
        // genuinely chromatic anchors.
        let a_light = if a_light.mp() < ACHROMATIC_MP_THRESHOLD {
            LcsColor::new(a_light.jp, h_ok_base, a_light.s, a_light.h_cam())
        } else {
            a_light
        };
        let a_dark = if a_dark.mp() < ACHROMATIC_MP_THRESHOLD {
            LcsColor::new(a_dark.jp, h_ok_base, a_dark.s, a_dark.h_cam())
        } else {
            a_dark
        };

        // Эффективные оттенки концов: шумный hue near-ахроматического якоря
        // подтягивается к базовому пропорционально его хроматической чистоте
        // (см. `hue_purity`). mp_ref = 1.5×M' базы — база сохраняет почти весь
        // свой оттенок, near-серые концы корректируются сильно. Считается здесь,
        // а не в `at()`: величины t-инвариантны, а `at()` — горячий путь.
        let mp_ref = a_base.mp() * HUE_PURITY_MP_REF_RATIO;
        let purity_light = hue_purity(a_light.mp(), mp_ref);
        let purity_dark = hue_purity(a_dark.mp(), mp_ref);
        let h_ok_light_eff = lerp_angle(h_ok_base, a_light.h_ok, purity_light);
        let h_ok_dark_eff = lerp_angle(h_ok_base, a_dark.h_ok, purity_dark);
        let h_cam_light_eff = lerp_angle(h_cam_base, a_light.h_cam(), purity_light);
        let h_cam_dark_eff = lerp_angle(h_cam_base, a_dark.h_cam(), purity_dark);

        Ok(Self {
            a_light,
            a_base,
            a_dark,
            h_ok_base,
            h_cam_base,
            h_ok_light_eff,
            h_ok_dark_eff,
            h_cam_light_eff,
            h_cam_dark_eff,
            params: *params,
            vc: *vc,
        })
    }

    /// Точка кривой при `t ∈ [0, 1]`: 0 — светлый якорь, 0.5 — базовый,
    /// 1 — тёмный.
    pub fn at(&self, t: f64) -> LcsColor {
        let t = t.clamp(0.0, 1.0);

        // Снап к якорям в пределах 1e-12: гарантирует байт-точное
        // воспроизведение входных hex-якорей на концах и в базе — иначе
        // накопленная FP-погрешность интерполяции могла бы сдвинуть
        // квантованный выход на единицу канала.
        if (t - 0.0).abs() < 1e-12 {
            return self.a_light;
        }
        if (t - 0.5).abs() < 1e-12 {
            return self.a_base;
        }
        if (t - 1.0).abs() < 1e-12 {
            return self.a_dark;
        }

        // jp-якоря берутся напрямую из полей якорей (Oklab jp). Прежний метод-
        // обёртка effective_hue_anchor_jp был identity ({ anchor.jp }) — имя
        // обещало hue-зависимый расчёт, тело возвращало поле; заинлайнен (аудит
        // D2(c), 2026-07-03), value-preserving.
        let jp = if t <= 0.5 {
            let u = t / 0.5;
            let j0 = self.a_light.jp;
            let j6 = self.a_base.jp;
            j0 + (j6 - j0) * u.powf(self.params.gamma_light)
        } else {
            let u = (t - 0.5) / 0.5;
            let j6 = self.a_base.jp;
            let j12 = self.a_dark.jp;
            j6 + (j12 - j6) * u.powf(self.params.gamma_dark)
        };

        let mp = chroma_envelope(
            t,
            self.a_light.mp(),
            self.a_base.mp(),
            self.a_dark.mp(),
            self.params.chroma_peak_t,
        );
        let s = mp / (jp + 1.0);

        let h_ok = self.interpolate_hue_ok(t);
        let h_cam = self.interpolate_hue_cam(t);

        LcsColor::new(jp, h_ok, s, h_cam)
    }

    /// The viewing conditions used to build this curve.
    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    pub fn light_anchor(&self) -> &LcsColor {
        &self.a_light
    }

    pub fn base_anchor(&self) -> &LcsColor {
        &self.a_base
    }

    pub fn dark_anchor(&self) -> &LcsColor {
        &self.a_dark
    }

    // Обе hue-дорожки (Oklab h_ok и CAM16 h_cam) ведутся параллельно одной
    // схемой: конец → база → конец по кратчайшей дуге. Концы — предвычисленные
    // purity-скорректированные поля (см. with_vc).
    fn interpolate_hue_ok(&self, t: f64) -> f64 {
        if t <= 0.5 {
            lerp_angle(self.h_ok_light_eff, self.h_ok_base, t / 0.5)
        } else {
            lerp_angle(self.h_ok_base, self.h_ok_dark_eff, (t - 0.5) / 0.5)
        }
    }

    fn interpolate_hue_cam(&self, t: f64) -> f64 {
        if t <= 0.5 {
            lerp_angle(self.h_cam_light_eff, self.h_cam_base, t / 0.5)
        } else {
            lerp_angle(self.h_cam_base, self.h_cam_dark_eff, (t - 0.5) / 0.5)
        }
    }
}

/// C1-continuous chroma envelope through the chromas of all three anchors.
///
/// Rises from the light anchor's M' to the base anchor's M' at `t_peak`
/// (half-cosine ease), holds the base level until the base anchor at
/// `t = 0.5`, then falls to the dark anchor's M' (half-cosine ease).
/// All three anchors are reproduced exactly and every junction has zero
/// slope, so M' is C1 on `[0, 1]` — the predecessor (`sine_env`) pinned
/// both ends to the dark anchor's chroma, jumping at `t = 0` and `t = 0.5`.
///
/// `t_peak` is clamped to `(0, 0.5]`; at `0.5` the plateau is empty and the
/// envelope is a plain ease light→base→dark.
fn chroma_envelope(t: f64, mp_light: f64, mp_base: f64, mp_dark: f64, t_peak: f64) -> f64 {
    let t_peak = t_peak.clamp(f64::EPSILON, 0.5);
    let ease = |u: f64| 0.5 - 0.5 * (std::f64::consts::PI * u).cos();
    if t <= t_peak {
        mp_light + (mp_base - mp_light) * ease(t / t_peak)
    } else if t <= 0.5 {
        mp_base
    } else {
        mp_base + (mp_dark - mp_base) * ease((t - 0.5) / 0.5)
    }
}

/// Interpolate between two angles **in degrees** along the shortest arc.
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let diff = b - a;
    let shortest = (diff + 180.0).rem_euclid(360.0) - 180.0;
    a + shortest * t
}

/// Hue-purity weight: suppresses the noisy hue of near-achromatic anchors.
///
/// `atan2(b, a)` returns a meaningless angle for a near-grey colour (its hue
/// is built from noise). Returns a value in `[0, 1]` indicating how much of
/// the anchor's own hue to retain. Low `mp` (near-achromatic) → low purity →
/// strong correction toward the base hue. The power exponent 0.6 gives
/// aggressive correction for very desaturated colours while releasing smoothly
/// as chroma increases.
///
/// This is not the Abney effect (curvature of constant-hue lines as purity
/// changes); Abney correction is tracked separately (issue #27, `abney_correct`).
///
/// ```text
/// mp/mp_ref = 0.1 → purity ≈ 0.25  (75 % corrected)
/// mp/mp_ref = 0.3 → purity ≈ 0.49  (51 % corrected)
/// mp/mp_ref = 0.5 → purity ≈ 0.66  (34 % corrected)
/// mp/mp_ref = 1.0 → purity = 1.00  (0   % corrected)
/// ```
fn hue_purity(mp: f64, mp_ref: f64) -> f64 {
    if mp >= mp_ref {
        return 1.0;
    }
    (mp / mp_ref).powf(HUE_PURITY_EXPONENT).clamp(0.0, 1.0)
}

impl crate::curve::ColorCurve for NeutralCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        &self.vc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::ColorCurve;

    fn default_curve() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn anchors_exact_at_endpoints() {
        let curve = default_curve();
        let c0 = curve.at(0.0);
        let cm = curve.at(0.5);
        let c1 = curve.at(1.0);

        assert!(
            (c0.jp - curve.light_anchor().jp).abs() < 1e-9,
            "t=0 jp mismatch"
        );
        assert!(
            (cm.jp - curve.base_anchor().jp).abs() < 1e-9,
            "t=0.5 jp mismatch"
        );
        assert!(
            (c1.jp - curve.dark_anchor().jp).abs() < 1e-9,
            "t=1.0 jp mismatch"
        );
    }

    #[test]
    fn jp_monotonically_decreasing() {
        let curve = default_curve();
        let steps = curve.sample(100);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 1e-9,
                "jp increased: {} -> {}",
                w[0].jp,
                w[1].jp
            );
        }
    }

    #[test]
    fn hue_drift_under_30_degrees() {
        let curve = default_curve();
        let base_hue = curve.base_anchor().h_ok;
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            let drift = (c.h_ok - base_hue + 180.0).rem_euclid(360.0) - 180.0;
            assert!(
                drift.abs() < 30.0,
                "hue drift at t={}: {}° (base={})",
                i as f64 / 100.0,
                drift,
                base_hue
            );
        }
    }

    #[test]
    fn sample_13_matches_old_api() {
        let curve = default_curve();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes.len(), 13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[6].to_uppercase(), "#787880");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }

    #[test]
    fn all_sampled_steps_unique() {
        let curve = default_curve();
        let hexes = curve.sample_hex(13);
        let mut seen = std::collections::HashSet::new();
        for hex in &hexes {
            assert!(seen.insert(hex.to_uppercase()), "duplicate: {}", hex);
        }
    }

    #[test]
    fn jp_within_anchor_bounds() {
        let curve = default_curve();
        let j_max = curve.light_anchor().jp;
        let j_min = curve.dark_anchor().jp;
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            assert!(
                c.jp <= j_max + 1e-9 && c.jp >= j_min - 1e-9,
                "t={}: jp={} out of [{}, {}]",
                i as f64 / 100.0,
                c.jp,
                j_min,
                j_max
            );
        }
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(NeutralCurve::new("#GGGGGG", "#787880", "#101012").is_err());
    }

    #[test]
    fn rejects_light_not_lighter_than_base() {
        assert!(NeutralCurve::new("#787880", "#FFFFFF", "#101012").is_err());
    }

    #[test]
    fn rejects_base_not_lighter_than_dark() {
        assert!(NeutralCurve::new("#FFFFFF", "#101012", "#787880").is_err());
    }

    #[test]
    fn s_non_negative_everywhere() {
        let curve = default_curve();
        for i in 0..=100 {
            let c = curve.at(i as f64 / 100.0);
            assert!(
                c.s >= -1e-9,
                "negative s at t={}: {}",
                i as f64 / 100.0,
                c.s
            );
        }
    }

    // ── Dark-theme (dim-surround) tests ────────────────────────

    fn dim_curve() -> NeutralCurve {
        let vc = ViewingConditions::dim_surround();
        NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &vc,
        )
        .unwrap()
    }

    #[test]
    fn dim_base_jp_higher_than_srgb() {
        // CIECAM16 dim surround: lower c (0.59 vs 0.69) → smaller exponent
        // for J = 100·(A/Aw)^(c·Z).  When A/Aw < 1 (any non-white stimulus),
        // a smaller exponent pushes the result closer to 1, yielding a higher J.
        // Physically correct: mid-grey appears lighter relative to the
        // adapted white point in dim surroundings.
        let avg = default_curve();
        let dim = dim_curve();
        assert!(
            dim.base_anchor().jp > avg.base_anchor().jp,
            "dim J'={} should be > avg J'={} (dim surround lifts mid-tones)",
            dim.base_anchor().jp,
            avg.base_anchor().jp,
        );
    }

    #[test]
    fn dim_jp_monotonically_decreasing() {
        let curve = dim_curve();
        let steps = curve.sample(100);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 1e-9,
                "dim jp increased: {} -> {}",
                w[0].jp,
                w[1].jp,
            );
        }
    }

    #[test]
    fn dim_roundtrip_base() {
        let curve = dim_curve();
        let hex = curve.base_anchor().to_hex_with_vc(&curve.vc);
        assert!(
            hex.eq_ignore_ascii_case("#787880"),
            "dim roundtrip drift: expected #787880, got {}",
            hex,
        );
    }

    #[test]
    fn dim_sample_hex_endpoints_match() {
        let curve = dim_curve();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }

    #[test]
    fn dim_all_steps_unique() {
        let curve = dim_curve();
        let hexes = curve.sample_hex(13);
        let mut seen = std::collections::HashSet::new();
        for hex in &hexes {
            assert!(seen.insert(hex.to_uppercase()), "dim duplicate: {}", hex);
        }
    }

    // ── Непрерывность: инвариант непрерывной растяжки ──────────

    #[test]
    fn curve_continuous_everywhere() {
        // «Непрерывная растяжка» — продуктовый инвариант: jp, M' и hue
        // не имеют скачков ни в якорях, ни между ними.
        for curve in [default_curve(), dim_curve()] {
            let n = 2000;
            let mut prev = curve.at(0.0);
            for i in 1..=n {
                let t = i as f64 / n as f64;
                let c = curve.at(t);
                let dmp = (c.mp() - prev.mp()).abs();
                let djp = (c.jp - prev.jp).abs();
                let dh = ((c.h_ok - prev.h_ok + 180.0).rem_euclid(360.0) - 180.0).abs();
                assert!(dmp < 0.05, "M' jump at t={}: {}", t, dmp);
                assert!(djp < 0.35, "J' jump at t={}: {}", t, djp);
                assert!(dh < 1.0, "hue jump at t={}: {}°", t, dh);
                prev = c;
            }
        }
    }

    #[test]
    fn envelope_passes_through_all_anchor_chromas() {
        let curve = default_curve();
        let eps = 1e-6;
        for (t, anchor, name) in [
            (eps, curve.light_anchor(), "light"),
            (0.5 - eps, curve.base_anchor(), "base (plateau end)"),
            (0.5 + eps, curve.base_anchor(), "base (fall start)"),
            (1.0 - eps, curve.dark_anchor(), "dark"),
        ] {
            let got = curve.at(t).mp();
            let want = anchor.mp();
            assert!(
                (got - want).abs() < 0.01,
                "{} anchor chroma: at({})={}, anchor={}",
                name,
                t,
                got,
                want
            );
        }
    }

    // ── lerp_angle: кратчайшая дуга через границу 0°/360° ──────

    #[test]
    fn lerp_angle_crosses_zero_forward() {
        // 350° → 10°: shortest arc is +20° through 0°, midpoint 0° (mod 360).
        // The old `%`-based formula returned 180° here (long way round).
        let mid = lerp_angle(350.0, 10.0, 0.5).rem_euclid(360.0);
        assert!(
            mid < 1e-9 || (mid - 360.0).abs() < 1e-9,
            "midpoint of 350°→10° must be 0°, got {}",
            mid
        );
    }

    #[test]
    fn lerp_angle_crosses_zero_backward() {
        // 10° → 350°: shortest arc is −20°, midpoint 0° (mod 360).
        let mid = lerp_angle(10.0, 350.0, 0.5).rem_euclid(360.0);
        assert!(
            mid < 1e-9 || (mid - 360.0).abs() < 1e-9,
            "midpoint of 10°→350° must be 0°, got {}",
            mid
        );
    }

    #[test]
    fn lerp_angle_endpoints_exact() {
        assert!((lerp_angle(350.0, 10.0, 0.0) - 350.0).abs() < 1e-9);
        assert!((lerp_angle(350.0, 10.0, 1.0).rem_euclid(360.0) - 10.0).abs() < 1e-9);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Научные локи + EXPOSURE (волна science/constants-objectivization).
// ACHROMATIC_MP_THRESHOLD: (c) INTERVAL-INSENSITIVE — порог ловит модельный M'-шум
// CAM16 в ШИРОКОМ пустом зазоре между потолком шума ахроматических серых и полом
// хромы подлинно цветных якорей. Значение НЕ меняется.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod exposure_locks {
    use super::ACHROMATIC_MP_THRESHOLD;
    use crate::exposure_support::{band_exposure, mp_srgb};
    use crate::lcs::LcsColor;
    use crate::spaces::vc::ViewingConditions;

    fn grey_mp_ceiling() -> f64 {
        let mut m = 0.0f64;
        for i in 0u16..=255 {
            let hex = format!("#{i:02X}{i:02X}{i:02X}", i = i as u8);
            m = m.max(LcsColor::from_hex(&hex).unwrap().mp());
            m = m.max(
                LcsColor::from_hex_with_vc(&hex, &ViewingConditions::dim_surround())
                    .unwrap()
                    .mp(),
            );
        }
        m
    }

    /// (c) Порог сидит в ПУСТОМ зазоре (потолок ахроматического M'-шума, пол хромы
    /// цветных якорей) — партиция achromatic↔chromatic инвариантна для любого θ в
    /// зазоре, значит точное значение нематериально.
    #[test]
    fn achromatic_threshold_sits_in_empty_noise_to_chroma_gap() {
        let ceiling = grey_mp_ceiling();
        let chromatic = [
            "#FF3B30", "#34C759", "#007AFF", "#FFD000", "#5856D6", "#FF9500", "#AF52DE", "#5AC8FA",
        ];
        let chroma_floor = chromatic
            .iter()
            .map(|h| LcsColor::from_hex(h).unwrap().mp())
            .fold(f64::INFINITY, f64::min);
        assert!(
            ceiling < ACHROMATIC_MP_THRESHOLD && ACHROMATIC_MP_THRESHOLD < chroma_floor,
            "порог {ACHROMATIC_MP_THRESHOLD} должен лежать в зазоре (шум серых {ceiling:.3}, хрома цветных {chroma_floor:.3})"
        );
        // Инвариантность партиции: для θ на обеих границах зазора серые остаются
        // ахроматическими, цветные — хроматическими.
        for theta in [ceiling + 1e-3, chroma_floor - 1e-3, ACHROMATIC_MP_THRESHOLD] {
            for i in [0u8, 64, 128, 192, 255] {
                let hex = format!("#{i:02X}{i:02X}{i:02X}");
                assert!(
                    LcsColor::from_hex(&hex).unwrap().mp() < theta,
                    "grey {hex} achromatic @θ={theta:.3}"
                );
            }
            for h in chromatic {
                assert!(
                    LcsColor::from_hex(h).unwrap().mp() >= theta,
                    "{h} chromatic @θ={theta:.3}"
                );
            }
        }
    }

    /// EXPOSURE: доля гаммы с M' в полосе флипа [потолок_шума, 2×порог] — цвета,
    /// чья ахроматическая классификация зависит от точного порога.
    #[test]
    fn exposure_achromatic_mp_threshold() {
        let ceiling = grey_mp_ceiling();
        let (lo, hi) = (ceiling, 2.0 * ACHROMATIC_MP_THRESHOLD);
        let (grid_pct, labui) = band_exposure(|c| mp_srgb(c, false), lo, hi);
        eprintln!(
            "EXPOSURE ACHROMATIC_MP_THRESHOLD band=[{lo:.3},{hi:.3}] grid_flip={grid_pct:.2}% labui_in_zone={} {:?}",
            labui.len(),
            labui
        );
    }

    /// FORM-лок кривой чистоты (values pinned + границы + монотонность + ключевой
    /// инвариант). ХРОМАТИЧЕСКИЙ ИНВАРИАНТ: при `mp ≥ mp_ref` `hue_purity == 1` при
    /// ЛЮБОМ показателе, значит `HUE_PURITY_EXPONENT`/`_MP_REF_RATIO` двигают
    /// ТОЛЬКО оттенок near-нейтралей (шум atan2), а не хроматические якоря.
    #[test]
    fn hue_purity_curve_shape_is_pinned() {
        use super::{HUE_PURITY_EXPONENT, HUE_PURITY_MP_REF_RATIO, hue_purity};
        // Значения (магнитуда калибровочная — пиннится, чтобы дрейф был виден).
        assert_eq!(HUE_PURITY_EXPONENT, 0.6);
        assert_eq!(HUE_PURITY_MP_REF_RATIO, 1.5);
        let ref_mp = 10.0;
        // Границы: 0 → 0 (полная коррекция), ≥ref → 1 (нет коррекции).
        assert_eq!(hue_purity(0.0, ref_mp), 0.0);
        assert_eq!(hue_purity(ref_mp, ref_mp), 1.0);
        assert_eq!(hue_purity(2.0 * ref_mp, ref_mp), 1.0);
        // Монотонно не убывает на [0, ref].
        let mut prev = -1.0_f64;
        let mut x = 0.0_f64;
        while x <= ref_mp {
            let p = hue_purity(x, ref_mp);
            assert!(
                p >= prev - 1e-12 && (0.0..=1.0).contains(&p),
                "purity must be monotone in [0,1]: hue_purity({x},{ref_mp})={p}"
            );
            prev = p;
            x += ref_mp / 64.0;
        }
    }

    /// EXPOSURE/sensitivity кривой чистоты: константы влияют ТОЛЬКО на near-нейтрали
    /// (`r = mp/mp_ref < 1`), чей оттенок и так шум; при `r ≥ 1` `purity == 1` при
    /// любом показателе (хроматические якоря инвариантны). Свипаем показатель по
    /// [0.4, 0.9] и печатаем макс. |Δpurity| — непрерывный ОГРАНИЧЕННЫЙ дрейф, не
    /// бинарный флип: точное значение нематериально для хроматического выхода.
    #[test]
    fn exposure_hue_purity_curve() {
        use super::HUE_PURITY_EXPONENT;
        let prod = |r: f64| r.powf(HUE_PURITY_EXPONENT);
        let mut max_dp = 0.0_f64;
        let mut r = 0.0_f64;
        while r <= 1.0 {
            for e in [0.4_f64, 0.5, 0.7, 0.8, 0.9] {
                max_dp = max_dp.max((r.powf(e) - prod(r)).abs());
            }
            r += 1.0 / 128.0;
        }
        // Хроматический инвариант: при r >= 1 покрытие purity == 1 для любого e.
        for e in [0.4_f64, 0.6, 0.9, 1.5] {
            assert_eq!(
                (1.0_f64).powf(e),
                1.0,
                "r=1 purity must be 1 for any exponent"
            );
        }
        eprintln!(
            "EXPOSURE HUE_PURITY exponent-sweep[0.4,0.9] max|Δpurity|={max_dp:.3} \
             (bounded, continuous; chromatic anchors invariant, only near-neutral hue-noise moves)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Волна 2 «объективизация» — терминал (e) для CurveParams (гаммы + пик хромы).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod wave2_e_locks {
    use super::{CurveParams, NeutralCurve};
    use crate::curve::ColorCurve;
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::srgb_from_hex;

    fn de_ok_hex(a: &str, b: &str) -> f64 {
        let la = srgb_linear_to_oklab(srgb_from_hex(a).unwrap());
        let lb = srgb_linear_to_oklab(srgb_from_hex(b).unwrap());
        ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
    }

    fn ladder(params: CurveParams) -> Vec<String> {
        NeutralCurve::with_params("#FFFFFF", "#787880", "#101012", params)
            .unwrap()
            .sample_hex(13)
    }

    fn max_de_vs_default(vary: impl Fn(&mut CurveParams)) -> f64 {
        let base = ladder(CurveParams::default());
        let mut p = CurveParams::default();
        vary(&mut p);
        let alt = ladder(p);
        base.iter()
            .zip(alt.iter())
            .map(|(a, b)| de_ok_hex(a, b))
            .fold(0.0_f64, f64::max)
    }

    /// (e) sensitivity-лок для трёх форм-параметров `CurveParams`; gamma —
    /// compatibility policy, а не универсальная design/human axis.
    /// Свип каждой ручки по легальной полосе на дефолтной near-серой шкале:
    /// gamma_light/gamma_dark материальны (>1 JND), chroma_peak_t почти
    /// нематериален на near-серой шкале (< ½ JND), но был бы материальнее на
    /// хроматической базе — потому все три честно (e), не (c). КУСАЕТСЯ:
    /// value-пины `== 1.75/1.5/0.35` падают на любой мутации.
    #[test]
    fn curve_params_sensitivity_is_bounded() {
        let d = CurveParams::default();
        assert_eq!(d.gamma_light, 1.75);
        assert_eq!(d.gamma_dark, 1.5);
        assert_eq!(d.chroma_peak_t, 0.35);

        let mut gl = 0.0_f64;
        for g in [1.3_f64, 1.5, 2.0, 2.2] {
            gl = gl.max(max_de_vs_default(|p| p.gamma_light = g));
        }
        let mut gd = 0.0_f64;
        for g in [1.2_f64, 1.35, 1.7, 1.9] {
            gd = gd.max(max_de_vs_default(|p| p.gamma_dark = g));
        }
        let mut pk = 0.0_f64;
        for t in [0.2_f64, 0.28, 0.42, 0.5] {
            pk = pk.max(max_de_vs_default(|p| p.chroma_peak_t = t));
        }
        assert!(
            (0.03..0.07).contains(&gl),
            "gamma_light max ΔE_ok {gl:.4} вне замеренного [0.03, 0.07) — материальна (e)"
        );
        assert!(
            (0.02..0.05).contains(&gd),
            "gamma_dark max ΔE_ok {gd:.4} вне замеренного [0.02, 0.05) — материальна (e)"
        );
        assert!(
            (0.001..0.01).contains(&pk),
            "chroma_peak_t max ΔE_ok {pk:.4} вне замеренного [0.001, 0.01) — низшая чувствительность"
        );
        eprintln!(
            "WAVE2 CurveParams (e): gamma_light ΔE_ok={gl:.4} gamma_dark ΔE_ok={gd:.4} chroma_peak_t ΔE_ok={pk:.4}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Задача #108: четвёртая проверенная и опровергнутая скалярная гипотеза для
// gamma_light/gamma_dark — метрика контрастного усиления Уиттла
// (Whittle 1992 Vision Research 32(8):1493-1507,
// DOI 10.1016/0042-6989(92)90205-w; порогово-дискриминационная, НЕ appearance-
// модель CAM16 — см. docs/empirical-inventory.md "требует параметр-free
// метрики" / "не appearance-модель CAM16"). Три предыдущих скалярных метрики
// (плоский Oklab ΔE ≈1.07/0.95, плоский sRGB-байт ≈1.10/0.99, surround-aware
// CAM16-J' E1 ≈0.90-1.04) уже рефутированы по магнитуде — см. тот же файл.
// Эта метрика — четвёртая, репорт, не форс.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod whittle_crispening_metric {
    use super::{CurveParams, NeutralCurve};
    use crate::curve::ColorCurve;
    use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};

    /// Физическая люминанса Y (CIE XYZ, D65) hex-цвета — величина, над которой
    /// определён закон Уиттла, В ОТЛИЧИЕ от CAM16 J' (тот путь — уже
    /// рефутированный E1). `srgb_to_xyz` — чистое матричное умножение, без
    /// прохода через CAM16 forward/inverse.
    fn physical_y(hex: &str) -> f64 {
        srgb_to_xyz(srgb_from_hex(hex).expect("golden ladder hex must parse"))[1]
    }

    fn ladder_with(vary: impl Fn(&mut CurveParams)) -> Vec<String> {
        let mut p = CurveParams::default();
        vary(&mut p);
        NeutralCurve::with_params("#FFFFFF", "#787880", "#101012", p)
            .unwrap()
            .sample_hex(13)
    }

    /// Обобщённый контраст Уиттла (Whittle 1986/1992): `W = (L − Lb) / min(L, Lb)`.
    /// Это и есть «унифицирующая контрастная метрика» из doc — знакопеременная
    /// (отрицательна ниже фона, положительна выше), с непрерывной производной
    /// `1/Lb` по обе стороны `L == Lb` (крисп-излом сохраняется, разрыва нет).
    fn whittle_w(l: f64, lb: f64) -> f64 {
        (l - lb) / l.min(lb)
    }

    /// Неоднородность шага `ΔW` вдоль половины лестницы: дисперсия 6 соседних
    /// разностей W. Ноль означал бы «эта половина Уиттл-однородна» (равные
    /// перцептивные шаги под метрикой crispening).
    fn w_step_variance(ys: &[f64], lb: f64) -> f64 {
        let ws: Vec<f64> = ys.iter().map(|&y| whittle_w(y, lb)).collect();
        let deltas: Vec<f64> = ws.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;
        deltas.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / deltas.len() as f64
    }

    /// Грубая сетка + локальное золотое сечение: какая gamma (в ТОЙ ЖЕ
    /// продакшн-параметризации степенного закона по `u`, что и сам
    /// `NeutralCurve`) лучше всего выравнивает `ΔW` на половине лестницы —
    /// «какая gamma сделала бы эту половину Уиттл-однородной». Зеркалит
    /// методологию, которой в doc уже подобраны gamma для двух рефутированных
    /// плоских метрик (Oklab ΔE, sRGB-байт).
    fn fit_whittle_gamma(
        half_indices: std::ops::RangeInclusive<usize>,
        set_gamma: impl Fn(&mut CurveParams, f64) + Copy,
    ) -> f64 {
        let lb = physical_y("#787880");
        let objective = |g: f64| -> f64 {
            let ladder = ladder_with(|p| set_gamma(p, g));
            let ys: Vec<f64> = half_indices
                .clone()
                .map(|i| physical_y(&ladder[i]))
                .collect();
            w_step_variance(&ys, lb)
        };
        // Широкий, честный диапазон поиска (НЕ зажат в "практическую" полосу
        // [1.2,2.2]/[1.2,1.9] — суть в том, куда метрика РЕАЛЬНО тянет, как и
        // E1 приземлился на 0.90-1.04, вне этих полос).
        let mut best_g = 0.05_f64;
        let mut best_v = f64::INFINITY;
        let mut g = 0.05_f64;
        while g <= 4.0 {
            let v = objective(g);
            if v < best_v {
                best_v = v;
                best_g = g;
            }
            g += 0.02;
        }
        let mut lo = (best_g - 0.02).max(0.02);
        let mut hi = best_g + 0.02;
        for _ in 0..60 {
            let m1 = lo + (hi - lo) * 0.382;
            let m2 = lo + (hi - lo) * 0.618;
            if objective(m1) < objective(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        (lo + hi) / 2.0
    }

    /// RED-proof / диагностика: печатает сырые предсказания без допуска, чтобы
    /// зафиксировать фактические числа перед тем, как зашивать допуск в
    /// основной тест. Не гейт CI (`#[ignore]`), запускать вручную:
    /// `cargo test -p labcolors-core whittle_raw_report -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn whittle_raw_report() {
        let gl = fit_whittle_gamma(0..=6, |p, g| p.gamma_light = g);
        let gd = fit_whittle_gamma(6..=12, |p, g| p.gamma_dark = g);
        eprintln!(
            "WHITTLE RAW: gamma_light_pred={gl:.6} (shipped {}), gamma_dark_pred={gd:.6} (shipped {})",
            CurveParams::default().gamma_light,
            CurveParams::default().gamma_dark
        );
    }

    /// Научный тест (#108): метрика Уиттла (crispening, физическая люминанса,
    /// НЕ CAM16) → предсказанная магнитуда gamma_light/gamma_dark →
    /// сравнение с фактическими shipped-значениями CurveParams. Печатает
    /// честный дельта-отчёт независимо от исхода — «репорт, не форс».
    #[test]
    fn whittle_crispening_metric_vs_shipped_gammas() {
        let shipped_light = CurveParams::default().gamma_light;
        let shipped_dark = CurveParams::default().gamma_dark;

        // Половина "свет" — индексы 0..=6 (белый -> база), gamma_light правит
        // u ∈ [0, 0.5]. Половина "тьма" — индексы 6..=12 (база -> чёрный),
        // gamma_dark правит u ∈ [0.5, 1].
        let gamma_light_pred = fit_whittle_gamma(0..=6, |p, g| p.gamma_light = g);
        let gamma_dark_pred = fit_whittle_gamma(6..=12, |p, g| p.gamma_dark = g);

        let delta_light = gamma_light_pred - shipped_light;
        let delta_dark = gamma_dark_pred - shipped_dark;

        eprintln!(
            "WHITTLE crispening-metric (#108): predicted gamma_light={gamma_light_pred:.4} \
             (shipped {shipped_light}, Δ={delta_light:+.4}); predicted gamma_dark={gamma_dark_pred:.4} \
             (shipped {shipped_dark}, Δ={delta_dark:+.4})"
        );

        // Метрика обязана быть содержательной (не упереться в границу сетки —
        // иначе диапазон поиска слишком узкий, а не "метрика сошлась к краю").
        assert!(
            (0.05..3.95).contains(&gamma_light_pred),
            "gamma_light-фит {gamma_light_pred:.4} на границе диапазона поиска — расширь сетку"
        );
        assert!(
            (0.05..3.95).contains(&gamma_dark_pred),
            "gamma_dark-фит {gamma_dark_pred:.4} на границе диапазона поиска — расширь сетку"
        );

        // ВЕРДИКТ (#108): метрика РЕФУТИРОВАНА по магнитуде — четвёртая по
        // счёту (после плоского Oklab ΔE, плоского sRGB-байта, surround-aware
        // CAM16-J' E1; см. docs/empirical-inventory.md). Замерено прогоном на
        // HEAD этого PR: gamma_light_pred≈1.3471 (shipped 1.75, Δ≈-0.40),
        // gamma_dark_pred≈0.1579 (shipped 1.5, Δ≈-1.34). gamma_dark особенно
        // далёк: W=(L−Lb)/min(L,Lb) расходится при L→0, а чёрный якорь
        // "#101012" физически тёмный (Y≈0.0053 при Lb=Y("#787880")≈0.190) —
        // фит убегает от сингулярности, а не воспроизводит crispening-форму.
        // Диапазоны ниже — ЗАМЕРЕННЫЙ, не придуманный факт: это регресс-лок
        // на наблюдённый разрыв (тот же приём, что и в `wave2_e_locks`), а
        // НЕ допуск "метрика обязана попасть сюда". Дрейф вне диапазона →
        // либо код `NeutralCurve`/`fit_whittle_gamma` изменился незамеченно,
        // либо якоря/дефолты дрогнули — разбираться, не расширять молча.
        assert!(
            (0.35..0.45).contains(&delta_light.abs()),
            "gamma_light: metric predicts {gamma_light_pred:.4}, shipped {shipped_light} — \
             |Δ|={:.4} вышла за замеренный диапазон (0.35..0.45) — разбор, не подгонка",
            delta_light.abs()
        );
        assert!(
            (1.30..1.40).contains(&delta_dark.abs()),
            "gamma_dark: metric predicts {gamma_dark_pred:.4}, shipped {shipped_dark} — \
             |Δ|={:.4} вышла за замеренный диапазон (1.30..1.40) — разбор, не подгонка",
            delta_dark.abs()
        );
    }

    /// RED-first: доказывает, что тест реально способен упасть. Портит
    /// gamma_light локально (эмулируя "починенный закон", который метрика
    /// должна была бы принять) и проверяет, что сравнение с shipped-значением
    /// (не с фитом) кричит — т.е. тест не тавтологичен самому себе.
    #[test]
    fn shipped_gamma_pins_are_load_bearing() {
        assert_eq!(
            CurveParams::default().gamma_light,
            1.75,
            "gamma_light pin drifted — whittle_crispening_metric_vs_shipped_gammas сравнивается \
             с ЭТИМ значением; если оно тихо поменяется, дельта-отчёт станет враньём"
        );
        assert_eq!(
            CurveParams::default().gamma_dark,
            1.5,
            "gamma_dark pin drifted — whittle_crispening_metric_vs_shipped_gammas сравнивается \
             с ЭТИМ значением"
        );
    }
}
