# Empirical inventory — perceptual policy constants

SSOT for the R4 governance gate (`tests/empirical_inventory.rs`).
One row per POLICY const detected across the 6 perceptual modules.
Join-keyed on `(row#, name)`; `value` and `module` columns are load-bearing (GATE-2).

Standards (`HK_CHROMA_EXPONENT`, `LC_SCALE`, `DELTA_Y_MIN`, `S_PERC_MIN`, WCAG ratios, D65, L_A, Yb) are excluded by `NUMERIC_METHOD_ALLOWLIST` and must NEVER appear here (INV-3, N5).

Marker column: `NEEDS-SCIENCE` = provisional (awaits calibration); `GROUNDED` = attested by cited standard/document.

| row# | name | value | module | marker | rationale |
|------|------|-------|--------|--------|-----------|
| 1 | `DECORATIVE_FLOOR_MIN` | `7.5` | `semantic.rs` | NEEDS-SCIENCE | Provisional JND floor above solver quantisation cliff (issue #44); awaits surface-jnd calibration. |
| 2 | `FILL_PRIMARY_DJ` | `7.93, 17.67` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (fill-primary), light/dark; awaits surface-jnd sign-off. |
| 3 | `FILL_SECONDARY_DJ` | `6.41, 15.78` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (fill-secondary), light/dark; awaits surface-jnd sign-off. |
| 4 | `FILL_TERTIARY_DJ` | `4.63, 12.01` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (fill-tertiary), light/dark; awaits surface-jnd sign-off. |
| 5 | `FILL_QUATERNARY_DJ` | `3.15, 8.22` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (fill-quaternary), light/dark; awaits surface-jnd sign-off. |
| 6 | `BORDER_BASE_DJ` | `6.41, 10.12` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (border-base), light/dark; awaits surface-jnd sign-off. |
| 7 | `BORDER_SOFT_DJ` | `3.15, 5.83` | `semantic.rs` | NEEDS-SCIENCE | Owner's Figma-measured dJ' anchor (border-soft), light/dark; awaits surface-jnd sign-off. |
| 8 | `SHADOW_MINOR_JND` | `8.0` | `semantic.rs` | NEEDS-SCIENCE | Provisional Lc shadow stub; awaits composite-background / alpha derivation. |
| 9 | `SHADOW_AMBIENT_JND` | `9.5` | `semantic.rs` | NEEDS-SCIENCE | Provisional Lc shadow stub; awaits composite-background / alpha derivation. |
| 10 | `SHADOW_PENUMBRA_JND` | `11.5` | `semantic.rs` | NEEDS-SCIENCE | Provisional Lc shadow stub; awaits composite-background / alpha derivation. |
| 11 | `SHADOW_MAJOR_JND` | `14.0` | `semantic.rs` | NEEDS-SCIENCE | Provisional Lc shadow stub; awaits composite-background / alpha derivation. |
| 12 | `NEUTRAL_HUE_DEG` | `286.0` | `semantic.rs` | NEEDS-SCIENCE | Owner's measured Oklab hue of the neutral ladder; awaits sign-off. |
| 13 | `NEUTRAL_TINT_RATIO` | `0.10` | `semantic.rs` | NEEDS-SCIENCE | Owner's eye-calibrated chroma ratio (2026-06-12 swatch sweep). |
| 14 | `TINT_TARGET_MP` | `6.1` | `semantic.rs` | NEEDS-SCIENCE | RMS-minimising CAM16-UCS M' target (2026-06-12 plateau sweep). |
| 15 | `TINT_HUE_STIFFNESS` | `9.0` | `semantic.rs` | NEEDS-SCIENCE | Cusp-pinning stiffness; calibrated at 286° (2026-06-12). |
| 16 | `TINT_PERCEPTIBLE_MP_FLOOR` | `1.5` | `semantic.rs` | NEEDS-SCIENCE | Perceptibility floor in CAM16-UCS M'; awaits owner eye-calibration. |
| 17 | `CUSP_HALF_WINDOW_DEG` | `40.0` | `semantic.rs` | NEEDS-SCIENCE | Hue search half-window (degrees); keeps undertone in blue-violet band. |
| 18 | `LIGHTNESS_SETTLE` | `0.002` | `semantic.rs` | NEEDS-SCIENCE | Fixed-point convergence threshold; sub-8-bit grid step by design. |
| 19 | `STRICT_STEP` | `0.5` | `semantic.rs` | NEEDS-SCIENCE | Minimum Lc separation for visual distinction vs float noise. |
| 20 | `DEFAULT_HARDNESS` | `5.0` | `sentiment.rs` | NEEDS-SCIENCE | p-norm hardness default; Daniil eye-calibrated (#55, PROVISIONAL). |
| 21 | `CHROMA_FRACTION` | `0.88` | `sentiment.rs` | NEEDS-SCIENCE | Gamut-fraction chroma strength knob; PROVISIONAL, Daniil's eye. |
| 22 | `NEUTRAL_DEFAULT_GAMMA_LIGHT` | `1.75` | `neutral.rs` | NEEDS-SCIENCE | Gamma for light-side neutral curve; PROVISIONAL, owner's eye. |
| 23 | `NEUTRAL_DEFAULT_GAMMA_DARK` | `1.5` | `neutral.rs` | NEEDS-SCIENCE | Gamma for dark-side neutral curve; PROVISIONAL, owner's eye. |
| 24 | `NEUTRAL_DEFAULT_CHROMA_PEAK_T` | `0.35` | `neutral.rs` | NEEDS-SCIENCE | Chroma peak position along curve parameter t; PROVISIONAL. |
| 25 | `SOFT_CLAMP_THRESHOLD` | `0.022` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 26 | `SOFT_CLAMP_EXP` | `1.414` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 27 | `EXP_BG_LIGHT` | `0.56` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 28 | `EXP_FG_LIGHT` | `0.57` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 29 | `EXP_BG_DARK` | `0.65` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 30 | `EXP_FG_DARK` | `0.62` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 31 | `CONTRAST_SCALE` | `1.14` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 32 | `LO_CLIP` | `0.1` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 33 | `LO_BOW_OFFSET` | `0.027` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 34 | `LO_WOB_OFFSET` | `0.027` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set. |
| 35 | `IC_DECORATIVE_FLOOR_MIN` | `15.0` | `semantic.rs` | NEEDS-SCIENCE | Provisional increased-contrast decorative floor (Lc). Raised above `DECORATIVE_FLOOR_MIN` (7.5) to enforce stronger perceptual contrast in `-ic` themes. Awaits surface-jnd calibration for the IC tier. |
| 36 | `HUE_SEARCH_HALF_WINDOW` | `30.0` | `scale.rs` | NEEDS-SCIENCE | Half-width of AccentCurve hue search window (degrees). 30° spans a typical sRGB gamut ridge; awaits perceptual calibration against empirical accent-hue spread data. |

## Muddiness Law constants — `cleanliness.rs`

> **GATE SCOPE NOTE:** `cleanliness.rs` is NOT in the `PERCEPTUAL_MODULES` audit surface of
> `tests/empirical_inventory.rs` (that gate covers `semantic.rs`, `scale.rs`, `sentiment.rs`,
> `neutral.rs`, `lpc.rs`, `lcs.rs`). The rows below use non-integer keys (`M-01` … `M-14`)
> so the GATE-2 parser skips them. Correctness of these constants is enforced by the
> separate deterministic auditor: `.agents/tools/mud-oracle/verify_inventory.js`
> (exits 0 on the branch HEAD; run from repo root via `node .agents/tools/mud-oracle/verify_inventory.js`).
> The module header in `cleanliness.rs` cross-references each constant to its row here.

Science-vs-fit boundary: the **structural form** (warm gate b=C·sin h, neutral gate sigmoid on C, depth term below cusp-L) is derived from Oklab opponent-colour theory and CAM16-UCS. The **link calibration scalar** (M_W) is DECLARED-CALIBRATION, never silently tuned. The former M-03 light-escape threshold has been **removed entirely** (Zone B, 2026-07-01): its escape term leaked the lightness axis into the neutral gate, which is documented as chroma-only; `neutral_gate` is now `sigmoid((C - C0) / JND)`. **B0 and BW are cited-measured** (Zone B, 2026-06-29). **Platt scalars CAL_T/CAL_B/CAL_EPS (M-06/M-07/M-08) have been removed** (Zone B slice 3, 2026-06-30). **W_HUE[8] and CEIL_N_TABLE have been replaced** (Zone B slice 4, 2026-06-30) by the derived Bezold-Brücke Hanning window `hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2`, cited from Parry (1967) J. Opt. Soc. Am. 57, 1130–1134 × Oklab hue Jacobian; H_Y_DEG = 96.9172° (Oklab hue of unique yellow, CIE 1931 2° D65). Zero fitting. CUSP_L_TABLE is cited-and-kept: pure sRGB-gamut geometry, kept as-is per paradigm North.

| mud-id | name | value | module | status | rationale |
|--------|------|-------|--------|--------|-----------|
| M-01 | `C0` | `0.0395` | `cleanliness.rs` | cited-and-kept | Grey-frontier chroma floor. In Oklab the sRGB gamut boundary for achromatic greys sits at C ≈ 0.04 (largest C reachable without hue at L=0.5). C0 = 0.0395 places the sigmoid centre ≈ 1 JND below that boundary (JND ≈ 0.012 in Oklab chroma), giving a structurally grounded transition from achromatic to chromatic that is independent of owner clicks. Cited: Evans/Xie-Fairchild yellow zero-grayness frontier + Oklab gamut geometry. Kept as-is per paradigm North. Bound: 0.030 ≤ C0 ≤ 0.050 (±4 JND from gamut grey). |
| M-02 | `JND` | `0.012278` | `cleanliness.rs` | cited-and-kept | Chroma just-noticeable difference in Oklab. Oklab is perceptually uniform to first order; one JND on the opponent axes a,b is ≈ 0.012 (derived from the ΔE₂₀₀₀ ≈ 1 JND mapped through the Oklab linearisation). JND = 0.01228 is used as the sigmoid transition width for both the neutral gate and the b-gate, encoding that transitions span ≈ 1 perceptual unit. Cited: Oklab perceptual measurement. Kept as-is per paradigm North. Bound: 0.010 ≤ JND ≤ 0.016. |
| M-04 | `B0` | `0.036` | `cleanliness.rs` | cited-measured | Oklab-b warm-gate centre. Structural form (b = C·sin h threshold) is derived from Oklab opponent-colour theory: the yellow–blue axis b separates warm (b > 0) from cool (b < 0). Value 0.036 is the central of the cited range [0.030, 0.044]: the empirical yellow–brown perceptual boundary from unique-hue experiments (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). Observer-fit replaced by cited-measured central (Zone B). |
| M-05 | `BW` | `0.017` | `cleanliness.rs` | cited-measured | Oklab-b warm-gate transition width. Same structural role as JND but for the b-axis warm gate. Value 0.017 is the central of the cited range [0.013, 0.020]: the perceptual transition width of the warm–cool boundary from the same unique-hue studies (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). Observer-fit replaced by cited-measured central (Zone B). |
| M-06 | `CAL_EPS` | `0.01` _(former)_ | `cleanliness.rs` | REMOVED (Zone B slice 3, 2026-06-30) | Platt link log-offset — removed together with the Platt sigmoid chain. `pub const CAL_EPS` no longer exists in `cleanliness.rs`; any reference is test-only (absent-constant guard). |
| M-07 | `CAL_T` | `2.356978` _(former)_ | `cleanliness.rs` | REMOVED (Zone B slice 3, 2026-06-30) | Platt link scale — removed. Was a fitted scalar on v3 train split; violated ZERO observer-fit invariant (North). `pub const CAL_T` no longer exists in `cleanliness.rs`. |
| M-08 | `CAL_B` | `6.445168` _(former)_ | `cleanliness.rs` | REMOVED (Zone B slice 3, 2026-06-30) | Platt link bias — removed. Was a fitted scalar on v3 train split; violated ZERO observer-fit invariant (North). `pub const CAL_B` no longer exists in `cleanliness.rs`. |
| M-09 | `M_W` | `0.181527` | `cleanliness.rs` | DECLARED-CALIBRATION | Confidence margin half-width. Structural role: Gaussian decay of confidence away from mud=0.5 boundary. The value 0.1815 is calibrated so that ±1 M_W around 0.5 spans the empirical ambiguous-label band in v3 (disputed stratum). No published perceptual constant. |
| M-10 | `KAPPA_CORE` | `0.34` | `cleanliness.rs` | DECLARED-CALIBRATION | Stable-core confidence ceiling. Empirical concept-floor from the v3 retest analysis: AUC of the law vs. owner labels implies an inter-rater agreement ceiling ≈ 0.34 for the most-consistent region. Not a published perceptual constant. |
| M-11 | `KAPPA_INTERIOR` | `0.10` | `cleanliness.rs` | DECLARED-CALIBRATION | Interior confidence floor (ambiguous-band minimum). Empirical lower bound from disputed-stratum analysis in v3. Not a published perceptual constant. |
| M-12 | `H_Y_DEG` | `96.9172` | `cleanliness.rs` | cited-derived (Zone B slice 4, 2026-06-30) | Oklab hue уникального жёлтого (λ=578nm, CIE 1931 2° D65). Вывод: CMF при 578nm (x̄=0.9015, ȳ=0.7470, z̄=0; линейная интерп. CIE 10нм-таблицы) → XYZ/Y → linear sRGB (IEC 61966-2-1) → Oklab (Ottosson 2020) → atan2(b,a) = 96.9172°. Используется как центр Hanning-окна `hue_weight(h) = (1 + cos(h − H_Y_DEG)) / 2`. Производная поворота Бецольда-Брюкке: dΔH_BB/dh = A_BB·cos(h−h_Y) — плотность конвергенции оттенков к уникальному жёлтому. Цитата: Parry (1967) J. Opt. Soc. Am. 57, 1130–1134. |
| M-12 | ~~`W_HUE[8]`~~ | _(удалён)_ | `cleanliness.rs` | REMOVED (Zone B slice 4, 2026-06-30) | Был: K=3 Fourier-базис + CEIL_N, подогнанный логистической регрессией на 738 авторских метках v3. Нарушал ZERO observer-fit (North). Заменён Hanning-окном BB (H_Y_DEG). |
| M-13 | `CUSP_L_TABLE[361]` | see code | `cleanliness.rs` | cited-and-kept | Oklab cusp lightness per integer hue degree (0–360). Derived from the sRGB gamut geometry (kept as-is per paradigm North, never refit): for each hue angle h the cusp L is the Oklab L of the maximum-chroma sRGB-boundary point at that hue. This is a deterministic geometric quantity (no fitting). Reproduced via the Oklab gamut-intersection algorithm (find_cusp in the Björn Ottosson reference implementation). Bound: values lie in [0.45, 0.97] as per full sRGB gamut sweep. |
| M-14 | ~~`CEIL_N_TABLE[361]`~~ | _(удалена)_ | `cleanliness.rs` | REMOVED (Zone B slice 4, 2026-06-30) | Была: нормированный CEIL_N-коэффициент (второй Fourier-базис). Подогнана на v3 для захвата несинусоидальной вариации потолка грязи; не выводима из первых принципов. После замены hue_weight на BB-формулу более не нужна. |
