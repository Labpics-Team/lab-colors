//! Reference selection and conditional capacity share one bounded work runtime.
//! Every capacity witness is a complete, hard-checked physical tuple. Memo keys
//! contain actual backdrop values and prior paints from the compared tuple,
//! never the incidental state of the DFS cursor.

use super::reference_completion::PreparedPointSelectionV1;
use super::*;

mod peers;
use crate::color_intent::reference_transport::{
    ReferenceTransportV1, contrast_ratio, luminance, polarity,
};
use crate::constraints::TonalPolarityV1;
use crate::program_wire::{
    ProgramReferenceCapacityV1, ProgramReferenceCaseV1, ProgramReferencePreferenceV1,
    ProgramReferenceSelectionV1,
};
use peers::PeerProfiles;
use std::collections::{BinaryHeap, HashMap};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, Default)]
struct TargetKeyV1 {
    capacity_shortfall: f64,
    continuous_displacement: f64,
    reference_polarity_mismatch: u8,
    final_distance: f64,
}

impl TargetKeyV1 {
    fn cmp(self, other: Self) -> Ordering {
        self.capacity_shortfall
            .total_cmp(&other.capacity_shortfall)
            .then_with(|| {
                self.continuous_displacement
                    .total_cmp(&other.continuous_displacement)
            })
            .then_with(|| {
                self.reference_polarity_mismatch
                    .cmp(&other.reference_polarity_mismatch)
            })
            .then_with(|| self.final_distance.total_cmp(&other.final_distance))
    }
    fn is_zero(self) -> bool {
        self.cmp(Self::default()) == Ordering::Equal
    }
}

#[derive(Clone, Copy)]
struct WorkRegionV1 {
    node: RegionNodeV1,
    capacity_upper: f64,
}

struct TraversalV1 {
    values: Vec<Srgb8>,
    stack: Vec<WorkRegionV1>,
}

struct CapacityV1 {
    target: usize,
    prior_paints: Vec<Srgb8>,
    backdrops: Vec<Srgb8>,
    signs: Vec<i8>,
    ratio_lower: f64,
    ratio_upper: f64,
    complete: bool,
    job: Option<usize>,
    witness: Vec<Srgb8>,
    dependents: Vec<usize>,
}

struct PendingPointV1 {
    values: Vec<Srgb8>,
    capacities: Vec<Option<usize>>,
    keys: Vec<TargetKeyV1>,
    revision: u64,
}

// A min-priority snapshot. Capacity upper bounds only decrease, so a queued
// lower key can become optimistically stale only in the safe direction: it
// is refreshed when it reaches the top. Better replacement tuples are queued
// immediately with a new revision, invalidating their previous snapshots.
struct CapacityPriorityV1 {
    point: usize,
    revision: u64,
    keys: Vec<TargetKeyV1>,
    source_changes: Vec<bool>,
    values: Vec<Srgb8>,
}

impl Ord for CapacityPriorityV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.keys
            .iter()
            .zip(&other.keys)
            .map(|(&left, &right)| left.cmp(right))
            .find(|order| *order != Ordering::Equal)
            .unwrap_or_else(|| {
                self.source_changes
                    .cmp(&other.source_changes)
                    .then_with(|| self.values.cmp(&other.values))
            })
            .then_with(|| self.point.cmp(&other.point))
            .then_with(|| self.revision.cmp(&other.revision))
            .reverse()
    }
}
impl PartialOrd for CapacityPriorityV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for CapacityPriorityV1 {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for CapacityPriorityV1 {}

struct ReferenceRuntimeV1<'a, 'p, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1> {
    peers: Option<PeerProfiles<'a, 'p, 'program, 'scratch, E>>,
    context: ContextV1<'a>,
    models: Vec<Option<ReferenceTransportV1>>,
    evidence: SearchEvidenceV1,
    budget: u64,
    appearance: Vec<AppearancePointV1>,
    legacy_scores: Vec<f64>,
    capacities: Vec<CapacityV1>,
    capacity_lookup: HashMap<u64, Vec<usize>>,
    pending: Vec<PendingPointV1>,
    pending_lookup: HashMap<u64, Vec<usize>>,
    capacity_queue: BinaryHeap<CapacityPriorityV1>,
    scratch_capacities: Vec<Option<usize>>,
    scratch_keys: Vec<TargetKeyV1>,
    math_cache: HashMap<(usize, Srgb8, i8), [f64; 3]>,
    jobs: Vec<Option<TraversalV1>>,
    best: Option<usize>,
    point_closed: bool,
}

fn cloned<T: Copy>(source: &[T]) -> Result<Vec<T>, ()> {
    let mut result = Vec::new();
    result.try_reserve_exact(source.len()).map_err(|_| ())?;
    result.extend_from_slice(source);
    Ok(result)
}

fn pushed<T>(values: &mut Vec<T>, value: T) -> Result<(), ()> {
    values.try_reserve(1).map_err(|_| ())?;
    values.push(value);
    Ok(())
}

// Every repair direction and physical case walks the same authored source
// tones. Generate the prefix actually reached under the shared search budget.
fn source_tone_step(tones: &mut Vec<Srgb8>, anchor: Srgb8, step: u8) -> Result<Srgb8, ()> {
    let index = usize::from(step);
    for next in tones.len()..=index {
        let source = color_at_lightness(anchor, next as f64 / 255.0);
        pushed(tones, source)?;
    }
    Ok(tones[index])
}

fn sign(value: TonalPolarityV1) -> i8 {
    match value {
        TonalPolarityV1::Darker => -1,
        TonalPolarityV1::Equal => 0,
        TonalPolarityV1::Lighter => 1,
    }
}

fn from_sign(value: i8) -> TonalPolarityV1 {
    match value {
        -1 => TonalPolarityV1::Darker,
        1 => TonalPolarityV1::Lighter,
        _ => TonalPolarityV1::Equal,
    }
}

fn fingerprint(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn insert_index(lookup: &mut HashMap<u64, Vec<usize>>, key: u64, index: usize) -> Result<(), ()> {
    lookup.try_reserve(1).map_err(|_| ())?;
    pushed(lookup.entry(key).or_default(), index)
}

fn tonal_proposals(anchor: Srgb8, domain: AssignmentDomainV1) -> impl Iterator<Item = Srgb8> {
    let mut previous = None;
    (0..=255_u32).filter_map(move |step| {
        #[cfg(test)]
        case_refinement_tests::tonal_conversion();
        let source = color_at_lightness(anchor, f64::from(step) / 255.0);
        let repeated = previous == Some(source);
        previous = Some(source);
        (!repeated && domain.contains(source)).then_some(source)
    })
}

fn polish_plan(
    targets: impl IntoIterator<Item = (usize, Srgb8, AssignmentDomainV1)>,
    maximum: usize,
) -> Result<Vec<(usize, Srgb8)>, ()> {
    let mut proposals = Vec::new();
    if maximum == 0 {
        return Ok(proposals);
    }
    for (target, anchor, domain) in targets {
        for source in tonal_proposals(anchor, domain) {
            if proposals.is_empty() {
                proposals.try_reserve_exact(maximum).map_err(|_| ())?;
            }
            proposals.push((target, source));
            if proposals.len() == maximum {
                return Ok(proposals);
            }
        }
    }
    Ok(proposals)
}

#[test]
fn terminal_proposals_count_only_distinct_admitted_sources() {
    let anchor = Srgb8::new([128; 3]);
    assert_eq!(
        tonal_proposals(anchor, AssignmentDomainV1::Gray { lower: 0, upper: 3 })
            .collect::<Vec<_>>(),
        (0..=3)
            .map(|value| Srgb8::new([value; 3]))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        tonal_proposals(
            anchor,
            AssignmentDomainV1::Cube {
                lower: [0, 0, 1],
                upper: [0, 0, 2]
            }
        )
        .count(),
        0,
        "an off-path source domain reserves no tonal visits"
    );
}

#[test]
fn terminal_plan_stops_at_its_reserve_and_empty_domains_do_not_allocate() {
    let anchor = Srgb8::new([128; 3]);
    let targets = std::iter::once((0, anchor, AssignmentDomainV1::Gray { lower: 0, upper: 3 }))
        .chain(std::iter::once_with(|| {
            panic!("the reserve needs no later target")
        }));
    assert_eq!(
        polish_plan(targets, 3).expect("bounded proposal storage"),
        (0..=2)
            .map(|value| (0, Srgb8::new([value; 3])))
            .collect::<Vec<_>>()
    );
    let (empty, allocations) = crate::test_support::measured_allocator_events(|| {
        polish_plan(
            [(
                0,
                anchor,
                AssignmentDomainV1::Cube {
                    lower: [0, 0, 1],
                    upper: [0, 0, 2],
                },
            )],
            256,
        )
    });
    assert!(empty.expect("empty proposal plan").is_empty());
    assert_eq!(allocations, crate::test_support::AllocatorEvents::default());
    assert!(
        polish_plan(
            std::iter::once_with(|| panic!("zero work never reads a target")),
            0
        )
        .expect("zero proposal budget")
        .is_empty()
    );
}

impl<'a, 'p, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1>
    ReferenceRuntimeV1<'a, 'p, 'program, 'scratch, E>
{
    fn new(
        selection: &'a ComputedSelectionV1,
        observation: LocalObservationV1<'a, '_>,
        order: Option<&'a identity::ObservedComponentOrder>,
        prepared: Option<&'p mut PreparedPointSelectionV1<'program, 'scratch, E>>,
    ) -> Result<Self, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let component = observation.component;
        let scenarios = observation.scenarios;
        let context = ContextV1::for_observation::<E>(selection, observation, order)?;
        let count = context.targets.len();
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        let cells = count
            .checked_mul(scenarios.len().get())
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        let mut models = Vec::new();
        models
            .try_reserve_exact(count)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        for target in context.targets.iter() {
            models.push(
                target
                    .reference_backdrop
                    .map(|backdrop| ReferenceTransportV1::new(target.effective_anchor(), backdrop)),
            );
        }
        let peers = match (component.mode, prepared) {
            (ProgramSelectionModeV1::JointReferencePeersV1, Some(prepared)) => {
                Some(PeerProfiles::new(&context, component, prepared)?)
            }
            (ProgramSelectionModeV1::JointReferencePeersV1, None) | (_, Some(_)) => {
                return Err(ProgramSessionEvaluationError::InternalInvariant);
            }
            _ => None,
        };
        Ok(Self {
            peers,
            context,
            models,
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
            appearance: buffer(cells, AppearancePointV1::default()).map_err(allocation)?,
            legacy_scores: buffer(count, 0.0).map_err(allocation)?,
            scratch_capacities: buffer(count, None).map_err(allocation)?,
            scratch_keys: buffer(count, TargetKeyV1::default()).map_err(allocation)?,
            capacities: Vec::new(),
            capacity_lookup: HashMap::new(),
            pending: Vec::new(),
            pending_lookup: HashMap::new(),
            capacity_queue: BinaryHeap::new(),
            math_cache: HashMap::new(),
            jobs: Vec::new(),
            best: None,
            point_closed: false,
        })
    }

    fn capacity_enabled(&self, target: usize) -> bool {
        self.models[target].is_some() && self.context.targets[target].tone_input.is_none()
    }

    fn virtual_lab(
        &mut self,
        target: usize,
        case: usize,
        backdrop: Srgb8,
        direction: i8,
    ) -> Result<[f64; 3], ()> {
        let model = self.models[target].ok_or(())?;
        if !self.capacity_enabled(target) {
            let mut result = model.reference_lab();
            result[0] = self.context.targets[target].backdrop_curve().map_or_else(
                || self.context.preferred[case * self.context.targets.len() + target][0],
                |curve| curve.preferred_lightness(backdrop),
            );
            return Ok(result);
        }
        let key = (target, backdrop, direction);
        if let Some(value) = self.math_cache.get(&key) {
            return Ok(*value);
        }
        let result = model.lab_for(backdrop, from_sign(direction));
        self.math_cache.try_reserve(1).map_err(|_| ())?;
        self.math_cache.insert(key, result);
        Ok(result)
    }

    fn capacity_sign(&self, target: usize, point: AppearancePointV1) -> i8 {
        if self.models[target].is_some_and(|model| model.reference_contrast() == 1.0) {
            0
        } else {
            sign(polarity(point.visible, point.backdrop))
        }
    }

    // An out-of-gamut continuous target must not collapse a chromatic source
    // to white/black outside its identity. Project a bounded source tonal walk
    // in the actual visible space instead; this is only a proposal, not a
    // capacity witness or a restriction of the searched RGB domain.
    fn source_hint(
        &self,
        target: usize,
        point: AppearancePointV1,
        preferred: [f64; 3],
        tones: &mut Vec<Srgb8>,
    ) -> Result<Srgb8, ()> {
        let compiled = &self.context.targets[target];
        let anchor = compiled.declaration.anchor;
        let Some(intent) = compiled.effective_intent() else {
            return Ok(anchor);
        };
        let distance_to = |source: Srgb8| {
            let visible = Srgb8::new(std::array::from_fn(|channel| {
                crate::composition::source_over_channel_srgb8(
                    source.bytes()[channel],
                    point.alpha,
                    point.backdrop.bytes()[channel],
                )
            }));
            if !self.context.domains[target].contains(source)
                || !intent
                    .identity
                    .contains(compiled.identity_point(source, visible))
            {
                return f64::INFINITY;
            }
            squared_distance(lab(visible), preferred)
        };
        let mut best = anchor;
        let mut distance = distance_to(anchor);
        for step in 0..=u8::MAX {
            let source = source_tone_step(tones, anchor, step)?;
            let next = distance_to(source);
            if next < distance {
                distance = next;
                best = source;
            }
        }
        Ok(best)
    }

    fn capacity_hash(&self, target: usize, values: &[Srgb8], points: &[AppearancePointV1]) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        target.hash(&mut hash);
        values[..target].hash(&mut hash);
        let count = self.context.targets.len();
        for &case in &self.context.local_cases.order {
            let point = points[case * count + target];
            point.backdrop.hash(&mut hash);
            self.capacity_sign(target, point).hash(&mut hash);
        }
        hash.finish()
    }

    fn capacity_matches(
        &self,
        capacity: &CapacityV1,
        values: &[Srgb8],
        points: &[AppearancePointV1],
    ) -> bool {
        if values[..capacity.target] != capacity.prior_paints {
            return false;
        }
        let count = self.context.targets.len();
        self.context
            .local_cases
            .order
            .iter()
            .enumerate()
            .all(|(index, &case)| {
                let point = points[case * count + capacity.target];
                point.backdrop == capacity.backdrops[index]
                    && self.capacity_sign(capacity.target, point) == capacity.signs[index]
            })
    }

    fn ratio(&self, target: usize, points: &[AppearancePointV1]) -> f64 {
        let reference = self.models[target]
            .expect("compiled reference")
            .reference_contrast();
        let count = self.context.targets.len();
        let mut minimum = 1.0_f64;
        let mut witness = None;
        for &case in &self.context.local_cases.order {
            let point = points[case * count + target];
            let ratio = (contrast_ratio(point.visible, point.backdrop) / reference).min(1.0);
            if ratio < minimum {
                minimum = ratio;
                witness = Some(case);
            }
        }
        if let Some(case) = witness {
            self.context.case_refinement.borrow_mut().add(case);
        }
        minimum
    }

    fn record_capacity(
        &mut self,
        target: usize,
        values: &[Srgb8],
        points: &[AppearancePointV1],
    ) -> Result<usize, ()> {
        let hash = self.capacity_hash(target, values, points);
        let existing = self.capacity_lookup.get(&hash).and_then(|indices| {
            indices.iter().copied().find(|&index| {
                self.capacities[index].target == target
                    && self.capacity_matches(&self.capacities[index], values, points)
            })
        });
        let ratio = self.ratio(target, points);
        if let Some(index) = existing {
            let improved = ratio > self.capacities[index].ratio_lower;
            if improved {
                self.capacities[index].witness.copy_from_slice(values);
            }
            self.capacities[index].ratio_lower = self.capacities[index].ratio_lower.max(ratio);
            if self.capacities[index].ratio_lower >= self.capacities[index].ratio_upper {
                self.capacities[index].ratio_upper = self.capacities[index].ratio_lower;
                self.capacities[index].complete = true;
            }
            if improved {
                for position in 0..self.capacities[index].dependents.len() {
                    self.consider_point(self.capacities[index].dependents[position]);
                }
            }
            return Ok(index);
        }
        let cases = self.context.local_cases.order.len();
        let mut backdrops = Vec::new();
        let mut signs = Vec::new();
        backdrops.try_reserve_exact(cases).map_err(|_| ())?;
        signs.try_reserve_exact(cases).map_err(|_| ())?;
        for &case in &self.context.local_cases.order {
            let point = points[case * self.context.targets.len() + target];
            backdrops.push(point.backdrop);
            signs.push(self.capacity_sign(target, point));
        }
        let upper = if signs.contains(&0) {
            (1.0 / self.models[target].ok_or(())?.reference_contrast()).min(1.0)
        } else {
            1.0
        };
        let index = self.capacities.len();
        pushed(
            &mut self.capacities,
            CapacityV1 {
                target,
                prior_paints: cloned(&values[..target])?,
                backdrops,
                signs,
                ratio_lower: ratio,
                ratio_upper: upper.max(ratio),
                complete: ratio >= upper,
                job: None,
                witness: cloned(values)?,
                dependents: Vec::new(),
            },
        )?;
        insert_index(&mut self.capacity_lookup, hash, index)?;
        Ok(index)
    }

    fn effective_key(&self, point: &PendingPointV1, target: usize, upper: bool) -> TargetKeyV1 {
        let mut key = point.keys[target];
        if let Some(index) = point.capacities[target] {
            let capacity = &self.capacities[index];
            key.capacity_shortfall = 1.0
                - if upper {
                    capacity.ratio_lower
                } else {
                    capacity.ratio_upper
                };
        }
        key
    }

    fn compare_points(
        &self,
        left: usize,
        left_upper: bool,
        right: usize,
        right_upper: bool,
    ) -> Ordering {
        let left = &self.pending[left];
        let right = &self.pending[right];
        for target in 0..self.context.targets.len() {
            let order = self
                .effective_key(left, target, left_upper)
                .cmp(self.effective_key(right, target, right_upper));
            if order != Ordering::Equal {
                return order;
            }
        }
        self.compare_sources(&left.values, &right.values)
    }

    fn compare_sources(&self, left: &[Srgb8], right: &[Srgb8]) -> Ordering {
        self.context
            .targets
            .iter()
            .zip(left)
            .map(|(target, value)| *value != target.declaration.anchor)
            .cmp(
                self.context
                    .targets
                    .iter()
                    .zip(right)
                    .map(|(target, value)| *value != target.declaration.anchor),
            )
            .then_with(|| left.cmp(right))
    }

    fn consider_point(&mut self, point: usize) {
        if self
            .best
            .is_none_or(|best| self.compare_points(point, true, best, true) == Ordering::Less)
        {
            self.best = Some(point);
        }
    }

    fn queue_point(&mut self, point: usize) -> Result<(), ()> {
        let value = &self.pending[point];
        if value
            .capacities
            .iter()
            .flatten()
            .all(|&index| self.capacities[index].complete)
        {
            return Ok(());
        }
        let mut keys = Vec::new();
        let mut source_changes = Vec::new();
        keys.try_reserve_exact(value.keys.len()).map_err(|_| ())?;
        source_changes
            .try_reserve_exact(value.keys.len())
            .map_err(|_| ())?;
        for (target, compiled) in self.context.targets.iter().enumerate() {
            keys.push(self.effective_key(value, target, false));
            source_changes.push(value.values[target] != compiled.declaration.anchor);
        }
        self.capacity_queue.try_reserve(1).map_err(|_| ())?;
        self.capacity_queue.push(CapacityPriorityV1 {
            point,
            revision: value.revision,
            keys,
            source_changes,
            values: cloned(&value.values)?,
        });
        Ok(())
    }

    fn retain_peer_tuple(
        &mut self,
        values: &[Srgb8],
        points: &[AppearancePointV1],
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        if let Some(peers) = &mut self.peers {
            let improved = peers.consider(&self.context, points, values)?;
            if improved {
                if let Some(best) = self.best {
                    self.pending[best].values.copy_from_slice(values);
                } else {
                    pushed(
                        &mut self.pending,
                        PendingPointV1 {
                            values: cloned(values).map_err(allocation)?,
                            capacities: cloned(&self.scratch_capacities).map_err(allocation)?,
                            keys: Vec::new(),
                            revision: 0,
                        },
                    )
                    .map_err(allocation)?;
                    self.best = Some(0);
                }
            }
            return Ok(());
        }
        Err(ProgramSessionEvaluationError::InternalInvariant)
    }

    fn retain_tuple(&mut self, values: &[Srgb8], points: &[AppearancePointV1]) -> Result<(), ()> {
        self.scratch_capacities.fill(None);
        self.scratch_keys.fill(TargetKeyV1::default());
        let count = self.context.targets.len();
        for target in 0..count {
            let Some(model) = self.models[target] else {
                self.scratch_keys[target].final_distance = self.legacy_scores[target];
                continue;
            };
            if self.capacity_enabled(target) {
                self.scratch_capacities[target] =
                    Some(self.record_capacity(target, values, points)?);
            }
            // Canonical physical case order eliminates multiplicity and ID effects.
            let mut extrema = [None; 3];
            for position in 0..self.context.local_cases.order.len() {
                let case = self.context.local_cases.order[position];
                let point = points[case * count + target];
                let direction = self.capacity_sign(target, point);
                let preferred = self.virtual_lab(target, case, point.backdrop, direction)?;
                let enabled = self.capacity_enabled(target);
                let key = &mut self.scratch_keys[target];
                if enabled {
                    let displacement = squared_distance(preferred, model.reference_lab());
                    if displacement > key.continuous_displacement {
                        extrema[0] = Some(case);
                    }
                    key.continuous_displacement = key.continuous_displacement.max(displacement);
                    if model.reference_contrast() != 1.0
                        && direction != sign(model.reference_polarity())
                        && key.reference_polarity_mismatch == 0
                    {
                        extrema[1] = Some(case);
                    }
                    key.reference_polarity_mismatch =
                        key.reference_polarity_mismatch.max(u8::from(
                            model.reference_contrast() != 1.0
                                && direction != sign(model.reference_polarity()),
                        ));
                }
                let distance = squared_distance(point.lab, preferred);
                if distance > key.final_distance {
                    extrema[2] = Some(case);
                }
                key.final_distance = key.final_distance.max(distance);
            }
            for case in extrema.into_iter().flatten() {
                self.context.case_refinement.borrow_mut().add(case);
            }
        }
        let hash = fingerprint(&self.scratch_capacities);
        let existing = self.pending_lookup.get(&hash).and_then(|indices| {
            indices
                .iter()
                .copied()
                .find(|&index| self.pending[index].capacities == self.scratch_capacities)
        });
        if let Some(index) = existing {
            let previous = &self.pending[index];
            let mut order = Ordering::Equal;
            for (&new, &old) in self.scratch_keys.iter().zip(&previous.keys) {
                order = new.cmp(old);
                if order != Ordering::Equal {
                    break;
                }
            }
            if order.then_with(|| self.compare_sources(values, &previous.values)) == Ordering::Less
            {
                let previous = &mut self.pending[index];
                previous.values.copy_from_slice(values);
                previous.keys.copy_from_slice(&self.scratch_keys);
                previous.revision += 1;
                self.consider_point(index);
                self.queue_point(index)?;
            }
        } else {
            let index = self.pending.len();
            pushed(
                &mut self.pending,
                PendingPointV1 {
                    values: cloned(values)?,
                    capacities: cloned(&self.scratch_capacities)?,
                    keys: cloned(&self.scratch_keys)?,
                    revision: 0,
                },
            )?;
            insert_index(&mut self.pending_lookup, hash, index)?;
            for &capacity in self.scratch_capacities.iter().flatten() {
                pushed(&mut self.capacities[capacity].dependents, index)?;
            }
            self.consider_point(index);
            self.queue_point(index)?;
        }
        Ok(())
    }

    fn assess_tuple(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        values: &mut [Srgb8],
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<Option<usize>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        self.assess_tuple_inner(runtime, epoch, scenarios, values, arena, false)
    }

    fn assess_tuple_inner(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        values: &mut [Srgb8],
        arena: &mut ProgramEvaluationArenaV1<E>,
        probe: bool,
    ) -> Result<Option<usize>, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        if self.evidence.visited >= self.budget {
            return Ok(None);
        }
        self.context.expand_sources(values);
        self.evidence.visited += 1;
        self.evidence.points += 1;
        self.evidence.examined += 1;
        if probe {
            if let Some(failures) = probe_cases(
                &self.context,
                runtime,
                epoch,
                scenarios,
                values,
                &mut self.legacy_scores,
                arena,
            )? {
                return Ok(Some(failures));
            }
        }
        let mut collector =
            IntentScoreCollectorV1::new(&self.context, values, &mut self.legacy_scores);
        collector.appearance = Some(&mut self.appearance);
        let failures = assess_observed(
            &self.context,
            runtime,
            epoch,
            scenarios,
            values,
            &mut collector,
            arena,
        )? + collector.violations();
        collector.refine_cases();
        drop(collector);
        if probe && failures != 0 {
            if let Some(failures) = probe_cases(
                &self.context,
                runtime,
                epoch,
                scenarios,
                values,
                &mut self.legacy_scores,
                arena,
            )? {
                return Ok(Some(failures));
            }
        }
        if failures == 0 {
            let points = std::mem::take(&mut self.appearance);
            let result = if self.peers.is_some() {
                self.retain_peer_tuple(values, &points)
            } else {
                self.retain_tuple(values, &points)
                    .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)
            };
            self.appearance = points;
            result?;
        }
        Ok(Some(failures))
    }

    fn traversal(&mut self, start: usize, initial: &[Srgb8]) -> Result<Option<TraversalV1>, ()> {
        let mut stack = Vec::new();
        let limit = self.context.domains[start..]
            .iter()
            .try_fold(1_usize, |size, domain| {
                size.checked_add(domain.split_depth() + 1)
            })
            .ok_or(())?;
        stack.try_reserve_exact(limit).map_err(|_| ())?;
        let case = self.context.local_cases.order[0];
        let Some(node) = region_node(
            start,
            self.context.domains[start],
            self.budget,
            self.context.preferred[case * self.context.targets.len() + start],
            &mut self.evidence,
        ) else {
            return Ok(None);
        };
        stack.push(WorkRegionV1 {
            node,
            capacity_upper: 1.0,
        });
        Ok(Some(TraversalV1 {
            values: cloned(initial)?,
            stack,
        }))
    }

    fn cancel_traversal(&mut self, traversal: TraversalV1) -> Result<(), ()> {
        for _ in traversal.stack {
            self.evidence.close_region()?;
        }
        Ok(())
    }

    fn needed_capacity(&mut self) -> Result<Option<usize>, ()> {
        let Some(best) = self.best else {
            return Ok(None);
        };
        while let Some(priority) = self.capacity_queue.peek() {
            let point = priority.point;
            let value = &self.pending[point];
            let next = value
                .capacities
                .iter()
                .flatten()
                .copied()
                .find(|&index| !self.capacities[index].complete);
            if priority.revision != value.revision || next.is_none() {
                self.capacity_queue.pop();
                continue;
            }
            if priority.keys.iter().enumerate().any(|(target, &key)| {
                key.cmp(self.effective_key(value, target, false)) != Ordering::Equal
            }) {
                self.capacity_queue.pop();
                self.queue_point(point)?;
                continue;
            }
            if self.compare_points(point, false, best, true) == Ordering::Greater {
                // All other queued lower keys are no better; leave them ready
                // for a possible improved replacement tuple in this context.
                return Ok(None);
            }
            return Ok(next);
        }
        Ok(None)
    }

    fn prepare_capacity_job(&mut self, capacity: usize) -> Result<Option<usize>, ()> {
        if let Some(job) = self.capacities[capacity].job {
            return Ok(Some(job));
        }
        let initial = cloned(&self.capacities[capacity].witness)?;
        let Some(traversal) = self.traversal(self.capacities[capacity].target, &initial)? else {
            return Ok(None);
        };
        let job = self.jobs.len();
        pushed(&mut self.jobs, Some(traversal))?;
        self.capacities[capacity].job = Some(job);
        Ok(Some(job))
    }

    fn capacity_upper(&self, capacity: usize, bounds: &BoundsV1) -> Option<f64> {
        let capacity = &self.capacities[capacity];
        let points = bounds.appearance.as_ref()?;
        let reference = self.models[capacity.target]?.reference_contrast();
        let count = self.context.targets.len();
        let mut upper = 1.0_f64;
        // The memo signature is complete; the endpoint buffer contains W.
        // A minimum over W can only overestimate the full-support minimum.
        let refinement = self.context.case_refinement.borrow();
        for &case in &refinement.cases {
            let position = refinement.ranks[case];
            let [lower, higher] = points[case * count + capacity.target];
            let backdrop = capacity.backdrops[position];
            if backdrop.bytes().iter().enumerate().any(|(channel, value)| {
                *value < lower.backdrop[channel] || *value > higher.backdrop[channel]
            }) {
                return None;
            }
            let by = luminance(backdrop);
            let lo = luminance(Srgb8::new(lower.visible));
            let hi = luminance(Srgb8::new(higher.visible));
            let contrast = match capacity.signs[position] {
                -1 if lo < by => (by + 0.05) / (lo + 0.05),
                1 if hi > by => (hi + 0.05) / (by + 0.05),
                0 if lo <= by && by <= hi => 1.0,
                _ => return None,
            };
            upper = upper.min((contrast / reference).min(1.0));
        }
        Some(upper)
    }

    fn lower_key(&self, target: usize, bounds: &BoundsV1) -> TargetKeyV1 {
        // Capacity ranges over the full conditional source domain, not just
        // this point-search box. Maxima over W give weaker lower bounds; old
        // prunes remain valid when subsequent full points enlarge W.
        let count = self.context.targets.len();
        let mut key = TargetKeyV1::default();
        let points = bounds
            .appearance
            .as_ref()
            .expect("reference bounds capture");
        let Some(model) = self.models[target] else {
            key.final_distance = bounds.scores[target];
            return key;
        };
        let refinement = self.context.case_refinement.borrow();
        for &case in &refinement.cases {
            let [lower, upper] = points[case * count + target];
            let (lab_lo, lab_hi) =
                crate::spaces::oklab::srgb8_box_oklab_bounds(lower.visible, upper.visible);
            if !self.capacity_enabled(target) {
                let mut preferred = model.reference_lab();
                preferred[0] = self.context.preferred[case * count + target][0];
                key.final_distance = key
                    .final_distance
                    .max(point_box_distance(preferred, lab_lo, lab_hi));
                continue;
            }
            let background = [
                luminance(Srgb8::new(lower.backdrop)),
                luminance(Srgb8::new(upper.backdrop)),
            ];
            let visible = [
                luminance(Srgb8::new(lower.visible)),
                luminance(Srgb8::new(upper.visible)),
            ];
            let mut displacement = f64::INFINITY;
            let mut final_distance = f64::INFINITY;
            let mut mismatch = 1_u8;
            let mut shortfall = 1.0_f64;
            let mut single_case_key: Option<TargetKeyV1> = None;
            for direction in [-1, 0, 1] {
                let possible = match direction {
                    -1 => visible[0] < background[1],
                    1 => visible[1] > background[0],
                    _ => visible[0] <= background[1] && background[0] <= visible[1],
                };
                if !possible {
                    continue;
                }
                // A constant ground has an exact operational virtual
                // point. Outward-widening this constant would make every
                // region artificially better in an earlier lexicographic
                // dimension, preventing all later distance pruning.
                let fixed = (lower.backdrop == upper.backdrop).then(|| {
                    let backdrop = Srgb8::new(lower.backdrop);
                    self.math_cache
                        .get(&(target, backdrop, direction))
                        .copied()
                        .unwrap_or_else(|| model.lab_for(backdrop, from_sign(direction)))
                });
                let target_l = fixed.map_or_else(
                    || model.virtual_lightness_bounds(background, from_sign(direction)),
                    |preferred| [preferred[0]; 2],
                );
                let reference = model.reference_lab();
                let move_l = interval_distance(target_l, [reference[0]; 2]);
                let branch_displacement = fixed.map_or(move_l * move_l, |preferred| {
                    squared_distance(preferred, reference)
                });
                displacement = displacement.min(branch_displacement);
                let dl = interval_distance([lab_lo[0], lab_hi[0]], target_l);
                let da = interval_distance([lab_lo[1], lab_hi[1]], [reference[1]; 2]);
                let db = interval_distance([lab_lo[2], lab_hi[2]], [reference[2]; 2]);
                let branch_distance = (dl * dl + da * da) + db * db;
                final_distance = final_distance.min(branch_distance);
                let branch_mismatch = u8::from(
                    model.reference_contrast() != 1.0
                        && direction != sign(model.reference_polarity()),
                );
                mismatch = mismatch.min(branch_mismatch);
                // Every conditional witness still lives in physical RGB8.
                // This upper bound concerns the full source domain, not this
                // point-search box. It is exact for unconstrained opaque gray
                // on a fixed ground, including unavoidable contrast clipping.
                let physical_capacity = match direction {
                    -1 => (background[1] + 0.05) / 0.05,
                    1 => 1.05 / (background[0] + 0.05),
                    _ => 1.0,
                };
                let branch_shortfall =
                    1.0 - (physical_capacity / model.reference_contrast()).min(1.0);
                shortfall = shortfall.min(branch_shortfall);
                let branch_key = TargetKeyV1 {
                    capacity_shortfall: branch_shortfall,
                    continuous_displacement: branch_displacement,
                    reference_polarity_mismatch: branch_mismatch,
                    final_distance: branch_distance,
                };
                if single_case_key.is_none_or(|value| branch_key.cmp(value) == Ordering::Less) {
                    single_case_key = Some(branch_key);
                }
            }
            if self.context.local_cases.order.len() == 1 {
                return single_case_key.unwrap_or(TargetKeyV1 {
                    capacity_shortfall: 1.0,
                    continuous_displacement: f64::INFINITY,
                    reference_polarity_mismatch: 1,
                    final_distance: f64::INFINITY,
                });
            }
            key.capacity_shortfall = key.capacity_shortfall.max(shortfall);
            key.continuous_displacement = key.continuous_displacement.max(displacement);
            key.reference_polarity_mismatch = key.reference_polarity_mismatch.max(mismatch);
            key.final_distance = key.final_distance.max(final_distance);
        }
        key
    }

    fn region_cannot_improve(&mut self, bounds: &BoundsV1) -> bool {
        debug_assert!(self.peers.is_none(), "joint peers use product regions");
        let Some(best) = self.best else {
            return false;
        };
        let best = &self.pending[best];
        for target in 0..self.context.targets.len() {
            let lower = self.lower_key(target, bounds);
            match lower.cmp(self.effective_key(best, target, true)) {
                Ordering::Greater => return true,
                Ordering::Less => return false,
                Ordering::Equal => {}
            }
        }
        bounds
            .changes_min
            .iter()
            .copied()
            .cmp(
                self.context
                    .targets
                    .iter()
                    .zip(&best.values)
                    .map(|(target, value)| *value != target.declaration.anchor),
            )
            .then_with(|| bounds.tuple_min.cmp(&best.values))
            != Ordering::Less
    }

    fn joint_region(
        &mut self,
        bounds: &mut BoundsV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        domains: &[AssignmentDomainV1],
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let domain_at = |index| domains[self.context.target_representative(index)];
        let possible =
            bounds.may_pass_domains(&self.context, runtime, epoch, scenarios, domain_at)?;
        self.evidence.endpoint_evaluations = bounds.evaluations;
        if !possible {
            return Ok(false);
        }
        let peers = self.peers.as_mut().expect("joint reference component");
        Ok(!peers.lower_profile(&self.context, bounds, domain_at)?)
    }

    // Subdivide the product of source domains, keeping all other domains
    // as intervals. A surface and its dependents can therefore be narrowed
    // together before any one paint is fixed. Both children are disjoint and
    // cover their parent. Branch ranking is the same conservative sorted
    // objective used to reject a region, independent of the incumbent.
    fn joint_search(
        &mut self,
        bounds: &mut BoundsV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        struct Split {
            target: usize,
            parent: AssignmentDomainV1,
            sibling: Option<AssignmentDomainV1>,
        }
        let mut domains = cloned(&self.context.domains).map_err(allocation)?;
        let count = domains.len();
        let depth = domains
            .iter()
            .enumerate()
            .filter(|(index, _)| self.context.represents_target(*index))
            .map(|(_, domain)| domain)
            .try_fold(0_usize, |sum, domain| sum.checked_add(domain.split_depth()))
            .ok_or_else(|| allocation(()))?;
        let mut stack = Vec::<Split>::new();
        stack.try_reserve_exact(depth).map_err(|_| allocation(()))?;
        let mut first_profile =
            buffer(count.checked_mul(4).ok_or_else(|| allocation(()))?, 0.0).map_err(allocation)?;
        let mut values = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
        let maximum_polish_work = ((0..count)
            .filter(|&index| self.context.represents_target(index))
            .count() as u64)
            .saturating_mul(256)
            .min(self.budget / COORDINATE_BUDGET_DIVISOR);
        let mut polish_proposals = None;

        self.evidence.visited += 1;
        self.evidence.regions += 1;
        self.evidence.open_regions += 1;
        let mut active = self.joint_region(bounds, runtime, epoch, scenarios, &domains)?;
        if !active {
            self.evidence.close_region().map_err(allocation)?;
        }
        loop {
            if !active {
                // Undo the preceding split, or activate its still-open sibling.
                // Only the modified domain is stored on the stack.
                while let Some(frame) = stack.last_mut() {
                    if let Some(sibling) = frame.sibling.take() {
                        domains[frame.target] = sibling;
                        // The sibling was already assessed when its split was
                        // created. Its open status is not a feasibility or
                        // optimality claim. Apply new incumbent and case
                        // information to its children (or fully assess a leaf).
                        // Each product region is assessed once, within budget.
                        active = true;
                        break;
                    } else {
                        domains[frame.target] = frame.parent;
                        stack.pop();
                    }
                }
                if !active {
                    debug_assert_eq!(self.evidence.open_regions, 0);
                    self.evidence.completion = CompletionV1::Complete;
                    return Ok(());
                }
            }

            let remaining = self.budget - self.evidence.visited;
            if self.best.is_some() && remaining <= maximum_polish_work {
                if polish_proposals.is_none() {
                    polish_proposals = Some(
                        polish_plan(
                            self.context
                                .targets
                                .iter()
                                .zip(&self.context.domains)
                                .enumerate()
                                .filter(|(index, _)| self.context.represents_target(*index))
                                .map(|(index, (target, &domain))| {
                                    (index, target.declaration.anchor, domain)
                                }),
                            usize::try_from(maximum_polish_work).map_err(|_| allocation(()))?,
                        )
                        .map_err(allocation)?,
                    );
                }
                if remaining <= polish_proposals.as_ref().expect("prepared reserve").len() as u64 {
                    break;
                }
            }
            if remaining == 0 {
                break;
            }
            // Resolve the highest-priority explicit tone domains first, then
            // balance subdivision depth within each objective tier. Leaving
            // a tone unconstrained while refining lower-priority dependent
            // uses spends work on compositions whose tone can be improved.
            let split = domains
                .iter()
                .enumerate()
                .filter(|(index, domain)| {
                    self.context.represents_target(*index) && domain.split_depth() != 0
                })
                .max_by_key(|(index, domain)| {
                    (
                        self.context.targets[*index].tone_input.is_some(),
                        domain.split_depth(),
                        std::cmp::Reverse(*index),
                    )
                });
            let Some((target, &parent)) = split else {
                for (value, domain) in values.iter_mut().zip(&domains) {
                    *value = Srgb8::new(domain.bounds().0);
                }
                self.assess_tuple_inner(runtime, epoch, scenarios, &mut values, arena, true)?;
                self.evidence.close_region().map_err(allocation)?;
                active = false;
                continue;
            };
            if remaining < 2 {
                break;
            }
            self.evidence.visited += 2;
            self.evidence.regions += 2;
            self.evidence.open_regions += 2;
            self.evidence.close_region().map_err(allocation)?;
            let (left, right) = parent.split();
            domains[target] = left;
            let left_possible = self.joint_region(bounds, runtime, epoch, scenarios, &domains)?;
            if left_possible {
                first_profile.copy_from_slice(
                    self.peers
                        .as_ref()
                        .expect("joint reference component")
                        .current_profile(),
                );
            } else {
                self.evidence.close_region().map_err(allocation)?;
            }
            domains[target] = right;
            let right_possible = self.joint_region(bounds, runtime, epoch, scenarios, &domains)?;
            if !right_possible {
                self.evidence.close_region().map_err(allocation)?;
            }
            let (first, sibling) = match (left_possible, right_possible) {
                (false, false) => {
                    domains[target] = parent;
                    active = false;
                    continue;
                }
                (true, false) => (left, None),
                (false, true) => (right, None),
                (true, true) => {
                    let right_profile = self
                        .peers
                        .as_ref()
                        .expect("joint reference component")
                        .current_profile();
                    if peers::compare(&first_profile, right_profile) == Ordering::Greater {
                        (right, Some(left))
                    } else {
                        (left, Some(right))
                    }
                }
            };
            domains[target] = first;
            stack.push(Split {
                target,
                parent,
                sibling,
            });
        }
        // Unvisited regions remain open. Local improvement never changes
        // the evidence for completeness or skips the full acceptance check.
        if let Some(proposals) = polish_proposals.as_deref() {
            self.polish(runtime, epoch, scenarios, arena, proposals)?;
        }
        Ok(())
    }

    fn step(
        &mut self,
        capacity: Option<usize>,
        bounds: &mut BoundsV1,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
    ) -> Result<bool, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        let job_index = if let Some(capacity) = capacity {
            let Some(job) = self.prepare_capacity_job(capacity).map_err(allocation)? else {
                return Ok(false);
            };
            job
        } else {
            0
        };
        let Some(mut job) = self.jobs[job_index].take() else {
            return Ok(false);
        };
        if capacity.is_some_and(|index| self.capacities[index].complete) {
            self.cancel_traversal(job).map_err(allocation)?;
            return Ok(true);
        }
        let Some(mut frame) = job.stack.pop() else {
            return Ok(false);
        };
        let node = frame.node;
        let volume = node
            .domain
            .cardinality()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let mut progressed = true;
        match node.ordinal {
            None => {
                let possible =
                    bounds.may_pass(&self.context, runtime, epoch, scenarios, node, &job.values)?;
                self.evidence.endpoint_evaluations = bounds.evaluations;
                let capacity_bound = capacity.map(|index| self.capacity_upper(index, bounds));
                let rejected = !possible
                    || if let Some(bound) = capacity_bound {
                        bound.is_none_or(|upper| {
                            upper <= self.capacities[capacity.expect("capacity job")].ratio_lower
                        })
                    } else {
                        self.region_cannot_improve(bounds)
                    };
                if rejected {
                    self.evidence.close_region().map_err(allocation)?;
                } else {
                    if let Some(Some(upper)) = capacity_bound {
                        frame.capacity_upper = frame.capacity_upper.min(upper);
                    }
                    if volume > 64 {
                        if self.budget.saturating_sub(self.evidence.visited) < 2 {
                            job.stack.push(frame);
                            progressed = false;
                        } else {
                            let (left, right) = node.domain.split();
                            let case = self.context.local_cases.order[0];
                            let preferred = self.context.preferred
                                [case * self.context.targets.len() + node.depth];
                            let left = region_node(
                                node.depth,
                                left,
                                self.budget,
                                preferred,
                                &mut self.evidence,
                            )
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                            let right = region_node(
                                node.depth,
                                right,
                                self.budget,
                                preferred,
                                &mut self.evidence,
                            )
                            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                            self.evidence.close_region().map_err(allocation)?;
                            let (first, second) = if left.lower_score <= right.lower_score {
                                (left, right)
                            } else {
                                (right, left)
                            };
                            job.stack.push(WorkRegionV1 {
                                node: second,
                                capacity_upper: frame.capacity_upper,
                            });
                            job.stack.push(WorkRegionV1 {
                                node: first,
                                capacity_upper: frame.capacity_upper,
                            });
                        }
                    } else {
                        frame.node.ordinal = Some(0);
                        job.stack.push(frame);
                    }
                }
            }
            Some(ordinal) => {
                let partial = node.depth + 1 < self.context.targets.len();
                let required = if partial { 2 } else { 1 };
                if self.budget.saturating_sub(self.evidence.visited) < required {
                    job.stack.push(frame);
                    progressed = false;
                } else {
                    if ordinal + 1 < volume {
                        frame.node.ordinal = Some(ordinal + 1);
                        job.stack.push(frame);
                    } else {
                        self.evidence.close_region().map_err(allocation)?;
                    }
                    job.values[node.depth] = node.domain.value(ordinal);
                    if partial {
                        self.evidence.visited += 1;
                        self.evidence.points += 1;
                        let depth = node.depth + 1;
                        let case = self.context.local_cases.order[0];
                        let next = region_node(
                            depth,
                            self.context.domains[depth],
                            self.budget,
                            self.context.preferred[case * self.context.targets.len() + depth],
                            &mut self.evidence,
                        )
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                        job.stack.push(WorkRegionV1 {
                            node: next,
                            capacity_upper: frame.capacity_upper,
                        });
                    } else {
                        self.assess_tuple_inner(
                            runtime,
                            epoch,
                            scenarios,
                            &mut job.values,
                            arena,
                            true,
                        )?;
                    }
                }
            }
        }
        if let Some(index) = capacity {
            if self.capacities[index].complete {
                self.cancel_traversal(job).map_err(allocation)?;
                return Ok(progressed);
            }
            let upper = job
                .stack
                .iter()
                .fold(self.capacities[index].ratio_lower, |upper, frame| {
                    upper.max(frame.capacity_upper)
                });
            self.capacities[index].ratio_upper = self.capacities[index]
                .ratio_upper
                .min(upper)
                .max(self.capacities[index].ratio_lower);
            if job.stack.is_empty() {
                self.capacities[index].complete = true;
                self.capacities[index].ratio_upper = self.capacities[index].ratio_lower;
            }
        } else if job.stack.is_empty() {
            self.point_closed = true;
        }
        if !job.stack.is_empty() {
            self.jobs[job_index] = Some(job);
        }
        Ok(progressed)
    }

    fn polish(
        &mut self,
        runtime: &mut ProgramEvaluationRuntimeV1<'_>,
        epoch: &ProgramEpochV1<E>,
        scenarios: NonEmptyScenarioSetV1<'_>,
        arena: &mut ProgramEvaluationArenaV1<E>,
        proposals: &[(usize, Srgb8)],
    ) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
        let Some(best) = self.best else {
            return Ok(());
        };
        let mut values = cloned(&self.pending[best].values)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        // A better early paint can reach the DFS with an almost unsearched
        // suffix. Refine the current composition in priority order, so later
        // uses see the selected earlier paints. Tonal proposals only discover
        // points: full-domain traversal, hard checks and typed keys still own
        // acceptance and completeness, including backward graph dependencies.
        for &(target, source) in proposals {
            if self.evidence.visited >= self.budget {
                return Ok(());
            }
            let best = self.best.expect("polish retains a feasible point");
            values.copy_from_slice(&self.pending[best].values);
            // Visit every admitted proposal, even if an earlier coordinate
            // move has made it the current source. This fixed work set is
            // exactly what the scheduler reserves; each visit is assessed.
            values[target] = source;
            self.assess_tuple_inner(runtime, epoch, scenarios, &mut values, arena, true)?;
        }
        Ok(())
    }

    fn authored_exact(&self) -> bool {
        let Some(best) = self.best else {
            return false;
        };
        if let Some(peers) = &self.peers {
            return peers.authored_exact();
        }
        let point = &self.pending[best];
        point
            .values
            .iter()
            .zip(self.context.targets.iter())
            .all(|(value, target)| *value == target.declaration.anchor)
            && (0..point.keys.len()).all(|target| self.effective_key(point, target, true).is_zero())
    }

    fn reference_reports(
        &self,
        selected: usize,
        target_order: impl IntoIterator<Item = usize>,
    ) -> Result<std::sync::Arc<[ProgramReferenceSelectionV1]>, ()> {
        reports(&self.context, &self.appearance, target_order, |target| {
            Ok(self.pending[selected].capacities[target].map(|index| {
                let capacity = &self.capacities[index];
                ProgramReferenceCapacityV1 {
                    ratio_lower_bits: capacity.ratio_lower.to_bits(),
                    ratio_upper_bits: capacity.ratio_upper.to_bits(),
                    complete: capacity.complete,
                }
            }))
        })
    }
}

fn reports(
    context: &ContextV1<'_>,
    appearance: &[AppearancePointV1],
    target_order: impl IntoIterator<Item = usize>,
    mut capacity: impl FnMut(usize) -> Result<Option<ProgramReferenceCapacityV1>, ()>,
) -> Result<std::sync::Arc<[ProgramReferenceSelectionV1]>, ()> {
    let count = context.targets.len();
    let mut reports = Vec::new();
    reports
        .try_reserve_exact(
            context
                .targets
                .iter()
                .filter(|target| target.reference_backdrop.is_some())
                .count(),
        )
        .map_err(|_| ())?;
    for target in target_order {
        let compiled = &context.targets[target];
        let Some(backdrop) = compiled.reference_backdrop else {
            continue;
        };
        let model = ReferenceTransportV1::new(compiled.effective_anchor(), backdrop);
        let capacity_enabled = compiled.tone_input.is_none();
        let mut cases = Vec::new();
        cases
            .try_reserve_exact(appearance.len() / count)
            .map_err(|_| ())?;
        for case_index in 0..appearance.len() / count {
            let appearance = appearance[case_index * count + target];
            let direction = polarity(appearance.visible, appearance.backdrop);
            let actual_contrast = contrast_ratio(appearance.visible, appearance.backdrop);
            let mut preferred = model.lab_for(appearance.backdrop, direction);
            if !capacity_enabled {
                preferred[0] = compiled.backdrop_curve().map_or_else(
                    || context.preferred[case_index * count + target][0],
                    |curve| curve.preferred_lightness(appearance.backdrop),
                );
            }
            cases.push(ProgramReferenceCaseV1 {
                case_index,
                reference_visible: compiled.effective_anchor(),
                reference_backdrop: compiled.reference_backdrop.ok_or(())?,
                actual_visible: appearance.visible,
                actual_backdrop: appearance.backdrop,
                reference_contrast_bits: model.reference_contrast().to_bits(),
                actual_contrast_bits: actual_contrast.to_bits(),
                polarity: direction,
                strength_loss_bits: (model.reference_log_contrast() - actual_contrast.ln())
                    .max(0.0)
                    .to_bits(),
                virtual_distance_squared_bits: squared_distance(appearance.lab, preferred)
                    .to_bits(),
            });
        }
        let capacity = capacity(target)?;
        reports.push(ProgramReferenceSelectionV1 {
            target_id: compiled.id.value(),
            occurrence_id: compiled
                .declaration
                .intent
                .as_ref()
                .and_then(|intent| intent.appearance)
                .ok_or(())?
                .value(),
            preference: if capacity_enabled {
                ProgramReferencePreferenceV1::ReferenceStrength
            } else {
                ProgramReferencePreferenceV1::ExplicitTone
            },
            capacity,
            cases: cases.into(),
        });
    }
    Ok(reports.into())
}

pub(super) fn reports_for_values<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1<'_>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    capacity: impl FnMut(usize) -> Result<Option<ProgramReferenceCapacityV1>, ()>,
) -> Result<
    std::sync::Arc<[ProgramReferenceSelectionV1]>,
    ProgramSessionEvaluationError<ProgramEvaluatorError<E>>,
> {
    let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut scores = buffer(context.targets.len(), 0.0).map_err(allocation)?;
    let mut appearance =
        buffer(context.preferred.len(), AppearancePointV1::default()).map_err(allocation)?;
    observe_selected(
        context,
        runtime,
        epoch,
        scenarios,
        values,
        &mut scores,
        &mut appearance,
    )?;
    reports(context, &appearance, 0..context.targets.len(), capacity).map_err(allocation)
}

fn observe_selected<E: ProgramConstraintEvaluatorSetV1>(
    context: &ContextV1<'_>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    values: &[Srgb8],
    scores: &mut [f64],
    appearance: &mut [AppearancePointV1],
) -> Result<(), ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    apply_targets::<E>(context.targets, runtime, values)?;
    let mut collector = IntentScoreCollectorV1::new(context, values, scores);
    collector.appearance = Some(appearance);
    observe_intents(context, runtime, epoch, scenarios, &mut collector)?;
    if collector.violations() != 0 {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    Ok(())
}

pub(in crate::program_session::computed_assignment) fn refresh_reports<
    E: ProgramConstraintEvaluatorSetV1,
>(
    selection: &ComputedSelectionV1,
    observation: LocalObservationV1<'_, '_>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    values: &[Srgb8],
    previous: &[ProgramReferenceSelectionV1],
) -> Result<
    std::sync::Arc<[ProgramReferenceSelectionV1]>,
    ProgramSessionEvaluationError<ProgramEvaluatorError<E>>,
> {
    let scenarios = observation.scenarios;
    let context = ContextV1::for_observation::<E>(selection, observation, None)?;
    // Capacity and search completeness describe the same local support. Case
    // indexes and appearances describe this observation and must be rebuilt.
    let mut previous = previous.iter();
    reports_for_values(&context, runtime, epoch, scenarios, values, |target| {
        let report = previous.next().ok_or(())?;
        if report.target_id != context.targets[target].id.value() {
            return Err(());
        }
        Ok(report.capacity)
    })
}

fn interval_distance(left: [f64; 2], right: [f64; 2]) -> f64 {
    (left[0] - right[1]).max(right[0] - left[1]).max(0.0)
}

fn point_box_distance(point: [f64; 3], lower: [f64; 3], upper: [f64; 3]) -> f64 {
    let d = std::array::from_fn::<_, 3, _>(|axis| {
        interval_distance([lower[axis], upper[axis]], [point[axis]; 2])
    });
    (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]
}

#[allow(clippy::too_many_arguments)]
pub(in crate::program_session::computed_assignment) fn select_component<
    E: ProgramConstraintEvaluatorSetV1,
>(
    selection: &ComputedSelectionV1,
    observation: LocalObservationV1<'_, '_>,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    prepared: Option<&mut PreparedPointSelectionV1<'_, '_, E>>,
) -> Result<SearchResultV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    let component = observation.component;
    let scenarios = observation.scenarios;
    let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
    let mut order = if let Some(graph) = &component.peer_graph {
        graph
            .order(&observation.cases.order, scenarios)
            .map_err(allocation)?
    } else {
        None
    };
    if let Some(order) = order
        .as_mut()
        .filter(|_| component.point_equivalence.is_some())
    {
        order
            .constraints
            .retain(|index| component.point_constraints().contains(index));
    }
    let mut search = ReferenceRuntimeV1::new(selection, observation, order.as_ref(), prepared)?;
    let count = search.context.targets.len();
    let mut authored = buffer(count, Srgb8::new([0; 3])).map_err(allocation)?;
    for (value, target) in authored.iter_mut().zip(search.context.targets.iter()) {
        *value = target.declaration.anchor;
    }
    let mut working = cloned(&authored).map_err(allocation)?;
    search.assess_tuple(runtime, epoch, scenarios, &mut working, arena)?;
    if let Some(peers) = &mut search.peers {
        peers
            .prepare(&search.context, &search.appearance)
            .map_err(allocation)?;
    }
    let mut current_scores = cloned(&search.legacy_scores).map_err(allocation)?;
    if !search.authored_exact() && search.evidence.visited < search.budget {
        let seed_limit = (search.budget / COORDINATE_BUDGET_DIVISOR).max(search.evidence.visited);
        let initial_appearance = cloned(&search.appearance).map_err(allocation)?;
        let mut seed_values = cloned(&authored).map_err(allocation)?;
        let mut source_tones = buffer(count, Vec::new()).map_err(allocation)?;
        let mut proposal = cloned(&working).map_err(allocation)?;
        if search.best.is_none() && count >= 2 {
            let targets = search.context.targets;
            let proposals = search
                .context
                .constraint_indices
                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                .iter()
                .filter_map(|&index| paired_tones(targets, &epoch.constraints[index]))
                .flatten();
            for (_, [(left, first), (right, second)]) in
                (search.evidence.visited..seed_limit).zip(proposals)
            {
                proposal.copy_from_slice(&authored);
                proposal[left] = first;
                proposal[right] = second;
                search.assess_tuple(runtime, epoch, scenarios, &mut proposal, arena)?;
            }
        }
        let mut current_hard_failures =
            buffer(search.context.probe.borrow().cells.len(), false).map_err(allocation)?;
        let mut proposal_hard_failures =
            buffer(search.context.probe.borrow().cells.len(), false).map_err(allocation)?;
        let mut proposal_scores = buffer(count, 0.0).map_err(allocation)?;
        let mut repair_order = Vec::new();
        repair_order
            .try_reserve_exact(count)
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        for (target, compiled) in search.context.targets.iter().enumerate() {
            if !search.context.represents_target(target) {
                continue;
            }
            let depth = compiled
                .appearance
                .map(|slot| {
                    epoch
                        .graph
                        .occurrence_dependency_depth(slot)
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)
                })
                .transpose()?
                .unwrap_or(usize::MAX);
            repair_order.push((depth, target));
        }
        repair_order.sort_unstable();
        let mut opposite_siblings = cloned(&repair_order).map_err(allocation)?;
        let moving_tone = search
            .context
            .targets
            .iter()
            .any(|target| target.backdrop_curve().is_some());
        if moving_tone {
            // Build one proposal along the physical dependencies. When the
            // reference polarity clips, try its attainable opposite locally.
            // The candidate still goes through the joint objective and all constraints.
            let case = search.context.local_cases.order[0];
            proposal.copy_from_slice(&authored);
            apply_targets::<E>(search.context.targets, runtime, &proposal)?;
            for &(_, target) in &repair_order {
                let compiled = &search.context.targets[target];
                let (backdrop, alpha) = physical_context(
                    compiled,
                    search.context.physical,
                    runtime,
                    epoch,
                    scenarios,
                    case,
                )?;
                let backdrop = Srgb8::new(backdrop);
                let desired = if let Some(curve) = compiled.backdrop_curve() {
                    curve.preferred_color(compiled.effective_anchor(), backdrop)
                } else if let Some(model) =
                    search.models[target].filter(|_| compiled.tone_input.is_none())
                {
                    let side = model.reference_polarity();
                    let mut aim = model.target_for(backdrop, side);
                    let other = model.target_for(backdrop, from_sign(-sign(side)));
                    if (other.raw_luminance - other.clipped_luminance).abs()
                        < (aim.raw_luminance - aim.clipped_luminance).abs()
                    {
                        aim = other;
                    }
                    color_at_lightness(compiled.effective_anchor(), aim.lab[0].clamp(0.0, 1.0))
                } else {
                    search.context.preferred_rgb[case * count + target]
                };
                let source = inverse_hint(desired, backdrop.bytes(), alpha);
                proposal[target] = source;
                runtime
                    .bindings
                    .overwrite_paint_input_at(
                        compiled.binding,
                        EncodedPointPaintValueV1::opaque(source),
                    )
                    .map_err(map_program_execution_binding_error)?;
            }
            search.assess_tuple(runtime, epoch, scenarios, &mut proposal, arena)?;
        }
        for siblings in opposite_siblings.chunk_by_mut(|left, right| left.0 == right.0) {
            siblings.reverse();
        }
        let sibling_orders = 1 + usize::from(opposite_siblings != repair_order);
        let coordinates = repair_order.len();
        let depth_orders = 1 + usize::from(repair_order[0].0 != repair_order[coordinates - 1].0);
        let trajectories = sibling_orders * depth_orders;
        let starts = search
            .context
            .local_cases
            .order
            .len()
            .checked_mul(2)
            .and_then(|count| count.checked_add(1))
            .and_then(|seeds| seeds.checked_mul(trajectories))
            .ok_or(ProgramSessionEvaluationError::ResourceExhausted)?;
        // Each branch gets its own repair trajectory. A jointly infeasible
        // branch can be close to a feasible composition even when no single
        // coordinate move from the authored paints improves the failure count.
        // Reverse dependency depth and sibling order independently. An upstream
        // move can satisfy both a backdrop and its authored foreground. Other
        // compositions need to repair a downstream relation first. On a chain
        // only depth varies; on a single level only sibling order varies.
        // All starts share the existing quarter-budget; unused work passes on.
        for start in 0..starts {
            if search.evidence.visited >= seed_limit {
                break;
            }
            let limit = search.evidence.visited
                + (seed_limit - search.evidence.visited) / (starts - start) as u64;
            let seed = start / trajectories;
            let trajectory = start % trajectories;
            if seed > 0 && trajectory == 0 {
                let case = search.context.local_cases.order[(seed - 1) / 2];
                let direction = if seed % 2 == 1 { -1 } else { 1 };
                for target in 0..count {
                    if !search.context.represents_target(target) {
                        continue;
                    }
                    let point = initial_appearance[case * count + target];
                    seed_values[target] = if search.models[target].is_some() {
                        let preferred = search
                            .virtual_lab(target, case, point.backdrop, direction)
                            .map_err(allocation)?;
                        search
                            .source_hint(target, point, preferred, &mut source_tones[target])
                            .map_err(allocation)?
                    } else {
                        inverse_hint(
                            search.context.preferred_rgb[case * count + target],
                            point.backdrop.bytes(),
                            point.alpha,
                        )
                    };
                }
            }
            working.copy_from_slice(&seed_values);
            let mut current_failures = search
                .assess_tuple_inner(runtime, epoch, scenarios, &mut working, arena, true)?
                .unwrap_or(usize::MAX);
            if current_failures == 0 {
                continue;
            }
            current_scores.copy_from_slice(&search.legacy_scores);
            for (failed, cell) in current_hard_failures
                .iter_mut()
                .zip(&search.context.probe.borrow().cells)
            {
                *failed = cell.violation;
            }
            // The legacy distance only guides repair. Every hard-valid point
            // is retained and compared by the unchanged typed reference key.
            let order = if trajectory % sibling_orders == 0 {
                &repair_order
            } else {
                &opposite_siblings
            };
            let downstream_first = trajectory < sibling_orders;
            'repair: for pass in 0.. {
                let mut changed = false;
                for position in 0..coordinates {
                    let coordinate = if (pass % 2 == 0) == downstream_first {
                        coordinates - 1 - position
                    } else {
                        position
                    };
                    let target = order[coordinate].1;
                    if search.context.targets[target].declaration.intent.is_none() {
                        continue;
                    }
                    let anchor = search.context.targets[target].declaration.anchor;
                    let mut previous = None;
                    for step in 0..=u8::MAX {
                        if search.evidence.visited >= limit {
                            break 'repair;
                        }
                        let source = source_tone_step(&mut source_tones[target], anchor, step)
                            .map_err(allocation)?;
                        if previous == Some(source) {
                            continue;
                        }
                        previous = Some(source);
                        if source == working[target] {
                            continue;
                        }
                        proposal.copy_from_slice(&working);
                        proposal[target] = source;
                        if !search.context.domains[target].contains(proposal[target]) {
                            continue;
                        }
                        let case_count = search.context.case_refinement.borrow().cases.len();
                        let Some(failures) = search.assess_tuple_inner(
                            runtime,
                            epoch,
                            scenarios,
                            &mut proposal,
                            arena,
                            true,
                        )?
                        else {
                            break 'repair;
                        };
                        if failures == 0 {
                            break 'repair;
                        }
                        proposal_scores.copy_from_slice(&search.legacy_scores);
                        let proposal_cell_count = search.context.probe.borrow().cells.len();
                        for (failed, cell) in proposal_hard_failures
                            .iter_mut()
                            .zip(&search.context.probe.borrow().cells)
                        {
                            *failed = cell.violation;
                        }
                        let refined_case_count =
                            search.context.case_refinement.borrow().cases.len();
                        if case_count != refined_case_count {
                            if search.evidence.visited >= limit {
                                break 'repair;
                            }
                            current_failures = search
                                .assess_tuple_inner(
                                    runtime,
                                    epoch,
                                    scenarios,
                                    &mut working,
                                    arena,
                                    true,
                                )?
                                .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                            // Current already failed on the old probe, a subset
                            // of the enlarged one. Unless that set is complete,
                            // its retained violation makes probe_cases return
                            // immediately. A complete set cannot grow either.
                            debug_assert_ne!(current_failures, 0);
                            debug_assert_eq!(
                                search.context.case_refinement.borrow().cases.len(),
                                refined_case_count
                            );
                            current_scores.copy_from_slice(&search.legacy_scores);
                            for (failed, cell) in current_hard_failures
                                .iter_mut()
                                .zip(&search.context.probe.borrow().cells)
                            {
                                *failed = cell.violation;
                            }
                        }
                        // With equal failure counts, exchanging a violated
                        // relation for a violated readability requirement is not
                        // progress toward feasibility. Keep satisfied hard cells
                        // while the legacy preference guides movement on a plateau.
                        if failures < current_failures
                            || failures == current_failures
                                && proposal_hard_failures[..proposal_cell_count]
                                    .iter()
                                    .zip(&current_hard_failures)
                                    .all(|(&failed, &was_failed)| !failed || was_failed)
                                && compare_scores(&proposal_scores, &current_scores)
                                    == Ordering::Less
                        {
                            working.copy_from_slice(&proposal);
                            changed = true;
                            current_failures = failures;
                            current_scores.copy_from_slice(&proposal_scores);
                            current_hard_failures.copy_from_slice(&proposal_hard_failures);
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }
    }
    if search.authored_exact() {
        search.evidence.completion = CompletionV1::AuthoredExact;
    } else if search.evidence.visited < search.budget {
        let constraints =
            region_constraints(epoch, search.context.constraint_indices).map_err(allocation)?;
        let mut bounds = BoundsV1 {
            constraint_lower: buffer(constraints.len(), None).map_err(allocation)?,
            constraints,
            lower: buffer(count, [0; 3]).map_err(allocation)?,
            scores: buffer(count, 0.0).map_err(allocation)?,
            changes_min: buffer(count, false).map_err(allocation)?,
            tuple_min: buffer(count, Srgb8::new([0; 3])).map_err(allocation)?,
            evaluations: 0,
            appearance: Some(
                buffer(search.appearance.len(), [RegionPointV1::default(); 2])
                    .map_err(allocation)?,
            ),
        };
        if search.peers.is_some() {
            search.joint_search(&mut bounds, runtime, epoch, scenarios, arena)?;
        } else {
            let initial = if let Some(best) = search.best {
                cloned(&search.pending[best].values)
            } else {
                cloned(&authored)
            }
            .map_err(allocation)?;
            let point_job = search.traversal(0, &initial).map_err(allocation)?;
            pushed(&mut search.jobs, point_job).map_err(allocation)?;
            // A capacity query and the outer search advance in the same scheduler
            // and spend the same node budget. A short quantum prevents an unknown
            // context from blocking discovery of a better feasible composition.
            let mut capacity_steps = 0_u8;
            let maximum_polish_work = if count > 1 {
                (count as u64)
                    .saturating_mul(256)
                    .min(search.budget / COORDINATE_BUDGET_DIVISOR)
            } else {
                0
            };
            let mut polish_proposals = None;
            while search.evidence.visited < search.budget {
                if maximum_polish_work != 0
                    && search.best.is_some()
                    && search.budget - search.evidence.visited <= maximum_polish_work
                {
                    if search.point_closed
                        && search.needed_capacity().map_err(allocation)?.is_none()
                    {
                        break;
                    }
                    // Most complete searches never reach this reserve. When one
                    // does, cache only the executable prefix, at most a quarter
                    // of the shared budget. Counting and execution reuse the RGBs.
                    if polish_proposals.is_none() {
                        polish_proposals = Some(
                            polish_plan(
                                search
                                    .context
                                    .targets
                                    .iter()
                                    .zip(&search.context.domains)
                                    .enumerate()
                                    .map(|(index, (target, &domain))| {
                                        (index, target.declaration.anchor, domain)
                                    }),
                                usize::try_from(maximum_polish_work).map_err(|_| allocation(()))?,
                            )
                            .map_err(allocation)?,
                        );
                    }
                    if search.budget - search.evidence.visited
                        <= polish_proposals.as_ref().expect("prepared reserve").len() as u64
                    {
                        break;
                    }
                }
                let needed = search.needed_capacity().map_err(allocation)?;
                let run_point = !search.point_closed && (needed.is_none() || capacity_steps == 32);
                let mut progressed = if run_point {
                    capacity_steps = 0;
                    search.step(None, &mut bounds, runtime, epoch, scenarios, arena)?
                } else if let Some(capacity) = needed {
                    capacity_steps = capacity_steps.saturating_add(1);
                    search.step(
                        Some(capacity),
                        &mut bounds,
                        runtime,
                        epoch,
                        scenarios,
                        arena,
                    )?
                } else {
                    break;
                };
                if !progressed {
                    // Splitting a region or extending a partial tuple needs two
                    // visits. The other traversal may still evaluate a leaf with
                    // the final visit, and that tuple may replace the current best.
                    if run_point {
                        if let Some(capacity) = needed {
                            capacity_steps = capacity_steps.saturating_add(1);
                            progressed = search.step(
                                Some(capacity),
                                &mut bounds,
                                runtime,
                                epoch,
                                scenarios,
                                arena,
                            )?;
                        }
                    } else if !search.point_closed {
                        capacity_steps = 0;
                        progressed =
                            search.step(None, &mut bounds, runtime, epoch, scenarios, arena)?;
                    }
                }
                if !progressed {
                    break;
                }
            }
            if search.point_closed && search.needed_capacity().map_err(allocation)?.is_none() {
                for index in 0..search.jobs.len() {
                    if let Some(job) = search.jobs[index].take() {
                        search.cancel_traversal(job).map_err(allocation)?;
                    }
                }
                if search.evidence.open_regions != 0 {
                    return Err(ProgramSessionEvaluationError::InternalInvariant);
                }
                search.evidence.completion = CompletionV1::Complete;
            } else if let Some(proposals) = polish_proposals.as_deref().filter(|proposals| {
                !proposals.is_empty() && search.evidence.visited < search.budget
            }) {
                // Do this after the last domain-search step. Resuming DFS could
                // accept another earlier paint with a still-unsearched suffix and
                // undo the improvement. The exact admitted-proposal count reserves
                // only executable work; the pass consumes the remaining visits.
                // Open regions still keep completeness explicitly unfinished.
                search.polish(runtime, epoch, scenarios, arena, proposals)?;
            }
        }
    }
    search.evidence.numerical_order_unresolved = search.evidence.completion
        != CompletionV1::AuthoredExact
        && search.peers.as_ref().is_some_and(PeerProfiles::unresolved);
    let (values, reference_selections) = if let Some(best) = search.best {
        let mut values = cloned(&search.pending[best].values).map_err(allocation)?;
        // Pending tuples need their typed keys and source paints, not a copy
        // of every physical case. Recompose only the selected report, without
        // running another search or another constraint evaluator. The caller
        // still performs the normal full-tuple validation before publication.
        observe_selected(
            &search.context,
            runtime,
            epoch,
            scenarios,
            &values,
            &mut search.legacy_scores,
            &mut search.appearance,
        )?;
        // Snapshot storage and history use the compiled component order.
        // Project both the full tuple and the subset of reference reports from
        // the same mapping, before filtering out non-reference targets.
        let local_order = if let Some(order) = order.as_ref() {
            let mut local_order = buffer(count, 0).map_err(allocation)?;
            for (local, ordinal) in order.targets.iter().enumerate() {
                let dst = component
                    .targets
                    .binary_search(ordinal)
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                local_order[dst] = local;
                values[dst] = search.pending[best].values[local];
            }
            Some(local_order)
        } else {
            None
        };
        let target_order =
            (0..count).map(|target| local_order.as_ref().map_or(target, |order| order[target]));
        let reports = search
            .reference_reports(best, target_order)
            .map_err(allocation)?;
        (Some(values), Some(reports))
    } else {
        (None, None)
    };
    Ok(SearchResultV1 {
        evidence: search.evidence,
        values,
        reference_selections,
    })
}
