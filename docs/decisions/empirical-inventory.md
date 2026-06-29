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
| 25 | `SOFT_CLAMP_THRESHOLD` | `0.022` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 26 | `SOFT_CLAMP_EXP` | `1.414` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 27 | `EXP_BG_LIGHT` | `0.56` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 28 | `EXP_FG_LIGHT` | `0.57` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 29 | `EXP_BG_DARK` | `0.65` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 30 | `EXP_FG_DARK` | `0.62` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 31 | `CONTRAST_SCALE` | `1.14` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 32 | `LO_CLIP` | `0.1` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 33 | `LO_BOW_OFFSET` | `0.027` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |
| 34 | `LO_WOB_OFFSET` | `0.027` | `lpc.rs` | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published set; see docs/decisions/apca-license.md. |

## Muddiness Law constants — `cleanliness.rs`

> **GATE SCOPE NOTE:** `cleanliness.rs` is NOT in the `PERCEPTUAL_MODULES` audit surface of
> `tests/empirical_inventory.rs` (that gate covers `semantic.rs`, `scale.rs`, `sentiment.rs`,
> `neutral.rs`, `lpc.rs`, `lcs.rs`). The rows below use non-integer keys (`M-01` … `M-14`)
> so the GATE-2 parser skips them. Correctness of these constants is enforced by the
> separate deterministic auditor: `.agents/tools/mud-oracle/verify_inventory.js`
> (exits 0 on the branch HEAD; run from repo root via `node .agents/tools/mud-oracle/verify_inventory.js`).
> The module header in `cleanliness.rs` cross-references each constant to its row here.

Science-vs-fit boundary: the **structural form** (warm gate b=C·sin h, neutral gate sigmoid on C, depth term below cusp-L, Platt link) is derived from Oklab opponent-colour theory and CAM16-UCS. The **link calibration scalars** (CAL_T, CAL_B, CAL_EPS, M_W, B0, BW, LESC, W_HUE, CEIL_N_TABLE) are Platt-fit scalars on the v3 labelled dataset; they are explicitly declared as DECLARED-CALIBRATION, never silently tuned. CUSP_L_TABLE is cited-and-kept: pure sRGB-gamut geometry (maximum chroma per hue degree in Oklab), kept as-is per paradigm North.

| mud-id | name | value | module | status | rationale |
|--------|------|-------|--------|--------|-----------|
| M-01 | `C0` | `0.0395` | `cleanliness.rs` | cited-and-kept | Grey-frontier chroma floor. In Oklab the sRGB gamut boundary for achromatic greys sits at C ≈ 0.04 (largest C reachable without hue at L=0.5). C0 = 0.0395 places the sigmoid centre ≈ 1 JND below that boundary (JND ≈ 0.012 in Oklab chroma), giving a structurally grounded transition from achromatic to chromatic that is independent of owner clicks. Cited: Evans/Xie-Fairchild yellow zero-grayness frontier + Oklab gamut geometry. Kept as-is per paradigm North. Bound: 0.030 ≤ C0 ≤ 0.050 (±4 JND from gamut grey). |
| M-02 | `JND` | `0.012278` | `cleanliness.rs` | cited-and-kept | Chroma just-noticeable difference in Oklab. Oklab is perceptually uniform to first order; one JND on the opponent axes a,b is ≈ 0.012 (derived from the ΔE₂₀₀₀ ≈ 1 JND mapped through the Oklab linearisation). JND = 0.01228 is used as the sigmoid transition width for both the neutral gate and the b-gate, encoding that transitions span ≈ 1 perceptual unit. Cited: Oklab perceptual measurement. Kept as-is per paradigm North. Bound: 0.010 ≤ JND ≤ 0.016. |
| M-03 | `LESC` | `0.820855` | `cleanliness.rs` | DECLARED-CALIBRATION | Light-escape lightness threshold. Structural role is science-derived: very high Oklab L pushes colours toward paper-white regardless of hue, destroying warm-pocket dirtiness (see Oklab L ≥ 0.85 → OKHSL S collapses). The threshold value 0.8209 is a Platt-fit scalar on the v3 dataset, not a published perceptual constant. Declared owner-debt: could be set to 0.85 from first principles; current value is 0.0041 below that, within 0.5 JND. |
| M-04 | `B0` | `0.028690` | `cleanliness.rs` | fitted-pending-cited-range | Oklab-b warm-gate centre. Structural form (b = C·sin h threshold) is derived from Oklab opponent-colour theory: the yellow–blue axis b separates warm (b > 0) from cool (b < 0). The centre value 0.02869 is Platt-fit on v3 labels; not derivable from theory alone (depends on the empirical boundary of the owner's dirty pocket). Target cited range [0.030, 0.044] central 0.036 (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). |
| M-05 | `BW` | `0.020241` | `cleanliness.rs` | fitted-pending-cited-range | Oklab-b warm-gate transition width. Same structural role as JND but for the b-axis warm gate. Value ≈ 1.65 × JND (slightly wider than the neutral gate) reflects that the warm–cool boundary is perceptually softer than the achromatic boundary; fitted on v3. Target cited range [0.013, 0.020] central 0.017 (Newhall-Nickerson-Judd 1943; Lindsey-Brown 2014 PNAS; Boynton 1975). |
| M-06 | `CAL_EPS` | `0.01` | `cleanliness.rs` | DECLARED-CALIBRATION | Platt link log-offset (prevents ln(0) for achromatic colours). Structural necessity (log regularisation); value 0.01 is a conventional epsilon chosen to be well below the meaningful raw-score range (raw > 0.05 for detectable warm colours). Not a perceptual constant. |
| M-07 | `CAL_T` | `2.356978` | `cleanliness.rs` | OPEN (flagged-provisional) | Platt link scale (sigmoid temperature on log-raw). Fitted scalar on v3 train split (logistic regression on ln(raw + eps)). No science derivation; pure calibration. Resolving study: CAL_T/CAL_B olive-brown naming-crossing (Zone C). |
| M-08 | `CAL_B` | `6.445168` | `cleanliness.rs` | OPEN (flagged-provisional) | Platt link bias (sigmoid intercept on log-raw). Fitted scalar on v3 train split. No science derivation; pure calibration. Resolving study: CAL_T/CAL_B olive-brown naming-crossing (Zone C). |
| M-09 | `M_W` | `0.181527` | `cleanliness.rs` | DECLARED-CALIBRATION | Confidence margin half-width. Structural role: Gaussian decay of confidence away from mud=0.5 boundary. The value 0.1815 is calibrated so that ±1 M_W around 0.5 spans the empirical ambiguous-label band in v3 (disputed stratum). No published perceptual constant. |
| M-10 | `KAPPA_CORE` | `0.34` | `cleanliness.rs` | DECLARED-CALIBRATION | Stable-core confidence ceiling. Empirical concept-floor from the v3 retest analysis: AUC of the law vs. owner labels implies an inter-rater agreement ceiling ≈ 0.34 for the most-consistent region. Not a published perceptual constant. |
| M-11 | `KAPPA_INTERIOR` | `0.10` | `cleanliness.rs` | DECLARED-CALIBRATION | Interior confidence floor (ambiguous-band minimum). Empirical lower bound from disputed-stratum analysis in v3. Not a published perceptual constant. |
| M-12 | `W_HUE[8]` | see code | `cleanliness.rs` | OPEN (flagged-provisional) | Fourier hue-basis dot-product weights (K=3 cosine/sine basis + constant + CEIL_N). Fitted via logistic regression on v3 train split to encode the band-limited hue dependence of dirtiness (peaking in the warm-yellow/olive pocket). Structural basis form (Fourier, K=3) chosen to avoid overfitting; weights are pure fit outputs. Resolving study: drab L-tilt grayness magnitude-estimation (Zone C). |
| M-13 | `CUSP_L_TABLE[361]` | see code | `cleanliness.rs` | cited-and-kept | Oklab cusp lightness per integer hue degree (0–360). Derived from the sRGB gamut geometry (kept as-is per paradigm North, never refit): for each hue angle h the cusp L is the Oklab L of the maximum-chroma sRGB-boundary point at that hue. This is a deterministic geometric quantity (no fitting). Reproduced via the Oklab gamut-intersection algorithm (find_cusp in the Björn Ottosson reference implementation). Bound: values lie in [0.45, 0.97] as per full sRGB gamut sweep. |
| M-14 | `CEIL_N_TABLE[361]` | see code | `cleanliness.rs` | DECLARED-CALIBRATION | Normalised hue-ceiling term used as the second Fourier basis coefficient (CEIL_N_TABLE[h] = basis[1]). Fitted from v3 data to capture non-sinusoidal hue variation in the muddiness ceiling; not derivable from first principles. Values span approximately [−1.32, 1.84] reflecting the asymmetric hue response of the warm pocket. |
