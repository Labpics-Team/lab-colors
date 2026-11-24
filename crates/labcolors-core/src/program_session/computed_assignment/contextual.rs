//! Поиск контекстных применений: физический граф, характер, предпочтение.
//! Тональные начальные приближения ускоряют поиск, но не сужают его область.

use super::*;
use crate::color_intent::color_at_lightness;
use crate::observation::PhysicalValuesV1;
use std::cell::{RefCell, RefMut};

// Performance policy: an initial repair and a final repair each receive at
// most one quarter of the existing synchronous budget. At least half remains
// for unrestricted domain search; mathematical acceptance never uses this split.
const COORDINATE_BUDGET_DIVISOR: u64 = 4;

pub(super) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"labcolors.assignment-execution.v17.nominal-visible-boxes.continuous-preferred-point.domain-preserving-neutral.independent-components.unique-zero-prefix.local-prefix-polish.conditional-factors.local-context-seeds.visible-oklab-max-cases.unchanged-authored-source.lex-rgb.independent-authored-context-seeds.reverse-forward-coordinate-tonal256.quarter-budget.full-domain-dfs.bounded-prefix-refinement.endpoints.canonical-case-refinement.full-recheck\0";
    let mut bytes = [0; DOMAIN.len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&budget.to_le_bytes());
    *crate::sha256::digest(&bytes).as_bytes()
}

pub(super) fn reference_execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"labcolors.assignment-reference.v8.nominal-visible-boxes.continuous-preferred-point.source-identity.immutable-visible-reference.upper-monotone-cubic.continuous-explicit-tone.conditional-prior-paints.actual-backdrop-vector.joint-capped-capacity.ordered-use-keys.shared-component-budget.final-visit-fallback.pending-points.source-tie.identity-projected-tonal256.branch-local-depth-priority-bidirectional-repair.hard-cell-preserving-plateaus.quarter-budget.canonical-case-refinement.full-capacity.terminal-exact-proposal-reserve-tonal256\0";
    let mut bytes = [0; DOMAIN.len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&budget.to_le_bytes());
    *crate::sha256::digest(&bytes).as_bytes()
}

#[derive(Clone, Copy)]
struct VisibleLabV1 {
    rgb: Srgb8,
    lab: [f64; 3],
}

struct ContextV1<'a> {
    targets: ComputedTargetsV1<'a>,
    constraint_indices: Option<&'a [usize]>,
    physical: Option<&'a CompiledAppearanceConeV1>,
    domains: Vec<AssignmentDomainV1>,
    preferred: Vec<[f64; 3]>,
    preferred_rgb: Vec<Srgb8>,
    local_cases: LocalCasesV1,
    case_refinement: RefCell<CaseRefinementV1>,
    score_extrema: RefCell<Vec<usize>>,
    identity_memberships: RefCell<Vec<Option<(Srgb8, bool)>>>,
    visible_labs: RefCell<Vec<Option<VisibleLabV1>>>,
    opaque: Vec<bool>,
    point_equivalence: Option<&'a PointEquivalenceV1>,
}

#[derive(Default)]
struct LocalCasesV1 {
    order: Vec<usize>,
    representatives: Vec<usize>,
}

// Search probes use a growing subset of canonical local cases. Its maxima
// are lower bounds and its minima are upper bounds on the complete support.
// Only complete assessments may supply incumbent objectives or capacities.
struct CaseRefinementV1 {
    cases: Vec<usize>,
    ranks: Vec<usize>,
    present: Vec<bool>,
    count: usize,
}

impl CaseRefinementV1 {
    fn new(local: &LocalCasesV1, count: usize) -> Result<Self, ()> {
        let mut result = Self {
            cases: Vec::new(),
            ranks: buffer(count, usize::MAX).map_err(|_| ())?,
            present: buffer(count, false).map_err(|_| ())?,
            count: local.order.len(),
        };
        result
            .cases
            .try_reserve_exact(local.order.len())
            .map_err(|_| ())?;
        for (rank, &case) in local.order.iter().enumerate() {
            result.ranks[case] = rank;
        }
        if let Some(&first) = local.order.first() {
            result.add(first);
        }
        if let Some(&last) = local.order.last() {
            result.add(last);
        }
        #[cfg(test)]
        if case_refinement_tests::eager() {
            for &case in &local.order {
                result.add(case);
            }
        }
        Ok(result)
    }

    fn complete(&self) -> bool {
        self.cases.len() == self.count
    }

    fn add(&mut self, case: usize) {
        if self.ranks[case] == usize::MAX || self.present[case] {
            return;
        }
        let rank = self.ranks[case];
        let position = self
            .cases
            .partition_point(|&other| self.ranks[other] < rank);
        self.cases.insert(position, case);
        self.present[case] = true;
    }

    fn first_case(&self, cases: impl Iterator<Item = usize>) -> Option<usize> {
        cases
            .filter(|&case| self.ranks[case] != usize::MAX)
            .min_by_key(|&case| self.ranks[case])
    }
}

pub(super) fn local_case_order<E: ProgramConstraintEvaluatorSetV1>(
    component: &ComputedComponentV1,
    scenarios: NonEmptyScenarioSetV1<'_>,
    mut record_duplicate: impl FnMut(usize, usize),
) -> Result<Vec<usize>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let count = scenarios.len().get();
    let mut cases = Vec::new();
    cases
        .try_reserve_exact(count)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for case in 0..count {
        let values = scenarios
            .physical_values(case)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if component.inputs.iter().any(|&input| input >= values.len()) {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
        cases.push((case, values));
    }
    let compare = |left: PhysicalValuesV1<'_>, right: PhysicalValuesV1<'_>| {
        component
            .inputs
            .iter()
            .map(|&input| left[input])
            .cmp(component.inputs.iter().map(|&input| right[input]))
    };
    cases.sort_unstable_by(|left, right| compare(left.1, right.1));
    cases.dedup_by(|later, retained| {
        if compare(later.1, retained.1) != Ordering::Equal {
            return false;
        }
        record_duplicate(later.0, retained.0);
        true
    });
    let mut order = Vec::new();
    order
        .try_reserve_exact(cases.len())
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for (case, _) in cases {
        order.push(case);
    }
    Ok(order)
}

fn local_cases<E: ProgramConstraintEvaluatorSetV1>(
    component: &ComputedComponentV1,
    scenarios: NonEmptyScenarioSetV1<'_>,
) -> Result<LocalCasesV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut representatives = buffer(scenarios.len().get(), 0)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let order = local_case_order::<E>(component, scenarios, |case, representative| {
        representatives[case] = representative;
    })?;
    for &case in &order {
        representatives[case] = case;
    }
    Ok(LocalCasesV1 {
        order,
        representatives,
    })
}

pub(super) use reference_search::refresh_reports;

impl<'a> ContextV1<'a> {
    fn new<E: ProgramConstraintEvaluatorSetV1>(
        selection: &'a ComputedSelectionV1,
        component: Option<&'a ComputedComponentV1>,
        scenarios: NonEmptyScenarioSetV1<'_>,
    ) -> Result<Self, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let targets = ComputedTargetsV1::new(selection, component);
        let count = targets.len();
        let cells = count
            .checked_mul(scenarios.len().get())
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        let local_cases = if let Some(component) = component {
            local_cases::<E>(component, scenarios)?
        } else {
            LocalCasesV1::default()
        };
        let joint_point = component
            .is_some_and(|component| component.mode == ProgramSelectionModeV1::JointPointV1);
        let mut preferred = buffer(cells, [0.0; 3]).map_err(allocation)?;
        let mut preferred_rgb = buffer(cells, Srgb8::new([0; 3])).map_err(allocation)?;
        let mut opaque = buffer(count, true).map_err(allocation)?;
        for (index, target) in targets.iter().enumerate() {
            opaque[index] = target.opaque;
        }
        for case in (0..scenarios.len().get())
            .filter(|&case| !joint_point || local_cases.representatives[case] == case)
        {
            let inputs = scenarios
                .physical_values(case)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            for (index, target) in targets.iter().enumerate() {
                #[cfg(test)]
                if joint_point {
                    point_search::tests::refreshed_point_seed(target.id);
                }
                let mut rgb = target.effective_anchor();
                let mut preferred_lab = target.anchor_lab;
                if let Some(intent) = target.effective_intent() {
                    if let (Some(curve), Some(input)) = (&intent.tone, target.tone_input) {
                        let source = if !target.declaration.reference_appearance
                            && intent.identity.neutral()
                        {
                            Srgb8::new([128; 3])
                        } else {
                            rgb
                        };
                        rgb = curve.preferred_color(source, inputs[input].srgb8());
                        // Gamut compression and RGB8 rounding produce only a
                        // search seed. Tone replaces L, retaining the authored
                        // chromatic preference even outside the source gamut.
                        preferred_lab[0] = curve.preferred_lightness(inputs[input].srgb8());
                    }
                }
                let cell = case * count + index;
                preferred_rgb[cell] = rgb;
                preferred[cell] = preferred_lab;
            }
        }
        if joint_point {
            // The legacy hard observer and public reports address physical
            // cases directly. Reuse each local preference for its duplicate
            // cases without repeating tone, gamut, or Oklab calculations.
            for (case, &representative) in local_cases.representatives.iter().enumerate() {
                if representative == case {
                    continue;
                }
                let start = representative * count;
                preferred.copy_within(start..start + count, case * count);
                preferred_rgb.copy_within(start..start + count, case * count);
            }
        }
        let mut domains = Vec::new();
        domains
            .try_reserve_exact(count)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        for target in targets.iter() {
            domains.push(target.search_domain());
        }
        let case_refinement = RefCell::new(
            CaseRefinementV1::new(&local_cases, scenarios.len().get()).map_err(allocation)?,
        );
        Ok(Self {
            targets,
            constraint_indices: component.map(|component| component.constraints.as_ref()),
            physical: component.map(|component| &component.physical),
            domains,
            preferred,
            preferred_rgb,
            local_cases,
            case_refinement,
            score_extrema: RefCell::new(buffer(count, usize::MAX).map_err(allocation)?),
            identity_memberships: RefCell::new(buffer(count, None).map_err(allocation)?),
            visible_labs: RefCell::new(buffer(count, None).map_err(allocation)?),
            opaque,
            point_equivalence: None,
        })
    }

    fn point<E: ProgramConstraintEvaluatorSetV1>(
        selection: &'a ComputedSelectionV1,
        component: &'a ComputedComponentV1,
        scenarios: NonEmptyScenarioSetV1<'_>,
    ) -> Result<Self, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let mut context = Self::new::<E>(selection, Some(component), scenarios)?;
        context.point_equivalence = component.point_equivalence.as_ref();
        context.constraint_indices = Some(component.point_constraints());
        Ok(context)
    }

    fn represents_target(&self, local: usize) -> bool {
        self.point_equivalence
            .is_none_or(|equivalence| equivalence.representatives[local] == local)
    }

    fn represents_case(&self, case: usize) -> bool {
        self.local_cases.representatives.is_empty()
            || self.local_cases.representatives[case] == case
    }

    fn exact_optimum(&self, values: &[Srgb8], scores: &[f64]) -> bool {
        scores.iter().all(|score| *score == 0.0)
            && (self.opaque.iter().all(|value| *value)
                || self
                    .targets
                    .iter()
                    .zip(values)
                    .all(|(target, &value)| value == target.declaration.anchor))
    }
}

// An unchanged authored source wins only when every visible objective ties.
// The flags follow authored target priority; they add no perceptual metric.
fn compare_tuples(
    context: &ContextV1,
    scores: &[f64],
    values: &[Srgb8],
    other_scores: &[f64],
    other_values: &[Srgb8],
) -> Ordering {
    compare_scores(scores, other_scores)
        .then_with(|| {
            context
                .targets
                .iter()
                .zip(values)
                .map(|(target, &value)| value != target.declaration.anchor)
                .cmp(
                    context
                        .targets
                        .iter()
                        .zip(other_values)
                        .map(|(target, &value)| value != target.declaration.anchor),
                )
        })
        .then_with(|| values.cmp(other_values))
}

struct IntentScoreCollectorV1<'a, 'context> {
    context: &'a ContextV1<'context>,
    values: &'a [Srgb8],
    scores: &'a mut [f64],
    violations: usize,
    has_violation: bool,
    appearance: Option<&'a mut [AppearancePointV1]>,
    violation_case: Option<usize>,
    extrema: RefMut<'a, Vec<usize>>,
    identity_memberships: RefMut<'a, Vec<Option<(Srgb8, bool)>>>,
    visible_labs: RefMut<'a, Vec<Option<VisibleLabV1>>>,
}

#[derive(Clone, Copy)]
struct AppearancePointV1 {
    visible: Srgb8,
    backdrop: Srgb8,
    lab: [f64; 3],
    alpha: f64,
}

impl Default for AppearancePointV1 {
    fn default() -> Self {
        Self {
            visible: Srgb8::new([0; 3]),
            backdrop: Srgb8::new([0; 3]),
            lab: [0.0; 3],
            alpha: 1.0,
        }
    }
}

impl<'a, 'context> IntentScoreCollectorV1<'a, 'context> {
    fn new(context: &'a ContextV1<'context>, values: &'a [Srgb8], scores: &'a mut [f64]) -> Self {
        scores.fill(0.0);
        let mut extrema = context.score_extrema.borrow_mut();
        extrema.fill(usize::MAX);
        Self {
            context,
            values,
            scores,
            violations: 0,
            has_violation: false,
            appearance: None,
            violation_case: None,
            extrema,
            identity_memberships: context.identity_memberships.borrow_mut(),
            visible_labs: context.visible_labs.borrow_mut(),
        }
    }

    fn violations(&self) -> usize {
        // Representatives normalize the heuristic count. Any observed failure
        // remains nonzero; only a complete pass can establish a valid tuple.
        self.violations.max(usize::from(self.has_violation))
    }

    fn refine_cases(&self) {
        let mut refinement = self.context.case_refinement.borrow_mut();
        if let Some(case) = self.violation_case {
            refinement.add(case);
        } else {
            // Each target is a separate lexicographic dimension. A smaller
            // distance on a later target can still supply its essential case.
            for &case in self.extrema.iter().filter(|&&case| case != usize::MAX) {
                refinement.add(case);
            }
        }
    }
}

impl ProgramPhysicalCaseObserverV1 for IntentScoreCollectorV1<'_, '_> {
    fn observe(
        &mut self,
        case: usize,
        evaluation: &AppearanceEvaluationView<'_, '_>,
    ) -> Result<(), ProgramPhysicalCaseObserverErrorV1> {
        #[cfg(test)]
        case_refinement_tests::point_case();
        let context = self.context;
        let count = context.targets.len();
        for (index, target) in context.targets.iter().enumerate() {
            let rgb = if let Some(slot) = target.appearance {
                Srgb8::new(
                    evaluation
                        .occurrence_at(slot)
                        .ok_or(ProgramPhysicalCaseObserverErrorV1::InternalInvariant)?
                        .visible(),
                )
            } else {
                self.values[index]
            };
            // Only the actual visible RGB8 determines this exact conversion.
            // Other case data and all case accounting still follow below.
            let entry = &mut self.visible_labs[index];
            let point_lab = match *entry {
                Some(previous) if previous.rgb == rgb => previous.lab,
                _ => {
                    #[cfg(test)]
                    case_refinement_tests::visible_lab_conversion();
                    let point_lab = lab(rgb);
                    *entry = Some(VisibleLabV1 {
                        rgb,
                        lab: point_lab,
                    });
                    point_lab
                }
            };
            if let Some(points) = &mut self.appearance {
                let backdrop = if let Some(slot) = target.appearance {
                    Srgb8::new(
                        evaluation
                            .occurrence_at(slot)
                            .ok_or(ProgramPhysicalCaseObserverErrorV1::InternalInvariant)?
                            .certificate()
                            .backdrop_rgb(),
                    )
                } else {
                    rgb
                };
                points[case * count + index] = AppearancePointV1 {
                    visible: rgb,
                    backdrop,
                    lab: point_lab,
                    alpha: if let Some(slot) = target.appearance {
                        evaluation
                            .occurrence_at(slot)
                            .ok_or(ProgramPhysicalCaseObserverErrorV1::InternalInvariant)?
                            .certificate()
                            .subject_opacity()
                            .value()
                    } else {
                        1.0
                    },
                };
            }
            if !context.represents_target(index) {
                continue;
            }
            let delta = f64::from_bits(target.declaration.max_delta_bits);
            let identity_failed = target.effective_intent().is_some_and(|intent| {
                let point = target.identity_point(self.values[index], rgb);
                // This Context fixes the target's identity. Only its actual
                // RGB8 argument can change between cases or complete tuples;
                // source and explicit visible identities share this same key.
                let entry = &mut self.identity_memberships[index];
                let contains = match *entry {
                    Some((previous, contains)) if previous == point => contains,
                    _ => {
                        #[cfg(test)]
                        case_refinement_tests::identity_check();
                        let contains = intent.identity.contains(point);
                        *entry = Some((point, contains));
                        contains
                    }
                };
                !contains
            });
            if identity_failed || squared_distance(point_lab, target.anchor_lab) > delta * delta {
                self.has_violation = true;
                self.violations += usize::from(context.represents_case(case));
                self.violation_case = context
                    .case_refinement
                    .borrow()
                    .first_case(self.violation_case.into_iter().chain(std::iter::once(case)));
            }
            let score = squared_distance(point_lab, context.preferred[case * count + index]);
            let previous = self.extrema[index];
            if context.represents_case(case)
                && (previous == usize::MAX
                    || score > self.scores[index]
                    || score == self.scores[index]
                        && context.case_refinement.borrow().ranks[case]
                            < context.case_refinement.borrow().ranks[previous])
            {
                self.extrema[index] = case;
            }
            self.scores[index] = self.scores[index].max(score);
        }
        Ok(())
    }
}

fn intent_scores<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut collector = IntentScoreCollectorV1::new(context, values, scores);
    observe_intents(context, runtime, epoch, scenarios, &mut collector)?;
    Ok(collector.violations())
}

fn observe_intents<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    collector: &mut impl ProgramPhysicalCaseObserverV1,
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let schema = epoch.observation_group.schema.as_slice();
    let mut paints = prepare_candidate_paints(
        &epoch.graph,
        context.physical,
        runtime.bindings,
        schema,
        runtime.workspace,
    )
    .map_err(map_program_execution_binding_error)?;
    for case in 0..scenarios.len().get() {
        let inputs = scenarios
            .physical_values(case)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
        collector
            .observe(case, &evaluation)
            .map_err(map_physical_case_error)?;
    }
    Ok(())
}

fn assess<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut collector = IntentScoreCollectorV1::new(context, values, scores);
    let failures = assess_observed(
        context,
        runtime,
        epoch,
        scenarios,
        values,
        &mut collector,
        arena,
    )?;
    collector.refine_cases();
    Ok(failures + collector.violations())
}

fn assess_refined<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    if let Some(failures) = probe_cases(context, runtime, epoch, scenarios, values, scores, arena)?
    {
        return Ok(failures);
    }
    let failures = assess(context, runtime, epoch, scenarios, values, scores, arena)?;
    // A counterexample may have enlarged the probe. Repair comparisons must
    // use its current cells and scores, not the failed complete-pass count.
    if failures != 0 {
        if let Some(failures) =
            probe_cases(context, runtime, epoch, scenarios, values, scores, arena)?
        {
            return Ok(failures);
        }
    }
    Ok(failures)
}

// A successful probe says only that a complete assessment is still needed.
// No partially filled appearance buffer can escape this helper.
fn probe_cases<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<Option<usize>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let refinement = context.case_refinement.borrow();
    if refinement.complete() {
        return Ok(None);
    }
    let mut collector = IntentScoreCollectorV1::new(context, values, scores);
    let failures = assess_observed_cases(
        context,
        runtime,
        epoch,
        scenarios,
        values,
        &mut collector,
        arena,
        Some(&refinement.cases),
    )? + collector.violations();
    Ok((failures != 0).then_some(failures))
}

fn assess_observed<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    collector: &mut impl ProgramPhysicalCaseObserverV1,
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    assess_observed_cases(
        context, runtime, epoch, scenarios, values, collector, arena, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn assess_observed_cases<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    collector: &mut impl ProgramPhysicalCaseObserverV1,
    arena: &mut ProgramEvaluationArenaV1<E>,
    cases: Option<&[usize]>,
) -> Result<usize, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    apply_targets::<E>(context.targets, runtime, values)?;
    arena.clear();
    let hard_failed = scan_program_candidate_scoped(
        runtime,
        epoch,
        scenarios,
        0,
        ProgramEvaluationPhaseV1::Hard,
        ProgramCandidateCollectionV1 {
            evidence: ProgramConstraintEvidenceCaptureV1::Report {
                cells: &mut arena.cells,
                relation_members: &mut arena.relation_members,
            },
            outputs: None,
            point_causal: None,
            physical_case: Some(collector),
        },
        cases.map_or_else(
            || {
                context.constraint_indices.map_or(
                    ProgramCandidateScanV1::Full { previous: None },
                    |indices| ProgramCandidateScanV1::Constraints {
                        indices,
                        physical: context.physical,
                    },
                )
            },
            |cases| ProgramCandidateScanV1::Cases {
                indices: context.constraint_indices,
                physical: context.physical,
                cases,
            },
        ),
    )?;
    if cases.is_none() {
        let witness = context.case_refinement.borrow().first_case(
            arena
                .cells
                .iter()
                .filter(|cell| cell.result().is_violation())
                .map(|cell| cell.case_index()),
        );
        if let Some(case) = witness {
            context.case_refinement.borrow_mut().add(case);
        }
    }
    let hard_violations = arena
        .cells
        .iter()
        .filter(|cell| context.represents_case(cell.case_index()) && cell.result().is_violation())
        .count();
    Ok(context
        .domains
        .iter()
        .zip(values)
        .enumerate()
        .filter(|(index, (domain, value))| {
            context.represents_target(*index) && !domain.contains(**value)
        })
        .count()
        + hard_violations.max(usize::from(hard_failed)))
}

/// Ближайший source по каждому encoded каналу — только начальное приближение.
/// Проверка всей композиции и всех требований всегда следует отдельно.
fn inverse_hint(desired: Srgb8, backdrop: [u8; 3], alpha: f64) -> Srgb8 {
    if alpha == 1.0 {
        return desired;
    }
    if alpha == 0.0 {
        return Srgb8::new([0; 3]);
    }
    Srgb8::new(std::array::from_fn(|index| {
        let goal = desired.bytes()[index];
        let evaluate =
            |source| crate::composition::source_over_channel_srgb8(source, alpha, backdrop[index]);
        let (mut lo, mut hi) = (0_u8, 255_u8);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if evaluate(mid) < goal {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo > 0 && evaluate(lo - 1).abs_diff(goal) <= evaluate(lo).abs_diff(goal) {
            lo - 1
        } else {
            lo
        }
    }))
}

// Legacy visible-appearance refinement. Reference targets use the independent
// source-identity walks in reference_search before this path is entered.
fn tonal_hint(
    target: &CompiledComputedTargetV1,
    intent: &EffectiveColorIntentV1<'_>,
    lightness: f64,
    backdrop: [u8; 3],
    alpha: f64,
) -> Srgb8 {
    debug_assert!(!target.declaration.reference_appearance);
    let source = if intent.identity.neutral() {
        Srgb8::new([128; 3])
    } else {
        target.effective_anchor()
    };
    inverse_hint(color_at_lightness(source, lightness), backdrop, alpha)
}

fn physical_context<E: ProgramConstraintEvaluatorSetV1>(
    target: &CompiledComputedTargetV1,
    physical: Option<&CompiledAppearanceConeV1>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    case: usize,
) -> Result<([u8; 3], f64), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let Some(slot) = target.appearance else {
        return Ok(([0; 3], 1.0));
    };
    let inputs = scenarios
        .physical_values(case)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let mut paints = prepare_candidate_paints(
        &epoch.graph,
        physical,
        runtime.bindings,
        epoch.observation_group.schema.as_slice(),
        runtime.workspace,
    )
    .map_err(map_program_execution_binding_error)?;
    let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
    let point = evaluation
        .occurrence_at(slot)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    Ok((
        point.certificate().backdrop_rgb(),
        point.certificate().subject_opacity().value(),
    ))
}

pub(super) fn recheck_intents<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let context = ContextV1::new::<E>(selection, None, scenarios)?;
    let mut scores =
        buffer(values.len(), 0.0).map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    if context
        .domains
        .iter()
        .zip(values)
        .any(|(domain, value)| !domain.contains(*value))
        || intent_scores(&context, runtime, epoch, scenarios, values, &mut scores)? != 0
    {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    Ok(())
}

struct BoundsV1 {
    lower: Vec<[u8; 3]>,
    constraints: Vec<RegionConstraintV1>,
    constraint_lower: Vec<RegionPointV1>,
    scores: Vec<f64>,
    changes_min: Vec<bool>,
    tuple_min: Vec<Srgb8>,
    evaluations: u64,
    appearance: Option<Vec<[RegionPointV1; 2]>>,
}

impl BoundsV1 {
    fn may_pass<E: ProgramConstraintEvaluatorSetV1>(
        &mut self,
        context: &ContextV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        node: RegionNodeV1,
        values: &[Srgb8],
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let count = context.targets.len();
        self.scores.fill(0.0);
        for (index, domain) in context.domains.iter().enumerate() {
            let lo = if index < node.depth {
                values[index].bytes()
            } else if index == node.depth {
                node.domain.bounds().0
            } else {
                domain.bounds().0
            };
            self.tuple_min[index] = Srgb8::new(lo);
            let anchor = context.targets[index].declaration.anchor;
            self.changes_min[index] = if index < node.depth {
                values[index] != anchor
            } else if index == node.depth {
                !node.domain.contains(anchor)
            } else {
                !domain.contains(anchor)
            };
        }
        let refinement = context.case_refinement.borrow();
        for &case in &refinement.cases {
            let inputs = scenarios
                .physical_values(case)
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            for high in [false, true] {
                for (index, target) in context.targets.iter().enumerate() {
                    let rgb = if index < node.depth {
                        values[index]
                    } else {
                        let (lo, hi) = if index == node.depth {
                            node.domain
                        } else {
                            context.domains[index]
                        }
                        .bounds();
                        Srgb8::new(if high { hi } else { lo })
                    };
                    runtime
                        .bindings
                        .overwrite_paint_input_at(
                            target.binding,
                            EncodedPointPaintValueV1::opaque(rgb),
                        )
                        .map_err(map_program_execution_binding_error)?;
                }
                let mut paints = prepare_candidate_paints(
                    &epoch.graph,
                    context.physical,
                    runtime.bindings,
                    epoch.observation_group.schema.as_slice(),
                    runtime.workspace,
                )
                .map_err(map_program_execution_binding_error)?;
                let evaluation = paints.evaluate_surface_inputs_canonical(|i| inputs[i].srgb8());
                self.evaluations += 1;
                #[cfg(test)]
                case_refinement_tests::bound_case();
                for (index, constraint) in self.constraints.iter().enumerate() {
                    let point = constraint
                        .point(&evaluation)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    if !high {
                        self.constraint_lower[index] = point;
                    } else if !constraint
                        .may_pass(self.constraint_lower[index], point)
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?
                    {
                        return Ok(false);
                    }
                }
                for (index, target) in context.targets.iter().enumerate() {
                    let rgb = if let Some(slot) = target.appearance {
                        evaluation
                            .occurrence_at(slot)
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                            .visible()
                    } else if index < node.depth {
                        values[index].bytes()
                    } else {
                        let (lo, hi) = if index == node.depth {
                            node.domain
                        } else {
                            context.domains[index]
                        }
                        .bounds();
                        if high { hi } else { lo }
                    };
                    if let Some(points) = &mut self.appearance {
                        let backdrop = if let Some(slot) = target.appearance {
                            evaluation
                                .occurrence_at(slot)
                                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                                .certificate()
                                .backdrop_rgb()
                        } else {
                            rgb
                        };
                        points[case * count + index][usize::from(high)] = RegionPointV1 {
                            visible: rgb,
                            backdrop,
                            weaker: [0; 3],
                        };
                    }
                    if !high {
                        self.lower[index] = rgb;
                        continue;
                    }
                    let lower = self.lower[index];
                    let identity_box = if target.declaration.reference_appearance {
                        if index < node.depth {
                            let point = values[index].bytes();
                            (point, point)
                        } else if index == node.depth {
                            node.domain.bounds()
                        } else {
                            context.domains[index].bounds()
                        }
                    } else {
                        (lower, rgb)
                    };
                    if target.effective_intent().is_some_and(|intent| {
                        !intent
                            .identity
                            .may_intersect_rgb_box(identity_box.0, identity_box.1)
                    }) {
                        return Ok(false);
                    }
                    let domain = AssignmentDomainV1::Cube { lower, upper: rgb };
                    let delta = f64::from_bits(target.declaration.max_delta_bits);
                    if domain.lower_score(target.anchor_lab) > delta * delta {
                        return Ok(false);
                    }
                    self.scores[index] = self.scores[index]
                        .max(domain.lower_score(context.preferred[case * count + index]));
                }
            }
        }
        Ok(true)
    }
}

pub(super) fn select_component<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    if component
        .targets
        .iter()
        .any(|&index| selection.targets[index].declaration.reference_appearance)
    {
        return reference_search::select_component(
            selection, component, runtime, epoch, scenarios, arena,
        );
    }
    select_component_inner(selection, component, runtime, epoch, scenarios, arena, true)
}

fn select_component_inner<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    allow_conditional: bool,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let targets = ComputedTargetsV1::new(selection, Some(component));
    let count = targets.len();
    let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    for (value, target) in values.iter_mut().zip(targets.iter()) {
        *value = target.declaration.anchor;
    }
    let mut evidence = SearchEvidenceV1 {
        completion: CompletionV1::BudgetExhausted,
        visited: 0,
        regions: 0,
        points: 0,
        endpoint_evaluations: 0,
        examined: 0,
        open_regions: 0,
    };
    if selection.budget == 0 {
        return Ok(SearchResultV1 {
            reference_selections: None,
            evidence,
            values: None,
        });
    }
    apply_targets::<E>(targets, runtime, &values)?;
    let context = ContextV1::new::<E>(selection, Some(component), scenarios)?;
    let seed_case = *context
        .local_cases
        .order
        .first()
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let mut scores = buffer(count, 0.0).map_err(allocation)?;
    let mut best_scores = buffer(count, 0.0).map_err(allocation)?;
    let mut best_values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    let mut current_scores = buffer(count, 0.0).map_err(allocation)?;
    let mut current_values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    let mut preferred_seed = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    let mut has_best = false;

    // Нулевая visible distance и сохранённые authored sources достигают
    // наименьшего возможного objective при любой прозрачности.
    let authored_failures = assess(
        &context,
        runtime,
        epoch,
        scenarios,
        &values,
        &mut scores,
        arena,
    )?;
    evidence.visited += 1;
    evidence.points += 1;
    evidence.examined += 1;
    current_scores.copy_from_slice(&scores);
    current_values.copy_from_slice(&values);
    if authored_failures == 0 {
        best_scores.copy_from_slice(&scores);
        best_values.copy_from_slice(&values);
        has_best = true;
        if context.exact_optimum(&values, &scores) {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                reference_selections: None,
                evidence,
                values: Some(values),
            });
        }
    }

    preferred_seed.copy_from_slice(&values);

    // Контекстные опоры строятся внутри Core. Изменение parent учитывается
    // физическим графом перед приготовлением source следующего применения.
    if evidence.visited < selection.budget {
        for (index, target) in targets.iter().enumerate() {
            if target.declaration.intent.is_none() {
                continue;
            }
            apply_targets::<E>(targets, runtime, &values)?;
            let (backdrop, alpha) = physical_context(
                target,
                context.physical,
                runtime,
                epoch,
                scenarios,
                seed_case,
            )?;
            values[index] = inverse_hint(
                context.preferred_rgb[seed_case * count + index],
                backdrop,
                alpha,
            );
        }
        preferred_seed.copy_from_slice(&values);
        let failures = assess_refined(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )?;
        evidence.visited += 1;
        evidence.points += 1;
        evidence.examined += 1;
        if failures == 0
            && (!has_best
                || compare_tuples(&context, &scores, &values, &best_scores, &best_values)
                    == Ordering::Less)
        {
            has_best = true;
            best_scores.copy_from_slice(&scores);
            best_values.copy_from_slice(&values);
        }
        if failures == 0 && context.exact_optimum(&values, &scores) {
            evidence.completion = CompletionV1::AuthoredExact;
            return Ok(SearchResultV1 {
                reference_selections: None,
                evidence,
                values: Some(values),
            });
        }
    }

    // Независимые исходные точки не отбрасывают контекстный замысел из-за
    // большего числа временных нарушений. Сначала меняются менее приоритетные
    // применения: надпись может приспособиться к панели, прежде чем панель
    // потеряет желаемый тон ради прежней надписи. Это направление одинаково
    // для каждой исходной точки: её порядковый номер не должен заставлять
    // parent обменивать одно нарушение на другое до приспособления child.
    // Обратный проход остаётся эвристикой поиска, а не семантикой порядка
    // графа или условием приёмки.
    let seed_limit = (selection.budget / COORDINATE_BUDGET_DIVISOR).max(evidence.visited);
    let distinct_origins = preferred_seed
        .iter()
        .zip(targets.iter())
        .any(|(value, target)| *value != target.declaration.anchor);
    let origin_count = if distinct_origins { 2 } else { 1 };
    let origin_budget = (seed_limit - evidence.visited) / origin_count;
    for origin in 0..origin_count {
        let origin_limit = if origin + 1 == origin_count {
            seed_limit
        } else {
            evidence.visited + origin_budget
        };
        if evidence.visited >= origin_limit {
            continue;
        }
        if origin == 0 {
            current_values.copy_from_slice(&preferred_seed);
        } else {
            for (value, target) in current_values.iter_mut().zip(targets.iter()) {
                *value = target.declaration.anchor;
            }
        }
        let mut current_failures = assess_refined(
            &context,
            runtime,
            epoch,
            scenarios,
            &current_values,
            &mut current_scores,
            arena,
        )?;
        evidence.visited += 1;
        evidence.points += 1;
        evidence.examined += 1;
        for pass in 0..2 {
            let mut changed = false;
            for position in 0..count {
                let index = if pass % 2 == 0 {
                    count - 1 - position
                } else {
                    position
                };
                let target = &targets[index];
                let Some(intent) = target.effective_intent() else {
                    continue;
                };
                for &case in &context.local_cases.order {
                    if evidence.visited >= origin_limit {
                        break;
                    }
                    values.copy_from_slice(&current_values);
                    apply_targets::<E>(targets, runtime, &values)?;
                    let (backdrop, alpha) = physical_context(
                        target,
                        context.physical,
                        runtime,
                        epoch,
                        scenarios,
                        case,
                    )?;
                    let mut previous = None;
                    for step in 0..=255_u32 {
                        if evidence.visited >= origin_limit {
                            break;
                        }
                        let candidate =
                            tonal_hint(target, &intent, f64::from(step) / 255.0, backdrop, alpha);
                        if previous == Some(candidate) {
                            continue;
                        }
                        previous = Some(candidate);
                        values.copy_from_slice(&current_values);
                        values[index] = candidate;
                        let case_count = context.case_refinement.borrow().cases.len();
                        let failures = assess_refined(
                            &context,
                            runtime,
                            epoch,
                            scenarios,
                            &values,
                            &mut scores,
                            arena,
                        )?;
                        evidence.visited += 1;
                        evidence.points += 1;
                        evidence.examined += 1;
                        if failures != 0
                            && current_failures != 0
                            && case_count != context.case_refinement.borrow().cases.len()
                        {
                            if evidence.visited >= origin_limit {
                                break;
                            }
                            current_failures = assess_refined(
                                &context,
                                runtime,
                                epoch,
                                scenarios,
                                &current_values,
                                &mut current_scores,
                                arena,
                            )?;
                            evidence.visited += 1;
                            evidence.points += 1;
                            evidence.examined += 1;
                        }
                        if failures < current_failures
                            || failures == current_failures
                                && compare_tuples(
                                    &context,
                                    &scores,
                                    &values,
                                    &current_scores,
                                    &current_values,
                                ) == Ordering::Less
                        {
                            current_failures = failures;
                            current_scores.copy_from_slice(&scores);
                            current_values.copy_from_slice(&values);
                            changed = true;
                        }
                        if failures == 0
                            && (!has_best
                                || compare_tuples(
                                    &context,
                                    &scores,
                                    &values,
                                    &best_scores,
                                    &best_values,
                                ) == Ordering::Less)
                        {
                            has_best = true;
                            best_scores.copy_from_slice(&scores);
                            best_values.copy_from_slice(&values);
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    let mut conditional_evidence = if allow_conditional && has_best {
        conditional::attempt(
            selection,
            component,
            runtime,
            epoch,
            scenarios,
            arena,
            conditional::CandidateV1 {
                context: &context,
                values: &mut best_values,
                scores: &mut best_scores,
                evidence: &mut evidence,
                mode: conditional::ModeV1::PreferredMinimum,
            },
        )?
    } else {
        None
    };
    if let Some(extra) = conditional_evidence {
        if extra.completion != CompletionV1::BudgetExhausted {
            components::merge_evidence(&mut evidence, extra).map_err(allocation)?;
            evidence.completion = CompletionV1::Complete;
            evidence.open_regions = 0;
            return Ok(SearchResultV1 {
                reference_selections: None,
                evidence,
                values: Some(best_values),
            });
        }
    }

    // Полный RGB8/gray domain остаётся доступен после приближений.
    let constraints = region_constraints(epoch, context.constraint_indices).map_err(allocation)?;
    let mut bounds = BoundsV1 {
        constraint_lower: buffer(constraints.len(), RegionPointV1::default())
            .map_err(allocation)?,
        constraints,
        lower: buffer(count, [0; 3]).map_err(allocation)?,
        scores: buffer(count, 0.0).map_err(allocation)?,
        changes_min: buffer(count, false).map_err(allocation)?,
        tuple_min: buffer(count, Srgb8::new([0; 3])).map_err(allocation)?,
        evaluations: 0,
        appearance: None,
    };
    let stack_limit = context
        .domains
        .iter()
        .try_fold(1_usize, |size, domain| {
            size.checked_add(domain.split_depth() + 1)
        })
        .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(stack_limit)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    let reserve = refinement::reserved_work(&context, selection.budget);
    let may_polish = allow_conditional && count > 1 && conditional_evidence.is_none();
    // Reserve one whole-tuple check only once a feasible result exists. If
    // search never finds one, its entire original budget stays available.
    let mut work_budget =
        selection.budget - u64::from(may_polish && has_best && evidence.visited < selection.budget);
    let mut search_limit = work_budget - if has_best { reserve } else { 0 };
    let mut complete = true;
    if let Some(root) = region_node(
        0,
        context.domains[0],
        search_limit,
        context.preferred[seed_case * count],
        &mut evidence,
    ) {
        stack.push(root);
    } else {
        complete = false;
    }
    values.copy_from_slice(&current_values);
    while let Some(mut node) = stack.pop() {
        let depth = node.depth;
        let volume = node
            .domain
            .cardinality()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if node.ordinal.is_none() {
            let possible = bounds.may_pass(&context, runtime, epoch, scenarios, node, &values)?;
            evidence.endpoint_evaluations = bounds.evaluations;
            let worse = has_best
                && match compare_scores(&bounds.scores, &best_scores) {
                    Ordering::Greater => true,
                    Ordering::Equal => {
                        bounds
                            .changes_min
                            .iter()
                            .copied()
                            .cmp(
                                context
                                    .targets
                                    .iter()
                                    .zip(&best_values)
                                    .map(|(target, &value)| value != target.declaration.anchor),
                            )
                            .then_with(|| bounds.tuple_min.cmp(&best_values))
                            != Ordering::Less
                    }
                    Ordering::Less => false,
                };
            if !possible || worse {
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                continue;
            }
            if volume > 64 {
                let (left, right) = node.domain.split();
                let Some(left) = region_node(
                    depth,
                    left,
                    search_limit,
                    context.preferred[seed_case * count + depth],
                    &mut evidence,
                ) else {
                    complete = false;
                    break;
                };
                let Some(right) = region_node(
                    depth,
                    right,
                    search_limit,
                    context.preferred[seed_case * count + depth],
                    &mut evidence,
                ) else {
                    complete = false;
                    break;
                };
                let (first, second) = if left.lower_score <= right.lower_score {
                    (left, right)
                } else {
                    (right, left)
                };
                evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                stack.push(second);
                stack.push(first);
            } else {
                node.ordinal = Some(0);
                stack.push(node);
            }
            continue;
        }
        if evidence.visited >= search_limit {
            complete = false;
            break;
        }
        evidence.visited += 1;
        evidence.points += 1;
        let ordinal = node
            .ordinal
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        if ordinal + 1 < volume {
            stack.push(RegionNodeV1 {
                ordinal: Some(ordinal + 1),
                ..node
            });
        } else {
            evidence
                .close_region()
                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
        }
        values[depth] = node.domain.value(ordinal);
        if depth + 1 < count {
            let Some(next) = region_node(
                depth + 1,
                context.domains[depth + 1],
                search_limit,
                context.preferred[seed_case * count + depth + 1],
                &mut evidence,
            ) else {
                complete = false;
                break;
            };
            stack.push(next);
            continue;
        }
        evidence.examined += 1;
        if assess_refined(
            &context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut scores,
            arena,
        )? == 0
            && (!has_best
                || compare_tuples(&context, &scores, &values, &best_scores, &best_values)
                    == Ordering::Less)
        {
            has_best = true;
            work_budget =
                selection.budget - u64::from(may_polish && evidence.visited < selection.budget);
            search_limit = work_budget - reserve;
            best_scores.copy_from_slice(&scores);
            best_values.copy_from_slice(&values);
        }
    }
    if complete {
        if evidence.open_regions != 0 {
            return Err(ProgramSessionEvaluationError::InternalInvariant);
        }
        evidence.completion = CompletionV1::Complete;
    } else if has_best && reserve != 0 {
        refinement::improve(
            &context,
            runtime,
            epoch,
            scenarios,
            arena,
            refinement::StateV1 {
                best_values: &mut best_values,
                best_scores: &mut best_scores,
                values: &mut values,
                scores: &mut scores,
                current_values: &mut current_values,
                current_scores: &mut current_scores,
                pool: &mut preferred_seed,
                evidence: &mut evidence,
                budget: work_budget,
            },
        )?;
    }
    // If the first feasible point consumes the very last visit, keep it.
    // No conditional move may evade its accounted whole-tuple recheck.
    if !complete && has_best && may_polish && evidence.visited < selection.budget {
        conditional_evidence = conditional::attempt(
            selection,
            component,
            runtime,
            epoch,
            scenarios,
            arena,
            conditional::CandidateV1 {
                context: &context,
                values: &mut best_values,
                scores: &mut best_scores,
                evidence: &mut evidence,
                mode: conditional::ModeV1::LocalPrefix,
            },
        )?;
    }
    if let Some(extra) = conditional_evidence {
        // The full fallback owns completion. Its proof also closes any open
        // conditional regions, while every attempted visit remains accounted.
        let completion = evidence.completion;
        let open_regions = evidence.open_regions;
        components::merge_evidence(&mut evidence, extra).map_err(allocation)?;
        evidence.completion = completion;
        evidence.open_regions = open_regions;
    }
    Ok(SearchResultV1 {
        reference_selections: None,
        evidence,
        values: has_best.then_some(best_values),
    })
}

mod conditional;
pub(super) mod point_search;
pub(crate) mod reference_completion;
mod reference_search;
mod refinement;

#[cfg(test)]
mod case_refinement_tests;
