//! Authored uses compare each criterion across the whole tuple.
//! A backdrop can depend on another paint in that same tuple.

use super::super::point_search::{FeasibleIncumbentV1, measured, profile_order};
use super::super::reference_completion::{
    PointMeasurementPrecisionV1, PointMeasurementV1, PreparedPointSelectionV1,
};
use super::*;
use crate::certified_numeric::{CertifiedScalarV1, sort_descending_profile};
use crate::composition::source_over_channel_srgb8;

#[derive(Clone, Copy)]
struct Direction {
    physical_shortfall: f64,
    preferred: [f64; 3],
}

struct UseCase {
    directions: [Option<Direction>; 3],
    backdrop: Srgb8,
    alpha: f64,
}

// Monotonic encoded source-over and luminance enclose every physical paint.
// The ratio uses independent numerator/denominator extrema when the backdrop
// varies. A fixed backdrop recovers the exact operational ceiling.
fn physical_ceilings(domain: AssignmentDomainV1, alpha: f64, backdrops: [Srgb8; 2]) -> [f64; 3] {
    // At zero opacity the subject and backdrop are the same physical paint.
    if alpha == 0.0 {
        return [1.0; 3];
    }
    let (low, high) = domain.bounds();
    let visible_y = |source: [u8; 3], backdrop: Srgb8| {
        luminance(Srgb8::new(std::array::from_fn(|i| {
            source_over_channel_srgb8(source[i], alpha, backdrop.bytes()[i])
        })))
    };
    [
        (luminance(backdrops[1]) + 0.05) / (visible_y(low, backdrops[0]) + 0.05),
        1.0,
        (visible_y(high, backdrops[1]) + 0.05) / (luminance(backdrops[0]) + 0.05),
    ]
}

impl UseCase {
    fn new(backdrop: Srgb8, alpha: f64) -> Self {
        Self {
            directions: [None; 3],
            backdrop,
            alpha,
        }
    }

    fn direction_at(
        &mut self,
        domain: AssignmentDomainV1,
        model: ReferenceTransportV1,
        index: usize,
    ) -> Direction {
        let backdrop = self.backdrop;
        let alpha = self.alpha;
        *self.directions[index].get_or_insert_with(|| {
            let ceiling = physical_ceilings(domain, alpha, [backdrop; 2])[index];
            Direction {
                physical_shortfall: 1.0 - (ceiling / model.reference_contrast()).min(1.0),
                preferred: model.lab_for(backdrop, from_sign(index as i8 - 1)),
            }
        })
    }

    fn transport_loss(
        &mut self,
        domain: AssignmentDomainV1,
        model: ReferenceTransportV1,
        point: AppearancePointV1,
    ) -> [f64; 2] {
        if self.backdrop != point.backdrop || self.alpha != point.alpha {
            *self = Self::new(point.backdrop, point.alpha);
        }
        let sign = if model.reference_contrast() == 1.0 {
            0
        } else {
            sign(polarity(point.visible, point.backdrop))
        };
        let direction = self.direction_at(domain, model, (sign + 1) as usize);
        [
            direction.physical_shortfall,
            squared_distance(point.lab, direction.preferred),
        ]
    }

    fn lower_bounds(
        &mut self,
        context: &ContextV1<'_>,
        model: ReferenceTransportV1,
        target: usize,
        [lower, upper]: [RegionPointV1; 2],
    ) -> [f64; 3] {
        let (lab_lo, lab_hi) =
            crate::spaces::oklab::srgb8_box_oklab_bounds(lower.visible, upper.visible);
        let backdrops = [Srgb8::new(lower.backdrop), Srgb8::new(upper.backdrop)];
        let fixed = backdrops[0] == backdrops[1];
        if fixed && self.backdrop != backdrops[0] {
            *self = Self::new(backdrops[0], self.alpha);
        }
        let background = backdrops.map(luminance);
        let visible = [Srgb8::new(lower.visible), Srgb8::new(upper.visible)].map(luminance);
        let domain = context.domains[target];
        let ceilings = (!fixed).then(|| physical_ceilings(domain, self.alpha, backdrops));
        let reference = model.reference_lab();
        let mut shortfall = 1.0_f64;
        let mut distance = f64::INFINITY;
        for index in 0..3 {
            let direction = index as i8 - 1;
            let possible = if model.reference_contrast() == 1.0 || self.alpha == 0.0 {
                direction == 0
            } else {
                match direction {
                    -1 => visible[0] < background[1],
                    1 => visible[1] > background[0],
                    _ => visible[0] <= background[1] && background[0] <= visible[1],
                }
            };
            if !possible {
                continue;
            }
            let (branch_shortfall, target_l) = if let Some(ceilings) = ceilings {
                (
                    1.0 - (ceilings[index] / model.reference_contrast()).min(1.0),
                    model.virtual_lightness_bounds(background, from_sign(direction)),
                )
            } else {
                let branch = self.direction_at(domain, model, index);
                (branch.physical_shortfall, [branch.preferred[0]; 2])
            };
            let dl = interval_distance([lab_lo[0], lab_hi[0]], target_l);
            let da = interval_distance([lab_lo[1], lab_hi[1]], [reference[1]; 2]);
            let db = interval_distance([lab_lo[2], lab_hi[2]], [reference[2]; 2]);
            shortfall = shortfall.min(branch_shortfall);
            distance = distance.min((dl * dl + da * da) + db * db);
        }
        [0.0, shortfall, distance]
    }
}

#[derive(Clone, Copy)]
enum Preference {
    Point,
    Tone,
    Transport {
        model: ReferenceTransportV1,
        case_start: usize,
    },
}

impl Preference {
    fn lower_loss(
        self,
        cases: &mut [UseCase],
        context: &ContextV1<'_>,
        target: usize,
        case: usize,
        row: usize,
        points: [RegionPointV1; 2],
    ) -> [f64; 3] {
        match self {
            Self::Transport { model, case_start } => {
                cases[case_start + row].lower_bounds(context, model, target, points)
            }
            Self::Point | Self::Tone => {
                let preferred = context.preferred[case * context.targets.len() + target];
                let [lower, upper] = points;
                let (lo, hi) =
                    crate::spaces::oklab::srgb8_box_oklab_bounds(lower.visible, upper.visible);
                let distance = point_box_distance(preferred, lo, hi);
                match self {
                    Self::Point => [0.0, 0.0, distance],
                    Self::Tone => [distance, 0.0, 0.0],
                    Self::Transport { .. } => unreachable!("point preference"),
                }
            }
        }
    }
}

type EvaluationError<E> = ProgramSessionEvaluationError<ProgramEvaluatorError<E>>;
type SourceBounds = ([u8; 3], [u8; 3]);

fn zero() -> CertifiedScalarV1 {
    CertifiedScalarV1::exact(0.0).expect("zero is finite")
}

struct ContinuousProfiles<'a, 'p, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1> {
    component: &'a ComputedComponentV1,
    prepared: &'p mut PreparedPointSelectionV1<'program, 'scratch, E>,
    compiled_to_observed: Vec<usize>,
    observed_to_compiled: Vec<usize>,
    point: Vec<CertifiedScalarV1>,
    source: Vec<CertifiedScalarV1>,
}

impl<E: ProgramConstraintEvaluatorSetV1> ContinuousProfiles<'_, '_, '_, '_, E> {
    fn measure(
        &mut self,
        context: &ContextV1<'_>,
        domain_at: impl Fn(usize) -> SourceBounds,
        profile: &mut [CertifiedScalarV1],
        precision: PointMeasurementPrecisionV1,
    ) -> Result<bool, EvaluationError<E>> {
        let count = context.targets.len();
        let measured = measured::<E>(self.prepared.measure_point_domains(
            self.component,
            |local| {
                let (lo, hi) = domain_at(self.compiled_to_observed[local]);
                (lo.map(f64::from), hi.map(f64::from))
            },
            &context.local_cases.order,
            &mut self.point,
            &mut self.source,
            PointMeasurementV1::Profile(precision),
        ))?;
        if measured {
            for variable in 0..self.component.point_variable_count() {
                let local = self.component.point_variable(variable);
                let target = self.compiled_to_observed[local];
                match context.targets[target].preferred_intent() {
                    CompiledPreferredIntentV1::Point { tone: Some(_), .. } => {
                        profile[target] = self.point[variable];
                    }
                    CompiledPreferredIntentV1::Point { tone: None, .. } => {
                        profile[2 * count + target] = self.point[variable];
                    }
                    CompiledPreferredIntentV1::ImplicitReference => {}
                }
                profile[3 * count + target] = self.source[variable];
            }
        }
        Ok(measured)
    }

    fn same_expression(&self, observed: usize, equal: impl Fn(usize) -> bool) -> bool {
        self.prepared.same_preference_expression(
            self.component,
            self.observed_to_compiled[observed],
            |local| equal(self.compiled_to_observed[local]),
        )
    }
}

pub(super) struct PeerProfiles<'a, 'p, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1> {
    continuous: ContinuousProfiles<'a, 'p, 'program, 'scratch, E>,
    preferences: Vec<(usize, Preference)>,
    cases: Vec<UseCase>,
    // Machine lower scores rank traversal children only.
    current: Vec<f64>,
    candidate: Vec<CertifiedScalarV1>,
    best: Vec<CertifiedScalarV1>,
    best_values: Vec<Srgb8>,
    left: Vec<CertifiedScalarV1>,
    right: Vec<CertifiedScalarV1>,
    sort: Vec<CertifiedScalarV1>,
    incumbent: FeasibleIncumbentV1,
    authored_exact: bool,
}

impl<'a, 'p, 'program, 'scratch, E: ProgramConstraintEvaluatorSetV1>
    PeerProfiles<'a, 'p, 'program, 'scratch, E>
{
    pub(super) fn new(
        context: &ContextV1<'_>,
        component: &'a ComputedComponentV1,
        prepared: &'p mut PreparedPointSelectionV1<'program, 'scratch, E>,
    ) -> Result<Self, EvaluationError<E>> {
        let allocation = |_| ProgramSessionEvaluationError::ResourceExhausted;
        let count = context.targets.len();
        let length = count.checked_mul(4).ok_or_else(|| allocation(()))?;
        let mut preferences = Vec::new();
        preferences
            .try_reserve_exact(count)
            .map_err(|_| allocation(()))?;
        let mut case_slots = 0_usize;
        for (index, target) in context.targets.iter().enumerate() {
            if !context.represents_target(index) {
                continue;
            }
            let preference = match target.preferred_intent() {
                CompiledPreferredIntentV1::Point { tone: None, .. } => Preference::Point,
                CompiledPreferredIntentV1::Point { tone: Some(_), .. } => Preference::Tone,
                CompiledPreferredIntentV1::ImplicitReference => {
                    let case_start = case_slots;
                    case_slots = case_slots
                        .checked_add(context.local_cases.order.len())
                        .ok_or_else(|| allocation(()))?;
                    let backdrop = target
                        .reference_backdrop
                        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
                    Preference::Transport {
                        model: ReferenceTransportV1::new(target.effective_anchor(), backdrop),
                        case_start,
                    }
                }
            };
            preferences.push((index, preference));
        }
        let mut cases = Vec::new();
        cases
            .try_reserve_exact(case_slots)
            .map_err(|_| allocation(()))?;
        let mut compiled_to_observed = buffer(count, usize::MAX).map_err(allocation)?;
        let mut observed_to_compiled = buffer(count, usize::MAX).map_err(allocation)?;
        let observed = context
            .targets
            .indices
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        for (index, &target) in observed.iter().enumerate() {
            let local = component
                .targets
                .binary_search(&target)
                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
            compiled_to_observed[local] = index;
            observed_to_compiled[index] = local;
        }
        prepared.refresh_point_component(component, &context.local_cases.order)
            .map_err(|error| match error {
                super::super::reference_completion::ReferenceCompletionErrorV1::ResourceExhausted => ProgramSessionEvaluationError::ResourceExhausted,
                _ => ProgramSessionEvaluationError::InternalInvariant,
            })?;
        Ok(Self {
            continuous: ContinuousProfiles {
                component,
                prepared,
                compiled_to_observed,
                observed_to_compiled,
                point: Vec::new(),
                source: Vec::new(),
            },
            preferences,
            cases,
            current: buffer(length, 0.0).map_err(allocation)?,
            candidate: buffer(length, zero()).map_err(allocation)?,
            best: buffer(length, zero()).map_err(allocation)?,
            best_values: buffer(count, Srgb8::new([0; 3])).map_err(allocation)?,
            left: buffer(count, zero()).map_err(allocation)?,
            right: buffer(count, zero()).map_err(allocation)?,
            sort: buffer(count, zero()).map_err(allocation)?,
            incumbent: FeasibleIncumbentV1::default(),
            authored_exact: false,
        })
    }

    pub(super) fn authored_exact(&self) -> bool {
        self.authored_exact
    }

    pub(super) fn unresolved(&self) -> bool {
        self.incumbent.unresolved()
    }

    pub(super) fn prepare(&mut self, context: &ContextV1<'_>, points: &[AppearancePointV1]) {
        let count = context.targets.len();
        let local_cases = &context.local_cases.order;
        if self.cases.is_empty() {
            for &(target, preference) in &self.preferences {
                if let Preference::Transport { case_start, .. } = preference {
                    debug_assert_eq!(case_start, self.cases.len());
                    for &case in local_cases {
                        self.cases.push(UseCase::new(
                            points[case * count + target].backdrop,
                            points[case * count + target].alpha,
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn consider(
        &mut self,
        context: &ContextV1<'_>,
        points: &[AppearancePointV1],
        values: &[Srgb8],
    ) -> Result<bool, EvaluationError<E>> {
        let count = values.len();
        let local_cases = &context.local_cases.order;
        self.prepare(context, points);
        self.candidate.fill(zero());
        let mut authored_transport_minimum = true;
        for &(target, preference) in &self.preferences {
            let Preference::Transport { model, case_start } = preference else {
                continue;
            };
            for (prepared, &case) in self.cases[case_start..].iter_mut().zip(local_cases) {
                let point = points[case * count + target];
                let [shortfall, distance] =
                    prepared.transport_loss(context.domains[target], model, point);
                for (index, loss) in [(count + target, shortfall), (2 * count + target, distance)] {
                    // Reference transport is an explicitly operational RGB8
                    // objective. Lift its binary64 value exactly, never a
                    // rounded proxy for a continuous Point or Tone expression.
                    self.candidate[index] = self.candidate[index]
                        .max(
                            CertifiedScalarV1::exact(loss)
                                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?,
                        )
                        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                    authored_transport_minimum &= loss == 0.0;
                }
            }
        }
        let domain_at = |index: usize| {
            let rgb = values[index].bytes();
            (rgb, rgb)
        };
        let (measured, precision, order) = self.assess_profile(context, domain_at)?;
        let improved = self.incumbent.consider(
            measured,
            precision,
            order,
            values.cmp(self.best_values.as_slice()),
        );
        if improved {
            std::mem::swap(&mut self.best, &mut self.candidate);
            self.best_values.copy_from_slice(values);
            self.authored_exact = false;
            if authored_transport_minimum
                && values
                    .iter()
                    .zip(context.targets.iter())
                    .all(|(&value, target)| value == target.declaration.anchor)
            {
                self.authored_exact = match self
                    .continuous
                    .prepared
                    .authored_continuous_terms_minimal(self.continuous.component, local_cases)
                {
                    Ok(proven) => proven,
                    Err(error) => {
                        super::super::point_search::measured::<E>(Err(error))?;
                        false
                    }
                };
            }
        }
        Ok(improved)
    }

    fn compare_profile(
        &mut self,
        context: &ContextV1<'_>,
        domain_at: impl Fn(usize) -> SourceBounds,
    ) -> Option<Ordering> {
        let count = context.targets.len();
        let equal = |index: usize| {
            let (lo, hi) = domain_at(index);
            lo == hi && lo == self.best_values[index].bytes()
        };
        for tier in 0..4 {
            let mut used = 0;
            for &(target, _) in &self.preferences {
                let same = if tier == 3 {
                    equal(target)
                } else {
                    self.continuous.same_expression(target, equal)
                };
                if same {
                    continue;
                }
                let index = tier * count + target;
                self.left[used] = self.candidate[index];
                self.right[used] = self.best[index];
                used += 1;
            }
            let left = &mut self.left[..used];
            let right = &mut self.right[..used];
            sort_descending_profile(left, &mut self.sort).ok()?;
            sort_descending_profile(right, &mut self.sort).ok()?;
            match profile_order(left, right)? {
                Ordering::Equal => {}
                order => return Some(order),
            }
        }
        Some(Ordering::Equal)
    }

    fn assess_profile(
        &mut self,
        context: &ContextV1<'_>,
        domain_at: impl Fn(usize) -> SourceBounds,
    ) -> Result<(bool, PointMeasurementPrecisionV1, Option<Ordering>), EvaluationError<E>> {
        let mut precision = PointMeasurementPrecisionV1::Screened;
        let mut measured =
            self.continuous
                .measure(context, &domain_at, &mut self.candidate, precision)?;
        if !measured {
            precision = PointMeasurementPrecisionV1::Refined;
            measured =
                self.continuous
                    .measure(context, &domain_at, &mut self.candidate, precision)?;
        }
        if self.incumbent.precision().is_none() || !measured {
            return Ok((measured, precision, None));
        }
        let mut order = self.compare_profile(context, &domain_at);
        if order.is_none() {
            if self.incumbent.precision() == Some(PointMeasurementPrecisionV1::Screened) {
                let values = &self.best_values;
                if self.continuous.measure(
                    context,
                    |index| {
                        let rgb = values[index].bytes();
                        (rgb, rgb)
                    },
                    &mut self.best,
                    PointMeasurementPrecisionV1::Refined,
                )? {
                    self.incumbent.refined();
                }
            }
            if precision == PointMeasurementPrecisionV1::Screened
                && self.continuous.measure(
                    context,
                    &domain_at,
                    &mut self.candidate,
                    PointMeasurementPrecisionV1::Refined,
                )?
            {
                precision = PointMeasurementPrecisionV1::Refined;
            }
            order = self.compare_profile(context, &domain_at);
        }
        Ok((measured, precision, order))
    }

    pub(super) fn lower_profile(
        &mut self,
        context: &ContextV1<'_>,
        bounds: &BoundsV1,
        domain_at: impl Fn(usize) -> AssignmentDomainV1,
    ) -> Result<bool, EvaluationError<E>> {
        let count = context.targets.len();
        let points = bounds
            .appearance
            .as_ref()
            .expect("reference bounds capture");
        let refinement = context.case_refinement.borrow();
        self.current.fill(0.0);
        self.candidate.fill(zero());
        let (explicit, remainder) = self.current.split_at_mut(count);
        let (shortfalls, distances) = remainder.split_at_mut(count);
        let (visible, source) = distances.split_at_mut(count);
        for &case in &refinement.cases {
            let row = refinement.ranks[case];
            for &(target, preference) in &self.preferences {
                let [tone, shortfall, distance] = preference.lower_loss(
                    &mut self.cases,
                    context,
                    target,
                    case,
                    row,
                    points[case * count + target],
                );
                explicit[target] = explicit[target].max(tone);
                shortfalls[target] = shortfalls[target].max(shortfall);
                visible[target] = visible[target].max(distance);
                if matches!(preference, Preference::Transport { .. }) {
                    for (index, loss) in
                        [(count + target, shortfall), (2 * count + target, distance)]
                    {
                        // The machine-domain bound follows monotonic source-over,
                        // luminance and the operational Oklab box evaluator. A
                        // sampled-case maximum remains a lower bound. If a
                        // bound is unavailable, zero is the nonnegative floor.
                        if let Ok(loss) = CertifiedScalarV1::exact(loss) {
                            self.candidate[index] = self.candidate[index]
                                .max(loss)
                                .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                        }
                    }
                }
            }
        }
        for (target, compiled) in context.targets.iter().enumerate() {
            if !context.represents_target(target) {
                continue;
            }
            let domain = domain_at(target);
            let (lo, hi) = domain.bounds();
            source[target] = if lo == hi {
                squared_distance(lab(Srgb8::new(lo)), compiled.source_anchor_lab)
            } else {
                domain.lower_score(compiled.source_anchor_lab)
            };
        }
        // Each coordinate is a lower bound. Sorting preserves componentwise
        // order, and a maximum over the sampled cases bounds the full maximum.
        // Reuse the comparison scratch; accepted profiles remain in `best`.
        sort_tiers(&mut self.current, count);
        if self.incumbent.precision().is_none() {
            return Ok(false);
        }
        let (measured, _, order) =
            self.assess_profile(context, |index| domain_at(index).bounds())?;
        // Equality of a lower profile is insufficient to reject a region:
        // it may contain a canonical representative of a proved tie.
        Ok(measured && order == Some(Ordering::Greater))
    }

    pub(super) fn current_profile(&self) -> &[f64] {
        &self.current
    }

    // Dropping the joint hard conditions bounds every sorted preference tier.
    // Unique local minima attain the bound together if their whole tuple passes.
    pub(super) fn independent_minimum(
        &self,
        context: &ContextV1<'_>,
        initial: &[Srgb8],
        mut remaining: u64,
    ) -> Result<Option<Vec<Srgb8>>, ()> {
        if self.preferences.len() < 2
            || self.preferences.iter().any(|&(index, preference)| {
                !matches!(preference, Preference::Transport { .. })
                    || !context.targets[index].independent_backdrop
                    || context.domains[index]
                        .cardinality()
                        .is_none_or(|size| size > remaining)
            })
        {
            return Ok(None);
        }
        let mut values = cloned(initial)?;
        let mut minima = HashMap::new();
        let rows = context.local_cases.order.len();
        for &(index, preference) in &self.preferences {
            let Preference::Transport { model, case_start } = preference else {
                unreachable!("all preferences were checked above")
            };
            let target = &context.targets[index];
            let domain = context.domains[index];
            let mut support = Vec::new();
            support.try_reserve_exact(rows).map_err(|_| ())?;
            support.extend(
                self.cases[case_start..case_start + rows]
                    .iter()
                    .map(|case| (case.backdrop, case.alpha.to_bits())),
            );
            support.sort_unstable();
            support.dedup();
            let key = (
                target.effective_anchor(),
                target.reference_backdrop,
                domain.identity_bytes(),
                support,
            );
            values[index] = if let Some(&minimum) = minima.get(&key) {
                minimum
            } else {
                let size = domain.cardinality().ok_or(())?;
                let Some(rest) = remaining.checked_sub(size) else {
                    return Ok(None);
                };
                remaining = rest;
                let Some(minimum) = strict_transport_minimum(domain, model, &key.3, size)? else {
                    return Ok(None);
                };
                minima.try_reserve(1).map_err(|_| ())?;
                minima.insert(key, minimum);
                minimum
            };
        }
        context.expand_sources(&mut values);
        Ok(Some(values))
    }
}

fn strict_transport_minimum(
    domain: AssignmentDomainV1,
    model: ReferenceTransportV1,
    support: &[(Srgb8, u64)],
    size: u64,
) -> Result<Option<Srgb8>, ()> {
    use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
    let mut cases = Vec::new();
    cases.try_reserve_exact(support.len()).map_err(|_| ())?;
    for &(backdrop, alpha) in support {
        let opacity = AdmittedOpacityV1::new(f64::from_bits(alpha)).map_err(|_| ())?;
        cases.push((UseCase::new(backdrop, opacity.value()), opacity));
    }
    let mut best = [f64::INFINITY; 2];
    let mut unique = None;
    for ordinal in 0..size {
        let source = domain.value(ordinal);
        let mut worst = [0.0_f64; 2];
        for (case, opacity) in &mut cases {
            let visible = Srgb8::new(CompositionProfileV1::EncodedSrgb8SourceOverV1.composite(
                source.bytes(),
                *opacity,
                case.backdrop.bytes(),
            ));
            let point = AppearancePointV1 {
                visible,
                backdrop: case.backdrop,
                alpha: case.alpha,
                lab: lab(visible),
            };
            let loss = case.transport_loss(domain, model, point);
            worst[0] = worst[0].max(loss[0]);
            worst[1] = worst[1].max(loss[1]);
        }
        match worst[0]
            .total_cmp(&best[0])
            .then_with(|| worst[1].total_cmp(&best[1]))
        {
            Ordering::Less => {
                best = worst;
                unique = Some(source);
            }
            Ordering::Equal => unique = None,
            Ordering::Greater => {}
        }
    }
    Ok(unique)
}

fn sort_tiers(profile: &mut [f64], count: usize) {
    for tier in profile.chunks_exact_mut(count) {
        tier.sort_unstable_by(|a, b| b.total_cmp(a));
    }
}

pub(super) fn compare(left: &[f64], right: &[f64]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(a, b)| a.total_cmp(b))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}
