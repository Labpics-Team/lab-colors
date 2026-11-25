use labcolors_core::program_wire::{
    OpaqueRasterReference, RasterAction, RasterSelection, RasterTrial, raster_paint_proposal,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct RasterReference(OpaqueRasterReference);

fn extent(width: JsValue, height: JsValue) -> Option<(u32, u32)> {
    super::checked_u32(width).zip(super::checked_u32(height))
}

#[wasm_bindgen]
impl RasterReference {
    pub fn capture(width: JsValue, height: JsValue, rgba: Vec<u8>) -> Option<RasterReference> {
        let (w, h) = extent(width, height)?;
        OpaqueRasterReference::capture(w, h, rgba).ok().map(Self)
    }

    pub fn start(
        &self,
        width: JsValue,
        height: JsValue,
        rgba: &[u8],
        has_paints: bool,
    ) -> RasterChoice {
        let same = extent(width, height).and_then(|(w, h)| self.0.same_field(w, h, rgba));
        RasterChoice(RasterSelection::start(same, has_paints))
    }
}

/// Scoped driver: JavaScript renders only the proposals requested by Core.
#[wasm_bindgen]
pub struct RasterChoice(RasterSelection);

#[wasm_bindgen]
impl RasterChoice {
    pub fn action(&self) -> String {
        match self.0.action() {
            RasterAction::TryInverse => "try-inverse",
            RasterAction::TryVisible => "try-visible",
            RasterAction::Preserved => "preserved",
            RasterAction::Restored => "restored",
            RasterAction::IncomparableBitmap => "incomparable-bitmap",
            RasterAction::NoReferencePaints => "no-reference-paints",
            RasterAction::DifferentBitmap => "different-bitmap",
            RasterAction::SearchLimit => "search-limit",
            RasterAction::ProposalLimit => "proposal-limit",
            RasterAction::NoReadyProposal => "no-ready-proposal",
        }
        .into()
    }

    pub fn inspect(
        &mut self,
        reference: &RasterReference,
        width: JsValue,
        height: JsValue,
        rgba: &[u8],
    ) {
        let same = extent(width, height).and_then(|(w, h)| reference.0.same_field(w, h, rgba));
        self.0.observe(match same {
            Some(true) => RasterTrial::SameField,
            Some(false) => RasterTrial::DifferentField,
            None => RasterTrial::Incomparable,
        });
    }

    #[wasm_bindgen(js_name = proposalLimit)]
    pub fn proposal_limit(&mut self) {
        self.0.observe(RasterTrial::ProposalLimit);
    }

    pub fn refused(&mut self, search_limit: bool) {
        self.0.observe(if search_limit {
            RasterTrial::SearchLimit
        } else {
            RasterTrial::NotReady
        });
    }
}

/// Wire projection of one opaque/alpha proposal, packed as RGB24. Absence is kept distinct from black.
#[wasm_bindgen(js_name = rasterPaintProposal)]
pub fn paint_proposal(desired: &[u8], backdrop: &[u8], alpha: f64) -> Option<u32> {
    let [r, g, b] =
        raster_paint_proposal(desired.try_into().ok()?, backdrop.try_into().ok()?, alpha)?;
    Some(u32::from_be_bytes([0, r, g, b]))
}
