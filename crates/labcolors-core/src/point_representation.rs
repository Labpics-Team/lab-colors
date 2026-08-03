//! Exact point representation over one declared backdrop.
//!
//! Proposal выбирает `(source, opacity)`, но не сертифицирует себя. Этот модуль
//! материализует ровно один финальный occurrence общей point-программой,
//! применяет exact identity constraint и только после `Pass` создаёт verified
//! value. Никакого результата с частичным evidence при mismatch не существует.

use crate::Srgb8;
use crate::appearance::{
    PhysicalProgramIdentityV1, PointOpacityOverSurfaceV1, ProgramOccurrenceBindingV1,
    SourceOverCertificateV1,
};
use crate::composition::AdmittedOpacityV1;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerifiedPointRepresentationV1 {
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
        self.evidence.binding().occurrence_ref()
    }

    pub(crate) fn evidence(&self) -> PointRepresentationEvidenceV1 {
        let binding = *self.evidence.binding();
        let occurrence = binding.occurrence();
        PointRepresentationEvidenceV1 {
            physical: ExactPointRepresentationV1::physical_identity(),
            constraint: *self.evidence.identity(),
            capability: *self.evidence.capability(),
            release: *self.evidence.release(),
            program_occurrence: binding.program_occurrence(),
            occurrence,
            target: self.evidence.target(),
            actual: self.evidence.actual(),
        }
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
pub(crate) fn first_opacity(target: Srgb8, backdrop: Srgb8) -> f64 {
    if target == backdrop {
        return 0.0;
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
    f64::from_bits(passing)
}

/// Канонический byte-источник при фиксированной opacity. Для каждого канала
/// берётся ближайший к непрерывной инверсии байт из полного проходящего
/// интервала.
pub(crate) fn source_at_opacity(target: Srgb8, opacity: f64, backdrop: Srgb8) -> Option<Srgb8> {
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
    minimum_opacity: AdmittedOpacityV1,
    backdrop: Srgb8,
) -> Result<(Srgb8, AdmittedOpacityV1), PointRepresentationProposalErrorV1> {
    if let Some(source) = source_at_opacity(target, minimum_opacity.value(), backdrop) {
        return Ok((source, minimum_opacity));
    }

    let opacity = AdmittedOpacityV1::new(first_opacity(target, backdrop))
        .map_err(|_| PointRepresentationProposalErrorV1::DerivedOpacityOutsideUnitInterval)?;
    debug_assert!(opacity.value() > minimum_opacity.value());
    let source = source_at_opacity(target, opacity.value(), backdrop)
        .ok_or(PointRepresentationProposalErrorV1::MissingSourceAtFirstOpacity)?;
    Ok((source, opacity))
}

/// Единственный coordinator byte-grid proposal, point occurrence и exact gate.
pub(crate) fn resolve_exact_point_representation_v1(
    target: Srgb8,
    minimum_opacity: AdmittedOpacityV1,
    backdrop: Srgb8,
) -> Result<VerifiedPointRepresentationV1, ResolvePointRepresentationErrorV1> {
    let (source, opacity) = propose(target, minimum_opacity, backdrop)
        .map_err(ResolvePointRepresentationErrorV1::Proposal)?;
    ExactPointRepresentationV1::evaluate(target, source, opacity, backdrop)
        .map_err(ResolvePointRepresentationErrorV1::ConstraintViolation)
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
    DerivedOpacityOutsideUnitInterval,
    MissingSourceAtFirstOpacity,
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
/// evaluator. Runtime bindings — только admitted bytes и alpha.
pub(crate) struct ExactPointRepresentationV1;

impl ExactPointRepresentationV1 {
    pub(crate) const fn physical_identity() -> PhysicalProgramIdentityV1 {
        PointOpacityOverSurfaceV1::physical_identity()
    }

    pub(crate) fn evaluate(
        target: Srgb8,
        source: Srgb8,
        opacity: AdmittedOpacityV1,
        backdrop: Srgb8,
    ) -> Result<VerifiedPointRepresentationV1, PointRepresentationViolationV1> {
        let occurrence =
            PointOpacityOverSurfaceV1::evaluate_admitted(source.bytes(), opacity, backdrop.bytes());
        let evidence = match assess_visible_point_hard(&occurrence, &ExactSrgb8IdentityV1, target) {
            Ok(HardDecision::Pass(evidence)) => evidence,
            Ok(HardDecision::Violation(violation)) => {
                return Err(PointRepresentationViolationV1 { violation });
            }
            Err(error) => match error {},
        };
        let verified = VerifiedPointRepresentationV1 { evidence };
        debug_assert_eq!(
            verified.evidence().actual,
            Srgb8::new(verified.certificate().output_rgb())
        );
        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admitted(alpha: f64) -> AdmittedOpacityV1 {
        AdmittedOpacityV1::new(alpha).expect("test alpha must be admitted")
    }

    #[test]
    fn requested_opacity_is_preserved_when_feasible_and_raised_when_required() {
        let backdrop = Srgb8::new([0; 3]);
        let feasible =
            resolve_exact_point_representation_v1(Srgb8::new([128; 3]), admitted(0.5), backdrop)
                .unwrap();
        assert_eq!(feasible.opacity().value().to_bits(), 0.5_f64.to_bits());

        let requested = admitted(0.001);
        let raised =
            resolve_exact_point_representation_v1(Srgb8::new([128; 3]), requested, backdrop)
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
                        alpha,
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
        let evidence = verified.evidence();

        assert_eq!(
            evidence.physical,
            PhysicalProgramIdentityV1::InputOpacityOverSurfaceEncodedSrgb8V1
        );
        assert_eq!(
            evidence.constraint,
            ExactConstraintIdentityV1::FinalSrgb8IdentityV1
        );
        assert_eq!(evidence.target, evidence.actual);
        assert_eq!(
            evidence.program_occurrence,
            verified.evidence.binding().program_occurrence()
        );
        assert_eq!(evidence.occurrence.output_rgb(), evidence.actual.bytes());
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
