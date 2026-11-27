//! Connected reference uses compare each criterion across the whole tuple.
//! A backdrop can depend on another paint in that same tuple.

use super::*;
use crate::composition::source_over_channel_srgb8;

#[derive(Clone, Copy)]
struct Direction {
    physical_shortfall: f64,
    preferred: [f64; 3],
}

struct UseCase {
    directions: [Direction; 3],
    reference_y_equal: bool,
    backdrop: Srgb8,
    alpha: f64,
}

// Monotonic encoded source-over and luminance enclose every physical paint.
// The ratio uses independent numerator/denominator extrema when the backdrop
// varies. A fixed backdrop recovers the exact operational ceiling.
fn physical_ceilings(domain: AssignmentDomainV1, alpha: f64, backdrops: [Srgb8; 2]) -> [f64; 3] {
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
    fn new(
        context: &ContextV1<'_>,
        model: ReferenceTransportV1,
        target: usize,
        case: usize,
        backdrop: Srgb8,
        alpha: f64,
    ) -> Self {
        let ceiling = physical_ceilings(context.domains[target], alpha, [backdrop; 2]);
        let explicit = context.targets[target].tone_input.is_some();
        let directions = std::array::from_fn(|i| {
            if explicit {
                let mut preferred = model.reference_lab();
                preferred[0] = context.preferred[case * context.targets.len() + target][0];
                Direction {
                    physical_shortfall: 0.0,
                    preferred,
                }
            } else {
                // Monotonic source-over and sRGB luminance put a ceiling on
                // contrast for this paint domain, opacity and actual backdrop.
                // Other hard conditions may further restrict attainable colours.
                let fraction = (ceiling[i] / model.reference_contrast()).min(1.0);
                Direction {
                    physical_shortfall: 1.0 - fraction,
                    preferred: model.lab_for(backdrop, from_sign(i as i8 - 1)),
                }
            }
        });
        Self {
            directions,
            reference_y_equal: model.reference_contrast() == 1.0,
            backdrop,
            alpha,
        }
    }

    fn direction(&self, point: AppearancePointV1) -> &Direction {
        let sign = if self.reference_y_equal {
            0
        } else {
            sign(polarity(point.visible, point.backdrop))
        };
        &self.directions[(sign + 1) as usize]
    }

    fn lower_bounds(
        &mut self,
        context: &ContextV1<'_>,
        model: ReferenceTransportV1,
        target: usize,
        case: usize,
        [lower, upper]: [RegionPointV1; 2],
    ) -> [f64; 2] {
        let (lab_lo, lab_hi) =
            crate::spaces::oklab::srgb8_box_oklab_bounds(lower.visible, upper.visible);
        if context.targets[target].tone_input.is_some() {
            let mut preferred = model.reference_lab();
            preferred[0] = context.preferred[case * context.targets.len() + target][0];
            return [0.0, point_box_distance(preferred, lab_lo, lab_hi)];
        }
        let backdrops = [Srgb8::new(lower.backdrop), Srgb8::new(upper.backdrop)];
        let fixed = backdrops[0] == backdrops[1];
        if fixed && self.backdrop != backdrops[0] {
            *self = Self::new(context, model, target, case, backdrops[0], self.alpha);
        }
        let background = backdrops.map(luminance);
        let visible = [Srgb8::new(lower.visible), Srgb8::new(upper.visible)].map(luminance);
        let ceilings = physical_ceilings(context.domains[target], self.alpha, backdrops);
        let reference = model.reference_lab();
        let mut shortfall = 1.0_f64;
        let mut distance = f64::INFINITY;
        for (index, ceiling) in ceilings.into_iter().enumerate() {
            let direction = index as i8 - 1;
            let possible = if self.reference_y_equal {
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
            let branch = &self.directions[index];
            let branch_shortfall = if fixed {
                branch.physical_shortfall
            } else {
                1.0 - (ceiling / model.reference_contrast()).min(1.0)
            };
            let target_l = if fixed {
                [branch.preferred[0]; 2]
            } else {
                model.virtual_lightness_bounds(background, from_sign(direction))
            };
            let dl = interval_distance([lab_lo[0], lab_hi[0]], target_l);
            let da = interval_distance([lab_lo[1], lab_hi[1]], [reference[1]; 2]);
            let db = interval_distance([lab_lo[2], lab_hi[2]], [reference[2]; 2]);
            shortfall = shortfall.min(branch_shortfall);
            distance = distance.min((dl * dl + da * da) + db * db);
        }
        [shortfall, distance]
    }
}

pub(super) struct PeerProfiles {
    cases: Vec<UseCase>,
    current: Vec<f64>,
    best: Vec<f64>,
    has_best: bool,
    authored_exact: bool,
}

impl PeerProfiles {
    pub(super) fn new(count: usize) -> Result<Self, ()> {
        let length = count.checked_mul(3).ok_or(())?;
        Ok(Self {
            cases: Vec::new(),
            current: buffer(length, 0.0)?,
            best: buffer(length, 0.0)?,
            has_best: false,
            authored_exact: false,
        })
    }

    pub(super) fn authored_exact(&self) -> bool {
        self.authored_exact
    }

    pub(super) fn consider(
        &mut self,
        context: &ContextV1<'_>,
        models: &[Option<ReferenceTransportV1>],
        points: &[AppearancePointV1],
        values: &[Srgb8],
    ) -> Result<bool, ()> {
        let count = values.len();
        let local_cases = &context.local_cases.order;
        if self.cases.is_empty() {
            self.cases
                .try_reserve_exact(count.checked_mul(local_cases.len()).ok_or(())?)
                .map_err(|_| ())?;
            for &case in local_cases {
                for (target, &model) in models.iter().enumerate() {
                    self.cases.push(UseCase::new(
                        context,
                        model.ok_or(())?,
                        target,
                        case,
                        points[case * count + target].backdrop,
                        points[case * count + target].alpha,
                    ));
                }
            }
        }
        self.current.fill(0.0);
        let (shortfalls, distances) = self.current.split_at_mut(count);
        let (visible, source) = distances.split_at_mut(count);
        let mut authored_reference = true;
        for (row, &case) in self.cases.chunks_exact_mut(count).zip(local_cases) {
            for (target, prepared) in row.iter_mut().enumerate() {
                let point = points[case * count + target];
                if prepared.backdrop != point.backdrop || prepared.alpha != point.alpha {
                    *prepared = UseCase::new(
                        context,
                        models[target].ok_or(())?,
                        target,
                        case,
                        point.backdrop,
                        point.alpha,
                    );
                }
                let direction = prepared.direction(point);
                shortfalls[target] = shortfalls[target].max(direction.physical_shortfall);
                visible[target] =
                    visible[target].max(squared_distance(point.lab, direction.preferred));
                let compiled = &context.targets[target];
                // An identical authored use has zero transport and source loss
                // by construction. A rounded tone comparison cannot prove this.
                authored_reference &= compiled.tone_input.is_none()
                    && Some(point.backdrop) == compiled.reference_backdrop
                    && point.visible == compiled.effective_anchor();
            }
        }
        for (target, &value) in values.iter().enumerate() {
            let compiled = &context.targets[target];
            source[target] = squared_distance(lab(value), compiled.source_anchor_lab);
            authored_reference &= value == compiled.declaration.anchor;
        }
        sort_tiers(&mut self.current, count);
        let improved = !self.has_best || compare(&self.current, &self.best) == Ordering::Less;
        if improved {
            std::mem::swap(&mut self.best, &mut self.current);
            self.has_best = true;
            self.authored_exact = authored_reference;
        }
        Ok(improved)
    }

    pub(super) fn cannot_improve(
        &mut self,
        context: &ContextV1<'_>,
        models: &[Option<ReferenceTransportV1>],
        bounds: &BoundsV1,
        node: RegionNodeV1,
        values: &[Srgb8],
    ) -> bool {
        if !self.has_best {
            return false;
        }
        let count = context.targets.len();
        let points = bounds
            .appearance
            .as_ref()
            .expect("reference bounds capture");
        let refinement = context.case_refinement.borrow();
        self.current.fill(0.0);
        let (shortfalls, distances) = self.current.split_at_mut(count);
        let (visible, source) = distances.split_at_mut(count);
        for &case in &refinement.cases {
            let row = refinement.ranks[case] * count;
            for (target, model) in models.iter().enumerate() {
                let [shortfall, distance] = self.cases[row + target].lower_bounds(
                    context,
                    model.expect("reference peer model"),
                    target,
                    case,
                    points[case * count + target],
                );
                shortfalls[target] = shortfalls[target].max(shortfall);
                visible[target] = visible[target].max(distance);
            }
        }
        for (target, compiled) in context.targets.iter().enumerate() {
            source[target] = if target < node.depth {
                squared_distance(lab(values[target]), compiled.source_anchor_lab)
            } else {
                let domain = if target == node.depth {
                    node.domain
                } else {
                    context.domains[target]
                };
                domain.lower_score(compiled.source_anchor_lab)
            };
        }
        // Each coordinate is a lower bound. Sorting preserves componentwise
        // order, and a maximum over the sampled cases bounds the full maximum.
        // Reuse the comparison scratch; accepted profiles remain in `best`.
        sort_tiers(&mut self.current, count);
        compare(&self.current, &self.best) != Ordering::Less
    }
}

fn sort_tiers(profile: &mut [f64], count: usize) {
    for tier in profile.chunks_exact_mut(count) {
        tier.sort_unstable_by(|a, b| b.total_cmp(a));
    }
}

fn compare(left: &[f64], right: &[f64]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(a, b)| a.total_cmp(b))
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}
