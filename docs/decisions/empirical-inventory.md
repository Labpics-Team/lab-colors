# Empirical inventory — perceptual POLICY constants (SSOT)

This is the single source of truth for the **R4 hygiene/governance** regime: every
numeric perceptual-**policy** magnitude in the six perceptual modules
(`semantic.rs`, `scale.rs`, `sentiment.rs`, `neutral.rs`, `lpc.rs`, `lcs.rs`)
must carry a paper-trail — an in-source marker (`// NEEDS-SCIENCE` or
`// GROUNDED`) **and** a row in the table below.

A "policy magnitude" is detected **type-agnostically** across three shapes, so it
cannot hide behind its syntax:

1. `const NAME: <any-type> = …` — including non-`f64` numeric consts.
2. `const NAME: DjMagnitude = DjMagnitude::new(light, dark)` — a two-`f64` anchor;
   its `value` column carries the `light, dark` pair.
3. `field: <float-literal>` inside a `fn default()` body — synthesised into a
   stable name `<MODULE>_DEFAULT_<FIELD>` (e.g. `NEUTRAL_DEFAULT_GAMMA_LIGHT`).

Two named allowlists keep non-perceptual numerics out by construction:
`NUMERIC_METHOD_ALLOWLIST` (upstream standards / derivation-identities / pure EPS)
and `STRUCTURAL_NONPOLICY_ALLOWLIST` (structural knobs such as the curve-plan cache
capacity and the refine-step count). Any non-`f64` numeric const that is on
*neither* list surfaces as an untracked policy site and fails GATE 1 — so a future
`u32`/`f32` perceptual threshold cannot slip past the gate by its type.

The gate `crates/labcolors-core/tests/empirical_inventory.rs` enforces this:

- **GATE 1** — every detected policy magnitude (const of any type, `DjMagnitude`
  anchor, or `fn default()` field literal) has a marker within a 2-line lookback
  (no magic number without a paper-trail).
- **GATE 2** — every markered const has a row, every row resolves to a
  currently-detected const (no stale rows after line-drift), **and** each row's
  `value` and `module` columns match the source literal / file (the documented
  empirical value/location is the value/location in use). Keyed on `(row#, name)`,
  so reformatting that moves lines does not break it.
- **GATE 3** — the `// NEEDS-SCIENCE` ↔ provisional-row contract holds both
  ways.
- **join-key sanity** — `(row#, name)` and `row#` are unique and resolve.
- **standards-exclusion** — known upstream-standard names (Hellwig 0.587, APCA
  `LC_SCALE`/`deltaYmin`, CIECAM16 `L_A`, UCS `Yb`, D65, WCAG ratio, …) are
  excluded *by construction* via the test's `NUMERIC_METHOD_ALLOWLIST` and must
  never appear as a row.

### What this gate does NOT assert (regime separation, INV-7)

It never reads the *magnitude* of a value — only the presence of its
paper-trail. Math-vs-paper (R1) is `golden_tests`; derivation-identity (R2) is
the recompute tests; behavioural value-drift (R3) is the `sample_hex` /
`resolve_set` snapshots. Conflating these into a flat failure count is a
reporting error.

### Marker semantics

- **`GROUNDED`** — the value is pinned by a cited upstream standard or a
  published constant set. It is not a free policy knob; changing it would
  diverge from the standard.
- **`NEEDS-SCIENCE`** — a provisional policy literal calibrated by the owner's
  eye or chosen by inspection, with no formal/published derivation yet. The
  value is honest about its unit and source but awaits real JND / perceptual
  calibration.

> The two APCA standard constants the curve also uses — `LC_SCALE` (`100.0`) and
> `DELTA_Y_MIN` (`0.0005`) — are standards excluded by construction (they sit in
> the test allowlist) and therefore carry **no** row here, by design.

## Inventory

| row# | name | value | module | marker | rationale |
|------|------|-------|--------|--------|-----------|
| 1 | `DECORATIVE_FLOOR_MIN` | 7.6 | semantic.rs | NEEDS-SCIENCE | Provisional quantisation-cliff floor (issue #44); held until real JND calibration. |
| 2 | `SHADOW_MINOR_JND` | 8.0 | semantic.rs | NEEDS-SCIENCE | Provisional Lc shadow placeholder; awaits surface-jnd alpha derivation. |
| 3 | `SHADOW_AMBIENT_JND` | 9.5 | semantic.rs | NEEDS-SCIENCE | Provisional Lc shadow placeholder; awaits surface-jnd alpha derivation. |
| 4 | `SHADOW_PENUMBRA_JND` | 11.5 | semantic.rs | NEEDS-SCIENCE | Provisional Lc shadow placeholder; awaits surface-jnd alpha derivation. |
| 5 | `SHADOW_MAJOR_JND` | 14.0 | semantic.rs | NEEDS-SCIENCE | Provisional Lc shadow placeholder; awaits surface-jnd alpha derivation. |
| 6 | `NEUTRAL_HUE_DEG` | 286.0 | semantic.rs | NEEDS-SCIENCE | Empirical Oklab hue measured on the owner's neutral anchors; no published derivation. |
| 7 | `NEUTRAL_TINT_RATIO` | 0.10 | semantic.rs | NEEDS-SCIENCE | Owner-calibrated optimum picked by eye (sweep 0.04 / 0.08 / 0.12, 2026-06-12). |
| 8 | `TINT_TARGET_MP` | 6.1 | semantic.rs | NEEDS-SCIENCE | Calibrated single-scalar fit to the owner's CAM16-UCS reference plateau; not a published derivation. |
| 9 | `TINT_HUE_STIFFNESS` | 9.0 | semantic.rs | NEEDS-SCIENCE | Empirical regime choice inside the pinned hue band; no formal derivation. |
| 10 | `TINT_PERCEPTIBLE_MP_FLOOR` | 1.5 | semantic.rs | NEEDS-SCIENCE | Approximate perceptibility threshold in M' units; awaits JND calibration. |
| 11 | `CUSP_HALF_WINDOW_DEG` | 40.0 | semantic.rs | NEEDS-SCIENCE | Hue-search bound chosen by inspection; no formal derivation. |
| 12 | `LIGHTNESS_SETTLE` | 0.002 | semantic.rs | NEEDS-SCIENCE | Fixed-point convergence tolerance chosen below the 8-bit grid step; not derived. |
| 13 | `STRICT_STEP` | 0.5 | semantic.rs | NEEDS-SCIENCE | Distinguishability threshold separating real distinction from float noise; awaits JND work. |
| 14 | `DEFAULT_HARDNESS` | 2.0 | sentiment.rs | NEEDS-SCIENCE | Asymptote hardness p, owner-calibrated by eye; no formal JND derivation. |
| 15 | `CHROMA_FRACTION` | 0.88 | sentiment.rs | NEEDS-SCIENCE | Provisional strength knob (fraction of in-gamut max chroma), owner-calibrated by eye. |
| 16 | `SOFT_CLAMP_THRESHOLD` | 0.022 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `blkThrs` (docs/decisions/apca-license.md). |
| 17 | `SOFT_CLAMP_EXP` | 1.414 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `blkClmp` (docs/decisions/apca-license.md). |
| 18 | `EXP_BG_LIGHT` | 0.56 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `normBG` (docs/decisions/apca-license.md). |
| 19 | `EXP_FG_LIGHT` | 0.57 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `normTXT` (docs/decisions/apca-license.md). |
| 20 | `EXP_BG_DARK` | 0.65 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `revBG` (docs/decisions/apca-license.md). |
| 21 | `EXP_FG_DARK` | 0.62 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `revTXT` (docs/decisions/apca-license.md). |
| 22 | `CONTRAST_SCALE` | 1.14 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `scaleBoW`==`scaleWoB` (docs/decisions/apca-license.md). |
| 23 | `LO_CLIP` | 0.1 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published low-contrast clip `loClip` (docs/decisions/apca-license.md). |
| 24 | `LO_BOW_OFFSET` | 0.027 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `loBoWoffset` (docs/decisions/apca-license.md). |
| 25 | `LO_WOB_OFFSET` | 0.027 | lpc.rs | GROUNDED | APCA SAPC-8 `0.0.98G-4g` published constant `loWoBoffset` (docs/decisions/apca-license.md). |
| 26 | `ANCHOR_FRACTION_PRIMARY` | 0.968 | semantic.rs | NEEDS-SCIENCE | Label-primary / border-strong anchor fraction calibrated against Daniel's Figma "Labels/Neutral" on white; awaits the owner's eye sign-off. |
| 27 | `ANCHOR_FRACTION_SECONDARY` | 0.627 | semantic.rs | NEEDS-SCIENCE | Label-secondary anchor fraction calibrated against Daniel's Figma anchors; awaits the owner's eye sign-off. |
| 28 | `ANCHOR_FRACTION_MUTED` | 0.461 | semantic.rs | NEEDS-SCIENCE | Muted/icon anchor fraction calibrated against Daniel's Figma anchors; awaits the owner's eye sign-off. |
| 29 | `ANCHOR_FRACTION_DISABLED` | 0.276 | semantic.rs | NEEDS-SCIENCE | Disabled (label-quaternary) anchor fraction calibrated against Daniel's Figma anchors; awaits the owner's eye sign-off. |
| 30 | `SEPARATOR_DECORATIVE_LC` | 8.0 | semantic.rs | NEEDS-SCIENCE | Provisional Lc separator placeholder above `DECORATIVE_FLOOR_MIN`; awaits the owner's dJ' anchor (surface-jnd). |
| 31 | `FILL_PRIMARY_DJ` | 7.93, 17.67 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for fill-primary; awaits surface-jnd eye calibration. |
| 32 | `FILL_SECONDARY_DJ` | 6.41, 15.78 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for fill-secondary; awaits surface-jnd eye calibration. |
| 33 | `FILL_TERTIARY_DJ` | 4.63, 12.01 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for fill-tertiary; awaits surface-jnd eye calibration. |
| 34 | `FILL_QUATERNARY_DJ` | 3.15, 8.22 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for fill-quaternary; awaits surface-jnd eye calibration. |
| 35 | `BORDER_BASE_DJ` | 6.41, 10.12 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for border-base; awaits surface-jnd eye calibration. |
| 36 | `BORDER_SOFT_DJ` | 3.15, 5.83 | semantic.rs | NEEDS-SCIENCE | Owner's literal Figma-computed dJ' anchor (light, dark) for border-soft; awaits surface-jnd eye calibration. |
| 37 | `NEUTRAL_DEFAULT_GAMMA_LIGHT` | 1.75 | neutral.rs | NEEDS-SCIENCE | Owner-calibrated light-arm gamma in `CurveParams::default`; awaits JND derivation. |
| 38 | `NEUTRAL_DEFAULT_GAMMA_DARK` | 1.5 | neutral.rs | NEEDS-SCIENCE | Owner-calibrated dark-arm gamma in `CurveParams::default`; awaits JND derivation. |
| 39 | `NEUTRAL_DEFAULT_CHROMA_PEAK_T` | 0.35 | neutral.rs | NEEDS-SCIENCE | Owner-calibrated chroma-peak position in `CurveParams::default`; awaits JND derivation. |
