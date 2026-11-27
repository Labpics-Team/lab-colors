//! Reference uses with independent backdrops share a hard relation. Each
//! criterion compares like units across the whole tuple before moving on.

use super::*;
use crate::composition::source_over_channel_srgb8;

struct Direction {
    physical_shortfall: f64,
    preferred: [f64; 3],
}

struct UseCase {
    directions: [Direction; 3],
    reference_y_equal: bool,
}

impl UseCase {
    fn new(
        context: &ContextV1<'_>,
        model: ReferenceTransportV1,
        target: usize,
        case: usize,
        point: AppearancePointV1,
    ) -> Self {
        let backdrop = point.backdrop;
        let by = luminance(backdrop);
        let (low, high) = context.domains[target].bounds();
        let visible_y = |source: [u8; 3]| {
            luminance(Srgb8::new(std::array::from_fn(|i| {
                source_over_channel_srgb8(source[i], point.alpha, backdrop.bytes()[i])
            })))
        };
        let ceiling = [
            (by + 0.05) / (visible_y(low) + 0.05),
            1.0,
            (visible_y(high) + 0.05) / (by + 0.05),
        ];
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
                        points[case * count + target],
                    ));
                }
            }
        }
        self.current.fill(0.0);
        let (shortfalls, distances) = self.current.split_at_mut(count);
        let (visible, source) = distances.split_at_mut(count);
        let mut authored_reference = true;
        for (row, &case) in self.cases.chunks_exact(count).zip(local_cases) {
            for (target, prepared) in row.iter().enumerate() {
                let point = points[case * count + target];
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
            source[target] = squared_distance(lab(value), compiled.anchor_lab);
            authored_reference &= value == compiled.declaration.anchor;
        }
        for tier in self.current.chunks_exact_mut(count) {
            tier.sort_unstable_by(|a, b| b.total_cmp(a));
        }
        let improved = !self.has_best
            || self
                .current
                .iter()
                .zip(&self.best)
                .map(|(a, b)| a.total_cmp(b))
                .find(|order| *order != Ordering::Equal)
                == Some(Ordering::Less);
        if improved {
            std::mem::swap(&mut self.best, &mut self.current);
            self.has_best = true;
            self.authored_exact = authored_reference;
        }
        Ok(improved)
    }
}
