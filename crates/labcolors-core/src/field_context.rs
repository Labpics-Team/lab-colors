//! Compact admission backing for one declared continuous source.
//!
//! The affine descriptor is authoritative. Its exact byte support is derived
//! internally for the existing quantized compositor; it is neither a collection
//! of user observations nor a guarantee about a browser rasterizer.

use std::collections::HashMap;

use crate::Srgb8;
use crate::gradient_domain::{AffineSegmentCell, OpaqueSrgb8AffineSegment};
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{ObservationError, PhysicalValuesV1};

/// One authored stop in the normalized gradient line. Its finite, normalized
/// binary64 position is kept exactly across snapshots and revision comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramGradientStopV1 {
    position_bits: u64,
    color: Srgb8,
}

impl ProgramGradientStopV1 {
    fn new(at: f64, color: [u8; 3]) -> Self {
        Self {
            position_bits: if at == 0.0 { 0 } else { at.to_bits() },
            color: Srgb8::new(color),
        }
    }

    /// Normalized position in the authored line.
    pub const fn position(self) -> f64 {
        f64::from_bits(self.position_bits)
    }
    /// Opaque encoded sRGB8 at the stop.
    pub const fn color(self) -> Srgb8 {
        self.color
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FieldStops<'a> {
    Affine(OpaqueSrgb8AffineSegment),
    Positioned {
        positions: &'a [f64],
        colors: &'a [u8],
    },
}

impl<'a> FieldStops<'a> {
    fn len(self) -> usize {
        match self {
            Self::Affine(_) => 2,
            Self::Positioned { positions, .. } => positions.len(),
        }
    }

    fn stop(self, index: usize) -> ProgramGradientStopV1 {
        match self {
            Self::Affine(segment) => {
                let (from, to) = segment.endpoints();
                if index == 0 {
                    ProgramGradientStopV1::new(0.0, from)
                } else {
                    ProgramGradientStopV1::new(1.0, to)
                }
            }
            Self::Positioned { positions, colors } => {
                let offset = 3 * index;
                ProgramGradientStopV1::new(
                    positions[index],
                    [colors[offset], colors[offset + 1], colors[offset + 2]],
                )
            }
        }
    }

    fn segments(self) -> impl Iterator<Item = OpaqueSrgb8AffineSegment> + 'a {
        (1..self.len()).map(move |i| {
            OpaqueSrgb8AffineSegment::new(
                self.stop(i - 1).color.bytes(),
                self.stop(i).color.bytes(),
            )
        })
    }
}

pub(crate) trait SchemaOrderedFieldSourceV1 {
    fn value_count(&self) -> usize;
    fn value(&self, input_index: usize) -> Srgb8;
    fn varying_input_index(&self) -> usize;
    fn stops(&self) -> FieldStops<'_>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldCell {
    pub(crate) segment_index: usize,
    pub(crate) local: AffineSegmentCell,
}

#[derive(Debug, PartialEq, Eq)]
struct FieldSupportV1 {
    value: ColorSignal,
    witness: FieldCell,
}

/// Private reusable storage. Constants are stored once, regardless of support
/// size. Each reachable byte color retains its earliest segment and exact witness.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeclaredFieldV1 {
    stops: Vec<ProgramGradientStopV1>,
    varying_input_index: usize,
    fixed_values: Vec<ColorSignal>,
    support: Vec<FieldSupportV1>,
    region_count: usize,
}

impl DeclaredFieldV1 {
    pub(crate) const fn empty() -> Self {
        Self {
            stops: Vec::new(),
            varying_input_index: 0,
            fixed_values: Vec::new(),
            support: Vec::new(),
            region_count: 0,
        }
    }

    pub(crate) fn materialize<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &mut self,
        source: &Source,
    ) -> Result<(), ObservationError> {
        let stops = source.stops();
        // Reserve before replacing any contents. This is a free arena slot;
        // failed growth cannot mutate an already-issued revision.
        reserve_total(&mut self.fixed_values, source.value_count())?;
        reserve_total(&mut self.stops, stops.len())?;
        self.stops.clear();
        self.stops.extend((0..stops.len()).map(|i| stops.stop(i)));
        self.varying_input_index = source.varying_input_index();
        self.fixed_values.clear();
        self.fixed_values
            .extend((0..source.value_count()).map(|index| {
                ColorSignal::from_srgb8(if index == self.varying_input_index {
                    stops.stop(0).color
                } else {
                    source.value(index)
                })
            }));
        let mut witnesses = HashMap::new();
        let mut segment_sizes = HashMap::new();
        self.region_count = 1;
        for (segment_index, segment) in stops.segments().enumerate() {
            // Positions change the witness's global placement, not the local byte
            // support. Repeated directed color pairs reuse their complete partition.
            let endpoints = segment.endpoints();
            let size = if let Some(&size) = segment_sizes.get(&endpoints) {
                size
            } else {
                let mut size = 0;
                for local in segment.partition() {
                    size += 1;
                    let color = local.nominal();
                    if !witnesses.contains_key(&color) {
                        witnesses
                            .try_reserve(1)
                            .map_err(|_| ObservationError::ResourceExhausted)?;
                        witnesses.insert(
                            color,
                            FieldCell {
                                segment_index,
                                local,
                            },
                        );
                    }
                }
                segment_sizes
                    .try_reserve(1)
                    .map_err(|_| ObservationError::ResourceExhausted)?;
                segment_sizes.insert(endpoints, size);
                size
            };
            // The preceding segment already includes the common stop.
            self.region_count = self
                .region_count
                .checked_add(size - 1)
                .ok_or(ObservationError::ResourceExhausted)?;
        }
        reserve_total(&mut self.support, witnesses.len())?;
        self.support.clear();
        self.support.extend(
            witnesses
                .into_iter()
                .map(|(color, witness)| FieldSupportV1 {
                    value: ColorSignal::from_srgb8(Srgb8::new(color)),
                    witness,
                }),
        );
        self.support
            .sort_unstable_by_key(|entry| entry.witness.local.nominal());
        Ok(())
    }

    pub(crate) fn matches<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &self,
        source: &Source,
    ) -> bool {
        let stops = source.stops();
        self.stops.len() == stops.len()
            && self
                .stops
                .iter()
                .enumerate()
                .all(|(i, stop)| *stop == stops.stop(i))
            && self.varying_input_index == source.varying_input_index()
            && self.fixed_values.len() == source.value_count()
            && self.fixed_values.iter().enumerate().all(|(index, value)| {
                index == self.varying_input_index
                    || *value == ColorSignal::from_srgb8(source.value(index))
            })
    }

    pub(crate) fn physical_case_count(&self) -> usize {
        self.support.len()
    }

    pub(crate) fn values(&self, case_index: usize) -> Option<PhysicalValuesV1<'_>> {
        let support = self.support.get(case_index)?;
        Some(PhysicalValuesV1::with_override(
            &self.fixed_values,
            self.varying_input_index,
            &support.value,
        ))
    }

    pub(crate) fn view(&self) -> DeclaredFieldViewV1<'_> {
        DeclaredFieldViewV1 { field: self }
    }
}

fn reserve_total<T>(storage: &mut Vec<T>, required: usize) -> Result<(), ObservationError> {
    if storage.capacity() < required {
        storage
            .try_reserve_exact(required - storage.len())
            .map_err(|_| ObservationError::ResourceExhausted)?;
    }
    Ok(())
}

/// Borrowed descriptor and exact coordinate provenance of this admitted
/// revision. Presence establishes the model domain, not evaluator coverage.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeclaredFieldViewV1<'a> {
    field: &'a DeclaredFieldV1,
}

impl<'a> DeclaredFieldViewV1<'a> {
    pub(crate) fn stops(self) -> &'a [ProgramGradientStopV1] {
        &self.field.stops
    }

    pub(crate) const fn varying_input_index(self) -> usize {
        self.field.varying_input_index
    }

    pub(crate) fn witness(self, case_index: usize) -> Option<FieldCell> {
        Some(self.field.support.get(case_index)?.witness)
    }

    pub(crate) fn cell_count(self) -> usize {
        self.field.region_count
    }

    /// Independently expand the retained descriptor for full-provenance tests.
    #[cfg(test)]
    pub(crate) fn cells(self, case_index: usize) -> Option<Vec<FieldCell>> {
        let color = self.witness(case_index)?.local.nominal();
        Some(
            self.stops()
                .windows(2)
                .enumerate()
                .flat_map(|(segment_index, pair)| {
                    let segment =
                        OpaqueSrgb8AffineSegment::new(pair[0].color.bytes(), pair[1].color.bytes());
                    segment
                        .partition()
                        .skip(usize::from(segment_index > 0))
                        .filter(move |local| local.nominal() == color)
                        .map(move |local| FieldCell {
                            segment_index,
                            local,
                        })
                })
                .collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn stored_value_count_for_test(self) -> usize {
        self.field.fixed_values.len() + self.field.support.len()
    }
}
