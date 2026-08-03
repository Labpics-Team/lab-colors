//! Exact point representation over one declared backdrop.
//!
//! Proposal выбирает least feasible `(source, opacity)` внутри объявленного
//! замкнутого opacity-domain, но не сертифицирует себя. Это отдельный explicit
//! `MostTransparent` release замороженного frontend-а, а не скрытый universal
//! `auto`. Модуль материализует ровно один финальный occurrence общей
//! point-программой, применяет exact identity constraint и только после `Pass`
//! создаёт verified value. Никакого результата с частичным evidence при
//! mismatch не существует.

use crate::Srgb8;
use crate::appearance::{
    PhysicalProgramIdentityV1, PointOpacityOverSurfaceV1, ProgramOccurrenceBindingV1,
    SourceOverCertificateV1,
};
use crate::composition::{AdmittedOpacityV1, OpacityDomainV1};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1, HardDecision,
    assess_visible_point_hard,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointRepresentationEvidenceV1 {
    physical: PhysicalProgramIdentityV1,
    constraint: ExactConstraintIdentityV1,
    capability: ExactIdentityCapabilityV1,
    release: ExactIdentityReleaseV1,
    program_occurrence: ProgramOccurrenceBindingV1,
    occurrence: SourceOverCertificateV1,
    target: Srgb8,
    actual: Srgb8,
    selection: PointRepresentationSelectionEvidenceV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerifiedPointRepresentationV1 {
    exact: VerifiedExactPointV1,
    selection: PointRepresentationSelectionEvidenceV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VerifiedExactPointV1 {
    evidence: ExactPassEvidenceV1,
}

impl VerifiedPointRepresentationV1 {
    pub(crate) fn source(&self) -> Srgb8 {
        Srgb8::new(self.certificate().subject_rgb())
    }

    pub(crate) fn opacity(&self) -> AdmittedOpacityV1 {
        self.certificate().subject_opacity()
    }

    pub(crate) fn certificate(&self) -> &SourceOverCertificateV1 {
        self.exact.evidence.binding().occurrence_ref()
    }

    pub(crate) const fn selection(&self) -> PointRepresentationSelectionEvidenceV1 {
        self.selection
    }

    pub(crate) fn evidence(&self) -> PointRepresentationEvidenceV1 {
        let binding = *self.exact.evidence.binding();
        let occurrence = binding.occurrence();
        PointRepresentationEvidenceV1 {
            physical: ExactPointRepresentationV1::physical_identity(),
            constraint: *self.exact.evidence.identity(),
            capability: *self.exact.evidence.capability(),
            release: *self.exact.evidence.release(),
            program_occurrence: binding.program_occurrence(),
            occurrence,
            target: self.exact.evidence.target(),
            actual: self.exact.evidence.actual(),
            selection: self.selection,
        }
    }
}

/// Identity of the exact selection law used by this point coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointRepresentationSelectionReleaseV1 {
    /// Full sRGB8 source domain and the least feasible opacity inside a
    /// declared closed interval. Это explicit policy fragment, не package auto.
    ExactSrgb8MostTransparentV1,
}

/// Evidence that selection happened inside one declared opacity-domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointRepresentationSelectionEvidenceV1 {
    release: PointRepresentationSelectionReleaseV1,
    domain: OpacityDomainV1,
    first_feasible: AdmittedOpacityV1,
    frontier_predecessor: Option<AdmittedOpacityV1>,
    selected: AdmittedOpacityV1,
}

impl PointRepresentationSelectionEvidenceV1 {
    pub(crate) const fn release(self) -> PointRepresentationSelectionReleaseV1 {
        self.release
    }

    pub(crate) const fn domain(self) -> OpacityDomainV1 {
        self.domain
    }

    pub(crate) const fn first_feasible(self) -> AdmittedOpacityV1 {
        self.first_feasible
    }

    pub(crate) const fn frontier_predecessor(self) -> Option<AdmittedOpacityV1> {
        self.frontier_predecessor
    }

    pub(crate) const fn selected(self) -> AdmittedOpacityV1 {
        self.selected
    }
}

/// Существование byte-источника проверяется по крайнему значению нужного
/// направления: source-over монотонен по source, а соседние байты при
/// `opacity <= 1` не могут
/// перепрыгнуть целевой байт.
fn target_is_feasible(target: Srgb8, opacity: f64, backdrop: Srgb8) -> bool {
    let target = target.bytes();
    let backdrop = backdrop.bytes();
    (0..3).all(|channel| match target[channel].cmp(&backdrop[channel]) {
        core::cmp::Ordering::Equal => true,
        core::cmp::Ordering::Greater => {
            crate::composition::source_over_channel_srgb8(u8::MAX, opacity, backdrop[channel])
                >= target[channel]
        }
        core::cmp::Ordering::Less => {
            crate::composition::source_over_channel_srgb8(u8::MIN, opacity, backdrop[channel])
                <= target[channel]
        }
    })
}

/// Первый `binary64` в `[0,1]`, на котором byte-grid допускает target.
/// Неотрицательные `f64` упорядочены битами, поэтому поиск точен и конечен.
pub(crate) fn first_opacity(target: Srgb8, backdrop: Srgb8) -> AdmittedOpacityV1 {
    if target == backdrop {
        return AdmittedOpacityV1::TRANSPARENT;
    }
    let mut failing = 0.0_f64.to_bits();
    let mut passing = 1.0_f64.to_bits();
    debug_assert!(!target_is_feasible(target, 0.0, backdrop));
    debug_assert!(target_is_feasible(target, 1.0, backdrop));

    while passing - failing > 1 {
        let middle = failing + (passing - failing) / 2;
        if target_is_feasible(target, f64::from_bits(middle), backdrop) {
            passing = middle;
        } else {
            failing = middle;
        }
    }
    AdmittedOpacityV1::new(f64::from_bits(passing))
        .expect("binary search is bounded by admitted transparent and opaque endpoints")
}

/// Канонический byte-источник при фиксированной opacity. Для каждого канала
/// берётся ближайший к непрерывной инверсии байт из полного проходящего
/// интервала.
pub(crate) fn source_at_opacity(
    target: Srgb8,
    opacity: AdmittedOpacityV1,
    backdrop: Srgb8,
) -> Option<Srgb8> {
    source_at_opacity_value(target, opacity.value(), backdrop)
}

fn source_at_opacity_value(target: Srgb8, opacity: f64, backdrop: Srgb8) -> Option<Srgb8> {
    if !target_is_feasible(target, opacity, backdrop) {
        return None;
    }
    let target = target.bytes();
    let backdrop = backdrop.bytes();
    let mut source = [0_u8; 3];

    for channel in 0..3 {
        let target_channel = target[channel];
        let backdrop_channel = backdrop[channel];
        if target_channel == backdrop_channel {
            source[channel] = backdrop_channel;
            continue;
        }
        if opacity == 0.0 {
            return None;
        }

        let output = |candidate: u8| {
            crate::composition::source_over_channel_srgb8(candidate, opacity, backdrop_channel)
        };
        let mut lo = 0_u16;
        let mut hi = 255_u16;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if output(mid as u8) < target_channel {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        // Feasibility помещает target между output(0) и output(255), а шаг
        // монотонного source-over не превышает один byte, поэтому lower-bound
        // не может ни выйти за 255, ни перепрыгнуть target.
        let first = lo as u8;
        debug_assert_eq!(output(first), target_channel);

        lo = u16::from(first);
        hi = 256;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if output(mid as u8) <= target_channel {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let last = (lo - 1) as u8;

        let ideal = f64::from(backdrop_channel)
            + (f64::from(target_channel) - f64::from(backdrop_channel)) / opacity;
        let floor = ideal.floor().clamp(f64::from(first), f64::from(last)) as u8;
        let ceil = ideal.ceil().clamp(f64::from(first), f64::from(last)) as u8;
        let error = |candidate: u8| {
            (crate::composition::source_over_channel_value(candidate, opacity, backdrop_channel)
                - f64::from(target_channel))
            .abs()
        };
        source[channel] = if error(floor).total_cmp(&error(ceil)).is_le() {
            floor
        } else {
            ceil
        };
    }

    let actual = [
        crate::composition::source_over_channel_srgb8(source[0], opacity, backdrop[0]),
        crate::composition::source_over_channel_srgb8(source[1], opacity, backdrop[1]),
        crate::composition::source_over_channel_srgb8(source[2], opacity, backdrop[2]),
    ];
    (actual == target).then_some(Srgb8::new(source))
}

fn propose(
    target: Srgb8,
    domain: OpacityDomainV1,
    backdrop: Srgb8,
) -> Result<
    (
        Srgb8,
        AdmittedOpacityV1,
        PointRepresentationSelectionEvidenceV1,
    ),
    PointRepresentationProposalErrorV1,
> {
    let first_feasible = first_opacity(target, backdrop);
    if domain.upper().bits() < first_feasible.bits() {
        return Err(PointRepresentationProposalErrorV1::NoFeasibleOpacity {
            domain,
            first_feasible,
        });
    }

    let feasible_lower = if domain.lower().bits() < first_feasible.bits() {
        first_feasible
    } else {
        domain.lower()
    };
    let selected = feasible_lower;
    debug_assert!(domain.contains(selected));
    let source = source_at_opacity(target, selected, backdrop)
        .ok_or(PointRepresentationProposalErrorV1::MissingSourceAtSelectedOpacity { selected })?;
    let frontier_predecessor = first_feasible.predecessor();
    let selection = PointRepresentationSelectionEvidenceV1 {
        release: PointRepresentationSelectionReleaseV1::ExactSrgb8MostTransparentV1,
        domain,
        first_feasible,
        frontier_predecessor,
        selected,
    };
    Ok((source, selected, selection))
}

/// Coordinator explicit MostTransparent release, point occurrence и exact gate.
pub(crate) fn resolve_exact_point_representation_v1(
    target: Srgb8,
    domain: OpacityDomainV1,
    backdrop: Srgb8,
) -> Result<VerifiedPointRepresentationV1, ResolvePointRepresentationErrorV1> {
    let (source, opacity, selection) =
        propose(target, domain, backdrop).map_err(ResolvePointRepresentationErrorV1::Proposal)?;
    let exact = ExactPointRepresentationV1::evaluate(target, source, opacity, backdrop)
        .map_err(ResolvePointRepresentationErrorV1::ConstraintViolation)?;
    let verified = VerifiedPointRepresentationV1 { exact, selection };
    let evidence = verified.evidence();
    debug_assert_eq!(evidence.selection.release(), selection.release());
    debug_assert_eq!(evidence.selection.domain(), domain);
    debug_assert_eq!(verified.selection().selected(), verified.opacity());
    debug_assert_eq!(selection.first_feasible(), first_opacity(target, backdrop));
    debug_assert!(
        !selection
            .frontier_predecessor()
            .is_some_and(|predecessor| target_is_feasible(target, predecessor.value(), backdrop)),
    );
    Ok(verified)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvePointRepresentationErrorV1 {
    Proposal(PointRepresentationProposalErrorV1),
    // Нынешний exact byte-grid proposal не может попасть сюда, но coordinator
    // не имеет права стирать physical witness, если его построитель изменится.
    ConstraintViolation(PointRepresentationViolationV1),
}

/// Typed отказ proposal до materialization occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointRepresentationProposalErrorV1 {
    NoFeasibleOpacity {
        domain: OpacityDomainV1,
        first_feasible: AdmittedOpacityV1,
    },
    MissingSourceAtSelectedOpacity {
        selected: AdmittedOpacityV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointRepresentationViolationV1 {
    violation: ExactViolationEvidenceV1,
}

impl PointRepresentationViolationV1 {
    #[cfg(test)]
    pub(crate) const fn violation(&self) -> &ExactViolationEvidenceV1 {
        &self.violation
    }
}

/// Code-owned compiled invocation: одна физическая topology и один exact
/// evaluator. Runtime bindings — только admitted bytes и opacity.
pub(crate) struct ExactPointRepresentationV1;

impl ExactPointRepresentationV1 {
    pub(crate) const fn physical_identity() -> PhysicalProgramIdentityV1 {
        PointOpacityOverSurfaceV1::physical_identity()
    }

    fn evaluate(
        target: Srgb8,
        source: Srgb8,
        opacity: AdmittedOpacityV1,
        backdrop: Srgb8,
    ) -> Result<VerifiedExactPointV1, PointRepresentationViolationV1> {
        let occurrence =
            PointOpacityOverSurfaceV1::evaluate_admitted(source.bytes(), opacity, backdrop.bytes());
        let evidence = match assess_visible_point_hard(&occurrence, &ExactSrgb8IdentityV1, target) {
            Ok(HardDecision::Pass(evidence)) => evidence,
            Ok(HardDecision::Violation(violation)) => {
                return Err(PointRepresentationViolationV1 { violation });
            }
            Err(error) => match error {},
        };
        let verified = VerifiedExactPointV1 { evidence };
        debug_assert_eq!(
            verified.evidence.actual(),
            Srgb8::new(verified.evidence.binding().occurrence_ref().output_rgb())
        );
        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::OpacityDomainV1;
    use proptest::prelude::*;

    fn admitted(alpha: f64) -> AdmittedOpacityV1 {
        AdmittedOpacityV1::new(alpha).expect("test alpha must be admitted")
    }

    fn domain(lower: f64, upper: f64) -> OpacityDomainV1 {
        OpacityDomainV1::try_new(lower, upper).expect("test opacity domain must be admitted")
    }

    fn exact_source_exists(target: Srgb8, opacity: AdmittedOpacityV1, backdrop: Srgb8) -> bool {
        let target = target.bytes();
        let backdrop = backdrop.bytes();
        (0..3).all(|channel| {
            (u8::MIN..=u8::MAX).any(|source| {
                crate::composition::source_over_channel_srgb8(
                    source,
                    opacity.value(),
                    backdrop[channel],
                ) == target[channel]
            })
        })
    }

    #[test]
    fn declared_range_lower_bound_changes_the_exact_most_transparent_representation() {
        let target = Srgb8::new([128; 3]);
        let backdrop = Srgb8::new([0; 3]);
        let unrestricted = domain(0.0, 1.0);
        let bounded = domain(0.75, 1.0);

        let frontier =
            resolve_exact_point_representation_v1(target, unrestricted, backdrop).unwrap();
        let bounded = resolve_exact_point_representation_v1(target, bounded, backdrop).unwrap();

        assert!(frontier.opacity().value() < bounded.opacity().value());
        assert_eq!(bounded.opacity().bits(), 0.75_f64.to_bits());
        for verified in [frontier, bounded] {
            assert_eq!(
                crate::composition::source_over_srgb8(
                    verified.source().bytes(),
                    verified.opacity().value(),
                    backdrop.bytes(),
                )
                .unwrap(),
                target.bytes(),
            );
        }
    }

    #[test]
    fn most_transparent_carries_an_exact_predecessor_witness() {
        let target = Srgb8::new([1, 0, 0]);
        let backdrop = Srgb8::new([0; 3]);
        let verified =
            resolve_exact_point_representation_v1(target, domain(0.0, 1.0), backdrop).unwrap();
        let selection = verified.selection();
        let predecessor = selection
            .frontier_predecessor()
            .expect("a positive frontier must have a binary64 predecessor");

        assert_eq!(verified.opacity(), selection.first_feasible());
        assert!(exact_source_exists(target, verified.opacity(), backdrop));
        assert!(!exact_source_exists(target, predecessor, backdrop));
    }

    #[test]
    fn a_range_ending_before_the_exact_frontier_is_a_typed_conflict() {
        let target = Srgb8::new([128; 3]);
        let backdrop = Srgb8::new([0; 3]);
        let frontier = first_opacity(target, backdrop);
        let predecessor = admitted(f64::from_bits(frontier.bits() - 1));
        let bounded = domain(0.0, predecessor.value());

        assert_eq!(
            resolve_exact_point_representation_v1(target, bounded, backdrop),
            Err(ResolvePointRepresentationErrorV1::Proposal(
                PointRepresentationProposalErrorV1::NoFeasibleOpacity {
                    domain: bounded,
                    first_feasible: frontier,
                },
            )),
        );
    }

    #[test]
    fn fixed_opacity_never_silently_moves() {
        let target = Srgb8::new([128; 3]);
        let backdrop = Srgb8::new([0; 3]);
        let fixed = domain(0.5, 0.5);
        let verified = resolve_exact_point_representation_v1(target, fixed, backdrop).unwrap();
        assert_eq!(verified.opacity().bits(), 0.5_f64.to_bits());

        let impossible = domain(0.1, 0.1);
        assert!(matches!(
            resolve_exact_point_representation_v1(target, impossible, backdrop),
            Err(ResolvePointRepresentationErrorV1::Proposal(
                PointRepresentationProposalErrorV1::NoFeasibleOpacity { domain, .. }
            )) if domain == impossible
        ));
    }

    #[test]
    fn target_equal_to_backdrop_has_a_canonical_transparent_frontier() {
        let target = Srgb8::new([12, 34, 56]);
        let verified =
            resolve_exact_point_representation_v1(target, domain(0.0, 1.0), target).unwrap();

        assert_eq!(verified.source(), target);
        assert_eq!(verified.opacity(), AdmittedOpacityV1::TRANSPARENT);
        assert_eq!(
            verified.selection().first_feasible(),
            AdmittedOpacityV1::TRANSPARENT
        );
        assert_eq!(verified.selection().frontier_predecessor(), None);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn selected_opacity_is_exact_and_minimal_inside_every_generated_domain(
            target in any::<[u8; 3]>(),
            backdrop in any::<[u8; 3]>(),
            first_endpoint in 0_u16..=1024,
            second_endpoint in 0_u16..=1024,
        ) {
            let lower_numerator = first_endpoint.min(second_endpoint);
            let upper_numerator = first_endpoint.max(second_endpoint);
            // Степени двойки дают точно представимые тестовые границы; сам поиск
            // остаётся непрерывным по всем допустимым значениям binary64.
            let lower = f64::from(lower_numerator) / 1024.0;
            let upper = f64::from(upper_numerator) / 1024.0;
            let domain = domain(lower, upper);
            let target = Srgb8::new(target);
            let backdrop = Srgb8::new(backdrop);
            let frontier = first_opacity(target, backdrop);
            let result = resolve_exact_point_representation_v1(target, domain, backdrop);

            if domain.upper().bits() < frontier.bits() {
                prop_assert_eq!(
                    result,
                    Err(ResolvePointRepresentationErrorV1::Proposal(
                        PointRepresentationProposalErrorV1::NoFeasibleOpacity {
                            domain,
                            first_feasible: frontier,
                        },
                    )),
                );
                return Ok(());
            }

            let verified = result.unwrap();
            let selected = verified.opacity();
            let expected = if domain.lower().bits() < frontier.bits() {
                frontier
            } else {
                domain.lower()
            };
            prop_assert_eq!(selected, expected);
            prop_assert!(domain.contains(selected));
            prop_assert!(exact_source_exists(target, selected, backdrop));
            prop_assert_eq!(
                crate::composition::source_over_srgb8(
                    verified.source().bytes(),
                    selected.value(),
                    backdrop.bytes(),
                ).unwrap(),
                target.bytes(),
            );
            if selected.bits() > domain.lower().bits() {
                let predecessor = admitted(f64::from_bits(selected.bits() - 1));
                prop_assert!(!exact_source_exists(target, predecessor, backdrop));
            }
        }
    }

    #[test]
    fn requested_opacity_is_preserved_when_feasible_and_raised_when_required() {
        let backdrop = Srgb8::new([0; 3]);
        let feasible =
            resolve_exact_point_representation_v1(Srgb8::new([128; 3]), domain(0.5, 1.0), backdrop)
                .unwrap();
        assert_eq!(feasible.opacity().value().to_bits(), 0.5_f64.to_bits());

        let requested = admitted(0.001);
        let raised = resolve_exact_point_representation_v1(
            Srgb8::new([128; 3]),
            domain(requested.value(), 1.0),
            backdrop,
        )
        .unwrap();
        assert!(raised.opacity().value() > requested.value());
        assert_eq!(
            crate::composition::source_over_srgb8(
                raised.source().bytes(),
                raised.opacity().value(),
                backdrop.bytes(),
            )
            .unwrap(),
            [128; 3]
        );
    }

    #[test]
    fn canonical_tint_matches_independent_exhaustive_byte_oracle() {
        for alpha in [0.125, 0.5, 0.875, 1.0] {
            for backdrop in u8::MIN..=u8::MAX {
                for target in u8::MIN..=u8::MAX {
                    let expected = (u8::MIN..=u8::MAX)
                        .filter(|&candidate| {
                            crate::composition::source_over_channel_srgb8(
                                candidate, alpha, backdrop,
                            ) == target
                        })
                        .min_by(|left, right| {
                            let error = |candidate| {
                                (crate::composition::source_over_channel_value(
                                    candidate, alpha, backdrop,
                                ) - f64::from(target))
                                .abs()
                            };
                            error(*left)
                                .total_cmp(&error(*right))
                                .then_with(|| left.cmp(right))
                        });
                    let actual = source_at_opacity(
                        Srgb8::new([target, backdrop, backdrop]),
                        admitted(alpha),
                        Srgb8::new([backdrop; 3]),
                    )
                    .map(|tint| tint.bytes()[0]);
                    assert_eq!(
                        actual, expected,
                        "target={target}, backdrop={backdrop}, alpha={alpha}"
                    );
                }
            }
        }
    }

    #[test]
    fn corrupted_candidate_cannot_create_verified_output() {
        let target = Srgb8::new([0, 0, 0]);
        crate::composition::reset_source_over_evaluation_count();
        let error = ExactPointRepresentationV1::evaluate(
            target,
            Srgb8::new([255, 255, 255]),
            admitted(0.5),
            Srgb8::new([0, 0, 0]),
        )
        .expect_err("wrong final bytes must not mint a verified representation");
        let witness = error;
        fn requires_violation(_: ExactViolationEvidenceV1) {}
        assert_eq!(witness.violation().target(), target);
        assert_eq!(witness.violation().actual(), Srgb8::new([128; 3]));
        requires_violation(witness.violation);
        assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    }

    #[test]
    fn exact_evidence_contains_only_physics_and_the_executed_constraint() {
        let verified = ExactPointRepresentationV1::evaluate(
            Srgb8::new([128, 128, 128]),
            Srgb8::new([0, 0, 0]),
            admitted(0.5),
            Srgb8::new([255, 255, 255]),
        )
        .unwrap();
        fn requires_pass(_: ExactPassEvidenceV1) {}
        requires_pass(verified.evidence);
        let evidence = verified.evidence;

        assert_eq!(
            ExactPointRepresentationV1::physical_identity(),
            PhysicalProgramIdentityV1::InputOpacityOverSurfaceEncodedSrgb8V1
        );
        assert_eq!(
            *evidence.identity(),
            ExactConstraintIdentityV1::FinalSrgb8IdentityV1
        );
        assert_eq!(evidence.target(), evidence.actual());
        assert_eq!(
            evidence.binding().program_occurrence(),
            verified.evidence.binding().program_occurrence(),
        );
        assert_eq!(
            evidence.binding().occurrence_ref().output_rgb(),
            evidence.actual().bytes(),
        );
    }

    #[test]
    fn exact_point_program_is_heap_allocation_free() {
        let target = Srgb8::new([0x80, 0x80, 0x80]);
        let tint = Srgb8::new([0x00, 0x00, 0x00]);
        let backdrop = Srgb8::new([0xFF, 0xFF, 0xFF]);

        let (result, allocations) = crate::test_support::measured_allocations(|| {
            ExactPointRepresentationV1::evaluate(target, tint, admitted(0.5), backdrop)
        });

        assert!(result.is_ok());
        assert_eq!(
            allocations, 0,
            "exact point execution must stay on the stack"
        );
    }
}
