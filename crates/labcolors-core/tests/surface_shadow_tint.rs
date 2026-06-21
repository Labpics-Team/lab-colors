//! TDD RED — the `surface-shadow-tint` chapter contract.
//!
//! BUG CLASS this guards: a consumer (labui's separator / shadowed-surface
//! components) currently hard-codes a fallback shadow tint (`#000000` / the
//! dark surface `#101012`) because the engine does not yet emit a law-derived
//! tint role. The whole point of this chapter is to make that consumer fallback
//! UNNECESSARY by deriving the tint in the engine. These tests pin the new
//! contract BEFORE the production code exists, so they must fail RED for the
//! right reason — the role / key / reachability is *missing*, not a typo.
//!
//! INVARIANTS these tests defend (constitution iron rules + no-misleading-stopgaps):
//!   - The 20 existing kebab keys keep their exact spellings (frozen labui contract).
//!   - The removed legacy `surface` key never returns.
//!   - The taxonomy count assertion always equals the TRUE enum cardinality —
//!     `Role::ALL.len()` — never a stale literal.
//!   - A not-yet-groundable value is DEFERRED with a documented blocker, never a
//!     faked or wrong-type value (no `#000000` literal smuggled in as a "tint").
//!
//! NOTE ON THE DESIGN FORK: `surface-shadow-tint` is the additive role this
//! chapter introduces. Until the production code lands, every assertion that the
//! role / its key / its reachability exists fails honestly. The cardinality
//! target (`EXPECTED_ROLE_COUNT`) is the post-chapter count: today's 20 frozen
//! roles plus the one new `surface-shadow-tint` role.

use std::collections::HashSet;

use labcolors_core::{BgInput, Resolved, Role, RoleTable, ViewingConditions, resolve_set};

/// The new kebab key this chapter adds to the contract. A single source of truth
/// so the spelling is pinned once and every test agrees on it.
const SURFACE_SHADOW_TINT_KEY: &str = "surface-shadow-tint";

/// The 20 frozen keys labui already consumes — their EXACT spellings. None may be
/// renamed, none removed. This is the engine↔labui contract, copied verbatim from
/// the in-crate anchor test so a rename in one place is caught by a mismatch here.
const FROZEN_KEYS: [&str; 20] = [
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
    "icon",
    "separator",
    "border-strong",
    "border-base",
    "border-soft",
    "border-ghost",
    "fill-primary",
    "fill-secondary",
    "fill-tertiary",
    "fill-quaternary",
    "fill-none",
    "shadow-minor",
    "shadow-ambient",
    "shadow-penumbra",
    "shadow-major",
    "none",
];

/// The post-chapter role count: the 20 frozen roles plus `surface-shadow-tint`.
/// Written in terms of the frozen set so an additive change updates it honestly,
/// never as a bare stale literal.
const EXPECTED_ROLE_COUNT: usize = FROZEN_KEYS.len() + 1;

/// The canonical background sweep the chapter must hold the tint reachable on:
/// pure white, the owner's dark surface, mid-grey, and the brand azure.
const CANONICAL_BGS: [&str; 4] = ["#FFFFFF", "#101012", "#7F7F7F", "#3478F6"];

/// Both viewing conditions the contract must hold under.
fn vcs() -> [(ViewingConditions, &'static str); 2] {
    [
        (ViewingConditions::srgb(), "srgb"),
        (ViewingConditions::dim_surround(), "dim"),
    ]
}

/// Every kebab key the default role table currently emits.
fn emitted_keys() -> HashSet<&'static str> {
    Role::ALL.iter().map(|r| r.key()).collect()
}

/// The emitted hex of `key` resolved against `bg_hex` under `vc`, iff it resolved
/// to a real colour (i.e. it "entered vars" — reachable). `None` for an absent
/// key, an honest `Resolved::None`, or an unreachable role.
fn reachable_hex(bg_hex: &str, key: &str, vc: &ViewingConditions) -> Option<String> {
    let bg = BgInput::solid(bg_hex).expect("canonical bg parses");
    let table = RoleTable::default();
    resolve_set(&bg, &table, vc)
        .into_iter()
        .find(|(role, _)| role.key() == key)
        .and_then(|(_, resolved)| match resolved {
            Resolved::Color { solved, .. } => Some(solved.hex().to_owned()),
            _ => None,
        })
}

/// CONTRACT — the EXACT `--lab-*` kebab key set and its count, derived from the
/// enum. Pins the 20 frozen keys, requires the new `surface-shadow-tint` key,
/// refuses any removed key reappearing (`surface` / `text-*`), and refuses any
/// count drift. The single guard of the engine↔labui contract.
///
/// RED REASON: `surface-shadow-tint` is not yet a role, so the key set is missing
/// it and the count is 20, not `EXPECTED_ROLE_COUNT` (21).
#[test]
fn role_keys_follow_the_hig_kebab_taxonomy_updated() {
    let keys = emitted_keys();

    // The 20 frozen keys must all survive verbatim.
    for frozen in FROZEN_KEYS {
        assert!(
            keys.contains(frozen),
            "frozen labui key {frozen} went missing"
        );
    }

    // The new role's key must be present and spelled exactly.
    assert!(
        keys.contains(SURFACE_SHADOW_TINT_KEY),
        "the chapter must emit the {SURFACE_SHADOW_TINT_KEY} role key"
    );

    // The removed legacy `surface` key — and the pre-HIG text-* names — must never
    // return.
    for legacy in [
        "surface",
        "text-primary",
        "text-secondary",
        "text-muted",
        "text-disabled",
    ] {
        assert!(!keys.contains(legacy), "legacy key {legacy} reappeared");
    }

    // The count equals the post-chapter cardinality, expressed via the frozen set
    // (never a bare stale literal).
    assert_eq!(
        keys.len(),
        EXPECTED_ROLE_COUNT,
        "taxonomy is exactly {EXPECTED_ROLE_COUNT} roles (20 frozen + surface-shadow-tint), found {}",
        keys.len()
    );
}

/// PROPERTY — `Role::ALL` enumerates exactly the enum variants, so the emitted set
/// can never silently diverge from the defined set. After the additive change the
/// cardinality must be `EXPECTED_ROLE_COUNT`; the keys must be unique (no two
/// variants share a string); and `ALL` must carry the new variant.
///
/// RED REASON: `Role::ALL` has 20 entries today and does not include a
/// `surface-shadow-tint` variant, so both the count and the membership assertion
/// bite.
#[test]
fn role_all_equals_enum_cardinality() {
    // Length pins the cardinality drift between defined and emitted.
    assert_eq!(
        Role::ALL.len(),
        EXPECTED_ROLE_COUNT,
        "Role::ALL must enumerate exactly {EXPECTED_ROLE_COUNT} variants, has {}",
        Role::ALL.len()
    );

    // Keys are unique — no variant collides onto another's string.
    let keys = emitted_keys();
    assert_eq!(
        keys.len(),
        Role::ALL.len(),
        "every Role::ALL entry must have a distinct key"
    );

    // The new variant participates in ALL (so resolve_set walks it).
    assert!(
        keys.contains(SURFACE_SHADOW_TINT_KEY),
        "Role::ALL must include the {SURFACE_SHADOW_TINT_KEY} variant"
    );
}

/// CONTRACT — `surface-shadow-tint` is emitted as a real colour and is reachable
/// on the canonical background sweep under BOTH viewing conditions. Its value is
/// law-derived (resolved by the engine), not a literal handed to the consumer.
///
/// RED REASON: the role does not exist, so `reachable_hex` returns `None` on every
/// background and the assertion fires.
#[test]
fn surface_shadow_tint_present_and_reachable() {
    for (vc, vc_name) in vcs() {
        for bg in CANONICAL_BGS {
            let hex = reachable_hex(bg, SURFACE_SHADOW_TINT_KEY, &vc);
            assert!(
                hex.is_some(),
                "{vc_name} {bg}: {SURFACE_SHADOW_TINT_KEY} must resolve to a colour (reachable), got None"
            );
        }
    }
}

/// REGRESSION — the emitted tint is NOT pure black and NOT the hard-coded dark
/// surface `#101012`. That is the whole point of the request: the engine derives
/// the tint by law, so the consumer's `#000000` / `#101012` fallback becomes
/// unnecessary. If the engine ever emitted exactly the literal the consumer used
/// as a stopgap, the law would have bought us nothing.
///
/// RED REASON: the role is absent, so the lookup yields `None`; the test fails
/// because there is no law-derived value to inspect yet.
#[test]
fn shadow_tint_value_is_law_derived_not_black() {
    for (vc, vc_name) in vcs() {
        for bg in CANONICAL_BGS {
            let hex = reachable_hex(bg, SURFACE_SHADOW_TINT_KEY, &vc).unwrap_or_else(|| {
                panic!("{vc_name} {bg}: {SURFACE_SHADOW_TINT_KEY} not yet emitted")
            });
            let upper = hex.to_ascii_uppercase();
            assert_ne!(
                upper, "#000000",
                "{vc_name} {bg}: shadow tint must be law-derived, not pure black"
            );
            assert_ne!(
                upper, "#101012",
                "{vc_name} {bg}: shadow tint must be law-derived, not the hard-coded dark surface"
            );
        }
    }
}

/// CONTRACT — the role set a separator component needs to paint its surface is
/// present and reachable: the hairline itself (`separator`), the fills it can sit
/// over (`fill-primary`), the borders framing it (`border-base`, `border-soft`),
/// AND the new `surface-shadow-tint` that lets a raised separator cast a
/// law-derived shadow instead of a hard-coded one.
///
/// RED REASON: `separator` / `fill-primary` / `border-*` already resolve, but
/// `surface-shadow-tint` does not — so the separator-surface set is incomplete and
/// the assertion names the precise missing role.
#[test]
fn separator_surface_roles_reachable() {
    let needed = [
        "separator",
        "fill-primary",
        "border-base",
        "border-soft",
        SURFACE_SHADOW_TINT_KEY,
    ];
    for (vc, vc_name) in vcs() {
        for bg in CANONICAL_BGS {
            for key in needed {
                assert!(
                    reachable_hex(bg, key, &vc).is_some(),
                    "{vc_name} {bg}: separator surface needs {key}, but it is not reachable"
                );
            }
        }
    }
}

/// PROPERTY — sweep EVERY role over the canonical background set under both
/// viewing conditions and pin which roles are reachable (enter vars) vs honestly
/// absent. Adding a role must not silently make an existing role unreachable, and
/// the new `surface-shadow-tint` must join the reachable set.
///
/// The zero tokens (`none`, `border-ghost`, `fill-none`) are honestly absent by
/// design (they resolve to `Resolved::None`) — they are EXEMPT from reachability.
/// Every other role must paint a colour on every canonical background.
///
/// RED REASON: `surface-shadow-tint` is neither a known zero token nor reachable
/// (it does not exist), so it fails the "every non-zero role is reachable" sweep.
#[test]
fn reachability_sweep_all_roles() {
    const ZERO_TOKENS: [&str; 3] = ["none", "border-ghost", "fill-none"];

    // The roles we expect reachable everywhere = the full post-chapter key set
    // minus the explicit zero tokens.
    let mut expected_reachable: HashSet<&str> = FROZEN_KEYS.into_iter().collect();
    expected_reachable.insert(SURFACE_SHADOW_TINT_KEY);
    for z in ZERO_TOKENS {
        expected_reachable.remove(z);
    }

    for (vc, vc_name) in vcs() {
        for bg in CANONICAL_BGS {
            for key in &expected_reachable {
                assert!(
                    reachable_hex(bg, key, &vc).is_some(),
                    "{vc_name} {bg}: role {key} must be reachable (enter vars), found absent"
                );
            }
            // The zero tokens stay honestly absent — never silently promoted.
            for z in ZERO_TOKENS {
                assert!(
                    reachable_hex(bg, z, &vc).is_none(),
                    "{vc_name} {bg}: zero token {z} must stay an honest zero, not a painted colour"
                );
            }
        }
    }
}
