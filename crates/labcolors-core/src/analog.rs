//! Приватная exact-программа допуска AlphaAnalog.
//!
//! Proposal выбирает `(tint, alpha)`, но не сертифицирует себя. Этот модуль
//! материализует ровно один финальный occurrence общей point-программой,
//! применяет exact identity constraint и только после PASS создаёт verified
//! value. Никакого результата с частичным evidence при mismatch не существует.

use crate::Srgb8;
use crate::appearance::{
    PhysicalProgramIdentityV1, PointOpacityOverSurfaceV1, ProgramOccurrenceBindingV1,
    ResolvedOccurrence, SourceOverCertificateV1,
};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityMismatchV1, ExactSrgb8IdentityV1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactIdentityCapabilityV1 {
    FinalOccurrenceSrgb8IdentityV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactIdentityReleaseV1 {
    V1,
}

/// Opaque identity authored invocation-а. Standalone helper не притворяется
/// client binding; named compiler назначает ordinal конкретной декларации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoredAlphaBindingIdV1 {
    Standalone,
    Named { declaration_ordinal: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityEvidenceV1 {
    physical: PhysicalProgramIdentityV1,
    authored: AuthoredAlphaBindingIdV1,
    constraint: ExactConstraintIdentityV1,
    capability: ExactIdentityCapabilityV1,
    release: ExactIdentityReleaseV1,
    program_occurrence: ProgramOccurrenceBindingV1,
    occurrence: SourceOverCertificateV1,
    target: Srgb8,
    actual: Srgb8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerifiedAlphaAnalogV1 {
    occurrence: ResolvedOccurrence,
    authored: AuthoredAlphaBindingIdV1,
    target: Srgb8,
}

impl VerifiedAlphaAnalogV1 {
    pub(crate) fn tint(&self) -> Srgb8 {
        Srgb8::new(self.occurrence.certificate().subject_rgb())
    }

    pub(crate) fn alpha(&self) -> f64 {
        f64::from_bits(self.occurrence.certificate().subject_opacity_bits())
    }

    pub(crate) const fn occurrence(&self) -> &ResolvedOccurrence {
        &self.occurrence
    }

    pub(crate) fn evidence(&self) -> ExactIdentityEvidenceV1 {
        ExactIdentityEvidenceV1 {
            physical: ExactAlphaProgramV1::physical_identity(),
            authored: self.authored,
            constraint: ExactAlphaProgramV1::constraint_identity(),
            capability: ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1,
            release: ExactIdentityReleaseV1::V1,
            program_occurrence: self.occurrence.program_occurrence_binding(),
            occurrence: *self.occurrence.certificate(),
            target: self.target,
            actual: Srgb8::new(self.occurrence.visible()),
        }
    }
}

/// Существование byte-тинта проверяется по крайнему тинту нужного направления:
/// source-over монотонен по tint, а соседние байты при `alpha <= 1` не могут
/// перепрыгнуть целевой байт.
fn target_is_feasible(target: Srgb8, alpha: f64, backdrop: Srgb8) -> bool {
    let target = target.bytes();
    let backdrop = backdrop.bytes();
    (0..3).all(|channel| match target[channel].cmp(&backdrop[channel]) {
        core::cmp::Ordering::Equal => true,
        core::cmp::Ordering::Greater => {
            crate::composition::source_over_channel_srgb8(u8::MAX, alpha, backdrop[channel])
                >= target[channel]
        }
        core::cmp::Ordering::Less => {
            crate::composition::source_over_channel_srgb8(u8::MIN, alpha, backdrop[channel])
                <= target[channel]
        }
    })
}

/// Первый `binary64` в `[0,1]`, на котором byte-grid допускает target.
/// Неотрицательные `f64` упорядочены битами, поэтому поиск точен и конечен.
pub(crate) fn first_alpha(target: Srgb8, backdrop: Srgb8) -> f64 {
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

/// Канонический byte-тинт при фиксированной alpha. Для каждого канала берётся
/// ближайший к непрерывной инверсии байт из полного проходящего интервала.
pub(crate) fn tint_at_alpha(target: Srgb8, alpha: f64, backdrop: Srgb8) -> Option<Srgb8> {
    if !target_is_feasible(target, alpha, backdrop) {
        return None;
    }
    let target = target.bytes();
    let backdrop = backdrop.bytes();
    let mut tint = [0_u8; 3];

    for channel in 0..3 {
        let target_channel = target[channel];
        let backdrop_channel = backdrop[channel];
        if target_channel == backdrop_channel {
            tint[channel] = backdrop_channel;
            continue;
        }
        if alpha == 0.0 {
            return None;
        }

        let output = |candidate: u8| {
            crate::composition::source_over_channel_srgb8(candidate, alpha, backdrop_channel)
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
            + (f64::from(target_channel) - f64::from(backdrop_channel)) / alpha;
        let floor = ideal.floor().clamp(f64::from(first), f64::from(last)) as u8;
        let ceil = ideal.ceil().clamp(f64::from(first), f64::from(last)) as u8;
        let error = |candidate: u8| {
            (crate::composition::source_over_channel_value(candidate, alpha, backdrop_channel)
                - f64::from(target_channel))
            .abs()
        };
        tint[channel] = if error(floor).total_cmp(&error(ceil)).is_le() {
            floor
        } else {
            ceil
        };
    }

    let actual = [
        crate::composition::source_over_channel_srgb8(tint[0], alpha, backdrop[0]),
        crate::composition::source_over_channel_srgb8(tint[1], alpha, backdrop[1]),
        crate::composition::source_over_channel_srgb8(tint[2], alpha, backdrop[2]),
    ];
    (actual == target).then_some(Srgb8::new(tint))
}

fn propose(target: Srgb8, requested_alpha: f64, backdrop: Srgb8) -> Result<(Srgb8, f64), String> {
    if !requested_alpha.is_finite() || !(0.0..=1.0).contains(&requested_alpha) {
        return Err(format!(
            "requested_alpha вне конечного [0,1]: {requested_alpha}"
        ));
    }
    if let Some(tint) = tint_at_alpha(target, requested_alpha, backdrop) {
        return Ok((tint, requested_alpha));
    }

    let alpha = first_alpha(target, backdrop);
    debug_assert!(alpha > requested_alpha);
    let tint = tint_at_alpha(target, alpha, backdrop)
        .ok_or_else(|| "первая sRGB8-alpha не дала допустимый byte-тинт".to_owned())?;
    Ok((tint, alpha))
}

/// Единственный coordinator byte-grid proposal, point occurrence и exact gate.
pub(crate) fn resolve_verified(
    authored: AuthoredAlphaBindingIdV1,
    target: Srgb8,
    requested_alpha: f64,
    backdrop: Srgb8,
) -> Result<VerifiedAlphaAnalogV1, String> {
    let (tint, alpha) = propose(target, requested_alpha, backdrop)?;
    ExactAlphaProgramV1::evaluate(authored, target, tint, alpha, backdrop)
        .map_err(|error| error.message())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExactAlphaProgramErrorV1 {
    InvalidOpacity(String),
    IdentityMismatch(ExactIdentityMismatchV1),
}

impl ExactAlphaProgramErrorV1 {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::InvalidOpacity(message) => message.clone(),
            Self::IdentityMismatch(mismatch) => format!(
                "alpha-analog не воспроизвёл sRGB8-цель: target={:?}, actual={:?}",
                mismatch.target().bytes(),
                mismatch.actual().bytes()
            ),
        }
    }
}

/// Code-owned compiled invocation: одна физическая topology и один exact
/// evaluator. Runtime bindings — только admitted bytes и alpha.
pub(crate) struct ExactAlphaProgramV1;

impl ExactAlphaProgramV1 {
    pub(crate) const fn physical_identity() -> PhysicalProgramIdentityV1 {
        PointOpacityOverSurfaceV1::physical_identity()
    }

    pub(crate) const fn constraint_identity() -> ExactConstraintIdentityV1 {
        ExactSrgb8IdentityV1::IDENTITY
    }

    pub(crate) fn evaluate(
        authored: AuthoredAlphaBindingIdV1,
        target: Srgb8,
        tint: Srgb8,
        alpha: f64,
        backdrop: Srgb8,
    ) -> Result<VerifiedAlphaAnalogV1, ExactAlphaProgramErrorV1> {
        let occurrence = PointOpacityOverSurfaceV1::evaluate(tint.bytes(), alpha, backdrop.bytes())
            .map_err(|error| {
                ExactAlphaProgramErrorV1::InvalidOpacity(error.message().to_owned())
            })?;
        let assessment = ExactSrgb8IdentityV1::evaluate(&occurrence, target)
            .map_err(ExactAlphaProgramErrorV1::IdentityMismatch)?;
        debug_assert_eq!(assessment.target(), assessment.actual());
        let verified = VerifiedAlphaAnalogV1 {
            occurrence,
            authored,
            target: assessment.target(),
        };
        debug_assert_eq!(verified.evidence().actual, assessment.actual());
        Ok(verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_alpha_facade_owns_no_byte_grid_proposal_or_verified_coordinator() {
        let facade = include_str!("alpha.rs");
        let (production, _) = facade
            .split_once("\n#[cfg(test)]\nmod tests {")
            .expect("alpha facade must retain one explicit test-module boundary");
        for forbidden in [
            "fn target_is_feasible",
            "fn first_alpha(",
            "fn tint_at_alpha",
            "fn propose(",
            "fn resolve_verified(",
            "fn srgb8_target_is_feasible",
            "fn first_srgb8_alpha",
            "fn srgb8_tint_at_alpha",
            "fn propose_alpha_analog_srgb8",
            "fn resolve_alpha_analog_srgb8_verified",
        ] {
            assert!(
                !production.contains(forbidden),
                "alpha facade still owns private exact machinery: {forbidden}"
            );
        }
        assert_eq!(
            production
                .matches("crate::analog::resolve_verified")
                .count(),
            1,
            "the public hex facade must delegate exactly once to the private coordinator"
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
                    let actual = tint_at_alpha(
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
    fn finite_invalid_alpha_is_rejected_before_any_proposal() {
        let target = Srgb8::new([1, 0, 0]);
        let backdrop = Srgb8::new([0; 3]);
        for requested_alpha in [-0.25, 1.25] {
            let error = resolve_verified(
                AuthoredAlphaBindingIdV1::Standalone,
                target,
                requested_alpha,
                backdrop,
            )
            .expect_err("invalid alpha must not be silently moved onto the exact frontier");
            assert!(
                error.contains("requested_alpha вне конечного [0,1]"),
                "unexpected rejection boundary: {error}"
            );
        }
    }

    #[test]
    fn corrupted_candidate_cannot_create_verified_output() {
        let target = Srgb8::new([0, 0, 0]);
        crate::composition::reset_source_over_evaluation_count();
        let error = ExactAlphaProgramV1::evaluate(
            AuthoredAlphaBindingIdV1::Standalone,
            target,
            Srgb8::new([255, 255, 255]),
            0.5,
            Srgb8::new([0, 0, 0]),
        )
        .expect_err("wrong final bytes must not mint VerifiedAlphaAnalogV1");
        let message = error.message();
        assert!(message.contains("target=[0, 0, 0]"), "{message}");
        assert!(message.contains("actual=[128, 128, 128]"), "{message}");

        let ExactAlphaProgramErrorV1::IdentityMismatch(mismatch) = error else {
            panic!("unexpected exact-program error: {error:?}");
        };
        assert_eq!(mismatch.target(), target);
        assert_eq!(mismatch.actual(), Srgb8::new([128, 128, 128]));
        assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    }

    #[test]
    fn exact_evidence_keeps_physics_and_authored_routing_separate() {
        let verified = ExactAlphaProgramV1::evaluate(
            AuthoredAlphaBindingIdV1::Named {
                declaration_ordinal: 7,
            },
            Srgb8::new([128, 128, 128]),
            Srgb8::new([0, 0, 0]),
            0.5,
            Srgb8::new([255, 255, 255]),
        )
        .unwrap();
        let evidence = verified.evidence();

        assert_eq!(
            evidence.physical,
            PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
        );
        assert_eq!(
            evidence.constraint,
            ExactConstraintIdentityV1::FinalSrgb8IdentityV1
        );
        assert_eq!(evidence.target, evidence.actual);
        assert_eq!(
            evidence.program_occurrence,
            verified.occurrence().program_occurrence_binding()
        );
        assert_eq!(evidence.occurrence.output_rgb(), evidence.actual.bytes());
        assert_eq!(
            evidence.authored,
            AuthoredAlphaBindingIdV1::Named {
                declaration_ordinal: 7
            }
        );
    }

    #[test]
    fn equal_physics_under_distinct_named_bindings_keeps_distinct_evidence() {
        let evaluate = |declaration_ordinal| {
            ExactAlphaProgramV1::evaluate(
                AuthoredAlphaBindingIdV1::Named {
                    declaration_ordinal,
                },
                Srgb8::new([128, 128, 128]),
                Srgb8::new([0, 0, 0]),
                0.5,
                Srgb8::new([255, 255, 255]),
            )
            .unwrap()
        };
        let first = evaluate(2).evidence();
        let second = evaluate(9).evidence();

        assert_eq!(first.physical, second.physical);
        assert_eq!(first.constraint, second.constraint);
        assert_eq!(first.capability, second.capability);
        assert_eq!(first.release, second.release);
        assert_eq!(first.program_occurrence, second.program_occurrence);
        assert_eq!(first.occurrence, second.occurrence);
        assert_eq!(first.target, second.target);
        assert_eq!(first.actual, second.actual);
        assert_ne!(first.authored, second.authored);
        assert_ne!(first, second);
    }

    #[test]
    fn exact_point_program_is_heap_allocation_free() {
        let target = Srgb8::new([0x80, 0x80, 0x80]);
        let tint = Srgb8::new([0x00, 0x00, 0x00]);
        let backdrop = Srgb8::new([0xFF, 0xFF, 0xFF]);

        let (result, allocations) = crate::test_support::measured_allocations(|| {
            ExactAlphaProgramV1::evaluate(
                AuthoredAlphaBindingIdV1::Standalone,
                target,
                tint,
                0.5,
                backdrop,
            )
        });

        assert!(result.is_ok());
        assert_eq!(
            allocations, 0,
            "exact point execution must stay on the stack"
        );
    }
}
