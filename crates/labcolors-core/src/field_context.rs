//! Compact admission backing for one declared continuous source.
//!
//! The affine descriptor is authoritative. Its exact byte support is derived
//! internally for the existing quantized compositor; it is neither a collection
//! of user observations nor a guarantee about a browser rasterizer.

use core::ops::Range;

use crate::Srgb8;
use crate::gradient_domain::{AffineSegmentCell, OpaqueSrgb8AffineSegment};
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{ObservationError, PhysicalValuesV1};

/// One opaque affine input and constants in compiled schema order.
///
/// Admission never reads `value(varying_input_index())`: that input is wholly
/// specified by `segment()`. No caller supplies support points or samples.
pub(crate) trait SchemaOrderedFieldSourceV1 {
    fn value_count(&self) -> usize;
    fn value(&self, input_index: usize) -> Srgb8;
    fn varying_input_index(&self) -> usize;
    fn segment(&self) -> OpaqueSrgb8AffineSegment;
}

#[derive(Debug, PartialEq, Eq)]
struct FieldSupportV1 {
    value: ColorSignal,
    cells: Range<usize>,
}

/// Private reusable storage. Constants are stored once, regardless of support
/// size. Each exact region occurs once and refers to one unique support value.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeclaredFieldV1 {
    segment: OpaqueSrgb8AffineSegment,
    varying_input_index: usize,
    fixed_values: Vec<ColorSignal>,
    support: Vec<FieldSupportV1>,
    cells: Vec<AffineSegmentCell>,
}

impl DeclaredFieldV1 {
    pub(crate) const fn empty() -> Self {
        Self {
            segment: OpaqueSrgb8AffineSegment::new([0; 3], [0; 3]),
            varying_input_index: 0,
            fixed_values: Vec::new(),
            support: Vec::new(),
            cells: Vec::new(),
        }
    }

    pub(crate) fn materialize<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &mut self,
        source: &Source,
    ) -> Result<(), ObservationError> {
        let segment = source.segment();
        let cell_count = segment.partition_cell_count();
        // Reserve before replacing any contents. This is a free arena slot;
        // failed growth cannot mutate an already-issued revision.
        reserve_total(&mut self.fixed_values, source.value_count())?;
        reserve_total(&mut self.cells, cell_count)?;
        reserve_total(&mut self.support, cell_count)?;
        self.segment = segment;
        self.varying_input_index = source.varying_input_index();
        self.fixed_values.clear();
        self.fixed_values
            .extend((0..source.value_count()).map(|index| {
                ColorSignal::from_srgb8(if index == self.varying_input_index {
                    Srgb8::new(segment.endpoints().0)
                } else {
                    source.value(index)
                })
            }));
        self.cells.clear();
        self.cells.extend(segment.partition());
        self.cells
            .sort_unstable_by_key(|cell| (cell.nominal(), cell.representative()));
        self.support.clear();
        for (index, cell) in self.cells.iter().enumerate() {
            let value = ColorSignal::from_srgb8(Srgb8::new(cell.nominal()));
            if let Some(previous) = self.support.last_mut().filter(|entry| entry.value == value) {
                previous.cells.end = index + 1;
            } else {
                self.support.push(FieldSupportV1 {
                    value,
                    cells: index..index + 1,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn matches<Source: SchemaOrderedFieldSourceV1 + ?Sized>(
        &self,
        source: &Source,
    ) -> bool {
        self.segment == source.segment()
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
    pub(crate) const fn segment(self) -> OpaqueSrgb8AffineSegment {
        self.field.segment
    }

    pub(crate) const fn varying_input_index(self) -> usize {
        self.field.varying_input_index
    }

    /// Every region producing the corresponding physical case. A constant
    /// field retains `{0}`, `(0, 1)` and `{1}` even with one support value.
    pub(crate) fn cells(self, case_index: usize) -> Option<&'a [AffineSegmentCell]> {
        let range = &self.field.support.get(case_index)?.cells;
        self.field.cells.get(range.clone())
    }

    pub(crate) fn cell_count(self) -> usize {
        self.field.cells.len()
    }

    #[cfg(test)]
    pub(crate) fn stored_value_count_for_test(self) -> usize {
        self.field.fixed_values.len() + self.field.support.len()
    }
}
