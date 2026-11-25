//! Native access to Core's comparison of a renderer's opaque sRGB8 readbacks.
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use labcolors_core::program_wire::{
    OpaqueRasterReference, RasterSelection, RasterTrial, raster_paint_proposal,
};

#[derive(uniffi::Object)]
pub struct RasterReference(OpaqueRasterReference);

/// Retains the complete row-major opaque sRGB RGBA8 image. Invalid readbacks have no reference.
#[uniffi::export]
pub fn capture_raster_reference(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Option<Arc<RasterReference>> {
    OpaqueRasterReference::capture(width, height, rgba)
        .ok()
        .map(|reference| Arc::new(RasterReference(reference)))
}

#[uniffi::export]
impl RasterReference {
    /// Compare the ordinary frame and retain this reference for every subsequent trial.
    pub fn start(
        self: Arc<Self>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        has_paints: bool,
    ) -> Arc<RasterChoice> {
        let same = self.0.same_field(width, height, &rgba);
        Arc::new(RasterChoice {
            reference: self,
            selection: Mutex::new(RasterSelection::start(same, has_paints)),
        })
    }
}

#[derive(uniffi::Object)]
pub struct RasterChoice {
    reference: Arc<RasterReference>,
    selection: Mutex<RasterSelection>,
}

impl RasterChoice {
    fn selection(&self) -> MutexGuard<'_, RasterSelection> {
        // Only Core's finite transition is mutated under this lock. Renderer callbacks run outside it.
        self.selection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
impl RasterChoice {
    pub fn action(&self) -> String {
        self.selection().action().as_str().into()
    }

    pub fn inspect(&self, width: u32, height: u32, rgba: Vec<u8>) {
        let same = self.reference.0.same_field(width, height, &rgba);
        self.selection().observe(same.into());
    }

    pub fn proposal_limit(&self) {
        self.selection().observe(RasterTrial::ProposalLimit);
    }

    pub fn refused(&self, search_limit: bool) {
        self.selection().observe(if search_limit {
            RasterTrial::SearchLimit
        } else {
            RasterTrial::NotReady
        });
    }
}

/// Invert one source-over use on the supplied backdrop. A reachable paint is packed as RGB24.
#[uniffi::export]
pub fn paint_proposal(desired: Vec<u8>, backdrop: Vec<u8>, alpha: f64) -> Option<u32> {
    let [r, g, b] =
        raster_paint_proposal(desired.try_into().ok()?, backdrop.try_into().ok()?, alpha)?;
    Some(u32::from_be_bytes([0, r, g, b]))
}
