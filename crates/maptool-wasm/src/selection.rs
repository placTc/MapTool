//! Box and filtered province selection on `MapDocument`.

use wasm_bindgen::prelude::*;

use crate::MapDocument;

#[wasm_bindgen]
impl MapDocument {
    /// The provinces in the rectangle `(x0, y0)`-`(x1, y1)` (image pixels, any corner
    /// order): those it touches, or with `whole` only those entirely inside it, then
    /// filtered by `flags` (see `filterFlags`).
    #[wasm_bindgen(js_name = provincesInRect)]
    pub fn provinces_in_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64, whole: bool, flags: u32) -> Vec<u32> {
        self.0.provinces_in_rect(x0, y0, x1, y1, whole, flags)
    }

    /// The provinces of `ids` that pass `flags` (see `filterFlags`), ascending, each once.
    #[wasm_bindgen(js_name = filterProvinces)]
    pub fn filter_provinces(&self, ids: &[u32], flags: u32) -> Vec<u32> {
        self.0.filter_provinces(ids, flags)
    }

    /// The provinces that are in no state.
    #[wasm_bindgen(js_name = unassignedProvinces)]
    pub fn unassigned_provinces(&self) -> Vec<u32> {
        self.0.unassigned_provinces()
    }
}

/// The bits that `provincesInRect` and `filterProvinces` take: `[skip provinces already
/// in a state, land only, sea only]`.
#[wasm_bindgen(js_name = filterFlags)]
pub fn filter_flags() -> Vec<u32> {
    vec![maptool_core::filter::SKIP_IN_STATES, maptool_core::filter::LAND_ONLY, maptool_core::filter::SEA_ONLY]
}
