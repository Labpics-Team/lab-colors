use labcolors_core::program_wire::{
    OpaqueRasterReference, RasterSelection, RasterTrial, raster_paint_proposal,
};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct RasterReference(Rc<OpaqueRasterReference>);

fn extent(width: JsValue, height: JsValue) -> Option<(u32, u32)> {
    super::checked_u32(width).zip(super::checked_u32(height))
}

#[wasm_bindgen(js_name = captureRasterReference)]
pub fn capture_raster_reference(
    width: JsValue,
    height: JsValue,
    rgba: Vec<u8>,
) -> Option<RasterReference> {
    let (w, h) = extent(width, height)?;
    Some(RasterReference(Rc::new(
        OpaqueRasterReference::capture(w, h, rgba).ok()?,
    )))
}

#[wasm_bindgen]
impl RasterReference {
    pub fn start(
        &self,
        width: JsValue,
        height: JsValue,
        rgba: &[u8],
        has_paints: bool,
    ) -> RasterChoice {
        let same = extent(width, height).and_then(|(w, h)| self.0.same_field(w, h, rgba));
        RasterChoice {
            reference: Rc::clone(&self.0),
            selection: RasterSelection::start(same, has_paints),
        }
    }
}

/// Scoped driver: JavaScript renders only the proposals requested by Core.
#[wasm_bindgen]
pub struct RasterChoice {
    reference: Rc<OpaqueRasterReference>,
    selection: RasterSelection,
}

#[wasm_bindgen]
impl RasterChoice {
    pub fn action(&self) -> String {
        self.selection.action().as_str().into()
    }

    #[wasm_bindgen(js_name = fieldRelation)]
    pub fn field_relation(&self, same_viewing_context: bool) -> String {
        self.selection
            .field_relation(same_viewing_context)
            .as_str()
            .into()
    }

    pub fn inspect(&mut self, width: JsValue, height: JsValue, rgba: &[u8]) {
        let same = extent(width, height).and_then(|(w, h)| self.reference.same_field(w, h, rgba));
        self.selection.observe(same.into());
    }

    #[wasm_bindgen(js_name = proposalLimit)]
    pub fn proposal_limit(&mut self) {
        self.selection.observe(RasterTrial::ProposalLimit);
    }

    pub fn refused(&mut self, search_limit: bool) {
        self.selection.observe(if search_limit {
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
