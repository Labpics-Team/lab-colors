//! Authored Point composition in the ordinary Program selector. Proposal order
//! is heuristic; certified symmetric profiles alone retain a feasible tuple.
//! This byte-domain discovery is not a proof about the continuous source domain.

use super::reference_completion::{
    PointMeasurementPrecisionV1, PointMeasurementV1, PreparedPointSelectionV1,
};
use super::*;
use crate::certified_numeric::{CertifiedScalarV1, sort_descending_profile};

pub(super) fn measured<E: ProgramConstraintEvaluatorSetV1>(
    value: Result<(), reference_completion::ReferenceCompletionErrorV1<ProgramEvaluatorError<E>>>,
) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    match value {
        Ok(()) => Ok(true),
        Err(reference_completion::ReferenceCompletionErrorV1::Numeric(_)) => Ok(false),
        Err(reference_completion::ReferenceCompletionErrorV1::ResourceExhausted) => {
            Err(ProgramSessionEvaluationError::ResourceExhausted)
        }
        Err(reference_completion::ReferenceCompletionErrorV1::Binding(error)) => {
            Err(map_program_execution_binding_error(error))
        }
        Err(reference_completion::ReferenceCompletionErrorV1::Evaluation(error)) => Err(error),
        Err(_) => Err(ProgramSessionEvaluationError::InternalInvariant),
    }
}

pub(in crate::program_session) fn execution_profile_identity(budget: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"labcolors.composition-point.v13.paired-source-tones.coupled-material-neighborhood.material-source-discovery.authored-segment-polish.monotone-source-over.nominal-visible-boxes.certified-full-point.worst-case-per-use.explicit-tone-first.substitution-equivalent-leaf-profile.lex-minimax.source-frontier.component-budget.hard-region.byte-discovery.authored-continuous-minimum.reference-visible-minimum.measured-incumbent-recovery.rounded-srgb8-lms-roots\0";
    execution_identity_with_budget(DOMAIN, budget)
}

#[derive(Default)]
pub(super) struct BuffersV1 {
    values: Vec<Srgb8>,
    best: Vec<Srgb8>,
    working: Vec<Srgb8>,
    authored: Vec<Srgb8>,
    scores: Vec<f64>,
    primary: Vec<CertifiedScalarV1>,
    source: Vec<CertifiedScalarV1>,
    best_primary: Vec<CertifiedScalarV1>,
    best_source: Vec<CertifiedScalarV1>,
    refined_primary: Vec<CertifiedScalarV1>,
    refined_source: Vec<CertifiedScalarV1>,
    left: Vec<CertifiedScalarV1>,
    right: Vec<CertifiedScalarV1>,
    sort: Vec<CertifiedScalarV1>,
}

fn copy<T: Copy>(destination: &mut Vec<T>, source: &[T]) -> Result<(), ()> {
    destination.clear();
    destination.try_reserve(source.len()).map_err(|_| ())?;
    destination.extend_from_slice(source);
    Ok(())
}

pub(super) fn profile_order(a: &[CertifiedScalarV1], b: &[CertifiedScalarV1]) -> Option<Ordering> {
    for (&a, &b) in a.iter().zip(b) {
        if a.strictly_below(b) {
            return Some(Ordering::Less);
        }
        if b.strictly_below(a) {
            return Some(Ordering::Greater);
        }
        if !a.proves_equal(b) {
            return None;
        }
    }
    Some(Ordering::Equal)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IncumbentProfileV1 {
    Unmeasured,
    Measured(PointMeasurementPrecisionV1),
}

/// A feasible tuple remains available even if its objective cannot be
/// measured. Once a certified profile is available, an unmeasured tuple
/// cannot displace it. Comparability and feasibility are independent.
#[derive(Default)]
pub(super) struct FeasibleIncumbentV1 {
    profile: Option<IncumbentProfileV1>,
    unresolved_comparison: bool,
}

impl FeasibleIncumbentV1 {
    pub(super) fn has_value(&self) -> bool {
        self.profile.is_some()
    }

    pub(super) fn precision(&self) -> Option<PointMeasurementPrecisionV1> {
        match self.profile {
            Some(IncumbentProfileV1::Measured(precision)) => Some(precision),
            _ => None,
        }
    }

    pub(super) fn refined(&mut self) {
        debug_assert!(self.precision().is_some());
        self.profile = Some(IncumbentProfileV1::Measured(
            PointMeasurementPrecisionV1::Refined,
        ));
    }

    pub(super) fn unresolved(&self) -> bool {
        self.unresolved_comparison || self.profile == Some(IncumbentProfileV1::Unmeasured)
    }

    pub(super) fn consider(
        &mut self,
        measured: bool,
        precision: PointMeasurementPrecisionV1,
        order: Option<Ordering>,
        tuple_order: Ordering,
    ) -> bool {
        // A distinct tuple with an unproved order must never count as a
        // proved tie. Keep conservative evidence even if it is later
        // discarded: an improvement of the incumbent alone cannot certify
        // its relationship to every previously incomparable tuple.
        self.unresolved_comparison |= self.has_value()
            && tuple_order != Ordering::Equal
            && (!measured || self.precision().is_none() || order.is_none());
        let next = if measured {
            IncumbentProfileV1::Measured(precision)
        } else {
            IncumbentProfileV1::Unmeasured
        };
        let retain = match (self.profile, next) {
            (None, _) => true,
            (Some(IncumbentProfileV1::Unmeasured), IncumbentProfileV1::Measured(_)) => true,
            (Some(IncumbentProfileV1::Unmeasured), IncumbentProfileV1::Unmeasured) => {
                tuple_order == Ordering::Less
            }
            (Some(IncumbentProfileV1::Measured(_)), IncumbentProfileV1::Unmeasured) => false,
            (Some(IncumbentProfileV1::Measured(_)), IncumbentProfileV1::Measured(_)) => {
                order == Some(Ordering::Less)
                    || order == Some(Ordering::Equal) && tuple_order == Ordering::Less
            }
        };
        if retain {
            self.profile = Some(next);
        }
        retain
    }
}

fn initial_precision() -> PointMeasurementPrecisionV1 {
    #[cfg(test)]
    if tests::force_refined() {
        return PointMeasurementPrecisionV1::Refined;
    }
    PointMeasurementPrecisionV1::Screened
}

fn sorted_order(buffers: &mut BuffersV1) -> Option<Ordering> {
    let zero = CertifiedScalarV1::exact(0.0).ok()?;
    buffers.sort.resize(buffers.left.len(), zero);
    sort_descending_profile(&mut buffers.left, &mut buffers.sort).ok()?;
    sort_descending_profile(&mut buffers.right, &mut buffers.sort).ok()?;
    profile_order(&buffers.left, &buffers.right)
}

fn compare_primary<E: ProgramConstraintEvaluatorSetV1>(
    prepared: &PreparedPointSelectionV1<'_, '_, E>,
    context: &ContextV1<'_>,
    component: &ComputedComponentV1,
    buffers: &mut BuffersV1,
    region: Option<RegionNodeV1>,
) -> Option<Ordering> {
    // The same tiers as reference composition, with no transport losses.
    // Each use contributes its worst loss across all of its observed contexts.
    for explicit_tone in [true, false] {
        buffers.left.clear();
        buffers.right.clear();
        // A shared immutable expression can be cancelled from both multisets.
        // Matching non-singleton bounds never authorize cancellation.
        for variable in 0..component.point_variable_count() {
            let local = component.point_variable(variable);
            if context.targets[local].tone_input.is_some() != explicit_tone
                || prepared.same_point_expression(
                    component,
                    local,
                    &buffers.values,
                    &buffers.best,
                    region,
                )
            {
                continue;
            }
            buffers.left.push(buffers.primary[variable]);
            buffers.right.push(buffers.best_primary[variable]);
        }
        match sorted_order(buffers)? {
            Ordering::Equal => {}
            order => return Some(order),
        }
    }
    Some(Ordering::Equal)
}

fn compare<E: ProgramConstraintEvaluatorSetV1>(
    prepared: &PreparedPointSelectionV1<'_, '_, E>,
    context: &ContextV1<'_>,
    component: &ComputedComponentV1,
    buffers: &mut BuffersV1,
    region: Option<RegionNodeV1>,
) -> Option<Ordering> {
    match compare_primary(prepared, context, component, buffers, region)? {
        Ordering::Equal => {}
        order => return Some(order),
    }
    buffers.left.clear();
    buffers.right.clear();
    for variable in 0..component.point_variable_count() {
        let local = component.point_variable(variable);
        if region.is_none_or(|node| local < node.depth)
            && buffers.values[local] == buffers.best[local]
        {
            continue;
        }
        buffers.left.push(buffers.source[variable]);
        buffers.right.push(buffers.best_source[variable]);
    }
    sorted_order(buffers)
}

struct SearchV1<'a, 'context, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1> {
    context: &'a ContextV1<'context>,
    component: &'a ComputedComponentV1,
    prepared: &'a mut PreparedPointSelectionV1<'program, 'scratch, E>,
    buffers: &'a mut BuffersV1,
    evidence: SearchEvidenceV1,
    budget: u64,
    incumbent: FeasibleIncumbentV1,
    initial_precision: PointMeasurementPrecisionV1,
}

impl<E: ProgramConstraintEvaluatorSetV1> SearchV1<'_, '_, '_, '_, E> {
    fn measure_current(
        &mut self,
        region: Option<RegionNodeV1>,
        precision: PointMeasurementPrecisionV1,
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        #[cfg(test)]
        tests::measured_profile(region.is_some(), precision);
        measured::<E>(self.prepared.measure_point_component(
            self.component,
            &self.buffers.values,
            region,
            &self.context.local_cases.order,
            &mut self.buffers.primary,
            &mut self.buffers.source,
            PointMeasurementV1::Profile(precision),
        ))
    }

    fn refine_incumbent(
        &mut self,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        if self.incumbent.precision() != Some(PointMeasurementPrecisionV1::Screened) {
            return Ok(());
        }
        #[cfg(test)]
        tests::refined_incumbent();
        let refined = measured::<E>(self.prepared.measure_point_component(
            self.component,
            &self.buffers.best,
            None,
            &self.context.local_cases.order,
            &mut self.buffers.refined_primary,
            &mut self.buffers.refined_source,
            PointMeasurementV1::Profile(PointMeasurementPrecisionV1::Refined),
        ))?;
        if refined {
            std::mem::swap(
                &mut self.buffers.best_primary,
                &mut self.buffers.refined_primary,
            );
            std::mem::swap(
                &mut self.buffers.best_source,
                &mut self.buffers.refined_source,
            );
            self.incumbent.refined();
        }
        Ok(())
    }

    fn refine_current(
        &mut self,
        region: Option<RegionNodeV1>,
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        // The screened enclosure is still valid when refinement fails.
        std::mem::swap(&mut self.buffers.primary, &mut self.buffers.refined_primary);
        std::mem::swap(&mut self.buffers.source, &mut self.buffers.refined_source);
        let refined = self.measure_current(region, PointMeasurementPrecisionV1::Refined)?;
        if !refined {
            std::mem::swap(&mut self.buffers.primary, &mut self.buffers.refined_primary);
            std::mem::swap(&mut self.buffers.source, &mut self.buffers.refined_source);
        }
        Ok(refined)
    }

    fn assess_profile(
        &mut self,
        region: Option<RegionNodeV1>,
    ) -> Result<
        (bool, PointMeasurementPrecisionV1, Option<Ordering>),
        ProgramSessionEvaluationError<ProgramEvaluatorError<E>>,
    > {
        let mut precision = self.initial_precision;
        let mut current_measured = self.measure_current(region, precision)?;
        if !current_measured && precision == PointMeasurementPrecisionV1::Screened {
            precision = PointMeasurementPrecisionV1::Refined;
            current_measured = self.measure_current(region, precision)?;
        }
        if self.incumbent.precision().is_none() || !current_measured {
            return Ok((current_measured, precision, None));
        }
        let mut order = compare(
            self.prepared,
            self.context,
            self.component,
            self.buffers,
            region,
        );
        if order.is_none()
            && (precision == PointMeasurementPrecisionV1::Screened
                || self.incumbent.precision() == Some(PointMeasurementPrecisionV1::Screened))
        {
            // Refine both operands before an ambiguous tuple decision or a
            // budget-affecting region branch. A wider screening interval may
            // neither retain a worse tuple nor consume extra search nodes.
            self.refine_incumbent()?;
            if precision == PointMeasurementPrecisionV1::Screened && self.refine_current(region)? {
                precision = PointMeasurementPrecisionV1::Refined;
            }
            order = compare(
                self.prepared,
                self.context,
                self.component,
                self.buffers,
                region,
            );
        }
        Ok((current_measured, precision, order))
    }

    fn synchronize(&mut self) {
        self.context.expand_sources(&mut self.buffers.values);
    }

    fn assess(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<Option<(usize, bool)>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>>
    {
        if self.evidence.visited >= self.budget {
            return Ok(None);
        }
        self.evidence.visited += 1;
        self.evidence.points += 1;
        self.evidence.examined += 1;
        self.synchronize();
        let failures = assess_refined(
            self.context,
            runtime,
            epoch,
            scenarios,
            &self.buffers.values,
            &mut self.buffers.scores,
            arena,
        )?;
        if failures != 0 {
            return Ok(Some((failures, false)));
        }
        let (measured, precision, order) = self.assess_profile(None)?;
        // At a proved P+S tie, canonical tuple order picks a representative;
        // it is not reported as a scientific improvement.
        let retain = self.incumbent.consider(
            measured,
            precision,
            order,
            self.buffers.values.cmp(&self.buffers.best),
        );
        if retain {
            copy(&mut self.buffers.best, &self.buffers.values)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            copy(&mut self.buffers.best_primary, &self.buffers.primary)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            copy(&mut self.buffers.best_source, &self.buffers.source)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        }
        Ok(Some((0, retain)))
    }

    fn discover_material_sources(
        &mut self,
        seed_limit: u64,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        // A desired visible tone can lie outside the gamut a translucent source
        // can produce. Probe the physical source itself with independently varied
        // lightness and chroma. Hold other paints at their authored values first,
        // then try either domain endpoint of one companion to cross a joint
        // constraint boundary. Only the complete tuple can become an incumbent.
        let component = self.component;
        let context = self.context;
        let variables = component.point_variable_count();
        let endpoints = (0..variables).flat_map(|v| {
            let local = component.point_variable(v);
            let (low, high) = context.domains[local].bounds();
            [(local, low), (local, high)].map(|(local, rgb)| (local, Srgb8::new(rgb)))
        });
        let mut remaining = seed_limit.saturating_sub(self.evidence.visited);
        self.buffers.values.copy_from_slice(&self.buffers.authored);
        for companion in std::iter::once(None).chain(endpoints.map(Some)) {
            if remaining == 0 {
                break;
            }
            if let Some((local, source)) = companion {
                if source == self.buffers.authored[local] {
                    continue;
                }
                self.buffers.values[local] = source;
            }
            for variable in 0..variables {
                let local = self.component.point_variable(variable);
                if companion.is_some_and(|(other, _)| other == local) {
                    continue;
                }
                let anchor = self.buffers.authored[local];
                let chroma_steps = if anchor.is_achromatic() { 0 } else { 8 };
                for chroma_step in 0..=chroma_steps {
                    let chroma = 1.0 - f64::from(chroma_step) / 8.0;
                    let mut previous = None;
                    for index in 0..=128_u8 {
                        // Count even a duplicate or out-of-domain hint so a small
                        // budget also bounds the cost of constructing proposals.
                        if remaining == 0 {
                            return Ok(());
                        }
                        remaining -= 1;
                        let step = if index == 0 {
                            128
                        } else {
                            (index - 1).reverse_bits() >> 1
                        };
                        let source = crate::color_intent::source_tone(
                            anchor,
                            f64::from(step) / 128.0,
                            chroma,
                        );
                        if previous == Some(source) || !self.context.domains[local].contains(source)
                        {
                            continue;
                        }
                        previous = Some(source);
                        self.buffers.values[local] = source;
                        self.assess(runtime, epoch, scenarios, arena)?;
                    }
                }
                self.buffers.values[local] = anchor;
            }
            if let Some((local, _)) = companion {
                self.buffers.values[local] = self.buffers.authored[local];
            }
        }
        Ok(())
    }

    fn polish_toward_authored(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        self.buffers.values.copy_from_slice(&self.buffers.best);
        for variable in 0..self.component.point_variable_count() {
            if self.evidence.visited >= self.budget {
                break;
            }
            let local = self.component.point_variable(variable);
            let from = self.buffers.values[local].bytes();
            let to = self.buffers.authored[local].bytes();
            let steps = u16::from(
                from.into_iter()
                    .zip(to)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap_or(0),
            );
            // Try the exact authored paint first; feasibility along the segment
            // need not be monotone on a colored material or a multi-case input.
            for step in (1..=steps).rev() {
                if self.evidence.visited >= self.budget {
                    return Ok(());
                }
                self.buffers.values[local] = Srgb8::new(std::array::from_fn(|i| {
                    ((u16::from(from[i]) * (steps - step) + u16::from(to[i]) * step + steps / 2)
                        / steps) as u8
                }));
                self.assess(runtime, epoch, scenarios, arena)?;
            }
            self.buffers.values[local] = self.buffers.best[local];
        }
        Ok(())
    }

    fn polish_materials(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        // With a colored backdrop the admissible sources can lie in a narrow
        // band oblique to both a constant-hue ray and the segment to the anchor.
        // Move several channels together. Only assess() may retain a tuple.
        self.buffers.values.copy_from_slice(&self.buffers.best);
        for bit in (0..u8::BITS).rev() {
            let stride = 1_u8 << bit;
            loop {
                let mut improved = false;
                // A changed fill makes different inks feasible and vice versa.
                // Revisit the connected paints before reducing the mesh width.
                for variable in 0..self.component.point_variable_count() {
                    let local = self.component.point_variable(variable);
                    let center = self.buffers.best[local];
                    let [red, green, blue] = center
                        .bytes()
                        .map(|v| [v.checked_sub(stride), Some(v), v.checked_add(stride)]);
                    for r in red.into_iter().flatten() {
                        for g in green.into_iter().flatten() {
                            for b in blue.into_iter().flatten() {
                                if self.evidence.visited >= self.budget {
                                    return Ok(());
                                }
                                let source = Srgb8::new([r, g, b]);
                                if source == center || !self.context.domains[local].contains(source)
                                {
                                    continue;
                                }
                                self.buffers.values[local] = source;
                                self.assess(runtime, epoch, scenarios, arena)?;
                            }
                        }
                    }
                    self.buffers.values[local] = self.buffers.best[local];
                    improved |= self.buffers.best[local] != center;
                }
                if !improved {
                    break;
                }
            }
        }
        Ok(())
    }

    fn region_worse(
        &mut self,
        node: RegionNodeV1,
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        if self.incumbent.precision().is_none() {
            return Ok(false);
        }
        if self.initial_precision == PointMeasurementPrecisionV1::Screened {
            // This box includes the tuple itself, so it cannot strictly lose.
            if self.buffers.values[..node.depth] == self.buffers.best[..node.depth]
                && node.domain.contains(self.buffers.best[node.depth])
            {
                return Ok(false);
            }
            let witnessed = measured::<E>(self.prepared.measure_point_component(
                self.component,
                &self.buffers.values,
                Some(node),
                &self.context.local_cases.order,
                &mut self.buffers.primary,
                &mut self.buffers.source,
                PointMeasurementV1::RegionWitness,
            ))?;
            // Each use has independent witnesses for its interval lower bound.
            // Such a profile can disprove pruning but cannot retain a tuple.
            if witnessed
                && compare_primary(
                    self.prepared,
                    self.context,
                    self.component,
                    self.buffers,
                    Some(node),
                ) == Some(Ordering::Less)
            {
                return Ok(false);
            }
        }
        let (measured, _, order) = self.assess_profile(Some(node))?;
        Ok(measured && order == Some(Ordering::Greater))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::program_session) fn select_component<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    prepared: &mut PreparedPointSelectionV1<'_, '_, E>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let mut buffers = prepared.take_point_buffers();
    let result = search(
        selection,
        component,
        runtime,
        epoch,
        scenarios,
        arena,
        prepared,
        &mut buffers,
    );
    prepared.restore_point_buffers(buffers);
    result
}

#[allow(clippy::too_many_arguments)]
fn search<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    component: &ComputedComponentV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    prepared: &mut PreparedPointSelectionV1<'_, '_, E>,
    buffers: &mut BuffersV1,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let context = ContextV1::point::<E>(selection, component, scenarios)?;
    prepared
        .refresh_point_component(component, &context.local_cases.order)
        .map_err(|error| match error {
            reference_completion::ReferenceCompletionErrorV1::ResourceExhausted => {
                ProgramSessionEvaluationError::ResourceExhausted
            }
            _ => ProgramSessionEvaluationError::InternalInvariant,
        })?;
    let count = context.targets.len();
    let variables = component.point_variable_count();
    buffers.authored.clear();
    buffers
        .authored
        .try_reserve(count)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    buffers.authored.extend(
        context
            .targets
            .iter()
            .map(|target| target.declaration.anchor),
    );
    copy(&mut buffers.values, &buffers.authored)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    copy(&mut buffers.working, &buffers.authored)
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    buffers.scores.resize(count, 0.0);
    for values in [&mut buffers.left, &mut buffers.right, &mut buffers.sort] {
        values.clear();
        values
            .try_reserve(count)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    }
    let mut state = SearchV1 {
        context: &context,
        component,
        prepared,
        buffers,
        evidence: SearchEvidenceV1 {
            completion: CompletionV1::BudgetExhausted,
            numerical_order_unresolved: false,
            visited: 0,
            regions: 0,
            points: 0,
            endpoint_evaluations: 0,
            examined: 0,
            open_regions: 0,
        },
        budget: selection.budget,
        incumbent: FeasibleIncumbentV1::default(),
        initial_precision: initial_precision(),
    };
    let authored = state.assess(runtime, epoch, scenarios, arena)?;
    let authored_optimal = authored.is_some_and(|(failures, _)| failures == 0)
        && match state
            .prepared
            .authored_continuous_terms_minimal(component, &context.local_cases.order)
        {
            Ok(proven) => proven,
            Err(error) => {
                measured::<E>(Err(error))?;
                false
            }
        };
    if authored_optimal {
        // Each primary term is zero or constant over the complete continuous
        // source domain. The authored tuple also minimizes every source distance.
        state.evidence.completion = CompletionV1::AuthoredExact;
    } else {
        let seed_limit = (selection.budget / COORDINATE_BUDGET_DIVISOR).max(state.evidence.visited);
        if !state.incumbent.has_value() && variables >= 2 {
            let proposals = component
                .point_constraints()
                .iter()
                .filter_map(|&index| paired_tones(context.targets, &epoch.constraints[index]))
                .flatten();
            for (_, [(left, first), (right, second)]) in
                (state.evidence.visited..seed_limit).zip(proposals)
            {
                state
                    .buffers
                    .values
                    .copy_from_slice(&state.buffers.authored);
                state.buffers.values[left] = first;
                state.buffers.values[right] = second;
                state.assess(runtime, epoch, scenarios, arena)?;
            }
        }
        // Independent contextual starts and physical coordinate repair share
        // the component's existing cap; no conditional subsolver runs here.
        for origin in 0..=context.local_cases.order.len() {
            if state.evidence.visited >= seed_limit {
                break;
            }
            let origin_limit = state.evidence.visited
                + (seed_limit - state.evidence.visited)
                    / (context.local_cases.order.len() + 1 - origin) as u64;
            copy(&mut state.buffers.values, &state.buffers.authored)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            if origin > 0 {
                let case = context.local_cases.order[origin - 1];
                for (local, target) in context.targets.iter().enumerate() {
                    if component.point_representative(local) != local {
                        continue;
                    }
                    apply_targets::<E>(context.targets, runtime, &state.buffers.values)?;
                    let (backdrop, alpha) = physical_context(
                        target,
                        context.physical,
                        runtime,
                        epoch,
                        scenarios,
                        case,
                    )?;
                    state.buffers.values[local] =
                        inverse_hint(context.preferred_rgb[case * count + local], backdrop, alpha);
                }
            }
            let Some((mut failures, _)) = state.assess(runtime, epoch, scenarios, arena)? else {
                break;
            };
            copy(&mut state.buffers.working, &state.buffers.values)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            let mut heuristic = state.buffers.scores.iter().copied().fold(0.0_f64, f64::max);
            'sweeps: for pass in 0..2 {
                for position in 0..variables {
                    let variable = if pass == 0 {
                        variables - 1 - position
                    } else {
                        position
                    };
                    let local = component.point_variable(variable);
                    let target = &context.targets[local];
                    let Some(intent) = target.effective_intent() else {
                        continue;
                    };
                    for &case in &context.local_cases.order {
                        if state.evidence.visited >= origin_limit {
                            break 'sweeps;
                        }
                        apply_targets::<E>(context.targets, runtime, &state.buffers.working)?;
                        let (backdrop, alpha) = physical_context(
                            target,
                            context.physical,
                            runtime,
                            epoch,
                            scenarios,
                            case,
                        )?;
                        let mut previous = None;
                        for step in 0..=255 {
                            if state.evidence.visited >= origin_limit {
                                break;
                            }
                            let value = if target.declaration.reference_appearance {
                                color_at_lightness(
                                    target.declaration.anchor,
                                    f64::from(step) / 255.0,
                                )
                            } else {
                                tonal_hint(
                                    target,
                                    &intent,
                                    f64::from(step) / 255.0,
                                    backdrop,
                                    alpha,
                                )
                            };
                            if previous == Some(value) || !context.domains[local].contains(value) {
                                continue;
                            }
                            previous = Some(value);
                            copy(&mut state.buffers.values, &state.buffers.working)
                                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
                            state.buffers.values[local] = value;
                            let Some((next_failures, retained)) =
                                state.assess(runtime, epoch, scenarios, arena)?
                            else {
                                break;
                            };
                            let score =
                                state.buffers.scores.iter().copied().fold(0.0_f64, f64::max);
                            if retained
                                || next_failures < failures
                                || next_failures != 0
                                    && next_failures == failures
                                    && score < heuristic
                            {
                                copy(&mut state.buffers.working, &state.buffers.values).map_err(
                                    |_| ProgramSessionEvaluationError::ResourceExhausted,
                                )?;
                                failures = next_failures;
                                heuristic = score;
                            }
                        }
                    }
                }
            }
        }
        let mut segment_polish = false;
        if !state.incumbent.has_value() {
            state.discover_material_sources(seed_limit, runtime, epoch, scenarios, arena)?;
            // A late change to a fill can make an earlier text adjustment unnecessary.
            segment_polish = state.incumbent.has_value();
        }
        let materials = (0..variables)
            .filter(|&variable| !context.opaque[component.point_variable(variable)])
            .count() as u64;
        let polish_work = if state.incumbent.has_value() {
            let segments = if segment_polish { variables as u64 } else { 0 };
            (segments + 2 * materials) * u64::from(u8::MAX)
        } else {
            0
        };
        let tree_limit =
            selection.budget - polish_work.min(selection.budget / COORDINATE_BUDGET_DIVISOR);
        let constraints = region_constraints(epoch, context.constraint_indices)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        let mut bounds = BoundsV1 {
            constraint_lower: buffer(constraints.len(), None)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            constraints,
            lower: buffer(count, [0; 3])
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            scores: buffer(count, 0.0)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            changes_min: buffer(count, false)
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            tuple_min: buffer(count, Srgb8::new([0; 3]))
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?,
            evaluations: 0,
            appearance: None,
        };
        let mut stack = Vec::new();
        let capacity = context
            .domains
            .iter()
            .try_fold(1usize, |n, domain| n.checked_add(domain.split_depth() + 1))
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        stack
            .try_reserve_exact(capacity)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        let seed = context.local_cases.order[0];
        if let Some(node) = region_node(
            0,
            context.domains[0],
            tree_limit,
            context.preferred[seed * count],
            &mut state.evidence,
        ) {
            stack.push(node);
        }
        let mut complete = !stack.is_empty();
        while let Some(mut node) = stack.pop() {
            state.synchronize();
            let depth = node.depth;
            let volume = node
                .domain
                .cardinality()
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
            let Some(ordinal) = node.ordinal else {
                let may_pass = bounds.may_pass(
                    &context,
                    runtime,
                    epoch,
                    scenarios,
                    node,
                    &state.buffers.values,
                )?;
                state.evidence.endpoint_evaluations = bounds.evaluations;
                if !may_pass || state.region_worse(node)? {
                    state
                        .evidence
                        .close_region()
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    continue;
                }
                if volume > 16 {
                    let (a, b) = node.domain.split();
                    let preferred = context.preferred[seed * count + depth];
                    let Some(a) = region_node(depth, a, tree_limit, preferred, &mut state.evidence)
                    else {
                        complete = false;
                        break;
                    };
                    let Some(b) = region_node(depth, b, tree_limit, preferred, &mut state.evidence)
                    else {
                        complete = false;
                        break;
                    };
                    state
                        .evidence
                        .close_region()
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    let (first, second) = if a.lower_score <= b.lower_score {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    stack.push(second);
                    stack.push(first);
                } else {
                    node.ordinal = Some(0);
                    stack.push(node);
                }
                continue;
            };
            if state.evidence.visited >= tree_limit {
                complete = false;
                break;
            }
            if ordinal + 1 < volume {
                stack.push(RegionNodeV1 {
                    ordinal: Some(ordinal + 1),
                    ..node
                });
            } else {
                state
                    .evidence
                    .close_region()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
            }
            state.buffers.values[depth] = node.domain.value(ordinal);
            let next_depth =
                (depth + 1..count).find(|&local| component.point_representative(local) == local);
            if let Some(next_depth) = next_depth {
                state.evidence.visited += 1;
                state.evidence.points += 1;
                if let Some(next) = region_node(
                    next_depth,
                    context.domains[next_depth],
                    tree_limit,
                    context.preferred[seed * count + next_depth],
                    &mut state.evidence,
                ) {
                    stack.push(next);
                } else {
                    complete = false;
                    break;
                }
            } else {
                state.assess(runtime, epoch, scenarios, arena)?;
            }
        }
        if complete {
            state.evidence.completion = CompletionV1::Complete;
        } else if tree_limit < selection.budget {
            if segment_polish {
                state.polish_toward_authored(runtime, epoch, scenarios, arena)?;
            }
            if materials != 0 {
                state.polish_materials(runtime, epoch, scenarios, arena)?;
            }
        }
    }
    state.evidence.numerical_order_unresolved = !authored_optimal && state.incumbent.unresolved();
    let values = if state.incumbent.has_value() {
        let mut values = Vec::new();
        copy(&mut values, &state.buffers.best)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        Some(values)
    } else {
        None
    };
    let reference_selections = if let Some(values) = &values {
        if context
            .targets
            .iter()
            .any(|target| target.reference_backdrop.is_some())
        {
            Some(reference_search::reports_for_values(
                &context,
                runtime,
                epoch,
                scenarios,
                values,
                |_| Ok(None),
            )?)
        } else {
            None
        }
    } else {
        None
    };
    Ok(SearchResultV1 {
        values,
        evidence: state.evidence,
        reference_selections,
    })
}

#[cfg(test)]
pub(super) mod tests;
