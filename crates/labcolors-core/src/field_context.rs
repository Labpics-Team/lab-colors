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
    fn field_count(&self) -> usize;
    fn field_input(&self, field: usize) -> usize;
    fn field_stops(&self, field: usize) -> FieldStops<'_>;
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

/// Shared by arena slots. Neither transient index belongs to an accepted snapshot.
#[derive(Debug, Default)]
pub(crate) struct FieldPartitionScratch {
    witnesses: HashMap<[u8; 3], FieldCell>,
    segment_sizes: HashMap<([u8; 3], [u8; 3]), usize>,
}

/// Constants and each independent source are retained once. Row ordinals
/// enumerate complete marginal supports, never a product of unrelated surfaces.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeclaredFieldV1 {
    fixed_values: Vec<ColorSignal>,
    inputs: Vec<InputField>,
    case_count: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct InputField {
    input: usize,
    case_offset: usize,
    stops: Vec<ProgramGradientStopV1>,
    support: Vec<FieldSupportV1>,
    region_count: usize,
}

impl InputField {
    fn materialize(
        &mut self,
        input: usize,
        stops: FieldStops<'_>,
        scratch: &mut FieldPartitionScratch,
    ) -> Result<(), ObservationError> {
        self.input = input;
        reserve_total(&mut self.stops, stops.len())?;
        self.stops.clear();
        self.stops.extend((0..stops.len()).map(|i| stops.stop(i)));
        let FieldPartitionScratch {
            witnesses,
            segment_sizes,
        } = scratch;
        witnesses.clear();
        segment_sizes.clear();
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
        self.support
            .extend(witnesses.drain().map(|(color, witness)| FieldSupportV1 {
                value: ColorSignal::from_srgb8(Srgb8::new(color)),
                witness,
            }));
        self.support
            .sort_unstable_by_key(|entry| entry.witness.local.nominal());
        Ok(())
    }
}

impl DeclaredFieldV1 {
    pub(crate) const fn empty() -> Self {
        Self {
            fixed_values: Vec::new(),
            inputs: Vec::new(),
            case_count: 0,
        }
    }

    pub(crate) fn materialize<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &mut self,
        source: &Source,
        scratch: &mut FieldPartitionScratch,
    ) -> Result<(), ObservationError> {
        reserve_total(&mut self.fixed_values, source.value_count())?;
        self.fixed_values.clear();
        reserve_total(&mut self.inputs, source.field_count())?;
        self.inputs.truncate(source.field_count());
        self.inputs
            .resize_with(source.field_count(), InputField::default);
        // A varying slot has no constant value supplied by the caller. Read
        // only genuinely fixed inputs, preserving the single-field contract
        // and avoiding needless calls for every declared gradient.
        let mut field = 0;
        for input in 0..source.value_count() {
            if field < self.inputs.len() && source.field_input(field) == input {
                self.fixed_values.push(ColorSignal::from_srgb8(
                    source.field_stops(field).stop(0).color,
                ));
                field += 1;
            } else {
                self.fixed_values
                    .push(ColorSignal::from_srgb8(source.value(input)));
            }
        }
        self.case_count = 0;
        for (ordinal, input) in self.inputs.iter_mut().enumerate() {
            input.materialize(
                source.field_input(ordinal),
                source.field_stops(ordinal),
                scratch,
            )?;
            self.fixed_values[input.input] = ColorSignal::from_srgb8(input.stops[0].color);
            input.case_offset = self.case_count;
            self.case_count = self
                .case_count
                .checked_add(input.support.len())
                .ok_or(ObservationError::ResourceExhausted)?;
        }
        Ok(())
    }

    pub(crate) fn matches<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &self,
        source: &Source,
    ) -> bool {
        self.inputs.len() == source.field_count()
            && self.inputs.iter().enumerate().all(|(i, input)| {
                let stops = source.field_stops(i);
                input.input == source.field_input(i)
                    && input.stops.len() == stops.len()
                    && input
                        .stops
                        .iter()
                        .enumerate()
                        .all(|(j, stop)| *stop == stops.stop(j))
            })
            && self.fixed_values.len() == source.value_count()
            && self.fixed_values.iter().enumerate().all(|(i, v)| {
                self.inputs
                    .binary_search_by_key(&i, |field| field.input)
                    .is_ok()
                    || *v == ColorSignal::from_srgb8(source.value(i))
            })
    }

    pub(crate) fn physical_case_count(&self) -> usize {
        self.case_count
    }

    pub(crate) fn values(&self, row: usize) -> Option<PhysicalValuesV1<'_>> {
        if row >= self.case_count {
            return None;
        }
        if let [field] = &self.inputs[..] {
            return Some(PhysicalValuesV1::with_override(
                &self.fixed_values,
                field.input,
                &field.support[row].value,
            ));
        }
        let index = self
            .inputs
            .partition_point(|field| field.case_offset <= row)
            .checked_sub(1)?;
        let field = &self.inputs[index];
        let value = &field
            .support
            .get(row.checked_sub(field.case_offset)?)?
            .value;
        Some(PhysicalValuesV1::with_override(
            &self.fixed_values,
            field.input,
            value,
        ))
    }

    pub(crate) fn view(&self) -> DeclaredFieldViewV1<'_> {
        DeclaredFieldViewV1 {
            all: self,
            field: &self.inputs[0],
            case_offset: self.inputs[0].case_offset,
        }
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

#[cfg(test)]
mod tests {
    use super::{
        DeclaredFieldV1, FieldPartitionScratch, FieldStops, ObservationError,
        SchemaOrderedFieldSourceV1,
    };
    use crate::Srgb8;
    use crate::gradient_domain::OpaqueSrgb8AffineSegment;

    struct Fields {
        inputs: &'static [usize],
    }

    impl SchemaOrderedFieldSourceV1 for Fields {
        fn value_count(&self) -> usize {
            2
        }

        fn value(&self, input_index: usize) -> Srgb8 {
            Srgb8::new([input_index as u8; 3])
        }

        fn field_count(&self) -> usize {
            self.inputs.len()
        }

        fn field_input(&self, field: usize) -> usize {
            self.inputs[field]
        }

        fn field_stops(&self, _field: usize) -> FieldStops<'_> {
            FieldStops::Affine(OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]))
        }
    }

    #[test]
    fn reusing_field_storage_with_fewer_inputs_discards_the_old_tail()
    -> Result<(), ObservationError> {
        let mut field = DeclaredFieldV1::empty();
        let mut scratch = FieldPartitionScratch::default();

        field.materialize(&Fields { inputs: &[0, 1] }, &mut scratch)?;
        assert_eq!(field.physical_case_count(), 1022);

        field.materialize(&Fields { inputs: &[1] }, &mut scratch)?;
        assert_eq!(field.inputs.len(), 1);
        assert_eq!(field.physical_case_count(), 511);
        assert_eq!(field.values(0).expect("first support value").len(), 2);
        assert!(field.values(511).is_none());
        Ok(())
    }
}

/// Borrowed descriptor and exact coordinate provenance of this admitted
/// revision. Presence establishes the model domain, not evaluator coverage.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeclaredFieldViewV1<'a> {
    all: &'a DeclaredFieldV1,
    field: &'a InputField,
    case_offset: usize,
}

impl<'a> DeclaredFieldViewV1<'a> {
    /// Resolve a factor's physical support before attaching a field witness.
    /// A factor spanning several varying inputs needs a joint source model.
    pub(crate) fn case_origin(
        self,
        dependencies: &[usize],
        case_index: usize,
    ) -> Result<crate::program_diagnostics::ProgramCaseOriginV1, ()> {
        use crate::program_diagnostics::{ProgramCaseOriginV1, ProgramFieldCaseV1};
        let mut selected = None;
        for &input in dependencies {
            if let Ok(index) = self
                .all
                .inputs
                .binary_search_by_key(&input, |field| field.input)
            {
                if selected.is_some_and(|previous| previous != index) {
                    return Err(());
                }
                selected = Some(index);
            }
        }
        let Some(index) = selected else {
            return Ok(if case_index == 0 {
                ProgramCaseOriginV1::Fixed
            } else {
                ProgramCaseOriginV1::OtherMarginal
            });
        };
        let field = &self.all.inputs[index];
        let Some(local) = case_index.checked_sub(field.case_offset) else {
            return Ok(ProgramCaseOriginV1::OtherMarginal);
        };
        Ok(if local < field.support.len() {
            ProgramCaseOriginV1::Field(ProgramFieldCaseV1 {
                input_index: field.input,
                case_index: local,
            })
        } else {
            ProgramCaseOriginV1::OtherMarginal
        })
    }

    pub(crate) fn fields(self) -> impl ExactSizeIterator<Item = Self> {
        self.all.inputs.iter().map(move |field| Self {
            all: self.all,
            field,
            case_offset: field.case_offset,
        })
    }

    pub(crate) fn support_count(self) -> usize {
        self.field.support.len()
    }

    pub(crate) fn stops(self) -> &'a [ProgramGradientStopV1] {
        &self.field.stops
    }

    pub(crate) const fn varying_input_index(self) -> usize {
        self.field.input
    }

    pub(crate) fn witness(self, case_index: usize) -> Option<FieldCell> {
        Some(
            self.field
                .support
                .get(case_index.checked_sub(self.case_offset)?)?
                .witness,
        )
    }

    pub(crate) const fn case_offset(self) -> usize {
        self.case_offset
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
        self.all.fixed_values.len()
            + self
                .all
                .inputs
                .iter()
                .map(|field| field.support.len())
                .sum::<usize>()
    }
}
