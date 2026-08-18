//! Дешёвый display-domain recheck финальных sRGB8 пар.
//!
//! Перенесён из legacy semantic recipe-модуля в C7c: это постоянный generic
//! контракт, независимый от RoleRecipe/NamedRoleTable.

use crate::{Srgb8, ViewingConditions};

/// Measure the frozen candidate `Lc` score and WCAG 2.1 ratio a foreground
/// colour achieves against a background — the cheap **recheck** primitive.
///
/// Both colours are **linear** sRGB; the result is `(lc, wcag_ratio)`. С
/// активации ADR-0003 (глава #64) замер полностью display-доменный — ни
/// одного CAM16-форварда, только WCAG-арифметика — **no solve**. The reactive
/// runtime calls this per frame to decide whether already-resolved colours still
/// pass their contract against a *changed* background, re-solving (and easing)
/// only when they stably do not, instead of re-solving every frame.
///
/// The returned `lc` is **signed** (its sign is the achieved polarity, matching
/// [`Resolved::lc`]), and it is exactly what the solver's `finish` stage measures
/// for the same pair. The ratio is the frozen boundary report projection from
/// the same final bytes; it is not stored in or consumed by [`Solved`].
pub fn measure_contrast(
    bg_linear: [f64; 3],
    fg_linear: [f64; 3],
    _vc: &ViewingConditions,
) -> (f64, f64) {
    // Candidate `Lc` и легальный WCAG читают ОДНУ люминансу
    // квантованного display-цвета (candidate score в `Ys`, ADR-0003), exactly as
    // the solver measures it (`finish` → `quantised_display`), so the recheck
    // reproduces the solver's reported `lc` and the boundary ratio projection
    // bit-for-bit from one emitted state.
    let fg_disp = crate::solve::quantised_display(fg_linear);
    let bg_disp = crate::solve::quantised_display(bg_linear);
    let lc = crate::lpc::contrast_core(
        crate::spaces::srgb::encoded_srgb_relative_luminance(fg_disp),
        crate::spaces::srgb::encoded_srgb_relative_luminance(bg_disp),
    );
    let wcag = crate::spaces::srgb::encoded_srgb_contrast_ratio(fg_disp, bg_disp);
    (lc, wcag)
}

/// Batch recheck: the `(lc, wcag_ratio)` each foreground hex achieves against one
/// **shared** background hex, under `vc`. The per-frame primitive the reactive
/// runtime calls.
///
/// The background's luminance is computed **once** for the whole batch. С
/// активации ADR-0003 форвард цвета — это ОДНА `relative_luminance` его
/// display-байтов (ни одного CAM16), so "recheck every role each frame" is
/// cheaper still than "re-solve every role each frame": the controller keeps
/// the current colours while they still pass and only re-solves the rare role
/// that stably fails.
///
/// Each result equals what the solver's `finish` measured for that fg/bg pair, so
/// a freshly-resolved set re-checks to its own reported contrasts. Returns `Err`
/// if any hex is invalid (only `#RRGGBB` or bare `RRGGBB` is accepted).
/// One colour's recheck ingredient from its hex: the WCAG relative luminance
/// `rl` of its display bytes — с активации ADR-0003 candidate `Lc` и
/// легальный WCAG читают ОДНУ и ту же люминансу, бывшая пара `(y_hk, rl)`
/// схлопнулась в один скаляр, а recheck стал VC-независимым (display-домен).
///
/// SINGLE SOURCE OF TRUTH for the forward, shared by [`recheck_against`] and
/// [`recheck_against_multi`] so they cannot drift — the byte-identity both
/// functions promise now holds *by construction*, not by two copies staying in
/// sync. The hot-path economy lives here: the WCAG display value is taken
/// straight from the byte (`byte/255`) by `srgb_encoded_from_hex`, so the
/// per-channel `quantised_display` encode `powf` is gone —
/// `byte/255 == quantised_display(decode(byte))` exactly (pinned in
/// `spaces::srgb::display_equals_quantised_display_on_every_byte`).
fn hex_forward(hex: &str) -> Result<f64, String> {
    let disp = crate::spaces::srgb::srgb_encoded_from_hex(hex)?;
    Ok(crate::spaces::srgb::encoded_srgb_relative_luminance(disp))
}

pub fn recheck_against(
    bg_hex: &str,
    fg_hexes: &[&str],
    _vc: &ViewingConditions,
) -> Result<Vec<(f64, f64)>, String> {
    // The background's forward is loop-invariant — computed once. Один скаляр
    // на цвет: та же люминанса кормит и `contrast_core`, и WCAG-ратио.
    let rl_bg = hex_forward(bg_hex)?;
    fg_hexes
        .iter()
        .map(|fg_hex| {
            let rl_fg = hex_forward(fg_hex)?;
            let lc = crate::lpc::contrast_core(rl_fg, rl_bg);
            let wcag = crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg);
            Ok((lc, wcag))
        })
        .collect()
}

/// Compute the complete flat result cardinality before the first forward or
/// metric call. This is the arithmetic/allocator safety floor; the lower product
/// limit admitted by the versioned public resource profile remains owned by
/// #429 and must not be invented here.
fn checked_recheck_output_len(backgrounds: usize, foregrounds: usize) -> Result<usize, String> {
    backgrounds
        .checked_mul(foregrounds)
        .and_then(|cells| cells.checked_mul(2))
        .ok_or_else(|| "recheck batch cardinality exceeds platform capacity".to_owned())
}

fn reserve_recheck_entries<T>(
    values: &mut Vec<T>,
    entries: usize,
    buffer: &'static str,
) -> Result<(), String> {
    values.try_reserve_exact(entries).map_err(|_| {
        format!("recheck batch resource exhausted while reserving {buffer} ({entries} entries)")
    })
}

/// Multi-background recheck: the `(lc, wcag_ratio)` each foreground achieves
/// against EACH of several background samples, sharing every foreground's
/// forward across all samples. The reactive controller's worst-case loop
/// rechecks the SAME foreground set against N backdrop samples (a gradient /
/// image); each foreground's `rl_fg` is computed ONCE and reused for every
/// sample — с активации ADR-0003 форвард подешевел до одной
/// `relative_luminance` display-байтов (CAM16 не входит в score), но
/// хойстинг сохранён: он несёт контракт byte-identity двух входов, не только
/// экономию.
///
/// The result is **byte-identical**, pair for pair, to calling [`recheck_against`]
/// once per background: the same float operations run in the same order, only the
/// loop nesting is inverted so the foreground forward is hoisted. Layout is flat
/// and background-major: entry `bg s`, foreground `i` is at
/// `out[(s*fg_hexes.len() + i) * 2 + {0:lc, 1:wcag}]`. Returns `Err` on any
/// invalid hex.
pub fn recheck_against_multi(
    bg_hexes: &[&str],
    fg_hexes: &[&str],
    _vc: &ViewingConditions,
) -> Result<Vec<f64>, String> {
    // Preflight the complete matrix and both allocations before the first
    // display-forward. Overflow/allocator refusal is atomic: no partial evidence.
    let output_len = checked_recheck_output_len(bg_hexes.len(), fg_hexes.len())?;
    let mut fg_pre = Vec::new();
    reserve_recheck_entries(&mut fg_pre, fg_hexes.len(), "foreground forwards")?;
    let mut out = Vec::new();
    reserve_recheck_entries(&mut out, output_len, "result lanes")?;

    // Precompute each foreground's background-independent forward exactly once,
    // through the SAME `hex_forward` `recheck_against` uses — so the shared-forward
    // path guarantees byte-identity between the two entry points by construction.
    for fg_hex in fg_hexes {
        fg_pre.push(hex_forward(fg_hex)?);
    }

    for bg_hex in bg_hexes {
        let rl_bg = hex_forward(bg_hex)?;
        for &rl_fg in &fg_pre {
            out.push(crate::lpc::contrast_core(rl_fg, rl_bg));
            out.push(crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg));
        }
    }
    Ok(out)
}

/// Decode a packed `0x00RRGGBB` colour to its three exact encoded-sRGB8 bytes
/// `[R, G, B]` via pure shifts — the same octets `hex_bytes("#RRGGBB")` yields.
///
/// The high byte is **reserved and required-zero**: `0x00RRGGBB` is the only
/// legal shape, so `0xAARRGGBB` (an RGBA/ARGB word leaking in) is rejected up
/// front by a single mask instead of being silently truncated. This is the
/// cheap validation the packed boundary performs once, with no allocation.
fn bytes_from_u32(packed: u32) -> Result<[u8; 3], String> {
    if packed >> 24 != 0 {
        return Err(format!(
            "expected packed 0x00RRGGBB with a zero high byte, got {packed:#010X}"
        ));
    }
    Ok([
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ])
}

/// One colour's recheck ingredient from its packed `0x00RRGGBB` word: the WCAG
/// relative luminance of its display bytes — the packed sibling of
/// [`hex_forward`].
///
/// Byte-identity to the hex path holds **by construction, not by a second
/// copy**: `bytes_from_u32(0x00RRGGBB)` returns exactly the `[R, G, B]` octets
/// `hex_bytes("#RRGGBB")` returns, both are lifted through the SAME
/// [`Srgb8::encoded`] projection, and the SAME [`crate::spaces::srgb::encoded_srgb_relative_luminance`]
/// SSOT reads them. The metric is never forked — a packed input and its hex
/// spelling cannot drift.
fn u32_forward(packed: u32) -> Result<f64, String> {
    let disp = Srgb8::new(bytes_from_u32(packed)?).encoded();
    Ok(crate::spaces::srgb::encoded_srgb_relative_luminance(disp))
}

/// Packed sibling of [`recheck_against`]: the `(lc, wcag_ratio)` each foreground
/// `0x00RRGGBB` word achieves against one shared background word, under `vc`.
///
/// This is byte-identical, pair for pair, to spelling the same colours as hex
/// and calling [`recheck_against`]: it hoists the background forward once and
/// feeds the SAME `crate::lpc::contrast_core` / `crate::spaces::srgb::relative_luminance_ratio`
/// in the SAME order — only the transport (a `u32` shift-decode instead of a
/// hex parse) differs. Returns `Err` if any word carries a non-zero high byte.
pub fn recheck_against_u32(
    bg: u32,
    fgs: &[u32],
    _vc: &ViewingConditions,
) -> Result<Vec<(f64, f64)>, String> {
    let rl_bg = u32_forward(bg)?;
    fgs.iter()
        .map(|&fg| {
            let rl_fg = u32_forward(fg)?;
            let lc = crate::lpc::contrast_core(rl_fg, rl_bg);
            let wcag = crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg);
            Ok((lc, wcag))
        })
        .collect()
}

/// Packed sibling of [`recheck_against_multi`]: the `(lc, wcag_ratio)` each
/// foreground `0x00RRGGBB` word achieves against EACH of several background
/// words, sharing every foreground's forward across all samples.
///
/// The result is **byte-identical**, entry for entry, to calling
/// [`recheck_against_multi`] with the same colours spelled as hex — same float
/// operations, same order, same background-major flat layout: entry `bg s`,
/// foreground `i` at `out[(s*fgs.len() + i) * 2 + {0:lc, 1:wcag}]`. Returns
/// `Err` on any word with a non-zero high byte.
pub fn recheck_against_multi_u32(
    bgs: &[u32],
    fgs: &[u32],
    _vc: &ViewingConditions,
) -> Result<Vec<f64>, String> {
    // Same atomic preflight as the string sibling. Keeping both transports on
    // this helper makes the overflow/resource law one SSOT.
    let output_len = checked_recheck_output_len(bgs.len(), fgs.len())?;
    let mut fg_pre = Vec::new();
    reserve_recheck_entries(&mut fg_pre, fgs.len(), "foreground forwards")?;
    let mut out = Vec::new();
    reserve_recheck_entries(&mut out, output_len, "result lanes")?;
    for &fg in fgs {
        fg_pre.push(u32_forward(fg)?);
    }

    for &bg in bgs {
        let rl_bg = u32_forward(bg)?;
        for &rl_fg in &fg_pre {
            out.push(crate::lpc::contrast_core(rl_fg, rl_bg));
            out.push(crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg));
        }
    }
    Ok(out)
}
