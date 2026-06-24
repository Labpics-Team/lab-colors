# Empirical inventory — perceptual policy constants

SSOT for the R4 governance gate (`tests/empirical_inventory.rs`).
One row per POLICY const detected across the 6 perceptual modules.
Join-keyed on `(row#, name)`; `value` and `module` columns are load-bearing (GATE-2).

Standards (`HK_CHROMA_EXPONENT`, `LC_SCALE`, `DELTA_Y_MIN`, `S_PERC_MIN`, WCAG ratios, D65, L_A, Yb) are excluded by `NUMERIC_METHOD_ALLOWLIST` and must NEVER appear here (INV-3, N5).

Marker column: `NEEDS-SCIENCE` = provisional (awaits calibration); `GROUNDED` = attested by cited standard/document.

| row# | name | value | module | marker | rationale |
|------|------|-------|--------|--------|-----------|
| 1 | `DECORATIVE_FLOOR_MIN` | `7.6` | `semantic.rs` | NEEDS-SCIENCE | Provisional JND floor above solver quantisation cliff (issue #44); awaits surface-jnd calibration. |
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
