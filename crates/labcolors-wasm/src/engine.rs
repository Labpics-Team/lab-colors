//! The application core of the bindings: resolve a background under a theme,
//! generically over whatever role set a loaded config provides.
//!
//! This layer knows the core and the DTOs; it does NOT know wasm-bindgen. It
//! holds the compiled config table (supplied by `load_config`) and the contract
//! cache, runs the core resolve, and maps the resolved vector into
//! [`ResolvedTheme`]. The engine is agnostic (ADR-0001 PR-c): it carries no
//! built-in design system, so `resolve_theme` needs a config first. The mapping
//! never enumerates roles — it walks whatever the core returns and keys each
//! entry by the config's own role name — so role growth flows through on a
//! rebuild.

use std::rc::Rc;

use std::collections::HashMap;

use labcolors_core::config::ThemeConfig;
use labcolors_core::semantic::NamedRoleTable;
use labcolors_core::{BgInput, Resolved, Solved, Unreachable};

use crate::cache::{CacheKey, ContractCache};
use crate::config_dto::{ConfigDto, fingerprint};
use crate::dto::{ResolvedTheme, RgbaColor, RoleEntry, RoleOutcome, SolvedColor};
use crate::error::BindingError;
use crate::theme::Theme;

/// How many distinct `(bg, theme, table)` resolves the cache holds before a
/// wholesale clear. A few thousand entries at well under 1 MB — generous for a
/// design tool sweeping backgrounds, bounded so memory cannot run away.
const CACHE_CAPACITY: usize = 4096;

/// A caching contrast engine over a consumer-supplied design system.
///
/// Construct once (`init`), load a config (`load_config`), then call
/// [`resolve_theme`](Self::resolve_theme) many times. The engine is agnostic —
/// it has no built-in role table — so a resolve before `load_config` returns
/// [`BindingError::ConfigRequired`], never a panic or a silent default. The
/// result is cached behind an `Rc` so a cache hit is a cheap reference-count
/// bump, not a re-clone of the whole set.
pub struct Engine {
    named: Option<NamedState>,
    cache: ContractCache<Rc<ResolvedTheme>>,
}

/// Загруженный конфиг потребителя: скомпилированная таблица + её отпечаток
/// (компонент ключа кэша — два конфига не делят записи) + полы ролей,
/// предвычисленные на загрузке (свойство контракта, не резолва; алиас несёт
/// пол своей цели).
struct NamedState {
    table: NamedRoleTable,
    fingerprint: u64,
    floors: HashMap<String, Option<f64>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// A fresh engine with no config loaded yet. [`resolve_theme`](Self::resolve_theme)
    /// returns [`BindingError::ConfigRequired`] until [`load_config`](Self::load_config)
    /// supplies a design system.
    pub fn new() -> Self {
        Self {
            named: None,
            cache: ContractCache::new(CACHE_CAPACITY),
        }
    }

    /// Загрузить конфиг потребителя из JSON: полный preflight ядра
    /// (validate = компиляция) + вычисленный отпечаток. После успешной
    /// загрузки [`resolve_theme`](Self::resolve_theme) эмитит РОЛИ КОНФИГА
    /// (string-keyed контракт) той же физикой; сигнатура resolve_theme
    /// неизменна. Возвращает отпечаток — компонент ключа кэша: другой конфиг
    /// даёт другой отпечаток, записи не делятся (нет кэш-коллизии).
    ///
    /// Ошибочный конфиг НЕ трогает текущее состояние: движок остаётся на
    /// прежней таблице (загрузка атомарна).
    pub fn load_config(&mut self, json: &str) -> Result<u64, BindingError> {
        let dto: ConfigDto =
            serde_json::from_str(json).map_err(|e| BindingError::InvalidConfig {
                reason: e.to_string(),
            })?;
        let fp = fingerprint(&dto);
        let cfg =
            ThemeConfig::try_from(dto).map_err(|reason| BindingError::InvalidConfig { reason })?;
        let table = cfg
            .compile_named_role_table()
            .map_err(|e| BindingError::InvalidConfig {
                reason: e.to_string(),
            })?;
        let mut floors: HashMap<String, Option<f64>> = table
            .entries()
            .iter()
            .map(|(name, spec)| (name.clone(), spec.legal_floor()))
            .collect();
        for (alias, target) in table.aliases() {
            let floor = floors.get(target).copied().flatten();
            floors.insert(alias.clone(), floor);
        }
        // Прошлое пространство записей сносится целиком: гарантия «чужой
        // конфиг не отдаст свои цвета» — очистка, а не вероятностная
        // уникальность 64-битного отпечатка (отпечаток в ключе остаётся
        // belt-and-suspenders и идентичностью конфига наружу).
        self.cache.clear();
        self.named = Some(NamedState {
            table,
            fingerprint: fp,
            floors,
        });
        Ok(fp)
    }

    /// Resolve every role for `bg_hex` under `theme`, returning the shared
    /// result. Repeated identical calls hit the contract cache.
    ///
    /// Errors (bad hex, unknown theme) are returned, never
    /// panicked. Per-role unreachability is part of a *successful* result.
    pub fn resolve_theme(
        &self,
        bg_hex: &str,
        theme: Theme,
    ) -> Result<Rc<ResolvedTheme>, BindingError> {
        let vc = theme.viewing_conditions();
        // Validate and normalise the background once, before the cache lookup,
        // so an invalid hex fails fast and the cache key is canonical.
        let normalised = normalise_hex(bg_hex)?;
        let bg = BgInput::solid(&normalised).map_err(|u| BindingError::InvalidBackground {
            reason: u.to_string(),
        })?;

        // Конфиг загружен → эмитится ЕГО контракт (string-keyed) той же
        // физикой; отпечаток в ключе разводит кэш-пространства конфигов.
        if let Some(named) = &self.named {
            let key = CacheKey::new(normalised.clone(), theme, named.fingerprint);
            let result = self.cache.get_or_insert_with(key, || {
                let set = labcolors_core::resolve_named_set(&bg, &named.table, &vc);
                let mut roles: Vec<RoleEntry> = set
                    .into_iter()
                    .map(|(name, resolved)| {
                        let floor = named.floors.get(&name).copied().flatten();
                        RoleEntry {
                            role_key: name,
                            outcome: map_resolved(resolved, floor),
                        }
                    })
                    .collect();
                // Алиасы — часть эмитируемого контракта (--lab-{alias} обязан
                // существовать у потребителя): ядро их не резолвит (алиас — не
                // рецепт), граница эмитит исход ЦЕЛИ под именем алиаса.
                for (alias, target) in named.table.aliases() {
                    if let Some(entry) = roles.iter().find(|e| &e.role_key == target) {
                        let outcome = entry.outcome.clone();
                        roles.push(RoleEntry {
                            role_key: alias.clone(),
                            outcome,
                        });
                    }
                }
                Rc::new(ResolvedTheme {
                    theme: theme.key(),
                    background: normalised.clone(),
                    roles,
                })
            });
            return Ok(result);
        }

        // Agnostic engine: no config, nothing to emit. An honest, matchable
        // failure — the boundary refuses rather than inventing a built-in system.
        Err(BindingError::ConfigRequired)
    }

    /// Recheck the contrasts a set of foreground colours achieve against a
    /// (possibly changed) `bg_hex` under `theme` — the cheap per-frame primitive
    /// of the reactive runtime. One CAM16 forward for the background plus one per
    /// foreground, **no solve**: the controller keeps current colours while they
    /// still pass and re-solves only the rare role that stably fails.
    ///
    /// Returns a flat, interleaved buffer `[lc0, wcag0, lc1, wcag1, …]` (mapped to
    /// a JS `Float64Array`) — no per-call object allocation on the hot path. The
    /// values equal what the solver measured, so a freshly-resolved set rechecks
    /// to its own reported contrasts.
    pub fn recheck(
        &self,
        bg_hex: &str,
        fg_hexes: &[String],
        theme: Theme,
    ) -> Result<Vec<f64>, BindingError> {
        let vc = theme.viewing_conditions();
        let bg = normalise_hex(bg_hex)?;
        // Normalise foregrounds through the same parser as the background and
        // `resolveTheme`, so the three entry points agree on what a valid hex is
        // (`#RGB` shorthand, missing `#`, any case) instead of the core's
        // stricter 6-digit-only parse rejecting a shorthand a resolve accepted.
        let normalised: Vec<String> = fg_hexes
            .iter()
            .map(|h| normalise_hex(h))
            .collect::<Result<_, _>>()?;
        let refs: Vec<&str> = normalised.iter().map(String::as_str).collect();
        let pairs = labcolors_core::recheck_against(&bg, &refs, &vc)
            .map_err(|reason| BindingError::InvalidBackground { reason })?;
        let mut out = Vec::with_capacity(pairs.len() * 2);
        for (lc, wcag) in pairs {
            out.push(lc);
            out.push(wcag);
        }
        Ok(out)
    }
}

/// Map one core [`Resolved`] into the boundary [`RoleOutcome`]. `legal_floor` is
/// the role's WCAG clamp (from the role table), carried onto a solved colour.
fn map_resolved(resolved: Resolved, legal_floor: Option<f64>) -> RoleOutcome {
    match resolved {
        Resolved::Color {
            solved,
            compressed,
            achieved_dj,
            hue_vanished,
        } => RoleOutcome::Color(map_solved(
            solved,
            compressed,
            achieved_dj,
            hue_vanished,
            legal_floor,
        )),
        Resolved::None => RoleOutcome::None,
        Resolved::Unreachable(reason) => RoleOutcome::Unreachable {
            code: unreachable_code(&reason),
            message: reason.to_string(),
        },
        // Полупрозрачная эмиссия лестницы/альфа-аналога (конфиг-путь):
        // наружу уходит oklch(L% C H / α), браузер композитит; контраст —
        // свойство композита на фоне резолва (закон лестницы ядра).
        Resolved::Translucent(rgba) => RoleOutcome::Translucent(RgbaColor {
            tint_hex: rgba.tint_hex().to_string(),
            alpha: rgba.alpha(),
            composite_hex: rgba.composite_hex().to_string(),
            composite_lc: rgba.composite_lc(),
            composite_wcag: rgba.composite_wcag(),
            alpha_coerced: rgba.alpha_coerced(),
            floor_coerced: rgba.floor_coerced(),
        }),
        // Свечение: слои + интенсивность, оператор потребителя — screen.
        Resolved::Glow(g) => RoleOutcome::Glow(crate::dto::GlowColor {
            core_hex: g.core_hex().to_string(),
            halo_hex: g.halo_hex().to_string(),
            alpha: g.alpha(),
            achieved_dj: g.achieved_dj(),
            degraded: g.degraded(),
        }),
        // ОСОЗНАННЫЙ ДОЛГ: `Resolved` — `#[non_exhaustive]`, поэтому catch-all
        // обязателен для будущих вариантов ядра. Пока маппит в стабильный код,
        // а не молча роняет неверный цвет; при экспорте rgba-границы каждый
        // новый вариант должен получить явный арм выше, а не оседать сюда.
        _ => RoleOutcome::Unreachable {
            code: "unreachable",
            message: "unmapped resolved variant".to_string(),
        },
    }
}

fn map_solved(
    solved: Solved,
    compressed: bool,
    achieved_dj: Option<f64>,
    hue_vanished: bool,
    legal_floor: Option<f64>,
) -> SolvedColor {
    SolvedColor {
        hex: solved.hex().to_owned(),
        lc: solved.lc(),
        wcag_ratio: solved.wcag_ratio(),
        compressed,
        achieved_dj,
        floor_override: solved.floor_override(),
        legal_floor,
        hue_vanished,
    }
}

/// A stable machine code for each unreachability reason. These strings are part
/// of the JS-facing contract — a caller may branch on them — so they must not
/// change silently (see `unreachable_codes_are_the_stable_js_contract`).
///
/// `Unreachable` is `#[non_exhaustive]`, so the catch-all is mandatory and
/// honest: a core variant we have not mapped yet reports `"unreachable"` rather
/// than failing to compile against a future core. Known variants get a specific
/// code so a JS caller can branch on the cause.
///
/// Note: with the v1 default role table, `resolve_theme` never actually yields
/// an unreachable role on any solid background (a wide sweep finds none) — every
/// default role is reachable everywhere. This mapping is therefore a forward-
/// compatible / defensive seam, exercised below by driving the core `solve`
/// directly into the cases a custom table or a future gamut would surface.
fn unreachable_code(reason: &Unreachable) -> &'static str {
    match reason {
        Unreachable::BelowContrastFloor { .. } => "below_contrast_floor",
        Unreachable::ExceedsRange { .. } => "exceeds_range",
        Unreachable::QuantizationGap { .. } => "quantization_gap",
        Unreachable::FloorUnreachable { .. } => "floor_unreachable",
        Unreachable::PolarityMismatch { .. } => "polarity_mismatch",
        Unreachable::GamutUnsupported => "gamut_unsupported",
        Unreachable::InvalidInput(_) => "invalid_input",
        _ => "unreachable",
    }
}

/// Normalise a background hex to the canonical `#RRGGBB` upper-case form the
/// cache keys on. Accepts `#`-led or bare, 3- or 6-digit; rejects anything else
/// with the core's own parse vocabulary so the message matches `BgInput::solid`.
fn normalise_hex(raw: &str) -> Result<String, BindingError> {
    let body = raw.strip_prefix('#').unwrap_or(raw);
    let expanded = match body.len() {
        3 => body.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => body.to_owned(),
        _ => {
            return Err(BindingError::InvalidBackground {
                reason: format!("expected #RGB or #RRGGBB, got '{raw}'"),
            });
        }
    };
    if !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BindingError::InvalidBackground {
            reason: format!("non-hex digit in '{raw}'"),
        });
    }
    Ok(format!("#{}", expanded.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An engine with the frozen labui passport loaded — the config the built-in
    /// default table used to hardcode. The agnostic engine has no built-in system,
    /// so every resolve test drives a real loaded contract.
    fn engine_with_labui() -> Engine {
        let mut engine = Engine::new();
        engine
            .load_config(&labui_json())
            .expect("labui passport loads");
        engine
    }

    #[test]
    fn resolve_theme_without_config_is_config_required() {
        // The agnostic contract: no built-in fallback. A resolve before any
        // load_config is an honest, matchable failure — not a panic, not a
        // silent default system.
        let engine = Engine::new();
        assert!(matches!(
            engine.resolve_theme("#FFFFFF", Theme::Light),
            Err(BindingError::ConfigRequired)
        ));
    }

    #[test]
    fn normalises_short_and_cased_hex() {
        assert_eq!(normalise_hex("#fff").unwrap(), "#FFFFFF");
        assert_eq!(normalise_hex("abcdef").unwrap(), "#ABCDEF");
        assert_eq!(normalise_hex("#1A2B3C").unwrap(), "#1A2B3C");
    }

    #[test]
    fn rejects_malformed_hex_with_reason() {
        assert!(matches!(
            normalise_hex("#12"),
            Err(BindingError::InvalidBackground { .. })
        ));
        assert!(matches!(
            normalise_hex("#gggggg"),
            Err(BindingError::InvalidBackground { .. })
        ));
    }

    #[test]
    fn resolves_white_light_to_keyed_entries() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert_eq!(result.theme, "light");
        assert_eq!(result.background, "#FFFFFF");
        // Generic over the role set: the config's own role names key each entry.
        // We assert the keys exist, not their count, so role growth does not
        // break this test.
        let keys: Vec<_> = result.roles.iter().map(|r| r.role_key.as_str()).collect();
        assert!(keys.contains(&"label-primary"));
        assert!(keys.contains(&"none"));
    }

    #[test]
    fn recheck_matches_resolve_theme_reported_contrasts() {
        // The WASM recheck end-to-end: resolve a set, then recheck each solved
        // colour against its OWN background — the returned interleaved (lc, wcag)
        // pairs must equal exactly what `resolve_theme` reported. This is the
        // identity the reactive controller stands on: "still passes?" means the
        // same thing as the original solve.
        let engine = engine_with_labui();
        for (bg, theme) in [
            ("#FFFFFF", Theme::Light),
            ("#3478F6", Theme::Light),
            ("#1C1C1E", Theme::Dark),
        ] {
            let result = engine.resolve_theme(bg, theme).unwrap();
            let mut fgs = Vec::new();
            let mut want = Vec::new();
            for r in &result.roles {
                if let RoleOutcome::Color(c) = &r.outcome {
                    fgs.push(c.hex.clone());
                    want.push((c.lc, c.wcag_ratio));
                }
            }
            let flat = engine.recheck(bg, &fgs, theme).unwrap();
            assert_eq!(flat.len(), want.len() * 2);
            for (i, (lc, wcag)) in want.iter().enumerate() {
                assert!((flat[2 * i] - lc).abs() < 1e-9, "{bg}: role {i} lc drift");
                assert!(
                    (flat[2 * i + 1] - wcag).abs() < 1e-9,
                    "{bg}: role {i} wcag drift"
                );
            }
        }
        // Invalid foreground hex surfaces a structured error, not a panic.
        assert!(
            Engine::new()
                .recheck("#FFFFFF", &["nothex".to_string()], Theme::Light)
                .is_err()
        );
    }

    #[test]
    fn recheck_accepts_the_same_hex_forms_as_resolve_theme() {
        // The three entry points share one hex contract: `#RGB` shorthand, a
        // missing `#`, and mixed case are all accepted by recheck exactly as by
        // resolve — and every spelling of a colour rechecks bit-identically.
        // `#123` and `#112233` are the SAME colour (each nibble is doubled), and
        // `#fff` is `#FFFFFF`, so all of these must agree with the canonical form.
        let engine = Engine::new();
        let canonical = engine
            .recheck("#FFFFFF", &["#112233".to_string()], Theme::Light)
            .unwrap();
        for bg in ["#fff", "FFFFFF", "#FFFFFF"] {
            for fg in ["#123", "112233", "#112233"] {
                let got = engine.recheck(bg, &[fg.to_string()], Theme::Light).unwrap();
                assert_eq!(got.len(), 2, "{bg}/{fg}: one (lc, wcag) pair");
                assert_eq!(got, canonical, "{bg}/{fg}: must match the canonical form");
            }
        }
    }

    #[test]
    fn unreachable_codes_are_the_stable_js_contract() {
        // The Unreachable→code mapping is a JS API contract. `Unreachable` is
        // `#[non_exhaustive]` (can't be constructed here), and the default table
        // never produces one through `resolve_theme`, so we drive the core
        // `solve` into two real cases and pin their codes against silent drift.
        use labcolors_core::ViewingConditions;
        use labcolors_core::solve::{ChromaPolicy, Contract, Gamut, Hue, solve};

        let vc = ViewingConditions::srgb();
        let neutral = ChromaPolicy::Neutral;

        // A non-sRGB gamut is reserved-but-unsupported in v1.
        let gamut_err = solve(
            BgInput::solid("#FFFFFF").unwrap(),
            Contract::text(7.0),
            Hue::deg(0.0),
            neutral,
            &vc,
            Gamut::DisplayP3,
        )
        .unwrap_err();
        assert_eq!(unreachable_code(&gamut_err), "gamut_unsupported");

        // A non-finite target is rejected up front as invalid input.
        let invalid_err = solve(
            BgInput::solid("#FFFFFF").unwrap(),
            Contract::text(f64::NAN),
            Hue::deg(0.0),
            neutral,
            &vc,
            Gamut::Srgb,
        )
        .unwrap_err();
        assert_eq!(unreachable_code(&invalid_err), "invalid_input");
    }

    #[test]
    fn none_role_resolves_to_none_outcome() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let none_entry = result.roles.iter().find(|r| r.role_key == "none").unwrap();
        assert_eq!(none_entry.outcome, RoleOutcome::None);
    }

    #[test]
    fn label_primary_on_white_is_a_dark_colour() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let tp = result
            .roles
            .iter()
            .find(|r| r.role_key == "label-primary")
            .unwrap();
        match &tp.outcome {
            RoleOutcome::Color(c) => {
                assert!(c.wcag_ratio >= 4.5, "primary text must clear AA on white");
                assert!(c.lc.abs() > 50.0, "primary text should be strong contrast");
            }
            other => panic!("expected a solved colour, got {other:?}"),
        }
    }

    #[test]
    fn legal_floor_rides_along_on_solved_colours() {
        // The DTO carries each role's legal WCAG clamp so the runtime can hold
        // the floor while easing. Anchored roles report their conformance ratio;
        // decorative / zero roles report None.
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let floor_of = |key: &str| {
            result
                .roles
                .iter()
                .find(|r| r.role_key == key)
                .map(|r| &r.outcome)
        };
        // AA text role → 4.5.
        match floor_of("label-primary") {
            Some(RoleOutcome::Color(c)) => assert_eq!(c.legal_floor, Some(4.5)),
            other => panic!("label-primary expected solved, got {other:?}"),
        }
        // AA UI role → 3.0.
        match floor_of("icon") {
            Some(RoleOutcome::Color(c)) => assert_eq!(c.legal_floor, Some(3.0)),
            other => panic!("icon expected solved, got {other:?}"),
        }
        // Decorative / JND roles carry no legal floor even when solved.
        if let Some(RoleOutcome::Color(c)) = floor_of("label-quaternary") {
            assert_eq!(c.legal_floor, None);
        }
    }

    #[test]
    fn cache_returns_identical_shared_result() {
        let engine = engine_with_labui();
        let first = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let second = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert!(
            Rc::ptr_eq(&first, &second),
            "second call must be a cache hit"
        );
    }

    #[test]
    fn cache_key_is_hex_normalised() {
        let engine = engine_with_labui();
        let canonical = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        let shorthand = engine.resolve_theme("#fff", Theme::Light).unwrap();
        assert!(
            Rc::ptr_eq(&canonical, &shorthand),
            "equivalent hex spellings must share a cache entry"
        );
    }

    #[test]
    fn ic_theme_resolves_without_error() {
        let engine = engine_with_labui();
        assert!(engine.resolve_theme("#FFFFFF", Theme::LightIc).is_ok());
    }

    #[test]
    fn css_vars_and_role_keys_are_consistent_for_the_loaded_contract() {
        // The WASM boundary contract: every role entry carries the config's own
        // `role_key` and the CSS var is built mechanically as `--lab-{role_key}`.
        // No parallel list — the emitted set follows whatever the loaded contract
        // declares. This sweeps the labui contract on representative backgrounds,
        // asserting the same non-empty role set is present every time, every key
        // is unique, and every key is constructible into a CSS var name.
        let engine = engine_with_labui();
        let reps = [
            ("#FFFFFF", Theme::Light),
            ("#000000", Theme::Dark),
            ("#808080", Theme::Light),
            // Increased-contrast variants: the same contract must hold.
            ("#FFFFFF", Theme::LightIc),
            ("#000000", Theme::DarkIc),
        ];
        // The role count is a property of the loaded contract, not a magic number:
        // pin it to the first sweep and assert every background emits the same set.
        let expected_len = engine
            .resolve_theme("#FFFFFF", Theme::Light)
            .unwrap()
            .roles
            .len();
        assert!(expected_len > 0, "loaded contract must emit roles");
        for (bg, theme) in reps {
            let result = engine.resolve_theme(bg, theme).unwrap();
            assert_eq!(
                result.roles.len(),
                expected_len,
                "{bg}: resolve must return the full loaded contract every time"
            );
            let mut seen = std::collections::HashSet::new();
            for entry in &result.roles {
                assert!(
                    seen.insert(entry.role_key.as_str()),
                    "{bg}: duplicate role_key {}",
                    entry.role_key
                );
                let css_var = format!("--lab-{}", entry.role_key);
                assert!(
                    css_var.starts_with("--lab-"),
                    "{bg} {}: CSS var {css_var} must follow --lab-{{key}} format",
                    entry.role_key
                );
                match &entry.outcome {
                    RoleOutcome::Translucent(r) => {
                        assert!(
                            r.tint_hex.starts_with('#') && r.composite_hex.starts_with('#'),
                            "{bg} {}: полупрозрачная эмиссия несёт hex-тинт и hex-композит",
                            entry.role_key
                        );
                        assert!(
                            r.alpha > 0.0 && r.alpha <= 1.0,
                            "{bg} {}: α в (0,1]",
                            entry.role_key
                        );
                    }
                    RoleOutcome::Color(c) => {
                        assert!(
                            c.hex.starts_with('#'),
                            "{bg} {}: hex must start with #",
                            entry.role_key
                        );
                        assert!(
                            c.hex.len() == 7,
                            "{bg} {}: hex {} must be #RRGGBB (7 chars), got {}",
                            entry.role_key,
                            c.hex,
                            c.hex.len(),
                        );
                    }
                    RoleOutcome::Glow(g) => {
                        assert!(
                            g.core_hex.starts_with('#') && g.halo_hex.starts_with('#'),
                            "{bg} {}: слои свечения несут hex",
                            entry.role_key
                        );
                        assert!(
                            g.alpha > 0.0 && g.alpha <= 1.0,
                            "{bg} {}: α свечения в (0,1]",
                            entry.role_key
                        );
                    }
                    RoleOutcome::None => {}
                    RoleOutcome::Unreachable { .. } => {}
                }
            }
            assert_eq!(
                seen.len(),
                expected_len,
                "{bg}: every role key must be unique"
            );
        }
    }

    /// JSON канонического labui-конфига — статический SSOT-паспорт
    /// (`tests/data/labui.config.json`). Дерево Даниила вынесено из прод-API ядра
    /// (ADR-0001 PR-c), поэтому граница читает замороженный паспорт, а не строит
    /// его из `labui_reference` (её больше нет в публичном API).
    fn labui_json() -> String {
        include_str!("../tests/data/labui.config.json").to_string()
    }

    /// Скомпилированная labui-таблица через тот же путь, что `load_config`:
    /// паспорт-JSON → DTO → `ThemeConfig` ядра → `compile_named_role_table`.
    fn labui_table() -> labcolors_core::semantic::NamedRoleTable {
        let dto: crate::config_dto::ConfigDto =
            serde_json::from_str(&labui_json()).expect("паспорт labui парсится");
        let cfg = labcolors_core::config::ThemeConfig::try_from(dto).expect("DTO → ThemeConfig");
        cfg.compile_named_role_table().expect("labui компилируется")
    }

    /// Минимальный конфиг второго клиента: другой бренд, своё пространство имён.
    fn acme_json() -> String {
        r##"{
          "brand": {"light": "#7C3AED", "dark": "#8B5CF6", "light_ic": "#5B21B6", "dark_ic": "#A78BFA"},
          "neutral": {
            "anchors": {"light": "#FFFFFF", "mid": "#7A7A82", "dark": "#17171A"},
            "tint": {"ratio": 0.1, "target_mp": 6.1, "hue_stiffness": 9.0}
          },
          "palette": [],
          "sentiments": {"categories": [], "hardness": 5.0, "chroma_fraction": 0.88},
          "themes": [{"name": "light", "preset": "srgb"}, {"name": "dark", "preset": "dim"}],
          "roles": [
            {"name": "accent-fill", "recipe": {"kind": "ladder", "source": {"kind": "brand"}, "position": "fill-primary"}},
            {"name": "body-text", "recipe": {"kind": "text-anchor", "fraction": 0.62, "floor": "aa-text"}}
          ],
          "aliases": [{"alias": "btn-label", "target": "body-text"}]
        }"##
        .to_string()
    }

    /// Загрузка конфига переключает контракт на string-keyed, отпечатки разных
    /// конфигов различны, и кэш не отдаёт чужие записи на одинаковом (bg, тема).
    #[test]
    fn load_config_switches_contract_and_separates_cache_spaces() {
        let mut engine = Engine::new();
        // Агностичный движок до load_config не несёт никакого контракта —
        // resolve обязан честно отказать, а не отдать встроенный дефолт.
        assert!(
            matches!(
                engine.resolve_theme("#FFFFFF", Theme::Light),
                Err(BindingError::ConfigRequired)
            ),
            "до load_config resolve_theme = ConfigRequired"
        );

        let fp_labui = engine.load_config(&labui_json()).expect("labui валиден");
        let labui_set = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert!(
            labui_set
                .roles
                .iter()
                .any(|r| r.role_key == "fill-brand-primary"
                    && matches!(r.outcome, RoleOutcome::Translucent(_))),
            "конфиг-контракт несёт полупрозрачная роль лестницы"
        );

        let fp_acme = engine.load_config(&acme_json()).expect("acme валиден");
        assert_ne!(fp_labui, fp_acme, "разные конфиги → разные отпечатки");
        assert_eq!(
            engine.cache.len(),
            0,
            "загрузка конфига сносит прошлое пространство записей целиком —              корректность кэша не опирается на вероятностную уникальность отпечатка"
        );
        // Тот же (bg, тема) СРАЗУ после смены конфига: попадание в чужую запись
        // было бы кэш-коллизией — пространство ключей обязано быть acme.
        let acme_set = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert!(acme_set.roles.iter().any(|r| r.role_key == "accent-fill"));
        assert!(
            acme_set
                .roles
                .iter()
                .all(|r| r.role_key != "fill-brand-primary"),
            "кэш-коллизия: под ключом acme отдан labui-контракт"
        );
        // Алиас наследует пол цели и через named-путь (btn-label → body-text,
        // aa-text → 4.5) — вторая половина класса «потерянный legal_floor».
        let alias_entry = acme_set
            .roles
            .iter()
            .find(|r| r.role_key == "btn-label")
            .expect("алиас в контракте acme");
        match &alias_entry.outcome {
            RoleOutcome::Color(c) => assert_eq!(
                c.legal_floor,
                Some(4.5),
                "алиас несёт AA-пол своей цели через named-путь"
            ),
            other => panic!("btn-label ожидался цветом, получено {other:?}"),
        }
    }

    /// Паритет: загруженный конфиг эмитит байт-в-байт то же, что прямой
    /// resolve_named_set той же таблицы (граница ничего не подменяет).
    #[test]
    fn loaded_config_matches_direct_named_resolve() {
        let mut engine = Engine::new();
        engine.load_config(&labui_json()).unwrap();
        let via_engine = engine.resolve_theme("#101012", Theme::Dark).unwrap();

        let table = labui_table();
        let bg = labcolors_core::BgInput::solid("#101012").unwrap();
        let direct =
            labcolors_core::resolve_named_set(&bg, &table, &Theme::Dark.viewing_conditions());

        assert_eq!(
            via_engine.roles.len(),
            direct.len() + table.aliases().len(),
            "полный контракт: роли ядра + алиасы границы"
        );
        // Оракул пола: та же семантика, что у загрузки — спека роли, алиас
        // наследует пол цели. Мутация, теряющая пол на named-пути, обязана
        // падать ЗДЕСЬ (выживший мутант map_resolved(_, None) — дыра ЗАКРЫТА).
        let mut expected_floor: std::collections::HashMap<&str, Option<f64>> = table
            .entries()
            .iter()
            .map(|(n, spec)| (n.as_str(), spec.legal_floor()))
            .collect();
        for (alias, target) in table.aliases() {
            let floor = expected_floor.get(target.as_str()).copied().flatten();
            expected_floor.insert(alias.as_str(), floor);
        }
        let mut anchored_seen = 0usize;
        for ((name, resolved), entry) in direct.iter().zip(via_engine.roles.iter()) {
            assert_eq!(name, &entry.role_key, "порядок и имена совпадают");
            if let RoleOutcome::Color(c) = &entry.outcome {
                let want = expected_floor.get(name.as_str()).copied().flatten();
                assert_eq!(c.legal_floor, want, "{name}: legal_floor конфиг-роли");
                if want.is_some() {
                    anchored_seen += 1;
                }
            }
            match (resolved, &entry.outcome) {
                (
                    Resolved::Color {
                        solved,
                        compressed,
                        achieved_dj,
                        hue_vanished,
                    },
                    RoleOutcome::Color(c),
                ) => {
                    assert_eq!(solved.hex(), c.hex, "{name}: hex");
                    assert_eq!(solved.lc(), c.lc, "{name}: lc");
                    assert_eq!(solved.wcag_ratio(), c.wcag_ratio, "{name}: wcag");
                    assert_eq!(*compressed, c.compressed, "{name}: compressed");
                    assert_eq!(*achieved_dj, c.achieved_dj, "{name}: achieved_dj");
                    assert_eq!(*hue_vanished, c.hue_vanished, "{name}: hue_vanished");
                    assert_eq!(
                        solved.floor_override(),
                        c.floor_override,
                        "{name}: floor_override"
                    );
                }
                (Resolved::Translucent(r), RoleOutcome::Translucent(o)) => {
                    assert_eq!(r.tint_hex(), o.tint_hex, "{name}: tint");
                    assert_eq!(r.alpha(), o.alpha, "{name}: alpha");
                    assert_eq!(r.composite_hex(), o.composite_hex, "{name}: composite");
                    assert_eq!(r.composite_lc(), o.composite_lc, "{name}: composite_lc");
                    assert_eq!(
                        r.composite_wcag(),
                        o.composite_wcag,
                        "{name}: composite_wcag"
                    );
                    assert_eq!(r.alpha_coerced(), o.alpha_coerced, "{name}: alpha_coerced");
                    assert_eq!(r.floor_coerced(), o.floor_coerced, "{name}: floor_coerced");
                }
                (Resolved::Glow(g), RoleOutcome::Glow(o)) => {
                    assert_eq!(g.core_hex(), o.core_hex, "{name}: glow core");
                    assert_eq!(g.halo_hex(), o.halo_hex, "{name}: glow halo");
                    assert_eq!(g.alpha(), o.alpha, "{name}: glow alpha");
                    assert_eq!(g.degraded(), o.degraded, "{name}: glow degraded");
                }
                (Resolved::None, RoleOutcome::None) => {}
                (a, b) => panic!("расхождение форм {name}: ядро {a:?} vs граница {b:?}"),
            }
        }
        assert!(
            anchored_seen > 0,
            "оракул пола вакуумный: ни одной конфиг-роли с ненулевым полом"
        );
        let label = via_engine
            .roles
            .iter()
            .find(|r| r.role_key == "label-primary")
            .expect("label-primary в контракте");
        match &label.outcome {
            RoleOutcome::Color(c) => assert_eq!(
                c.legal_floor,
                Some(4.5),
                "AA-пол текстового якоря доходит до границы через named-путь"
            ),
            other => panic!("label-primary ожидался цветом, получено {other:?}"),
        }
    }

    /// Невалидный конфиг отклоняется и НЕ меняет состояние (атомарность).
    #[test]
    fn invalid_config_is_rejected_atomically() {
        let mut engine = Engine::new();
        engine.load_config(&acme_json()).unwrap();

        assert!(matches!(
            engine.load_config("{ не json"),
            Err(BindingError::InvalidConfig { .. })
        ));
        let bad_position = acme_json().replace("fill-primary", "fill-quinary");
        match engine.load_config(&bad_position) {
            Err(BindingError::InvalidConfig { reason }) => {
                assert!(reason.contains("fill-quinary"), "ошибка называет позицию");
            }
            other => panic!("ждали InvalidConfig, получено {other:?}"),
        }
        // Недоменная α альфа-аналога режется полным preflight-ом ядра
        // (validate = компиляция), а не отдельной проверкой границы.
        let bad_alpha = acme_json().replace(
            r#"{"kind": "ladder", "source": {"kind": "brand"}, "position": "fill-primary"}"#,
            r#"{"kind": "alpha-analog", "of": {"kind": "brand"}, "alpha": 1.5}"#,
        );
        match engine.load_config(&bad_alpha) {
            Err(BindingError::InvalidConfig { reason }) => {
                assert!(reason.contains("alpha"), "ошибка называет ручку: {reason}");
            }
            other => panic!("α=1.5 обязана быть отвергнута, получено {other:?}"),
        }

        // Состояние прежнее: контракт acme жив.
        let set = engine.resolve_theme("#FFFFFF", Theme::Light).unwrap();
        assert!(set.roles.iter().any(|r| r.role_key == "accent-fill"));
    }
}
