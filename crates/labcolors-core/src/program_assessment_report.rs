//! Owned-проекция финальных оценок Program. Вычисление остаётся у Core evaluator.

use crate::Srgb8;
use crate::cleanliness::assessment::ProgramCleanlinessAssessmentV1;
use crate::program::VerifiedOutputV1;
use crate::program_session::{
    CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1, CoreProgramViolationEvidenceV1,
    DeclaredSrgb8CleanSetViolationV1, ProgramConstraintPassEvidenceV1, ProgramConstraintResultV1,
    ProgramConstraintViolationEvidenceV1,
};

/// Сохранённый исход ограничения, без нового вычисления на стороне потребителя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramAssessmentVerdictV1 {
    Pass,
    Violation,
}

/// Закрытое семейство публичных итогов. Численные внутренности evaluator не
/// становятся обязанностью клиента; Clean Set описывает только точную конвенцию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramConstraintAssessmentV1 {
    NominalSrgb8VisiblePass {
        visible: Srgb8,
    },
    NominalSrgb8VisibleRejected {
        visible: Srgb8,
        rejected_blue_interval: [u8; 2],
    },
    VisibleSeparation {
        verdict: ProgramAssessmentVerdictV1,
        measurement: crate::constraints::ProgramVisibleSeparationMeasurementV1,
    },
    VisiblePairSeparation {
        verdict: ProgramAssessmentVerdictV1,
        measurement: crate::constraints::ProgramVisibleSeparationMeasurementV1,
    },
    ExactSrgb8 {
        verdict: ProgramAssessmentVerdictV1,
    },
    Wcag22Srgb8 {
        verdict: ProgramAssessmentVerdictV1,
    },
    IntrinsicUnary {
        verdict: ProgramAssessmentVerdictV1,
    },
    Relation {
        verdict: ProgramAssessmentVerdictV1,
    },
    TonalHierarchy {
        verdict: ProgramAssessmentVerdictV1,
    },
    DeclaredSrgb8CleanSetPass {
        visible: Srgb8,
    },
    DeclaredSrgb8CleanSetRejected {
        visible: Srgb8,
        rejected_blue_interval: [u8; 2],
    },
    DeclaredSrgb8CleanSetFinalOwnedDomainAbsent,
}

/// Итог одной клетки case × constraint выбранного или фиксированного состояния.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramAssessmentCellV1 {
    constraint_id: u32,
    case_index: usize,
    hard: bool,
    assessment: ProgramConstraintAssessmentV1,
}

impl ProgramAssessmentCellV1 {
    #[must_use]
    pub const fn constraint_id(self) -> u32 {
        self.constraint_id
    }

    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    #[must_use]
    pub const fn hard(self) -> bool {
        self.hard
    }

    #[must_use]
    pub const fn assessment(self) -> ProgramConstraintAssessmentV1 {
        self.assessment
    }
}

/// ID исходного сценария, сохранённый после схлопывания физически одинаковых cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramScenarioProvenanceV1 {
    case_index: usize,
    scenario_id: u32,
}

impl ProgramScenarioProvenanceV1 {
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    #[must_use]
    pub const fn scenario_id(self) -> u32 {
        self.scenario_id
    }
}

/// Final encoded evidence for one adjacent pair in an authored tonal hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramTonalHierarchyPairV1 {
    constraint_id: u32,
    case_index: usize,
    stronger_occurrence_id: u32,
    weaker_occurrence_id: u32,
    measurement: crate::constraints::TonalHierarchyPairMeasurementV1,
}
impl ProgramTonalHierarchyPairV1 {
    #[must_use]
    pub const fn constraint_id(self) -> u32 {
        self.constraint_id
    }
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }
    #[must_use]
    pub const fn stronger_occurrence_id(self) -> u32 {
        self.stronger_occurrence_id
    }
    #[must_use]
    pub const fn weaker_occurrence_id(self) -> u32 {
        self.weaker_occurrence_id
    }
    #[must_use]
    pub const fn measurement(self) -> crate::constraints::TonalHierarchyPairMeasurementV1 {
        self.measurement
    }
}

/// Which authored preference determined this reference use's objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramReferencePreferenceV1 {
    ReferenceStrength,
    ExplicitTone,
}

/// Правило выбора для всей физической компоненты задаётся до поиска.
/// При исчерпании выделенного объёма поиска компонента сохраняет свой режим.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramSelectionModeV1 {
    OrderedV1,
    JointPointV1,
    OrderedReferenceV1,
    JointReferencePeersV1,
}

impl ProgramSelectionModeV1 {
    pub(crate) fn symmetric(self) -> bool {
        matches!(self, Self::JointPointV1 | Self::JointReferencePeersV1)
    }
}

/// Применения с общим правилом выбора и объёмом поиска. Имена и псевдонимы
/// выходов относятся к представлению результата и не влияют на классификацию.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSelectionComponentV1 {
    pub(crate) targets: Box<[u32]>,
    pub(crate) mode: ProgramSelectionModeV1,
}

impl ProgramSelectionComponentV1 {
    #[must_use]
    pub fn targets(&self) -> &[u32] {
        &self.targets
    }

    #[must_use]
    pub const fn mode(&self) -> ProgramSelectionModeV1 {
        self.mode
    }
}

/// Bounds on joint, baseline-capped preservation capacity. One hard-feasible
/// complete tuple must serve every case while retaining the prior paint values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramReferenceCapacityV1 {
    pub(crate) ratio_lower_bits: u64,
    pub(crate) ratio_upper_bits: u64,
    pub(crate) complete: bool,
}
impl ProgramReferenceCapacityV1 {
    #[must_use]
    pub fn ratio_lower(self) -> f64 {
        f64::from_bits(self.ratio_lower_bits)
    }
    #[must_use]
    pub fn ratio_upper(self) -> f64 {
        f64::from_bits(self.ratio_upper_bits)
    }
    /// Complete at ratio 1 proves preservation is attainable, not maximal CR.
    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

/// The same final composited bytes used by the selector and final verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramReferenceCaseV1 {
    pub(crate) case_index: usize,
    pub(crate) reference_visible: Srgb8,
    pub(crate) reference_backdrop: Srgb8,
    pub(crate) actual_visible: Srgb8,
    pub(crate) actual_backdrop: Srgb8,
    pub(crate) reference_contrast_bits: u64,
    pub(crate) actual_contrast_bits: u64,
    pub(crate) polarity: crate::constraints::TonalPolarityV1,
    pub(crate) strength_loss_bits: u64,
    pub(crate) virtual_distance_squared_bits: u64,
}
impl ProgramReferenceCaseV1 {
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }
    #[must_use]
    pub const fn reference_visible(self) -> Srgb8 {
        self.reference_visible
    }
    #[must_use]
    pub const fn reference_backdrop(self) -> Srgb8 {
        self.reference_backdrop
    }
    #[must_use]
    pub const fn actual_visible(self) -> Srgb8 {
        self.actual_visible
    }
    #[must_use]
    pub const fn actual_backdrop(self) -> Srgb8 {
        self.actual_backdrop
    }
    #[must_use]
    pub fn reference_contrast(self) -> f64 {
        f64::from_bits(self.reference_contrast_bits)
    }
    #[must_use]
    pub fn actual_contrast(self) -> f64 {
        f64::from_bits(self.actual_contrast_bits)
    }
    #[must_use]
    pub const fn polarity(self) -> crate::constraints::TonalPolarityV1 {
        self.polarity
    }
    #[must_use]
    pub fn strength_loss(self) -> f64 {
        f64::from_bits(self.strength_loss_bits)
    }
    #[must_use]
    pub fn virtual_distance_squared(self) -> f64 {
        f64::from_bits(self.virtual_distance_squared_bits)
    }
}

/// One use's historical selection evidence. Capacity belongs to the whole
/// joint sign signature; it is not an independent per-case maximum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramReferenceSelectionV1 {
    pub(crate) target_id: u32,
    pub(crate) occurrence_id: u32,
    pub(crate) preference: ProgramReferencePreferenceV1,
    pub(crate) capacity: Option<ProgramReferenceCapacityV1>,
    pub(crate) cases: std::sync::Arc<[ProgramReferenceCaseV1]>,
}
impl ProgramReferenceSelectionV1 {
    #[must_use]
    pub const fn target_id(&self) -> u32 {
        self.target_id
    }
    #[must_use]
    pub const fn occurrence_id(&self) -> u32 {
        self.occurrence_id
    }
    #[must_use]
    pub const fn preference(&self) -> ProgramReferencePreferenceV1 {
        self.preference
    }
    #[must_use]
    pub const fn capacity(&self) -> Option<ProgramReferenceCapacityV1> {
        self.capacity
    }
    #[must_use]
    pub fn cases(&self) -> &[ProgramReferenceCaseV1] {
        &self.cases
    }
}

/// Исторический owned-отчёт одного Ready-снимка, а не capability публикации.
/// Клиент связывает constraint ID со своей декларацией той же content identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramAssessmentReportV1 {
    content_identity: [u8; 32],
    stream_id: u32,
    revision: u64,
    scenario_provenance: Vec<ProgramScenarioProvenanceV1>,
    cells: Vec<ProgramAssessmentCellV1>,
    tonal_hierarchy: Vec<ProgramTonalHierarchyPairV1>,
    reference_selections: Option<std::sync::Arc<[ProgramReferenceSelectionV1]>>,
    selection_components: Option<std::sync::Arc<[ProgramSelectionComponentV1]>>,
    cleanliness: ProgramCleanlinessAssessmentV1,
}

impl ProgramAssessmentReportV1 {
    /// The Core-selected rule for each computed component; no client-side
    /// inference from reference declarations or currently emitted values.
    #[must_use]
    pub fn selection_components(&self) -> &[ProgramSelectionComponentV1] {
        self.selection_components.as_deref().unwrap_or(&[])
    }

    #[must_use]
    pub fn reference_selections(&self) -> &[ProgramReferenceSelectionV1] {
        self.reference_selections.as_deref().unwrap_or(&[])
    }
    /// Measured adjacent pairs from the same final physical recheck.
    #[must_use]
    pub fn tonal_hierarchy(&self) -> &[ProgramTonalHierarchyPairV1] {
        &self.tonal_hierarchy
    }
    #[must_use]
    pub const fn content_identity(&self) -> [u8; 32] {
        self.content_identity
    }

    #[must_use]
    pub const fn stream_id(&self) -> u32 {
        self.stream_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn scenario_provenance(&self) -> &[ProgramScenarioProvenanceV1] {
        &self.scenario_provenance
    }

    #[must_use]
    pub fn cells(&self) -> &[ProgramAssessmentCellV1] {
        &self.cells
    }

    /// Сводка тех же конечных оценок, без повторного обхода или классификации.
    #[must_use]
    pub const fn cleanliness(&self) -> ProgramCleanlinessAssessmentV1 {
        self.cleanliness
    }
}

#[derive(Default)]
pub(super) struct ProgramAssessmentReportScratchV1 {
    scenario_provenance: Vec<ProgramScenarioProvenanceV1>,
    cells: Vec<ProgramAssessmentCellV1>,
    tonal_hierarchy: Vec<ProgramTonalHierarchyPairV1>,
}

impl ProgramAssessmentReportScratchV1 {
    pub(super) fn clear(&mut self) {
        self.scenario_provenance.clear();
        self.cells.clear();
        self.tonal_hierarchy.clear();
    }

    /// Резервирование завершается до атомарного перехода Session или sink.
    pub(super) fn reserve(
        &mut self,
        cells: usize,
        scenarios: usize,
        relation_members: usize,
    ) -> Result<(), ()> {
        self.clear();
        self.cells.try_reserve_exact(cells).map_err(|_| ())?;
        self.tonal_hierarchy
            .try_reserve_exact(relation_members)
            .map_err(|_| ())?;
        self.scenario_provenance
            .try_reserve_exact(scenarios)
            .map_err(|_| ())
    }

    /// Ёмкости ограничены prospective bound владельца и числом входных сценариев.
    /// Копирование не аллоцирует, не переоценивает и не меняет выбранные Paint.
    pub(super) fn copy_verified(
        &mut self,
        verified: VerifiedOutputV1<'_>,
    ) -> ProgramAssessmentReportV1 {
        let observation = verified.observation();
        for (case_index, case) in observation.physical_cases().enumerate() {
            self.scenario_provenance
                .extend(
                    case.provenance()
                        .map(|scenario| ProgramScenarioProvenanceV1 {
                            case_index,
                            scenario_id: scenario.value(),
                        }),
                );
        }
        let mut cleanliness = ProgramCleanlinessAssessmentV1::default();
        self.cells.extend(verified.cells().map(|cell| {
            let mut assessment = copy_assessment(cell.result());
            if let ProgramConstraintAssessmentV1::Relation { verdict } = assessment {
                let span = match cell.result() {
                    ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::Relation(span))
                    | ProgramConstraintResultV1::Violation(ProgramConstraintViolationEvidenceV1::Relation(span)) => *span,
                    _ => unreachable!("relation verdict has a member span"),
                };
                let members = verified.relation_members(span);
                if let [crate::program_session::ProgramRelationMemberEvidenceV1::Visible {
                    measurement: crate::constraints::CoreRelationMeasurementV1::PairSeparation(measurement), ..
                }] = members {
                    assessment = ProgramConstraintAssessmentV1::VisiblePairSeparation { verdict, measurement: *measurement };
                } else if matches!(cell.subject(), crate::program_session::ProgramConstraintSubjectV1::ComputedVisibleRelation) {
                    assessment = ProgramConstraintAssessmentV1::TonalHierarchy { verdict };
                    for member in members {
                        if let crate::program_session::ProgramRelationMemberEvidenceV1::TonalHierarchy {
                            stronger, weaker, measurement: crate::constraints::CoreRelationMeasurementV1::TonalHierarchy(measurement), ..
                        } = member {
                            self.tonal_hierarchy.push(ProgramTonalHierarchyPairV1 {
                                constraint_id: cell.constraint().value(),
                                case_index: cell.case_index(),
                                stronger_occurrence_id: stronger.occurrence().value(),
                                weaker_occurrence_id: weaker.occurrence().value(),
                                measurement: *measurement,
                            });
                        }
                    }
                }
            }
            match assessment {
                ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetPass { .. }
                | ProgramConstraintAssessmentV1::NominalSrgb8VisiblePass { .. } => {
                    cleanliness.accept();
                }
                ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetRejected { .. }
                | ProgramConstraintAssessmentV1::NominalSrgb8VisibleRejected { .. } => {
                    cleanliness.reject();
                }
                ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent => {
                    cleanliness.omit_domain();
                }
                _ => {}
            }
            ProgramAssessmentCellV1 {
                constraint_id: cell.constraint().value(),
                case_index: cell.case_index(),
                hard: cell.is_hard(),
                assessment,
            }
        }));
        ProgramAssessmentReportV1 {
            content_identity: *verified.content_identity().as_bytes(),
            stream_id: observation.stream().value(),
            revision: observation.revision(),
            scenario_provenance: std::mem::take(&mut self.scenario_provenance),
            cells: std::mem::take(&mut self.cells),
            tonal_hierarchy: std::mem::take(&mut self.tonal_hierarchy),
            reference_selections: verified.reference_selections().cloned(),
            selection_components: verified.selection_components().cloned(),
            cleanliness,
        }
    }
}

fn copy_assessment(
    assessment: &ProgramConstraintResultV1<CoreProgramEvaluatorsV1>,
) -> ProgramConstraintAssessmentV1 {
    let verdict = if assessment.is_violation() {
        ProgramAssessmentVerdictV1::Violation
    } else {
        ProgramAssessmentVerdictV1::Pass
    };
    match assessment {
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::NominalSrgb8Visible(evidence),
        )) => ProgramConstraintAssessmentV1::NominalSrgb8VisiblePass {
            visible: *evidence.measurement().value(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::NominalSrgb8Visible(evidence),
            ),
        ) => ProgramConstraintAssessmentV1::NominalSrgb8VisibleRejected {
            visible: *evidence.measurement().value(),
            rejected_blue_interval: evidence.measurement().classification().endpoints(),
        },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::VisibleSeparation(evidence),
        )) => ProgramConstraintAssessmentV1::VisibleSeparation {
            verdict,
            measurement: *evidence.measurement().value(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::VisibleSeparation(evidence),
            ),
        ) => ProgramConstraintAssessmentV1::VisibleSeparation {
            verdict,
            measurement: *evidence.measurement().value(),
        },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::ExactSrgb8(_),
        ))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::ExactSrgb8(_),
            ),
        ) => ProgramConstraintAssessmentV1::ExactSrgb8 { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::Wcag22Srgb8(_),
        ))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::Wcag22Srgb8(_),
            ),
        ) => ProgramConstraintAssessmentV1::Wcag22Srgb8 { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::IntrinsicUnary(_))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::IntrinsicUnary(_),
        ) => ProgramConstraintAssessmentV1::IntrinsicUnary { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::Relation(_))
        | ProgramConstraintResultV1::Violation(ProgramConstraintViolationEvidenceV1::Relation(_)) => {
            ProgramConstraintAssessmentV1::Relation { verdict }
        }
        ProgramConstraintResultV1::Pass(
            ProgramConstraintPassEvidenceV1::DeclaredSrgb8CleanSet(value),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetPass {
            visible: value.visible(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                DeclaredSrgb8CleanSetViolationV1::Rejected {
                    visible,
                    rejected_blue_interval,
                },
            ),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetRejected {
            visible: *visible,
            rejected_blue_interval: rejected_blue_interval.endpoints(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                DeclaredSrgb8CleanSetViolationV1::FinalOwnedDomainAbsent,
            ),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleanliness::assessment::ProgramCleanlinessStatusV1;
    use crate::program;
    use crate::program_wire::{
        CompiledProgramV1, ProgramScenarioV1, ProgramSessionV1, ProgramSnapshotStateV1,
    };

    fn runtime(nominal_profile: bool) -> Result<ProgramSessionV1, String> {
        let source = program::SourceIdV1::new(1);
        let target = program::TargetIdV1::new(2);
        let port = program::SurfaceInputPortIdV1::new(3);
        let opacity = program::OpacityInputIdV1::new(4);
        let solid = program::PaintIdV1::new(5);
        let material = program::PaintIdV1::new(6);
        let surface = program::SurfaceIdV1::new(7);
        let occurrence = program::OccurrenceIdV1::new(8);
        let root = program::PresentationRootIdV1::new(9);
        let context =
            program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average)
                .map_err(|error| format!("appearance context: {error:?}"))?;
        let white = Srgb8::new([255; 3]);
        let mut draft = program::DraftV1::new();
        draft.push_source(source, white);
        draft.push_fixed_target(target, source);
        draft.push_surface_input_port(port);
        draft.push_opacity_input(opacity, 0.5);
        draft.push_solid_paint(solid, target);
        draft.push_opacity_paint(material, solid, opacity);
        draft.push_input_surface(surface, port);
        draft.push_source_over_occurrence(occurrence, material, surface, context);
        draft.push_point_presentation_root(root, occurrence);
        draft.push_point_presentation_target(root, occurrence);
        draft.push_exact_intrinsic_unary_hard(program::ConstraintIdV1::new(10), target, white);
        if nominal_profile {
            draft.push_declared_srgb8_clean_set_report_only(
                program::ConstraintIdV1::new(11),
                root,
                occurrence,
            );
        }
        draft.push_output(program::OutputSlotIdV1::new(12), material);
        draft.set_assignment(Vec::new());
        CompiledProgramV1 {
            owner: draft
                .compile()
                .map_err(|error| format!("compile: {error:?}"))?,
        }
        .instantiate(1)
        .map_err(|error| format!("instantiate: {error:?}"))
    }

    fn scenario(id: u32, rgb: [u8; 3]) -> ProgramScenarioV1 {
        ProgramScenarioV1::new(id, vec![Srgb8::new(rgb)])
    }

    #[test]
    fn cleanliness_summary_never_promotes_other_conditions_to_conformance() -> Result<(), String> {
        let mut session = runtime(false)?;
        let snapshot = session
            .update_observed(1, &[scenario(1, [1; 3])])
            .map_err(|error| format!("observe: {error:?}"))?;
        assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
        let summary = snapshot.cleanliness_assessment().ok_or("current summary")?;
        assert_eq!(summary.status(), ProgramCleanlinessStatusV1::NotAssessed);
        assert_eq!(
            (
                summary.accepted(),
                summary.rejected(),
                summary.unavailable()
            ),
            (0, 0, 0)
        );
        assert_eq!(
            snapshot
                .assessment_report()
                .ok_or("current report")?
                .cleanliness(),
            summary
        );
        Ok(())
    }

    #[test]
    fn cleanliness_summary_retains_all_final_cases_and_unavailable_domains() -> Result<(), String> {
        let mut session = runtime(true)?;
        // One physical material yields accepted gray, rejected tinted gray and
        // no owned contribution. Its white source alone cannot establish these.
        let snapshot = session
            .update_observed(
                1,
                &[
                    scenario(1, [1; 3]),
                    scenario(2, [1, 1, 3]),
                    scenario(3, [255; 3]),
                ],
            )
            .map_err(|error| format!("observe initial material: {error:?}"))?;
        assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
        let summary = snapshot.cleanliness_assessment().ok_or("current summary")?;
        assert_eq!(summary.status(), ProgramCleanlinessStatusV1::Violates);
        assert_eq!(
            (
                summary.accepted(),
                summary.rejected(),
                summary.unavailable()
            ),
            (1, 1, 1)
        );

        let absent = session
            .update_observed(2, &[scenario(4, [255; 3])])
            .map_err(|error| format!("observe absent contribution: {error:?}"))?;
        let unavailable = absent
            .cleanliness_assessment()
            .ok_or("unavailable summary")?;
        assert_eq!(
            unavailable.status(),
            ProgramCleanlinessStatusV1::Unavailable
        );
        assert_eq!(unavailable.unavailable(), 1);

        let accepted = session
            .update_observed(3, &[scenario(5, [1; 3]), scenario(6, [1; 3])])
            .map_err(|error| format!("observe conforming material: {error:?}"))?;
        let conforms = accepted
            .cleanliness_assessment()
            .ok_or("conforming summary")?;
        assert_eq!(conforms.status(), ProgramCleanlinessStatusV1::Conforms);
        assert_eq!(
            conforms.accepted(),
            1,
            "duplicate scenarios share one physical case"
        );
        assert_eq!(
            accepted
                .assessment_report()
                .ok_or("conforming report")?
                .scenario_provenance()
                .len(),
            2
        );
        assert_eq!(snapshot.cleanliness_assessment(), Some(summary));
        Ok(())
    }
}
