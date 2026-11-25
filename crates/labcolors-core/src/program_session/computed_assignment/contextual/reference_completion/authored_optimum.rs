//! An unchanged authored tuple can already attain the component's minimum.
//! This proof uses the real encoded appearance, before gamma or byte emission.

use super::*;

impl<E: ProgramConstraintEvaluatorSetV1> PreparedPointSelectionV1<'_, '_, E> {
    /// The caller must first establish hard feasibility of the authored tuple.
    /// Its source distances are exactly zero by source identity. Each primary
    /// distance below either attains zero or is invariant throughout a box
    /// enclosing the complete source domain. Thus no primary order statistic
    /// can decrease, and no secondary source distance can improve on zero.
    pub(in crate::program_session::computed_assignment::contextual) fn authored_point_optimal(
        &mut self,
        component: &ComputedComponentV1,
        cases: &[usize],
    ) -> Result<bool, Error<E>> {
        let CompiledTargetSelectionV1::Computed(selection) = &self.epoch.target_selection else {
            return Err(Error::<E>::RequiresComputedTargets);
        };
        if component.targets.iter().any(|&target| {
            matches!(
                selection.targets[target].preferred_intent(),
                CompiledPreferredIntentV1::ImplicitReference
            )
        }) {
            return Ok(false);
        }
        let storage = &mut *self.scratch;
        let count = component.targets.len();
        let rows = count
            .checked_mul(cases.len())
            .ok_or(Error::<E>::ResourceExhausted)?;
        storage.authored_minimum_rows.clear();
        storage
            .authored_minimum_rows
            .try_reserve(rows)
            .map_err(|_| Error::<E>::ResourceExhausted)?;
        let authored = storage.bindings.paint_inputs_canonical();
        // The first pass proves exact authored minima. Only unresolved rows
        // need the second pass, which proves independence of every source.
        for whole_domain in [false, true] {
            let source = |target: usize| {
                let target = &selection.targets[target];
                if whole_domain {
                    let (lo, hi) = target.search_domain().bounds();
                    CertifiedRgbV1::from_encoded_bounds(lo.map(f64::from), hi.map(f64::from))
                } else {
                    CertifiedRgbV1::from_encoded(target.declaration.anchor.bytes().map(f64::from))
                }
            };
            let mut paints = self.epoch.graph.prepare_certified_surface_cases(
                Some(&component.physical),
                &storage.bindings,
                |binding| {
                    let rgb = storage.binding_targets[binding]
                        .filter(|target| component.targets.binary_search(target).is_ok())
                        .map_or_else(
                            || {
                                CertifiedRgbV1::from_encoded(
                                    authored[binding].1.source().bytes().map(f64::from),
                                )
                            },
                            source,
                        );
                    PointPaintValueV1::from_admitted(rgb, authored[binding].1.opacity())
                },
                &mut storage.certified_workspace,
            )?;
            for (local_case, &case) in cases.iter().enumerate() {
                let inputs = self
                    .observation
                    .physical_values(case)
                    .ok_or(Error::<E>::MissingFiniteObservation)?;
                let view = paints.evaluate_surface_inputs_canonical(|index| {
                    CertifiedRgbV1::from_encoded(inputs[index].srgb8().bytes().map(f64::from))
                });
                for (local, &target) in component.targets.iter().enumerate() {
                    let row = local_case * count + local;
                    if whole_domain && storage.authored_minimum_rows[row] {
                        continue;
                    }
                    let visible = if let Some(slot) = storage.uses[target].slot {
                        view.occurrence_at(slot)
                            .ok_or(Error::<E>::InvalidUse)?
                            .visible?
                    } else {
                        source(target)?
                    };
                    if whole_domain {
                        // A singleton enclosure over the full source box
                        // proves the color invariant. Non-singleton equal
                        // intervals cannot authorize this conclusion.
                        if !visible.proves_equal(visible) {
                            return Ok(false);
                        }
                    } else {
                        let unmodified_anchor = matches!(
                            selection.targets[target].preferred_intent(),
                            CompiledPreferredIntentV1::Point {
                                tone: None,
                                anchor: PreferredPointAnchorV1::AuthoredSource,
                            }
                        );
                        storage
                            .authored_minimum_rows
                            .push(unmodified_anchor && visible.proves_equal(source(target)?));
                    }
                }
            }
            if storage.authored_minimum_rows.iter().all(|proved| *proved) {
                return Ok(true);
            }
        }
        Ok(true)
    }
}
