# Surface JND — decorative-contract magnitudes (derive-to-root)

Status: **accepted (partial) — shadow ramp HARD BLOCKED on the non-solid-backgrounds chapter**
Date: 2026-06-21
Scope: the perceptual magnitudes of the LPC **decorative** contract in
`crates/labcolors-core/src/semantic.rs` — `DECORATIVE_FLOOR_MIN`, the
`Role::Separator` hairline JND, and the shadow ramp
`SHADOW_MINOR/AMBIENT/PENUMBRA/MAJOR_JND`.

Owner directive: **no magic numbers, jeweler precision, no offloading to eye /
tolerance.** Every magnitude is either *derived-to-root* from the engine's own
math, *authoritatively-sourced* (≥2 citations), or a *HARD BLOCKER* with the
exact open scientific question. Nothing is back-fitted to the current
placeholders.

Owner intent (Graphiti): *"minimally perceptible delta — visible just enough,
never over-separated (over-separation = dirt)."*

---

## 0. The metric the decorative contract is written in

The decorative contract is **ours (LPC)**, not WCAG/APCA — but its unit *is*
APCA Lc. `lpc.rs::contrast_core` is a faithful port of the APCA/SAPC
perceptual-contrast curve fed the **H-K-corrected** luminance `Y_hk`
(Hellwig-2022, `lpc.rs::j_hk_from_xyz`), and it reproduces the canonical APCA
reference **black-on-white ≈ 106.04** bit-for-bit
(`lpc.rs:463`, `golden_tests.rs:230`). Therefore APCA's *own author-published*
Lc thresholds define the metric — they are **PRIMARY** sources here, not
external oracles. WCAG ratios appear only as the polarity gate
(`POLARITY_FLOOR_RATIO`, `semantic.rs:209`).

Key property, stated by the metric's author and inherited by the engine:
**APCA-Lc is perceptually uniform by construction.** The soft black clamp
(`lpc.rs:209`) plus the asymmetric power curve (`lpc.rs:290`/`298`) absorb the
Weber/Fechner non-linearity. Consequence that decides the shadow-ramp model:
**equal perceived steps are equal *additive* +Lc steps in this space.** The
Weber-geometric (constant-ratio) reasoning applies to *raw luminance* — the
input to `soft_clamp` — **not** to the Lc output the contract is written in.
(Verified against Whittle 1986: Weber's law `ΔL/L = const` holds *in luminance*;
APCA folds exactly that curvature into Lc. See §5.)

---

> **Floor and separator ratified by epic `separator-tracks-jnd-floor`.**
> `DECORATIVE_FLOOR_MIN = 15.0` and `Role::Separator = decorative(DECORATIVE_FLOOR_MIN)`
> are now ratified constants (scope `raise-floor-and-pin-separator`). The §1b sourced
> *thin-line invisibility threshold* (Lc 15) and every §1a engine fact (the 7.30 Lc
> analytic clip, the 7.6 grid description) remain ground truth. The shadow ramp
> (`SHADOW_*_JND`) remains **TBD — filled by scope `shadow-ramp-derivation`**; its
> `HARD BLOCKER` status (non-solid-backgrounds chapter) is unchanged.

## 1. `DECORATIVE_FLOOR_MIN` — VERDICT: **derived-to-root (engine) + authoritatively-sourced** (chosen value **15.0**, ratified)

### 1a. What the current 7.6 actually is — derived to root from engine math

The placeholder `DECORATIVE_FLOOR_MIN = 7.6` (`semantic.rs:152`) is **not a
JND**. It falls straight out of the LPC low-contrast clip and the 8-bit sRGB
lattice, with **zero perceptual content**:

```
lpc.rs:198  LO_CLIP        = 0.1     (APCA loClip — scaled deltas inside ±0.1 → 0)
lpc.rs:200  LO_BOW_OFFSET  = 0.027   (APCA loBoWoffset, normal polarity)
lpc.rs:204  LC_SCALE       = 100.0   (offset-contrast → Lc range)
lpc.rs:280-305  contrast_core(): normal-polarity branch returns
                0 if sapc < LO_CLIP, else (sapc − 0.027)·100
solve.rs:1011-1013  lc_floor = (LO_CLIP − offset)·LC_SCALE; below it →
                Unreachable::BelowContrastFloor
```

Arithmetic (verified, `awk`): **(0.1 − 0.027) · 100 = 7.30 Lc** — the analytic
dead-zone floor. The extra ~0.3 Lc up to **7.6** is the 8-bit hex grid: the
discrete 256-code lattice doesn't land cleanly inside (7.3, 7.6), so 7.6 is
where an on-grid hex first *reliably* clears the clip (issue #44
`QuantizationGap`, bridged by `NEIGHBOR_STEPS=2`, `solve.rs:675`).

**Conclusion:** 7.6 = analytic clip (7.30) + 8-bit grid gap. A property of
`LO_CLIP`, the polarity offset, and the 256-code lattice — pure engine
mechanics. The comment at `semantic.rs:146-152` conflates *"reliable lower bound
on emission"* with *"JND"*; that conflation is the real defect. The engine
carries **no JND constant of its own** (the CAM16-UCS `dJ'`/`dM'` path in
`lcs.rs`/`lpc.rs:373` is the *separate* `DecorativeDj` fill/border metric and is
uncalibrated for detection — see §6).

### 1b. The perceptual floor — authoritatively sourced

The engine can *measure* a hairline's Lc but cannot *derive* the
minimally-perceptible Lc; that number is imported from the metric's author.

| Source | Verbatim | Threshold |
|---|---|---|
| Somers/Myndex, **APCAeasyIntro.html** (W3C Invited Expert, APCA Research Lead; accessed 2026-06-21) | *"Lc 15 is the point of invisibility for many users. This is especially true for thin lines or borders."* / *"Lc 15 — The absolute minimum for any non-text that needs to be discernible and differentiable…"* | **Lc 15** |
| Somers/Myndex, **WhyAPCA.html** (accessed 2026-06-21) | *"Lc 15 the point of invisibility for many users, particularly for thin lines"* | **Lc 15** |
| SAPC-APCA **Discussion #39** (Somers) | discernible-non-semantic category names *"border… dividers… form outlines"*; below Lc 15 *"invisible… will not be visible for many users"* | corroboration |

Two independent APCA-author primary sources, stated **explicitly for thin
lines/borders/dividers**, in the **exact Lc unit** the engine's decorative
contract uses — no cross-unit mapping.

### 1c. Reconciliation (engine ↔ source)

Engine floor **7.30** (analytic) / 7.6 (grid) sits at **~0.49×** of the
perceptual floor **Lc 15** — i.e. *inside* the "invisible for many users" band.
That is *why* it is a cliff and not a JND. The move **7.6 → 15.0 is UP and AWAY**
from the placeholder, grounded in the discernibility wording — **not a back-fit.**
Implied single JND ≈ **Lc 9–10** (spot Lc30 ÷ 3; fluent Lc90 ÷ 10); Lc 15 ≈
1.5× that JND = the reliable just-discernible step across users.

**GROUNDED VALUE: `DECORATIVE_FLOOR_MIN = 15.0`** — ratified by epic
`separator-tracks-jnd-floor` / chapter `raise-floor-and-pin-separator`. (The
sourced thin-line floor is Lc 15; this const is now closed.)

---

## 2. `Role::Separator` hairline JND — VERDICT: **authoritatively-sourced**

A hairline **is** the "thin line / border" case APCA names at its invisibility
floor. The perceptibility floor is therefore **Lc 15** directly (same two
primary sources as §1b). Owner intent *"visible just enough, never
over-separated"* maps to **just above the Lc 15 floor, strictly below Lc 30**
(the discernible-shape minimum, which APCA scopes to non-text *≥5px in its
smallest dimension* — a 1px hairline is governed by the **thin-line** clause,
Lc 15, not the shape clause; using Lc 30 for a hairline would be
over-separation = dirt).

- Current placeholder `decorative(8.0)` (`semantic.rs:1008`) measures **Lc 8.17**
  on white (`#ECECEC`) — **below the Lc 15 invisibility floor**, i.e.
  sub-perceptual *and* only +0.57 Lc above the 7.6 engine cliff. It was set to
  clear the *artifact*, not to be perceptible. The literature **contradicts**
  the placeholder (no back-fit).
- Engine-measured checkpoints on white (solver emits on-grid):
  Lc 15 → `#DFDFDF` (15.43); Lc 18 → `#DADADA` (18.19); Lc 30 → `#C4C4C4`
  (30.05). All comfortably above the 7.6 cliff — **the engine can solve the
  sourced band**; the cliff does not constrain it.

**GROUNDED VALUE: `Role::Separator` = `decorative(DECORATIVE_FLOOR_MIN)` = Lc 15**
— ratified by epic `separator-tracks-jnd-floor` / chapter
`raise-floor-and-pin-separator`. The sourced band is [Lc 15, Lc 18]; the set point
at the floor (Lc 15 exactly, tracking `DECORATIVE_FLOOR_MIN`) is the conservative,
source-grounded choice — no magic number, no independent literal.

---

## 3. Shadow ramp `SHADOW_MINOR/AMBIENT/PENUMBRA/MAJOR_JND` — VERDICT: **HARD BLOCKER**

The shadow anchors are **alpha opacities** (`@1/@2/@4/@12`) of a progressive
translucent gradient composited over **variable (non-solid) content**
(`semantic.rs:186-195`). The Ship's flag is correct, and **both the engine and
the science confirm it.**

**Why it is irreducible:** every perceptual kernel the engine owns is defined
only for a **foreground colour against a SOLID background** —
`contrast_core(y_fg, y_bg)` (`lpc.rs:280`) structurally requires a *single*
fixed `y_bg`; `lpc_surface` takes two *opaque* `LcsColor`s (`lpc.rs:373`). A
translucent shadow over arbitrary content has **no single (fg,bg) pair** — its
perceived contrast depends on the content it falls on. So **no Lc rung value can
be computed** without the alpha→effective-luminance compositing model.

**EXACT OPEN SCIENTIFIC QUESTION FOR THE OWNER:**
> *Against what reference background luminance do we anchor each shadow step's
> Lc, given each step composites an alpha opacity (@1/@2/@4/@12) over
> arbitrary/variable content rather than a solid surface?*

This is owned by the **non-solid-backgrounds / composite-backgrounds chapter**
(not yet in the engine), which must define the alpha→effective-luminance model
**and** the reference background it anchors to.

**Second blocker — inter-step grain:** APCA exposes no sourceable grain finer
than its 1-JND ≈ Lc 15 increment. A full additive +15 ramp would put
shadow-major at **Lc 60** (≈ body-text strength) — over-separation = dirt. A
sub-15 decorative step is below APCA's own sourceable grain. You cannot
simultaneously (i) anchor an absolute base, (ii) keep four rungs each a real
JND apart, and (iii) keep the top subtle — until the missing chapter resolves
the alpha→Lc mapping.

**What is decided now (not faked):**
- The current `8.0/9.5/11.5/14.0` are **additive, equal-spaced, and all below the
  real Lc 15 floor** — sub-threshold and not a valid contract. They must **not**
  survive.
- The spacing model, *if* the owner anchors a solid reference, is **ADDITIVE in
  perceptually-uniform Lc** (APCA's own 15/30/45/… ladder), **not** geometric.
  The prior research summary's "multiplicative ~1.25–1.6×" model is **refuted by
  the engine**: Lc is perceptually uniform (Weber lives in luminance, before
  `soft_clamp`, §0), and the actual `dJ'` fill ratios are *drifting*
  (1.237/1.384/1.470, from `semantic.rs:176-179`), so they corroborate **no**
  single geometric factor.
- **Honest contract today = strict ascending ORDER only** (minor < ambient <
  penumbra < major), with the base **raised onto the Lc 15 floor**, and the four
  rung values marked BLOCKED with the open question above as a code comment. **Do
  NOT fabricate per-step shadow JNDs.**

**Provisional, order-only placeholder values:** `15.5 / 17.5 / 19.5 / 21.5`
(SHADOW_MINOR/AMBIENT/PENUMBRA/MAJOR_JND in `semantic.rs`). These are **not**
derived JNDs — they are order-preserving stubs lifted onto the ratified Lc 15
floor by epic `separator-tracks-jnd-floor`. They remain **TBD — filled by scope
`shadow-ramp-derivation`** (≥ floor, strictly ascending; perceptual magnitude
explicitly NOT claimed). The shadow ramp stays **HARD BLOCKED** on the
non-solid-backgrounds chapter (see above).

---

## 4. Recommended edits (no back-fit)

| `semantic.rs` | from | to (ratified / stub) | basis | status |
|---|---|---|---|---|
| `DECORATIVE_FLOOR_MIN` (:156) | 7.6 | **15.0** | §1 — engine cliff proof + 2 APCA sources | **ratified** by `separator-tracks-jnd-floor` |
| `Role::Separator` (:1014) | `decorative(8.0)` | `decorative(DECORATIVE_FLOOR_MIN)` = Lc 15 | §2 | **ratified** by `separator-tracks-jnd-floor` |
| `SHADOW_*_JND` (:205-208) | 8.0/9.5/11.5/14.0 | 15.5/17.5/19.5/21.5, order-only ≥ floor, **BLOCKED** | §3 | **HARD BLOCKER** — filled by `shadow-ramp-derivation` |

The floor and separator edits are now complete. The shadow stubs are order-only
placeholders on the Lc 15 floor; perceptual JNDs are not claimed and await
the non-solid-backgrounds chapter.

Also fix the `DECORATIVE_FLOOR_MIN` doc comment (`:146-152`): it must say
*"engine quantisation/emission floor (≈7.3 analytic clip + 8-bit grid)"* and
separately *"perceptual floor = Lc 15 (APCA, thin lines)"* — two different
quantities, currently conflated.

---

## 5. Authoritative sources (live-verified 2026-06-21)

1. Somers, A. (Myndex), **"The Easy Intro to the APCA Contrast Method"**,
   git.apcacontrast.com/documentation/APCAeasyIntro.html — *"Lc 15 is the point
   of invisibility for many users. This is especially true for thin lines or
   borders."*; *"Spot reading… contrast needs to be three times the JND."*;
   *"contrast should be at least ten times the JND. The preferred contrast
   reserve is twenty times threshold…"*; *"Lc 30 — absolute minimum… for
   large/solid… non-text."*
2. Somers, A. (Myndex), **"Why APCA?"**,
   git.apcacontrast.com/documentation/WhyAPCA.html — *"contrast must be at least
   ten times the contrast sensitivity threshold (CS) which is the point of 'just
   noticeable differences' (JND)."*; *"Twenty times is preferred for adequate
   contrast reserve…"*; *"In this case the contrast needs to be three times that
   of the JND."*; *"Lc 15 the point of invisibility… particularly for thin
   lines."*
3. SAPC-APCA **Discussion #39** (Somers) — discernible-non-text names
   *"border… dividers… form outlines"*; below Lc 15 *"invisible for many users."*
4. Whittle, P. (1986), *"Increments and decrements: luminance discrimination"*,
   Vision Research; **PMID 3617509** — *"threshold was proportional to the
   luminance difference, ΔL"* (Weber's law in **luminance**); decrements more
   discriminable than increments of equal ΔL (asymmetry — mirrored by the
   engine's polarity-dependent offsets). *Substrate for "Weber lives in
   luminance, Lc is uniform."*
5. Whittaker & Lovie-Kitchin (1993), *"Visual Requirements for Reading"*, Optom
   Vis Sci; **PMID 8430009** — *"contrast reserve [print contrast relative to
   contrast threshold]"*; fluent reading needs contrast *"several times
   threshold."* The peer-reviewed origin of APCA's 3×/10× multiples; governs
   **text** roles, **not** a non-text hairline/shadow.

Engine files: `lpc.rs:198,200,204,280-305,463`; `solve.rs:1011-1013,675`;
`semantic.rs:146-152,176-184,200-203,1008,1166`; `golden_tests.rs:230`.

---

## 6. Why the CAM16-UCS path does NOT supply these numbers

`lcs.rs` CAM16-UCS `dJ'`/`dM'` (`lpc.rs::lpc_surface`, `lcs.rs::mp`) is the
**separate** `DecorativeDj` metric for the **fill/border** ladders
(`semantic.rs:176-184`, owner's literal Figma-computed anchors). It is a
distinguishability distance with **no detection threshold attached**
(`dJ'=1` = one lightness unit, no visibility meaning). It corroborates the
*unit family* but supplies **neither** the Lc floor **nor** a geometric ramp.
The fill `dJ'` ratios *drift* (1.237/1.384/1.470) — there is no single constant
factor in the engine.

---

## 7. INVARIANTS to lock (compile-time + test)

1. **Decorative floor never violated.** Every decorative magnitude
   `≥ DECORATIVE_FLOOR_MIN` at the type level: `decorative_contract`
   (`semantic.rs:1166`) already clamps via `.max(DECORATIVE_FLOOR_MIN)`. Add a
   `const` assertion that each named decorative magnitude (separator, all four
   shadow rungs) is `≥ DECORATIVE_FLOOR_MIN` so a sub-floor literal **fails to
   compile**.
2. **Shadow strict order**, proven over a corpus — not just the constants:
   for a representative background corpus (light/dark + extremes), the *resolved*
   shadow Lc values satisfy `minor < ambient < penumbra < major` (a property
   test, Class C — order is a set property of the solve, not just the literals).
3. **Magnitudes are `const` / compile-time.** All five live as module `const`s
   (they do today); no runtime mutation path. A test asserts the table's
   `Decorative { magnitude }` for these roles equals the named `const`s.
4. **Floor is a perceptual quantity, not the engine cliff.** A regression test
   pins `DECORATIVE_FLOOR_MIN == 15.0` (the ratified value, exact; any drift
   fails the assertion — `>= 15.0` is not sufficient) *and* documents that the
   engine emission cliff is the *separate* `(LO_CLIP − offset)·LC_SCALE = 7.30`
   — guarding against anyone re-collapsing the two (the original #44 defect).
   The 7.30 figure is an engine fact and stays; the chosen floor is now 15.0.

---

## 8. Verdict summary

| Magnitude | Chosen value | Sourced basis | Verdict | Status |
|---|---|---|---|---|
| `DECORATIVE_FLOOR_MIN` | **15.0** | Lc 15 sourced; cliff 7.30 disproven as JND | **derived-to-root + authoritatively-sourced** | **ratified** by `separator-tracks-jnd-floor` |
| `Role::Separator` hairline | **`decorative(15.0)`** | sourced band [Lc 15, 18]; floor = conservative set point | **authoritatively-sourced** | **ratified** by `separator-tracks-jnd-floor` |
| `SHADOW_MINOR_JND` | **15.5** (order-only stub) | order-only ≥ floor (rung value blocked) | **HARD BLOCKER** | TBD — `shadow-ramp-derivation` |
| `SHADOW_AMBIENT_JND` | **17.5** (order-only stub) | order-only, > minor (rung value blocked) | **HARD BLOCKER** | TBD — `shadow-ramp-derivation` |
| `SHADOW_PENUMBRA_JND` | **19.5** (order-only stub) | order-only, > ambient (rung value blocked) | **HARD BLOCKER** | TBD — `shadow-ramp-derivation` |
| `SHADOW_MAJOR_JND` | **21.5** (order-only stub) | order-only, > penumbra (rung value blocked) | **HARD BLOCKER** | TBD — `shadow-ramp-derivation` |

**Alpha-over-variable-content shadow case is flagged irreducible** for the owner
and the **non-solid-backgrounds chapter** (the alpha→effective-luminance
compositing model + its reference background). Until that chapter exists the
shadow stack stays an **order-only** Lc stub raised onto the Lc 15 floor — no
fabricated per-step JNDs.
