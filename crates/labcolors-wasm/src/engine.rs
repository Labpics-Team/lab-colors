//! The application core of the bindings: resolve a background under a theme,
//! generically over whatever role set a loaded config provides.
//!
//! This layer knows the core and the DTOs; it does NOT know wasm-bindgen. It
//! holds the compiled config table (supplied by `load_config`) and the contract
//! cache, runs the core resolve, and maps the resolved vector into
//! [`ResolvedTheme`]. The engine is agnostic (ADR-0001): it carries no
//! built-in design system, so `resolve_theme` needs a config first. The mapping
//! never enumerates roles — it walks whatever the core returns and keys each
//! entry by the config's own role name — so role growth flows through on a
//! rebuild.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use labcolors_core::config::{ThemeConfig, VcPreset};
use labcolors_core::semantic::NamedRoleTable;
use labcolors_core::{BgInput, ResolveSetError, Resolved, Solved, recheck_against};

use crate::cache::{CacheKey, ContractCache};
use crate::config_dto::{ConfigDto, fingerprint};
use crate::dto::{ResolvedTheme, RgbaColor, RoleEntry, RoleOutcome, SolvedColor};
use crate::error::{BindingError, OutputConflict, OutputConflicts};

/// How many distinct `(bg, theme, table)` resolves the cache holds before a
/// целиком. Фиксированная граница исключает неограниченный рост при
/// произвольном сэмплинге фона; это политика ёмкости, не байтовая claim.
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
    /// Общая иммутабельная проекция скомпилированного output-контракта таблицы.
    /// `OutputBindingSet::clone` разделяет immutable key storage.
    output_bindings: labcolors_core::config::OutputBindingSet,
    fingerprint: u64,
    floors: HashMap<String, Option<f64>>,
    /// Иммутабельный словарь тем конфига: (клиентский ключ, VC-пресет) в
    /// порядке объявления. Позиция пары — слот ключа кэша.
    themes: Vec<(String, VcPreset)>,
}

impl NamedState {
    /// Найти клиентский ключ темы в словаре: `(слот, VC-пресет)`.
    /// Неизвестный ключ (включая любой ключ при пустом словаре) — `None`;
    /// типизацию ошибки выбирает вызывающий.
    fn theme_binding(&self, key: &str) -> Option<(u32, VcPreset)> {
        self.themes
            .iter()
            .position(|(name, _)| name == key)
            .map(|slot| (slot as u32, self.themes[slot].1))
    }
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
    /// неизменна. Возвращает детерминированный 64-битный отпечаток. Это
    /// вероятностный идентификатор, не доказательство уникальности; пространства
    /// записей не смешиваются благодаря полному очищению кэша при reload.
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
        let themes = cfg.themes.entries.clone();
        let table = cfg
            .compile_named_role_table()
            .map_err(|e| BindingError::InvalidConfig {
                reason: e.to_string(),
            })?;
        let output_bindings = table.output_bindings().clone();
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
            output_bindings,
            fingerprint: fp,
            floors,
            themes,
        });
        Ok(fp)
    }

    /// Resolve every role for `bg_hex` under `theme`, returning the shared
    /// result. Repeated identical calls hit the contract cache.
    ///
    /// Ошибки возвращаются, не паникуют. `Unresolved` остаётся локальным
    /// успешным исходом роли; ordinary `Unreachable` агрегируется и отклоняет
    /// весь вызов. Rejected/unsupported/internal провенанс набора также не
    /// допускает появления темы.
    pub fn resolve_theme(
        &self,
        bg_hex: &str,
        theme_key: &str,
    ) -> Result<Rc<ResolvedTheme>, BindingError> {
        // Validate and normalise the background once, before the cache lookup,
        // so an invalid hex fails fast and the cache key is canonical.
        let normalised = normalise_hex(bg_hex)?;
        let bg = BgInput::solid(&normalised).map_err(|u| BindingError::InvalidBackground {
            reason: u.to_string(),
        })?;

        // Конфиг загружен → эмитится ЕГО контракт (string-keyed) той же
        // физикой; тема — КЛИЕНТСКИЙ ключ словаря конфига (канонический путь
        // client key → binding → VcPreset → ViewingConditions), отпечаток в
        // ключе разводит кэш-пространства конфигов, слот — темы внутри одного.
        if let Some(named) = &self.named {
            let (slot, preset) =
                named
                    .theme_binding(theme_key)
                    .ok_or_else(|| BindingError::UnknownTheme {
                        requested: theme_key.to_string(),
                    })?;
            let vc = preset.viewing_conditions();
            let key = CacheKey::new(normalised.clone(), slot, named.fingerprint);
            let result = self.cache.get_or_try_insert_with(key, || {
                let set = labcolors_core::resolve_named_set(&bg, &named.table, &vc)
                    .map_err(resolve_set_error_to_binding)?;
                admit_complete_output(&set)?;
                let mut reports = project_solved_reports(&set, &normalised, &vc)?.into_iter();
                let mut roles: Vec<RoleEntry> = set
                    .into_iter()
                    .map(|(name, resolved)| -> Result<RoleEntry, BindingError> {
                        let floor = named.floors.get(&name).copied().flatten();
                        let report = if resolved.solved().is_some() {
                            Some(reports.next().ok_or_else(|| {
                                BindingError::Internal {
                                    reason:
                                        "solved report projection ended before the resolved set"
                                            .to_string(),
                                }
                            })?)
                        } else {
                            None
                        };
                        Ok(RoleEntry {
                            role_key: name,
                            outcome: map_resolved(resolved, floor, report)?,
                        })
                    })
                    .collect::<Result<_, _>>()?;
                if reports.next().is_some() {
                    return Err(BindingError::Internal {
                        reason: "solved report projection outlived the resolved set".to_string(),
                    });
                }
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
                Ok(Rc::new(ResolvedTheme {
                    // Результат несёт ИСХОДНЫЙ клиентский ключ, не пресет.
                    theme: theme_key.to_string(),
                    background: normalised.clone(),
                    output_bindings: named.output_bindings.clone(),
                    roles,
                }))
            });
            return result;
        }

        // Agnostic engine: no config, nothing to emit. An honest, matchable
        // failure — the boundary refuses rather than inventing a built-in system.
        Err(BindingError::ConfigRequired)
    }

    /// Mint the numeric theme handle for a client theme key: the slot of the
    /// key in the loaded config's theme dictionary. This is the cold-edge
    /// string→number lowering (F1/F2): the controller resolves a theme name to
    /// its handle ONCE at a solve boundary, then addresses it numerically in the
    /// per-frame recheck loop — so the hot path never re-scans the dictionary by
    /// string. Recheck without a loaded config is impossible (no dictionary), and
    /// an unknown key is a typed [`BindingError::UnknownTheme`].
    pub fn theme_handle(&self, theme_key: &str) -> Result<u32, BindingError> {
        let named = self.named.as_ref().ok_or(BindingError::ConfigRequired)?;
        let (slot, _) =
            named
                .theme_binding(theme_key)
                .ok_or_else(|| BindingError::UnknownTheme {
                    requested: theme_key.to_string(),
                })?;
        Ok(slot)
    }

    /// Recheck the contrasts a set of packed `0x00RRGGBB` foreground colours
    /// achieve against a (possibly changed) packed `bg` background under the
    /// theme addressed by `theme_handle` — the cheap per-frame primitive of the
    /// reactive runtime. One display-forward for the background plus one per
    /// foreground, **no solve**: the controller keeps current colours while they
    /// still pass and re-solves only the rare role that stably fails.
    ///
    /// The packed input is one contiguous typed-array copy into linear memory:
    /// zero hex parse, zero `String`/`Cow` per foreground. The reserved high byte
    /// of every word is required-zero and validated once, without allocation.
    /// Returns a flat, interleaved buffer `[lc0, wcag0, lc1, wcag1, …]` (mapped to
    /// a JS `Float64Array`) — the same output layout the string boundary emitted,
    /// byte for byte. Values equal what the solver measured, so a freshly-resolved
    /// set rechecks to its own reported contrasts.
    pub fn recheck_u32(
        &self,
        bg: u32,
        fgs: &[u32],
        theme_handle: u32,
    ) -> Result<Vec<f64>, BindingError> {
        let vc = self.recheck_vc_by_handle(theme_handle)?;
        let pairs = labcolors_core::recheck_against_u32(bg, fgs, &vc)
            .map_err(|reason| BindingError::InvalidBackground { reason })?;
        let mut out = Vec::with_capacity(pairs.len() * 2);
        for (lc, wcag) in pairs {
            out.push(lc);
            out.push(wcag);
        }
        Ok(out)
    }

    /// Recheck one packed foreground set against MANY packed background samples in
    /// a single call, sharing each foreground's display-forward across all
    /// samples. Byte-identical, entry for entry, to N separate
    /// [`recheck_u32`](Self::recheck_u32) calls; see [`recheck_against_multi_u32`].
    /// Exported to JS as `recheckContrastMulti` and used by the `adaptTheme`
    /// controller's multi-sample worst-case backdrop loop. The flat output is
    /// background-major: sample `s`, foreground `i` sits at `(s*fgs.len()+i)*2`.
    pub fn recheck_multi_u32(
        &self,
        bgs: &[u32],
        fgs: &[u32],
        theme_handle: u32,
    ) -> Result<Vec<f64>, BindingError> {
        let vc = self.recheck_vc_by_handle(theme_handle)?;
        labcolors_core::recheck_against_multi_u32(bgs, fgs, &vc)
            .map_err(|reason| BindingError::InvalidBackground { reason })
    }

    /// Условия просмотра для recheck-пути по numeric handle: слот прямо индексирует
    /// канонический словарь тем загруженного конфига — тот же словарь, что у
    /// [`resolve_theme`](Self::resolve_theme), но адресуемый численно, без
    /// строкового пере-сканирования на каждом кадре. Recheck без загруженного
    /// конфига невозможен (нет словаря), а handle вне диапазона типизирован.
    fn recheck_vc_by_handle(
        &self,
        theme_handle: u32,
    ) -> Result<labcolors_core::ViewingConditions, BindingError> {
        let named = self.named.as_ref().ok_or(BindingError::ConfigRequired)?;
        let preset = named
            .themes
            .get(theme_handle as usize)
            .map(|(_, preset)| *preset)
            .ok_or_else(|| BindingError::UnknownTheme {
                requested: format!("theme handle {theme_handle}"),
            })?;
        Ok(preset.viewing_conditions())
    }
}

/// Core сохраняет ordinary `Unreachable` как локальное физическое свидетельство,
/// но текущий delivery snapshot умеет выразить отсутствие CSS только как remove.
/// Поэтому граница собирает ВСЕ такие роли до mapping/aliases/cache и отклоняет
/// whole resolve, не позволяя ambient CSS стать неявным fallback.
fn admit_complete_output(set: &[(String, Resolved)]) -> Result<(), BindingError> {
    let mut conflicts = set.iter().filter_map(|(role, resolved)| {
        let Resolved::Failure(failure) = resolved else {
            return None;
        };
        is_output_conflict_category(failure.category())
            .then(|| OutputConflict::unreachable(role.clone(), failure.code(), failure.to_string()))
    });
    let Some(first) = conflicts.next() else {
        return Ok(());
    };
    Err(BindingError::OutputConflict {
        conflicts: OutputConflicts::new(first, conflicts.collect()),
    })
}

/// `Unresolved` остаётся честным локальным исходом bounded search, а доказанный
/// `Unreachable` запрещает полный snapshot.
const fn is_output_conflict_category(category: labcolors_core::RoleFailureCategory) -> bool {
    matches!(category, labcolors_core::RoleFailureCategory::Unreachable)
}

/// Map one core [`Resolved`] into the boundary [`RoleOutcome`]. `legal_floor` is
/// the role's WCAG clamp (from the role table), carried onto a solved colour.
/// Ordinary `Unreachable` должен быть удалён [`admit_complete_output`] раньше;
/// его появление здесь — внутренний дрейф, не второй одиночный conflict path.
fn map_resolved(
    resolved: Resolved,
    legal_floor: Option<f64>,
    report: Option<(f64, f64)>,
) -> Result<RoleOutcome, BindingError> {
    Ok(match resolved {
        Resolved::Color {
            solved,
            compressed,
            achieved_dj,
        } => RoleOutcome::Color(map_solved(
            solved,
            compressed,
            achieved_dj,
            legal_floor,
            report.ok_or_else(|| BindingError::Internal {
                reason: "solved colour lost its report projection".to_string(),
            })?,
        )?),
        Resolved::None => RoleOutcome::None,
        Resolved::Failure(failure) => match failure.category() {
            labcolors_core::RoleFailureCategory::Unreachable => {
                return Err(BindingError::Internal {
                    reason: "ordinary Unreachable escaped output admission".to_string(),
                });
            }
            labcolors_core::RoleFailureCategory::Unresolved => RoleOutcome::Unresolved {
                code: failure.code(),
                message: failure.to_string(),
            },
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
            alpha_css: g.alpha_css().to_string(),
            target_dj: g.target_dj(),
            composite_profile: g.composite_profile(),
            composite_guarantee: g.halo_composite_certificate().guarantee(),
            layer_recipe_profile: g.layer_recipe_profile(),
            appearance_diagnostic_profile: g.appearance_diagnostic_profile(),
            selection_diagnostic_profile: g.selection_diagnostic_profile(),
            decision_outcome: g.decision_outcome(),
            constraint_layer: g.constraint_layer(),
            target_status: g.target_status(),
            halo_composite_hex: g.halo_composite_hex().to_string(),
            halo_achieved_dj: g.halo_achieved_dj(),
            core_composite_hex: g.core_composite_hex().to_string(),
            core_achieved_dj: g.core_achieved_dj(),
        }),
        Resolved::GlowIndeterminate(g) => {
            RoleOutcome::GlowIndeterminate(crate::dto::GlowIndeterminateColor {
                source_hex: g.source_hex().to_string(),
                target_dj: g.target_dj(),
                decision_profile: g.decision_profile(),
                site_id: g.site_id(),
                evidence: g.evidence(),
                constraint_layer: g.constraint_layer(),
            })
        }
        // Двухслойный материал (whitepaper, «Точечные композиции»): тинт 01 (oklch/α) + опаковая база 02.
        Resolved::Material(m) => RoleOutcome::Material(crate::dto::MaterialColor {
            tone_hex: m.tint_hex().to_string(),
            alpha: m.alpha(),
            worst_contrast: m.worst_contrast(),
            alpha_guarantee: m.alpha_guarantee(),
            alpha_status: m.alpha_status(),
            floor: m.floor(),
            pole_white: matches!(m.pole(), labcolors_core::Pole::White),
            achieved_dj: m.achieved_dj(),
            tone_compressed: m.tone_compressed(),
            distinct: m.distinct(),
        }),
        // `Resolved` is non-exhaustive across this crate boundary. An unknown
        // future semantic outcome is a structural adapter failure, never a
        // physically-looking terminal role.
        _ => return Err(unmapped_resolved_variant()),
    })
}

fn unmapped_resolved_variant() -> BindingError {
    BindingError::Internal {
        reason: "unmapped core Resolved variant".to_string(),
    }
}

fn project_solved_reports(
    set: &[(String, Resolved)],
    background_hex: &str,
    vc: &labcolors_core::ViewingConditions,
) -> Result<Vec<(f64, f64)>, BindingError> {
    let foregrounds = set
        .iter()
        .filter_map(|(_, resolved)| resolved.solved().map(Solved::hex))
        .collect::<Vec<_>>();
    let reports = recheck_against(background_hex, &foregrounds, vc).map_err(|reason| {
        BindingError::Internal {
            reason: format!("generated solved colours failed report projection: {reason}"),
        }
    })?;
    if reports.len() != foregrounds.len() {
        return Err(BindingError::Internal {
            reason: format!(
                "{} solved colours produced {} report entries",
                foregrounds.len(),
                reports.len()
            ),
        });
    }
    Ok(reports)
}

fn map_solved(
    solved: Solved,
    compressed: bool,
    achieved_dj: Option<f64>,
    legal_floor: Option<f64>,
    report: (f64, f64),
) -> Result<SolvedColor, BindingError> {
    let (measured_lc, wcag_ratio) = report;
    if measured_lc.to_bits() != solved.lc().to_bits() {
        return Err(BindingError::Internal {
            reason: format!(
                "solved/report Lc mismatch for {}: {} != {}",
                solved.hex(),
                solved.lc(),
                measured_lc
            ),
        });
    }

    Ok(SolvedColor {
        hex: solved.hex().to_owned(),
        lc: solved.lc(),
        wcag_ratio,
        compressed,
        achieved_dj,
        floor_override: solved.final_emission_adjusted(),
        legal_floor,
    })
}

fn resolve_set_error_to_binding(error: ResolveSetError) -> BindingError {
    BindingError::Internal {
        reason: error.reason().to_string(),
    }
}

/// A recheck-ready hex that BORROWS when the input is already a valid 6-hex-digit
/// colour (with or without `#`, any case), allocating only for `#RGB` shorthand
/// or a form that must change. `srgb_from_hex` (the recheck parser) is case- and
/// `#`-insensitive on a 6-digit body, so the borrowed form is byte-identical to
/// the normalised one for the numeric result — and recheck does not cache, so it
/// needs no canonical (upper-cased, `#`-led) key. Validation is preserved: a
/// 6-length non-hex body falls through to [`normalise_hex`], which rejects it
/// with the same message; other lengths likewise route through `normalise_hex`.
pub(crate) fn hex_for_recheck(raw: &str) -> Result<Cow<'_, str>, BindingError> {
    let body = raw.strip_prefix('#').unwrap_or(raw);
    if body.len() == 6 && body.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Ok(Cow::Borrowed(raw));
    }
    normalise_hex(raw).map(Cow::Owned)
}

/// Normalise a background hex to the canonical `#RRGGBB` upper-case form the
/// cache keys on. Accepts `#`-led or bare, 3- or 6-digit; rejects anything else
/// with the core's own parse vocabulary so the message matches `BgInput::solid`.
pub(crate) fn normalise_hex(raw: &str) -> Result<String, BindingError> {
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

    #[test]
    fn hot_recheck_hex_path_borrows_six_digit_inputs() {
        assert!(matches!(
            hex_for_recheck("#C0B2FA").unwrap(),
            Cow::Borrowed("#C0B2FA")
        ));
        assert!(matches!(
            hex_for_recheck("c0b2fa").unwrap(),
            Cow::Borrowed("c0b2fa")
        ));
        assert!(matches!(hex_for_recheck("#CBF").unwrap(), Cow::Owned(_)));
    }

    #[test]
    fn unknown_core_outcome_is_a_structural_boundary_error() {
        let error = unmapped_resolved_variant();
        assert!(matches!(
            error,
            BindingError::Internal { ref reason }
                if reason == "unmapped core Resolved variant"
        ));
        assert_eq!(error.code(), "internal_error");
    }

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
            engine.resolve_theme("#FFFFFF", "light"),
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
        let result = engine.resolve_theme("#FFFFFF", "light").unwrap();
        assert_eq!(result.theme, "light");
        assert_eq!(result.background, "#FFFFFF");
        // Generic over the role set: the config's own role names key each entry.
        // We assert the keys exist, not their count, so role growth does not
        // break this test.
        let keys: Vec<_> = result.roles.iter().map(|r| r.role_key.as_str()).collect();
        assert!(keys.contains(&"label-primary"));
        assert!(keys.contains(&"none"));
    }

    /// Parse an engine-emitted `#RRGGBB` into its packed `0x00RRGGBB` word —
    /// the boundary transport the packed recheck path consumes.
    fn pack_hex(hex: &str) -> u32 {
        u32::from_str_radix(hex.trim_start_matches('#'), 16).expect("engine hex is #RRGGBB")
    }

    #[test]
    fn recheck_matches_resolve_theme_reported_contrasts() {
        // The WASM recheck end-to-end: resolve a set, then recheck each solved
        // colour (packed to `0x00RRGGBB`) against its OWN background under the
        // minted theme handle — the returned interleaved (lc, wcag) pairs must
        // equal exactly what `resolve_theme` reported. This is the identity the
        // reactive controller stands on: "still passes?" means the same thing as
        // the original solve.
        let engine = engine_with_labui();
        for (bg, theme) in [
            ("#FFFFFF", "light"),
            ("#3478F6", "light"),
            ("#1C1C1E", "dark"),
        ] {
            let result = engine.resolve_theme(bg, theme).unwrap();
            let handle = engine.theme_handle(theme).unwrap();
            let mut fgs = Vec::new();
            let mut want = Vec::new();
            for r in &result.roles {
                if let RoleOutcome::Color(c) = &r.outcome {
                    fgs.push(pack_hex(&c.hex));
                    want.push((c.lc, c.wcag_ratio));
                }
            }
            let flat = engine.recheck_u32(pack_hex(bg), &fgs, handle).unwrap();
            assert_eq!(flat.len(), want.len() * 2);
            for (i, (lc, wcag)) in want.iter().enumerate() {
                assert!((flat[2 * i] - lc).abs() < 1e-9, "{bg}: role {i} lc drift");
                assert!(
                    (flat[2 * i + 1] - wcag).abs() < 1e-9,
                    "{bg}: role {i} wcag drift"
                );
            }
        }
        // A word with a non-zero reserved high byte (an RGBA/ARGB leak) surfaces a
        // structured error, not a panic — проверяется С ЗАГРУЖЕННЫМ конфигом,
        // иначе первым сработал бы ConfigRequired (C5.1: recheck требует словарь).
        let handle = engine_with_labui().theme_handle("light").unwrap();
        assert!(matches!(
            engine_with_labui().recheck_u32(0xFF00_0000, &[0x000000], handle),
            Err(BindingError::InvalidBackground { .. })
        ));
        assert!(matches!(
            engine_with_labui().recheck_u32(0x000000, &[0x0100_0000], handle),
            Err(BindingError::InvalidBackground { .. })
        ));
    }

    #[test]
    fn recheck_multi_is_byte_identical_to_per_sample_packed_recheck() {
        // C2 at the engine layer: the background-major multi buffer equals N
        // per-sample packed recheck calls exactly — the byte-identity the
        // controller's batch path stands on.
        let engine = engine_with_labui();
        let handle = engine.theme_handle("dark").unwrap();
        let result = engine.resolve_theme("#3A3A3C", "dark").unwrap();
        let fgs: Vec<u32> = result
            .roles
            .iter()
            .filter_map(|r| match &r.outcome {
                RoleOutcome::Color(c) => Some(pack_hex(&c.hex)),
                _ => None,
            })
            .collect();
        let bgs = [
            pack_hex("#38383A"),
            pack_hex("#404042"),
            pack_hex("#2E2E30"),
        ];
        let multi = engine.recheck_multi_u32(&bgs, &fgs, handle).unwrap();
        assert_eq!(multi.len(), bgs.len() * fgs.len() * 2);
        for (s, &bg) in bgs.iter().enumerate() {
            let per = engine.recheck_u32(bg, &fgs, handle).unwrap();
            let base = s * fgs.len() * 2;
            for (i, value) in per.iter().enumerate() {
                assert_eq!(multi[base + i], *value, "sample {s} index {i} drift");
            }
        }
    }

    #[test]
    fn theme_handle_addresses_the_dictionary_numerically() {
        // The numeric handle is the dictionary slot; an unknown key is typed, and
        // a handle out of range routes through the same typed rejection.
        let engine = engine_with_labui();
        let light = engine.theme_handle("light").unwrap();
        let dark = engine.theme_handle("dark").unwrap();
        assert_ne!(light, dark, "distinct themes mint distinct handles");
        assert!(matches!(
            engine.theme_handle("no-such-theme"),
            Err(BindingError::UnknownTheme { .. })
        ));
        assert!(matches!(
            engine.recheck_u32(0x000000, &[0x000000], u32::MAX),
            Err(BindingError::UnknownTheme { .. })
        ));
    }

    /// Реальный mixed-набор с конфликтами в начале, середине и конце. На
    /// `#808080` Core доказывает недостижимость всех трёх Lc-целей; на белом те
    /// же декларации законно решаются. Имена намеренно не лексикографические,
    /// чтобы сортировка вместо declaration order не прошла тест случайно.
    fn output_conflict_json() -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&labui_json()).expect("паспорт labui — валидный JSON");
        {
            let roles = value["roles"].as_array_mut().expect("roles — массив");
            roles.insert(
                0,
                serde_json::json!({
                    "name": "conflict-z",
                    "recipe": {"kind": "decorative-lc", "magnitude": 50.0}
                }),
            );
            let middle = roles.len() / 2;
            roles.insert(
                middle,
                serde_json::json!({
                    "name": "conflict-m",
                    "recipe": {"kind": "decorative-lc", "magnitude": 51.0}
                }),
            );
            roles.push(serde_json::json!({
                "name": "conflict-a",
                "recipe": {"kind": "decorative-lc", "magnitude": 52.0}
            }));
        }
        {
            let aliases = value["aliases"].as_array_mut().expect("aliases — массив");
            for (alias, target) in [
                ("conflict-z-alias", "conflict-z"),
                ("conflict-m-alias", "conflict-m"),
                ("conflict-a-alias", "conflict-a"),
            ] {
                aliases.push(serde_json::json!({"alias": alias, "target": target}));
            }
        }
        value.to_string()
    }

    #[test]
    fn ordinary_unreachable_aggregates_before_mapping_aliases_and_cache() {
        let mut engine = Engine::new();
        engine
            .load_config(&output_conflict_json())
            .expect("контрольный mixed-контракт валиден");

        let assert_conflict = |error: &BindingError| {
            let BindingError::OutputConflict { conflicts } = error else {
                panic!("ordinary Unreachable обязан стать OutputConflict, получено {error:?}");
            };
            let actual: Vec<_> = conflicts
                .iter()
                .map(|conflict| (conflict.role(), conflict.code(), conflict.message()))
                .collect();
            assert_eq!(
                actual
                    .iter()
                    .map(|(role, code, _)| (*role, *code))
                    .collect::<Vec<_>>(),
                [
                    ("conflict-z", "exceeds_range"),
                    ("conflict-m", "exceeds_range"),
                    ("conflict-a", "exceeds_range"),
                ],
                "aggregate сохраняет declaration order и не дублирует aliases"
            );
            for (index, target) in [50.0, 51.0, 52.0].into_iter().enumerate() {
                assert!(
                    actual[index].2.contains(&format!("target Lc {target:.2}")),
                    "конфликт обязан сохранить исходную диагностику Core"
                );
            }
            assert!(
                actual.iter().all(|(role, _, _)| !role.ends_with("-alias")),
                "alias не является отдельным solve и не дублирует конфликт цели"
            );
        };

        let first = engine
            .resolve_theme("#808080", "light")
            .expect_err("mixed Color/Unreachable-набор не публикует partial ResolvedTheme");
        assert_conflict(&first);
        assert_eq!(engine.cache.len(), 0, "ошибка не входит в contract cache");

        let retry = engine
            .resolve_theme("#808080", "light")
            .expect_err("то же observation после конфликта можно повторить");
        assert_conflict(&retry);
        assert_eq!(retry, first, "retry детерминирован");
        assert_eq!(engine.cache.len(), 0, "retry ошибки также не кэшируется");

        let success = engine
            .resolve_theme("#FFFFFF", "light")
            .expect("те же роли достижимы на контрольном фоне");
        let hit = engine
            .resolve_theme("#FFFFFF", "light")
            .expect("законный success остаётся кэшируемым");
        assert!(Rc::ptr_eq(&success, &hit));
        assert_eq!(engine.cache.len(), 1);
    }

    #[test]
    fn only_ordinary_unreachable_category_is_an_output_conflict() {
        assert!(is_output_conflict_category(
            labcolors_core::RoleFailureCategory::Unreachable
        ));
        assert!(!is_output_conflict_category(
            labcolors_core::RoleFailureCategory::Unresolved
        ));
    }

    #[test]
    fn ordinary_unreachable_leaking_past_set_admission_is_internal() {
        let table = labcolors_core::NamedRoleTable::new(
            vec![(
                "opaque-client-id".into(),
                labcolors_core::RoleSpec::Decorative { magnitude: 300.0 },
            )],
            Vec::new(),
            labcolors_core::RoleChroma::Neutral,
        )
        .unwrap();
        let mut set = labcolors_core::resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &labcolors_core::ViewingConditions::srgb(),
        )
        .expect("fixture создаёт admitted core failure");
        let Resolved::Failure(failure) = set.remove(0).1 else {
            panic!("fixture обязан упражнять ordinary Unreachable");
        };

        assert!(matches!(
            map_resolved(Resolved::Failure(failure), None, None),
            Err(BindingError::Internal { reason })
                if reason == "ordinary Unreachable escaped output admission"
        ));
    }

    #[test]
    fn none_role_resolves_to_none_outcome() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", "light").unwrap();
        let none_entry = result.roles.iter().find(|r| r.role_key == "none").unwrap();
        assert_eq!(none_entry.outcome, RoleOutcome::None);
    }

    #[test]
    fn label_primary_on_white_is_a_dark_colour() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", "light").unwrap();
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
        let result = engine.resolve_theme("#FFFFFF", "light").unwrap();
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
        // AA UI role → 3.0. (`icon` каноном #92 стал алиасом на label-tertiary —
        // сам label-tertiary и есть AA-UI роль с полом 3.0.)
        match floor_of("label-tertiary") {
            Some(RoleOutcome::Color(c)) => assert_eq!(c.legal_floor, Some(3.0)),
            other => panic!("label-tertiary expected solved, got {other:?}"),
        }
        // Decorative / JND roles carry no legal floor even when solved.
        if let Some(RoleOutcome::Color(c)) = floor_of("label-quaternary") {
            assert_eq!(c.legal_floor, None);
        }
    }

    #[test]
    fn final_emission_adjustment_report_is_non_vacuous_and_recheck_bound() {
        let engine = engine_with_labui();
        let result = engine.resolve_theme("#FFFFFF", "light").unwrap();
        let color = |key: &str| {
            let entry = result
                .roles
                .iter()
                .find(|entry| entry.role_key == key)
                .unwrap_or_else(|| panic!("missing role {key}"));
            match &entry.outcome {
                RoleOutcome::Color(color) => color,
                other => panic!("{key} expected color, got {other:?}"),
            }
        };

        let primary = color("label-primary");
        let tertiary = color("label-tertiary");
        assert!(
            !primary.floor_override,
            "already-admissible primary must not report movement"
        );
        assert!(
            tertiary.floor_override,
            "3:1 final criterion must report the movement from its RED candidate"
        );

        let vc = labcolors_core::VcPreset::Srgb.viewing_conditions();
        for (key, solved) in [("label-primary", primary), ("label-tertiary", tertiary)] {
            let report = labcolors_core::recheck_against("#FFFFFF", &[&solved.hex], &vc)
                .expect("emitted boundary hex rechecks");
            assert_eq!(report.len(), 1);
            assert_eq!(report[0].0.to_bits(), solved.lc.to_bits(), "{key}: lc");
            assert_eq!(
                report[0].1.to_bits(),
                solved.wcag_ratio.to_bits(),
                "{key}: wcag ratio"
            );
        }
    }

    #[test]
    fn cache_returns_identical_shared_result() {
        let engine = engine_with_labui();
        let first = engine.resolve_theme("#FFFFFF", "light").unwrap();
        let second = engine.resolve_theme("#FFFFFF", "light").unwrap();
        assert!(
            Rc::ptr_eq(&first, &second),
            "second call must be a cache hit"
        );
    }

    #[test]
    fn cache_key_is_hex_normalised() {
        let engine = engine_with_labui();
        let canonical = engine.resolve_theme("#FFFFFF", "light").unwrap();
        let shorthand = engine.resolve_theme("#fff", "light").unwrap();
        assert!(
            Rc::ptr_eq(&canonical, &shorthand),
            "equivalent hex spellings must share a cache entry"
        );
    }

    #[test]
    fn ic_theme_resolves_without_error() {
        let engine = engine_with_labui();
        assert!(engine.resolve_theme("#FFFFFF", "light-ic").is_ok());
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
            ("#FFFFFF", "light"),
            ("#000000", "dark"),
            ("#808080", "light"),
            // Increased-contrast variants: the same contract must hold.
            ("#FFFFFF", "light-ic"),
            ("#000000", "dark-ic"),
        ];
        // The role count is a property of the loaded contract, not a magic number:
        // pin it to the first sweep and assert every background emits the same set.
        let expected_len = engine
            .resolve_theme("#FFFFFF", "light")
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
                    RoleOutcome::GlowIndeterminate(g) => {
                        assert_eq!(
                            g.decision_profile,
                            labcolors_core::GlowDecisionProfileV1::StableV1
                        );
                    }
                    RoleOutcome::Material(m) => {
                        assert!(
                            m.tone_hex.starts_with('#') && m.tone_hex.len() == 7,
                            "{bg} {}: тон материала — #RRGGBB",
                            entry.role_key
                        );
                        assert!(
                            m.alpha > 0.0 && m.alpha <= 1.0,
                            "{bg} {}: α материала в (0,1]",
                            entry.role_key
                        );
                    }
                    RoleOutcome::None => {}
                    RoleOutcome::Unresolved { .. } => {}
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
    /// (ADR-0001), поэтому граница читает замороженный паспорт, а не строит
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

    /// Конфиг с ПРОИЗВОЛЬНЫМИ клиентскими именами тем: словарь принадлежит
    /// клиенту (C5.1), встроенных имён у движка нет.
    fn custom_theme_names_json() -> String {
        let mut v: serde_json::Value = serde_json::from_str(&labui_json()).unwrap();
        v["themes"] = serde_json::json!([
            {"name": "paper", "preset": "srgb"},
            {"name": "oled", "preset": "dim"},
            {"name": "paper-contrast", "preset": "srgb-ic"}
        ]);
        v.to_string()
    }

    /// C5.1, канонический путь: клиентский ключ словаря резолвится, а прежние
    /// «встроенные» имена БЕЗ объявления в словаре — типизированный отказ.
    #[test]
    fn client_theme_keys_resolve_and_builtin_names_are_gone() {
        let mut engine = Engine::new();
        engine
            .load_config(&custom_theme_names_json())
            .expect("конфиг с клиентскими именами валиден");

        let resolved = engine.resolve_theme("#FFFFFF", "paper").unwrap();
        assert_eq!(
            resolved.theme, "paper",
            "результат несёт исходный клиентский ключ"
        );

        // «light» больше не встроен: его нет в словаре ЭТОГО конфига.
        match engine.resolve_theme("#FFFFFF", "light") {
            Err(BindingError::UnknownTheme { requested }) => assert_eq!(requested, "light"),
            other => panic!("ожидался UnknownTheme для необъявленного ключа, got {other:?}"),
        }
    }

    /// Два клиентских ключа одного VcPreset: одинаковая физика (байт-в-байт
    /// те же роли), но результат сохраняет РАЗНЫЕ имена; перестановка
    /// объявлений не меняет физику по имени.
    #[test]
    fn two_keys_of_one_preset_share_physics_but_keep_names() {
        let mut v: serde_json::Value = serde_json::from_str(&labui_json()).unwrap();
        v["themes"] = serde_json::json!([
            {"name": "day", "preset": "srgb"},
            {"name": "paper", "preset": "srgb"}
        ]);
        let mut engine = Engine::new();
        engine.load_config(&v.to_string()).unwrap();

        let day = engine.resolve_theme("#FFFFFF", "day").unwrap();
        let paper = engine.resolve_theme("#FFFFFF", "paper").unwrap();
        assert_eq!(day.theme, "day");
        assert_eq!(paper.theme, "paper");
        assert_eq!(
            day.roles, paper.roles,
            "один пресет ⇒ идентичная физика ролей"
        );
        assert_eq!(day.background, paper.background);

        // Перестановка словаря: физика по имени не меняется.
        v["themes"] = serde_json::json!([
            {"name": "paper", "preset": "srgb"},
            {"name": "day", "preset": "srgb"}
        ]);
        let mut engine2 = Engine::new();
        engine2.load_config(&v.to_string()).unwrap();
        let day2 = engine2.resolve_theme("#FFFFFF", "day").unwrap();
        assert_eq!(day.roles, day2.roles, "слот в ключе кэша — не физика");
    }

    /// Пустой словарь тем — отказ НА ЗАГРУЗКЕ (симметрия с EmptyContract у
    /// ролей): без единой темы resolve/recheck тотально неработоспособны, и
    /// поздний unknown_theme был бы неотличим от опечатки. Прежнее состояние
    /// движка не тронуто (атомарность).
    #[test]
    fn empty_theme_dictionary_is_rejected_at_load() {
        let mut v: serde_json::Value = serde_json::from_str(&labui_json()).unwrap();
        v["themes"] = serde_json::json!([]);
        let mut engine = engine_with_labui();
        match engine.load_config(&v.to_string()) {
            Err(BindingError::InvalidConfig { reason }) => {
                assert!(
                    reason.contains("словарь тем пуст"),
                    "причина обязана называть пустой словарь тем, got: {reason}"
                );
            }
            other => panic!("пустой словарь тем обязан отклоняться на загрузке, got {other:?}"),
        }
        // Прежний конфиг жив: resolve по его словарю работает.
        assert!(engine.resolve_theme("#FFFFFF", "light").is_ok());
    }

    /// C5.1: recheck-путь требует загруженный конфиг НАРАВНЕ с resolve —
    /// без словаря нет ни одного валидного ключа темы.
    #[test]
    fn recheck_without_config_is_config_required() {
        let engine = Engine::new();
        assert!(matches!(
            engine.recheck_u32(0xFFFFFF, &[0x112233], 0),
            Err(BindingError::ConfigRequired)
        ));
        assert!(matches!(
            engine.recheck_multi_u32(&[0xFFFFFF], &[0x112233], 0),
            Err(BindingError::ConfigRequired)
        ));
        assert!(matches!(
            engine.theme_handle("light"),
            Err(BindingError::ConfigRequired)
        ));
    }

    /// Неудачный reload сохраняет прежние state и cache: движок продолжает
    /// отвечать прежним контрактом (атомарность загрузки).
    #[test]
    fn failed_reload_preserves_state_and_cache() {
        let mut engine = engine_with_labui();
        let before = engine.resolve_theme("#FFFFFF", "light").unwrap();

        assert!(engine.load_config("{не json").is_err());
        // И валидный JSON с невалидным конфигом:
        assert!(engine.load_config("{}").is_err());
        // C6: прежнее специальное поле не становится ignored compatibility
        // после выреза. Строгий schema-отказ также обязан быть атомарным.
        let mut legacy: serde_json::Value =
            serde_json::from_str(&labui_json()).expect("fixture JSON");
        let retired_field = legacy.get("sentiments").cloned().unwrap_or_else(|| {
            serde_json::json!({
                "categories": [],
                "hardness": 5.0,
                "chroma_fraction": 0.88
            })
        });
        legacy["sentiments"] = retired_field;
        assert!(engine.load_config(&legacy.to_string()).is_err());

        // Вложенная удалённая ручка тоже не становится тихим no-op:
        // strictness действует на всей объектной границе, а не только в корне.
        let mut nested_legacy: serde_json::Value =
            serde_json::from_str(&labui_json()).expect("fixture JSON");
        nested_legacy["palette"][0]["preferred_side"] = serde_json::json!(1);
        assert!(engine.load_config(&nested_legacy.to_string()).is_err());

        let after = engine.resolve_theme("#FFFFFF", "light").unwrap();
        assert!(
            Rc::ptr_eq(&before, &after),
            "после неудачного reload прежний cache-hit жив (state не тронут)"
        );
    }

    /// Минимальный конфиг второго клиента: другой бренд, своё пространство имён.
    fn acme_json() -> String {
        r##"{
          "brand": {"light": "#7C3AED", "dark": "#8B5CF6", "light_ic": "#5B21B6", "dark_ic": "#A78BFA"},
          "neutral": {
            "anchors": {"light": "#FFFFFF", "mid": "#7A7A82", "dark": "#17171A"},
            "tint": {"target_mp": 6.1, "hue_stiffness": 9.0}
          },
          "palette": [],
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
                engine.resolve_theme("#FFFFFF", "light"),
                Err(BindingError::ConfigRequired)
            ),
            "до load_config resolve_theme = ConfigRequired"
        );

        let fp_labui = engine.load_config(&labui_json()).expect("labui валиден");
        let labui_set = engine.resolve_theme("#FFFFFF", "light").unwrap();
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
        let acme_set = engine.resolve_theme("#FFFFFF", "light").unwrap();
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

    /// Пустой контракт (JSON со структурой, но БЕЗ `roles`) отклоняется НА
    /// ЗАГРУЗКЕ — `#[serde(default)]` на roles не должен превращать пропуск
    /// словаря в тихий пустой контракт (дефект уехал бы на использование).
    #[test]
    fn load_config_empty_contract_is_rejected() {
        let full: serde_json::Value = serde_json::from_str(&labui_json()).unwrap();
        let obj = {
            let mut m = full.as_object().unwrap().clone();
            m.remove("roles");
            m.remove("aliases");
            // роли/алиасы удалены — контракт пуст.
            m
        };
        let json = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();

        let mut engine = Engine::new();
        let err = engine
            .load_config(&json)
            .expect_err("пустой контракт обязан отклоняться на загрузке");
        assert!(
            matches!(err, BindingError::InvalidConfig { .. }),
            "структурная ошибка конфига, не паника: {err:?}"
        );
    }

    /// Паритет: загруженный конфиг эмитит байт-в-байт то же, что прямой
    /// resolve_named_set той же таблицы (граница ничего не подменяет).
    #[test]
    fn loaded_config_matches_direct_named_resolve() {
        let mut engine = Engine::new();
        engine.load_config(&labui_json()).unwrap();
        let via_engine = engine.resolve_theme("#101012", "dark").unwrap();

        let table = labui_table();
        let bg = labcolors_core::BgInput::solid("#101012").unwrap();
        let vc = labcolors_core::VcPreset::Dim.viewing_conditions();
        let direct = labcolors_core::resolve_named_set(&bg, &table, &vc)
            .expect("valid loaded table resolves atomically");

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
                    },
                    RoleOutcome::Color(c),
                ) => {
                    assert_eq!(solved.hex(), c.hex, "{name}: hex");
                    assert_eq!(solved.lc(), c.lc, "{name}: lc");
                    let report = labcolors_core::recheck_against("#101012", &[solved.hex()], &vc)
                        .expect("emitted core hex rechecks");
                    assert_eq!(report.len(), 1, "{name}: one report row");
                    assert_eq!(report[0].0.to_bits(), c.lc.to_bits(), "{name}: report lc");
                    assert_eq!(
                        report[0].1.to_bits(),
                        c.wcag_ratio.to_bits(),
                        "{name}: wcag report projection"
                    );
                    assert_eq!(*compressed, c.compressed, "{name}: compressed");
                    assert_eq!(*achieved_dj, c.achieved_dj, "{name}: achieved_dj");
                    assert_eq!(
                        solved.final_emission_adjusted(),
                        c.floor_override,
                        "{name}: final-emission adjustment report"
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
                    assert_eq!(g.alpha_css(), o.alpha_css, "{name}: glow alpha_css");
                    assert_eq!(g.target_dj(), o.target_dj, "{name}: glow target_dj");
                    assert_eq!(g.composite_profile(), o.composite_profile);
                    assert_eq!(
                        g.halo_composite_certificate().guarantee(),
                        o.composite_guarantee
                    );
                    assert_eq!(g.layer_recipe_profile(), o.layer_recipe_profile);
                    assert_eq!(
                        g.appearance_diagnostic_profile(),
                        o.appearance_diagnostic_profile
                    );
                    assert_eq!(
                        g.selection_diagnostic_profile(),
                        o.selection_diagnostic_profile
                    );
                    assert_eq!(g.decision_outcome(), o.decision_outcome);
                    assert_eq!(
                        g.decision_profile(),
                        o.decision_outcome.decision_profile(),
                        "{name}: derived decision profile"
                    );
                    assert_eq!(g.constraint_layer(), o.constraint_layer);
                    assert_eq!(g.target_status(), o.target_status);
                    assert_eq!(g.halo_composite_hex(), o.halo_composite_hex);
                    assert_eq!(g.halo_achieved_dj(), o.halo_achieved_dj);
                    assert_eq!(g.core_composite_hex(), o.core_composite_hex);
                    assert_eq!(g.core_achieved_dj(), o.core_achieved_dj);
                }
                (Resolved::GlowIndeterminate(g), RoleOutcome::GlowIndeterminate(o)) => {
                    assert_eq!(g.source_hex(), o.source_hex);
                    assert_eq!(g.target_dj(), o.target_dj);
                    assert_eq!(g.decision_profile(), o.decision_profile);
                    assert_eq!(g.site_id(), o.site_id);
                    assert_eq!(g.evidence(), o.evidence);
                }
                (Resolved::Material(m), RoleOutcome::Material(o)) => {
                    assert_eq!(m.tint_hex(), o.tone_hex, "{name}: material tone");
                    assert_eq!(m.alpha(), o.alpha, "{name}: material alpha");
                    assert_eq!(
                        m.worst_contrast(),
                        o.worst_contrast,
                        "{name}: material worst_contrast"
                    );
                    assert_eq!(m.floor(), o.floor, "{name}: material floor");
                    assert_eq!(
                        m.alpha_status(),
                        o.alpha_status,
                        "{name}: material alpha status"
                    );
                    assert_eq!(
                        matches!(m.pole(), labcolors_core::Pole::White),
                        o.pole_white,
                        "{name}: material pole_white"
                    );
                    assert_eq!(
                        m.achieved_dj(),
                        o.achieved_dj,
                        "{name}: material achieved_dj"
                    );
                    assert_eq!(
                        m.tone_compressed(),
                        o.tone_compressed,
                        "{name}: material tone_compressed"
                    );
                    assert_eq!(m.distinct(), o.distinct, "{name}: material distinct");
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
        let set = engine.resolve_theme("#FFFFFF", "light").unwrap();
        assert!(set.roles.iter().any(|r| r.role_key == "accent-fill"));
    }

    #[test]
    fn stable_glow_is_typed_indeterminate_and_emits_no_vars() {
        let stable_json = labui_json().replacen("legacy-platform-dependent-v1", "stable-v1", 1);
        let mut engine = Engine::new();
        engine
            .load_config(&stable_json)
            .expect("stable profile валиден");
        let resolved = engine
            .resolve_theme("#101012", "dark")
            .expect("resolve возвращает per-role terminal outcomes");
        let role = resolved
            .roles
            .iter()
            .find(|entry| entry.role_key == "fx-glow-brand")
            .expect("stable glow role присутствует");
        assert!(matches!(
            role.outcome,
            RoleOutcome::GlowIndeterminate(ref g)
                if g.site_id == labcolors_core::NumericalSiteIdV1::GlowTargetOrMaximumV1
                    && g.evidence
                        == labcolors_core::NumericalIndeterminacyV1::SoundBoundUnavailable
        ));

        let projected = crate::projection::resolved_json(&resolved).unwrap();
        let value: serde_json::Value = serde_json::from_str(&projected).unwrap();
        assert_eq!(
            value["roles"]["fx-glow-brand"]["kind"],
            "glow-indeterminate"
        );
        assert!(
            value["vars"].get("--lab-fx-glow-brand").is_none()
                && value["vars"].get("--lab-fx-glow-brand-core").is_none()
                && value["vars"].get("--lab-fx-glow-brand-alpha").is_none(),
            "Indeterminate не имеет права эмитить legacy fallback vars"
        );
    }

    #[test]
    fn stable_glow_exact_noop_is_bit_exact_across_wasm_projection() {
        let stable_json = labui_json().replacen("legacy-platform-dependent-v1", "stable-v1", 1);
        let mut engine = Engine::new();
        engine
            .load_config(&stable_json)
            .expect("stable profile валиден");
        let resolved = engine
            .resolve_theme("#FFFFFF", "light")
            .expect("white screen point is an exact no-op");
        let role = resolved
            .roles
            .iter()
            .find(|entry| entry.role_key == "fx-glow-brand")
            .expect("stable glow role присутствует");
        assert!(matches!(
            role.outcome,
            RoleOutcome::Glow(ref g)
                if matches!(
                    g.decision_outcome,
                    labcolors_core::glow::GlowDecisionOutcomeV1::StableExactNoop { .. }
                )
                    && g.halo_composite_hex == "#FFFFFF"
                    && g.core_composite_hex == "#FFFFFF"
        ));

        let projected: serde_json::Value =
            serde_json::from_str(&crate::projection::resolved_json(&resolved).unwrap()).unwrap();
        let glow = &projected["roles"]["fx-glow-brand"];
        assert_eq!(glow["kind"], "glow");
        assert_eq!(glow["decisionProfile"], "stable-v1");
        assert_eq!(glow["decisionGuarantee"]["kind"], "bit-exact");
        assert_eq!(glow["targetStatus"], "exact-noop-unreachable");
        assert_eq!(glow["layerRecipeProfile"], "cam16-jprime-oklab-cusp-v1");
        assert_eq!(
            glow["appearanceDiagnosticProfile"], "cam16-ucs-jprime-li2017-v1",
            "full Glow computes coreAchievedDj through CAM16"
        );
        assert_eq!(
            glow["selectionDiagnosticProfile"],
            serde_json::Value::Null,
            "exact no-op selection does not execute CAM16"
        );
        assert_eq!(glow["haloCompositeHex"], "#FFFFFF");
        for key in [
            "--lab-fx-glow-brand",
            "--lab-fx-glow-brand-core",
            "--lab-fx-glow-brand-alpha",
        ] {
            assert!(projected["vars"].get(key).is_some(), "missing {key}");
        }
    }

    /// `None` остаётся полноценным client-owned токеном в metadata, но ни сама
    /// zero-роль, ни алиас на неё не получают CSS-значение. Это anti-vacuum
    /// проверка ровно той границы, которую namespace preflight защищает.
    #[test]
    fn none_roles_and_aliases_publish_metadata_but_never_values() {
        let mut engine = Engine::new();
        engine.load_config(&labui_json()).expect("labui валиден");
        let resolved = engine
            .resolve_theme("#101012", "dark")
            .expect("валидный контракт резолвится");
        let projected: serde_json::Value =
            serde_json::from_str(&crate::projection::resolved_json(&resolved).unwrap()).unwrap();

        for name in ["none", "border-none", "fill-none", "border-ghost"] {
            let css_var = format!("--lab-{name}");
            let role = &projected["roles"][name];
            assert_eq!(role["kind"], "none", "{name} обязан остаться none");
            assert_eq!(role["cssVar"], css_var, "{name} сохраняет своё имя");
            assert!(
                projected["vars"].get(&css_var).is_none(),
                "{name}: none-токен не имеет права получить CSS-значение"
            );
        }
    }

    #[test]
    fn resolved_theme_carries_static_core_output_bindings_and_vars_are_a_subset() {
        let mut config: serde_json::Value =
            serde_json::from_str(&labui_json()).expect("labui passport is valid JSON");
        let roles = config["roles"].as_array_mut().expect("roles is an array");
        let glow = roles
            .iter_mut()
            .find(|entry| entry["name"] == "fx-glow-brand")
            .expect("fixture carries the Glow role");
        glow["recipe"]["decision_profile"] = serde_json::json!("stable-v1");
        roles.push(serde_json::json!({
            "name": "probe-material",
            "recipe": {
                "kind": "material",
                "source": {"kind": "neutral", "pick": "mid"},
                "tone_light": 10.0,
                "tone_dark": 10.0,
                "floor": "aa-text"
            }
        }));
        let aliases = config["aliases"]
            .as_array_mut()
            .expect("aliases is an array");
        aliases.extend([
            serde_json::json!({"alias": "probe-glow-alias", "target": "fx-glow-brand"}),
            serde_json::json!({"alias": "probe-material-alias", "target": "probe-material"}),
            serde_json::json!({"alias": "probe-none-alias", "target": "none"}),
        ]);

        let mut engine = Engine::new();
        engine
            .load_config(&config.to_string())
            .expect("mixed output contract compiles");
        let dark = engine
            .resolve_theme("#101012", "dark")
            .expect("mixed output contract resolves");
        let light = engine
            .resolve_theme("#FFFFFF", "light")
            .expect("same contract resolves on another background/theme");

        assert!(
            std::ptr::eq(dark.output_bindings.keys(), light.output_bindings.keys()),
            "snapshots share one immutable compiler artifact instead of cloning all keys"
        );
        assert_eq!(
            dark.output_bindings, light.output_bindings,
            "output ownership is a static compiler artifact, not a solve outcome"
        );
        let bindings = dark.output_bindings.keys();
        let unique: std::collections::BTreeSet<_> = bindings.iter().collect();
        assert_eq!(unique.len(), bindings.len(), "compiled bindings are unique");
        for expected_shape in [
            [
                "--lab-fx-glow-brand",
                "--lab-fx-glow-brand-core",
                "--lab-fx-glow-brand-alpha",
            ],
            [
                "--lab-probe-glow-alias",
                "--lab-probe-glow-alias-core",
                "--lab-probe-glow-alias-alpha",
            ],
            [
                "--lab-probe-material",
                "--lab-probe-material-01",
                "--lab-probe-material-02",
            ],
            [
                "--lab-probe-material-alias",
                "--lab-probe-material-alias-01",
                "--lab-probe-material-alias-02",
            ],
        ] {
            assert!(
                bindings
                    .windows(expected_shape.len())
                    .any(|window| window.iter().map(String::as_str).eq(expected_shape)),
                "missing exact contiguous output shape {expected_shape:?}"
            );
        }
        for key in ["--lab-none", "--lab-probe-none-alias"] {
            assert!(dark.output_bindings.contains(key), "missing reserved {key}");
        }

        let projected: serde_json::Value =
            serde_json::from_str(&crate::projection::resolved_json(&dark).unwrap()).unwrap();
        let projected_bindings = projected["outputBindings"]
            .as_array()
            .expect("outputBindings is an array")
            .iter()
            .map(|value| value.as_str().expect("binding is a string"))
            .collect::<Vec<_>>();
        assert_eq!(
            projected_bindings,
            bindings.iter().map(String::as_str).collect::<Vec<_>>()
        );
        for key in projected["vars"]
            .as_object()
            .expect("vars is an object")
            .keys()
        {
            assert!(
                dark.output_bindings.contains(key),
                "unbound emitted var {key}"
            );
        }
        for absent in [
            "--lab-fx-glow-brand",
            "--lab-fx-glow-brand-core",
            "--lab-fx-glow-brand-alpha",
            "--lab-probe-glow-alias",
            "--lab-probe-glow-alias-core",
            "--lab-probe-glow-alias-alpha",
            "--lab-none",
            "--lab-probe-none-alias",
        ] {
            assert!(
                projected["vars"].get(absent).is_none(),
                "reserved non-emitting binding {absent} must not gain a value"
            );
        }
    }

    /// Differential contract: берём фактические satellite keys из WASM-
    /// проекции валидных Glow/Material outcomes и убеждаемся, что публичный
    /// `load_config` не разрешает объявить ни zero-роль, ни zero-alias с любым
    /// из этих имён. Тест не дублирует список суффиксов валидатора как oracle.
    #[test]
    fn load_config_rejects_zero_names_for_every_projected_satellite() {
        let cases = [
            (
                "probe-glow",
                serde_json::json!({
                    "kind": "glow",
                    "source": {"kind": "brand"},
                    "step": "base",
                    "decision_profile": "legacy-platform-dependent-v1"
                }),
            ),
            (
                "probe-material",
                serde_json::json!({
                    "kind": "material",
                    "source": {"kind": "neutral", "pick": "mid"},
                    "tone_light": 10.0,
                    "tone_dark": 10.0,
                    "floor": "aa-text"
                }),
            ),
        ];

        for (owner, recipe) in cases {
            let mut valid: serde_json::Value =
                serde_json::from_str(&labui_json()).expect("паспорт labui — валидный JSON");
            valid["roles"]
                .as_array_mut()
                .expect("roles — массив")
                .push(serde_json::json!({"name": owner, "recipe": recipe}));

            let mut engine = Engine::new();
            engine
                .load_config(&valid.to_string())
                .expect("контрольный многоключевой рецепт валиден");
            let resolved = engine
                .resolve_theme("#101012", "dark")
                .expect("контрольный рецепт резолвится");
            let projected: serde_json::Value =
                serde_json::from_str(&crate::projection::resolved_json(&resolved).unwrap())
                    .unwrap();
            let prefix = format!("--lab-{owner}-");
            let satellites: Vec<String> = projected["vars"]
                .as_object()
                .expect("vars — объект")
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .cloned()
                .collect();
            assert_eq!(satellites.len(), 2, "{owner}: ожидаются два сателлита");

            for css_key in satellites {
                let zero_name = css_key
                    .strip_prefix("--lab-")
                    .expect("проекция использует канонический prefix");

                for as_alias in [false, true] {
                    let mut colliding = valid.clone();
                    if as_alias {
                        colliding["aliases"]
                            .as_array_mut()
                            .expect("aliases — массив")
                            .push(serde_json::json!({
                                "alias": zero_name,
                                "target": "none"
                            }));
                    } else {
                        colliding["roles"]
                            .as_array_mut()
                            .expect("roles — массив")
                            .push(serde_json::json!({
                                "name": zero_name,
                                "recipe": {"kind": "zero"}
                            }));
                    }

                    let mut candidate = Engine::new();
                    match candidate.load_config(&colliding.to_string()) {
                        Err(BindingError::InvalidConfig { reason }) => {
                            assert!(reason.contains(&css_key), "{reason}");
                            assert!(reason.contains("reserved CSS namespace"), "{reason}");
                        }
                        other => panic!(
                            "{css_key}: zero {} обязан быть отвергнут, получено {other:?}",
                            if as_alias { "alias" } else { "role" }
                        ),
                    }
                }
            }
        }
    }

    /// Публичная JSON-граница обязана применять полный namespace-preflight
    /// ядра: иначе алиас мог затереть числовой `-alpha` цветовой строкой уже в
    /// `vars`, хотя оба отдельных имени выглядели валидными.
    #[test]
    fn load_config_rejects_alias_colliding_with_glow_satellite() {
        let mut value: serde_json::Value =
            serde_json::from_str(&labui_json()).expect("паспорт labui — валидный JSON");
        value["aliases"]
            .as_array_mut()
            .expect("aliases — массив")
            .push(serde_json::json!({
                "alias": "fx-glow-brand-alpha",
                "target": "label-primary"
            }));

        let mut engine = Engine::new();
        match engine.load_config(&value.to_string()) {
            Err(BindingError::InvalidConfig { reason }) => {
                assert!(reason.contains("--lab-fx-glow-brand-alpha"), "{reason}");
                assert!(reason.contains("reserved CSS namespace"), "{reason}");
            }
            other => panic!("коллизия производного имени обязана быть отвергнута: {other:?}"),
        }
    }
}
