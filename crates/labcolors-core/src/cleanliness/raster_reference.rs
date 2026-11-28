//! Selection against a complete opaque sRGB8 bitmap in one renderer's scope.

use crate::composition::source_over_channel_srgb8;

/// Complete authored field captured by a renderer in row-major RGBA8/sRGB.
#[derive(Debug)]
pub struct OpaqueRasterReference {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// An unusable renderer readback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterReferenceError {
    /// Empty extent, mismatched byte count, or a pixel with unresolved transparency.
    InvalidBitmap,
}

impl OpaqueRasterReference {
    /// Takes ownership of a complete sRGB RGBA8 readback, without copying its pixels.
    pub fn capture(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, RasterReferenceError> {
        let length = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0
            || height == 0
            || length != Some(rgba.len() as u64)
            || rgba.chunks_exact(4).any(|pixel| pixel[3] != 255)
        {
            return Err(RasterReferenceError::InvalidBitmap);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    /// Sufficient evidence of preservation for the same viewing context.
    /// Byte equality covers every pixel including its alpha and position.
    #[must_use]
    pub fn same_field(&self, width: u32, height: u32, rgba: &[u8]) -> Option<bool> {
        if width != self.width || height != self.height || rgba.len() != self.rgba.len() {
            return None;
        }
        if rgba == self.rgba {
            // The captured bitmap was opaque; equality proves the candidate is as well.
            return Some(true);
        }
        rgba.chunks_exact(4)
            .all(|pixel| pixel[3] == 255)
            .then_some(false)
    }
}

/// Inverts source-over on this specific opaque backdrop at this opacity in encoded sRGB8.
/// For each channel, chooses the byte closest to the continuous inverse at the center
/// of the visible byte's rounding interval, including reachable gamut endpoints.
/// A fully transparent paint cannot change this use. A derived backdrop can change
/// in a joint V3 trial and require a different paint. V3 checks all authored conditions;
/// only a comparable, identical renderer readback establishes restoration.
#[must_use]
pub fn raster_paint_proposal(desired: [u8; 3], backdrop: [u8; 3], alpha: f64) -> Option<[u8; 3]> {
    if !(alpha > 0.0 && alpha <= 1.0) {
        return None;
    }
    let mut source = [0; 3];
    for i in 0..3 {
        let base = f64::from(backdrop[i]);
        let inverse = base + (f64::from(desired[i]) - base) / alpha;
        // The saturating cast chooses the closest legal byte even at gamut edges.
        source[i] = inverse.round() as u8;
        if source_over_channel_srgb8(source[i], alpha, backdrop[i]) != desired[i] {
            return None;
        }
    }
    Some(source)
}

/// Outcome of submitting one proposal to V3 and rendering any ready result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterTrial {
    /// All pixels match the authored readback.
    SameField,
    /// Comparable geometry and opacity, different bytes.
    DifferentField,
    /// Missing comparable geometry or full opaque coverage.
    Incomparable,
    /// V3 stopped its search before a ready result.
    SearchLimit,
    /// The proposed graph exceeded V3's representable size.
    ProposalLimit,
    /// No usable proposal or no ready result from V3.
    NotReady,
}

impl From<Option<bool>> for RasterTrial {
    fn from(same_field: Option<bool>) -> Self {
        match same_field {
            Some(true) => Self::SameField,
            Some(false) => Self::DifferentField,
            None => Self::Incomparable,
        }
    }
}

/// Next proposal to render, or a final decision for this update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterAction {
    /// Ask V3 for the inverse source paints on the current backdrops.
    TryInverse,
    /// Ask V3 for colors reproducing the reference visible uses.
    TryVisible,
    /// Keep the ordinary frame, whose rendered field matches the reference.
    Preserved,
    /// Select the last ready trial, whose rendered field matches the reference.
    Restored,
    /// Return the ordinary frame. A readback could not be compared.
    IncomparableBitmap,
    /// Return the ordinary frame. No authored paint targets were available.
    NoReferencePaints,
    /// Return the ordinary frame. A rendered trial differed.
    DifferentBitmap,
    /// Return the ordinary frame. A V3 search reached its limit.
    SearchLimit,
    /// Return the ordinary frame. A proposal exceeded the graph's structural limit.
    ProposalLimit,
    /// Return the ordinary frame. Neither trial supplied a ready frame.
    NoReadyProposal,
}

impl RasterAction {
    /// Stable instruction or outcome shared by the renderer adapters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TryInverse => "try-inverse",
            Self::TryVisible => "try-visible",
            Self::Preserved => "preserved",
            Self::Restored => "restored",
            Self::IncomparableBitmap => "incomparable-bitmap",
            Self::NoReferencePaints => "no-reference-paints",
            Self::DifferentBitmap => "different-bitmap",
            Self::SearchLimit => "search-limit",
            Self::ProposalLimit => "proposal-limit",
            Self::NoReadyProposal => "no-ready-proposal",
        }
    }
}

/// Whole-field Cleanliness relative to the captured image, under the identity comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterFieldRelation {
    /// Every final pixel matches at its original position in the same viewing context.
    NotDirtier,
    /// Both complete, comparable fields were observed and differ; their order is unknown.
    Incomparable,
    /// No selected field, comparable observation, or common viewing context.
    Unavailable,
}

impl RasterFieldRelation {
    /// Stable spelling shared by renderer adapters.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotDirtier => "not-dirtier",
            Self::Incomparable => "incomparable",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Finite selection protocol for one ready ordinary frame and its reference.
#[derive(Debug)]
pub struct RasterSelection {
    action: RasterAction,
    ordinary_differs: bool,
    inverse_trial: RasterTrial,
}

impl RasterSelection {
    /// Starts from the complete ordinary readback, before any alternative is proposed.
    #[must_use]
    pub fn start(ordinary: Option<bool>, has_paints: bool) -> Self {
        let action = match ordinary {
            Some(true) => RasterAction::Preserved,
            None => RasterAction::IncomparableBitmap,
            Some(false) if has_paints => RasterAction::TryInverse,
            Some(false) => RasterAction::NoReferencePaints,
        };
        Self {
            action,
            ordinary_differs: ordinary == Some(false),
            inverse_trial: RasterTrial::NotReady,
        }
    }

    /// Current instruction for the renderer adapter.
    #[must_use]
    pub fn action(&self) -> RasterAction {
        self.action
    }

    /// Assesses only the selected field. The renderer supplies the viewing-context
    /// premise independently of pixel dimensions and any limits encountered by trials.
    #[must_use]
    pub fn field_relation(&self, same_viewing_context: bool) -> RasterFieldRelation {
        if !same_viewing_context {
            return RasterFieldRelation::Unavailable;
        }
        match self.action {
            RasterAction::TryInverse | RasterAction::TryVisible => RasterFieldRelation::Unavailable,
            RasterAction::Preserved | RasterAction::Restored => RasterFieldRelation::NotDirtier,
            _ if self.ordinary_differs => RasterFieldRelation::Incomparable,
            _ => RasterFieldRelation::Unavailable,
        }
    }

    /// Records one requested trial. A terminal decision cannot be changed.
    pub fn observe(&mut self, trial: RasterTrial) {
        use RasterAction::{TryInverse, TryVisible};
        self.action = match (self.action, trial) {
            (TryInverse | TryVisible, RasterTrial::SameField) => RasterAction::Restored,
            (TryInverse, _) => {
                self.inverse_trial = trial;
                TryVisible
            }
            (TryVisible, _) => match (self.inverse_trial, trial) {
                // Preserve the outstanding search or comparison limitation when another
                // trial happened to draw a different field or produced no ready frame.
                (RasterTrial::SearchLimit, _) | (_, RasterTrial::SearchLimit) => {
                    RasterAction::SearchLimit
                }
                (RasterTrial::ProposalLimit, _) | (_, RasterTrial::ProposalLimit) => {
                    RasterAction::ProposalLimit
                }
                (RasterTrial::Incomparable, _) | (_, RasterTrial::Incomparable) => {
                    RasterAction::IncomparableBitmap
                }
                (RasterTrial::DifferentField, _) | (_, RasterTrial::DifferentField) => {
                    RasterAction::DifferentBitmap
                }
                _ => RasterAction::NoReadyProposal,
            },
            (terminal, _) => terminal,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_keeps_geometry_opaque_pixels_and_the_immutable_reference()
    -> Result<(), RasterReferenceError> {
        let mut pixels = [20, 40, 60, 255, 10, 30, 50, 255];
        let owned = pixels.to_vec();
        let allocation = owned.as_ptr();
        let reference = OpaqueRasterReference::capture(2, 1, owned)?;
        assert_eq!(reference.rgba.as_ptr(), allocation);
        assert_eq!(reference.same_field(2, 1, &pixels), Some(true));
        assert_eq!(reference.same_field(1, 2, &pixels), None);
        assert_eq!(reference.same_field(2, 1, &pixels[..4]), None);
        pixels[0] += 1;
        assert_eq!(reference.same_field(2, 1, &pixels), Some(false));
        pixels[7] = 254;
        assert_eq!(reference.same_field(2, 1, &pixels), None);
        assert!(matches!(
            OpaqueRasterReference::capture(2, 1, pixels.to_vec()),
            Err(RasterReferenceError::InvalidBitmap)
        ));
        assert!(matches!(
            OpaqueRasterReference::capture(u32::MAX, u32::MAX, vec![]),
            Err(RasterReferenceError::InvalidBitmap)
        ));
        for (w, h, len) in [(0, 1, 0), (1, 0, 0), (2, 1, 4), (1, 1, 5)] {
            assert!(OpaqueRasterReference::capture(w, h, vec![255; len]).is_err());
        }
        Ok(())
    }

    #[test]
    fn inverse_uses_the_center_instead_of_the_canvas_rounding_edge() {
        // 80 and 81 both pass V3 on this backdrop. 80 produced 119 in Canvas.
        assert_eq!(source_over_channel_srgb8(80, 0.5, 159), 120);
        assert_eq!(
            raster_paint_proposal([1, 120, 200], [0, 159, 255], 0.5),
            Some([2, 81, 145])
        );
        assert_eq!(
            raster_paint_proposal([0, 120, 200], [255, 159, 255], 0.5),
            None
        );
        assert_eq!(raster_paint_proposal([128; 3], [0; 3], 0.5), Some([255; 3]));
        assert_eq!(
            raster_paint_proposal([0, 128, 255], [255, 10, 0], 1.0),
            Some([0, 128, 255])
        );
        for alpha in [0.0, -0.5, 1.01, f64::NAN, f64::INFINITY] {
            assert_eq!(raster_paint_proposal([80; 3], [80; 3], alpha), None);
        }
    }

    #[test]
    fn proposals_cover_the_quantized_range_on_the_supplied_backdrop() {
        // Independent oracle: enumerate every paint using integer dyadic opacity.
        for numerator in [1_u32, 16, 123, 128, 255, 256] {
            for b in [0_u8, 1, 63, 127, 159, 254, 255] {
                let visible = |s: u8| {
                    ((numerator * u32::from(s) + (256 - numerator) * u32::from(b) + 128) / 256)
                        as u8
                };
                let mut reachable = [false; 256];
                for source in 0..=255 {
                    reachable[usize::from(visible(source))] = true;
                }
                for desired in 0..=255 {
                    let proposal =
                        raster_paint_proposal([desired; 3], [b; 3], f64::from(numerator) / 256.0);
                    assert_eq!(
                        proposal.map(|p| p.map(visible)),
                        reachable[usize::from(desired)].then_some([desired; 3])
                    );
                }
            }
        }
    }

    #[test]
    fn only_a_matching_render_can_replace_the_ordinary_frame() {
        for (ordinary, paints, action) in [
            (Some(true), false, RasterAction::Preserved),
            (None, true, RasterAction::IncomparableBitmap),
            (Some(false), false, RasterAction::NoReferencePaints),
        ] {
            let mut selection = RasterSelection::start(ordinary, paints);
            assert_eq!(selection.action(), action);
            selection.observe(RasterTrial::SameField);
            assert_eq!(selection.action(), action);
        }
        let mut selection = RasterSelection::start(Some(false), true);
        assert_eq!(selection.action(), RasterAction::TryInverse);
        selection.observe(RasterTrial::SameField);
        assert_eq!(selection.action(), RasterAction::Restored);
        selection.observe(RasterTrial::SearchLimit);
        assert_eq!(selection.action(), RasterAction::Restored);
    }

    #[test]
    fn visible_trial_can_restore_after_every_inverse_failure_and_preserves_failure_reasons() {
        use RasterTrial::{
            DifferentField, Incomparable, NotReady, ProposalLimit, SameField, SearchLimit,
        };
        for (inverse, visible, action) in [
            (DifferentField, SameField, RasterAction::Restored),
            (Incomparable, SameField, RasterAction::Restored),
            (NotReady, SameField, RasterAction::Restored),
            (SearchLimit, SameField, RasterAction::Restored),
            (ProposalLimit, SameField, RasterAction::Restored),
            (NotReady, NotReady, RasterAction::NoReadyProposal),
            (DifferentField, NotReady, RasterAction::DifferentBitmap),
            (NotReady, DifferentField, RasterAction::DifferentBitmap),
            (
                DifferentField,
                Incomparable,
                RasterAction::IncomparableBitmap,
            ),
            (
                Incomparable,
                DifferentField,
                RasterAction::IncomparableBitmap,
            ),
            (SearchLimit, DifferentField, RasterAction::SearchLimit),
            (DifferentField, SearchLimit, RasterAction::SearchLimit),
            (ProposalLimit, ProposalLimit, RasterAction::ProposalLimit),
            (ProposalLimit, DifferentField, RasterAction::ProposalLimit),
            (NotReady, ProposalLimit, RasterAction::ProposalLimit),
            (SearchLimit, ProposalLimit, RasterAction::SearchLimit),
        ] {
            let mut selection = RasterSelection::start(Some(false), true);
            selection.observe(inverse);
            assert_eq!(selection.action(), RasterAction::TryVisible);
            selection.observe(visible);
            assert_eq!(selection.action(), action);
            selection.observe(SameField);
            assert_eq!(selection.action(), action);
        }
    }

    #[test]
    fn field_relation_belongs_to_the_returned_bitmap_in_the_declared_viewing_context() {
        use RasterFieldRelation::{Incomparable, NotDirtier, Unavailable};

        for (ordinary, paints, expected) in [
            (Some(true), false, NotDirtier),
            (Some(false), false, Incomparable),
            (None, true, Unavailable),
        ] {
            let choice = RasterSelection::start(ordinary, paints);
            assert_eq!(choice.field_relation(true), expected);
            assert_eq!(choice.field_relation(false), Unavailable);
        }
        for trial in [
            RasterTrial::DifferentField,
            RasterTrial::Incomparable,
            RasterTrial::SearchLimit,
            RasterTrial::ProposalLimit,
            RasterTrial::NotReady,
            RasterTrial::SameField,
        ] {
            let mut choice = RasterSelection::start(Some(false), true);
            assert_eq!(choice.field_relation(true), Unavailable);
            choice.observe(RasterTrial::NotReady);
            assert_eq!(choice.field_relation(true), Unavailable);
            choice.observe(trial);
            let expected = if trial == RasterTrial::SameField {
                NotDirtier
            } else {
                Incomparable
            };
            assert_eq!(choice.field_relation(true), expected);
            assert_eq!(choice.field_relation(false), Unavailable);
        }
    }
}
