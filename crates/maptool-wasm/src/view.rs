//! Picking, drawing data (palette, borders), and view-level grouping queries on
//! `MapDocument` — the read-only geometry side of the editor.

use maptool_core::ViewMode;
use wasm_bindgen::prelude::*;

use crate::support::{js_err, level_of};
use crate::{MapDocument, color, stats};

#[wasm_bindgen]
impl MapDocument {
    /// Source color of a province as 0xRRGGBB.
    pub fn color(&self, id: usize) -> Result<u32, JsError> {
        let [r, g, b] = self.info(id)?.color;
        Ok(color::pack_rgb(r, g, b))
    }

    #[wasm_bindgen(js_name = pixelCount)]
    pub fn pixel_count(&self, id: usize) -> Result<u32, JsError> {
        Ok(self.info(id)?.pixel_count)
    }

    /// `[x0, y0, x1, y1]` in pixels, upper bound exclusive.
    pub fn bbox(&self, id: usize) -> Result<Vec<u32>, JsError> {
        Ok(self.info(id)?.bbox.to_vec())
    }

    /// The province under the image point `(x, y)`, or -1.
    pub fn pick(&self, x: f64, y: f64) -> i32 {
        self.0.mesh.pick(x, y).map_or(-1, |id| id as i32)
    }

    /// RGBA palette for the fills, four bytes per province. `mode` is 0 original
    /// colors, 1 by state, 2 by type, 3 by biome, 4 by country, 5 by strategic region.
    /// `selected` provinces are tinted yellow and `hovered` ones white.
    pub fn palette(&self, mode: u8, selected: &[u32], hovered: &[u32]) -> Result<Vec<u8>, JsError> {
        let mode = ViewMode::from_u8(mode).ok_or_else(|| js_err(format!("unknown view mode {mode}")))?;
        Ok(self.0.palette(mode, selected, hovered))
    }

    /// Border segments (index pairs into `linePositions`) to draw at a view level (0
    /// provinces, 1 states, 2 countries, 3 strategic regions): every border except those
    /// between two provinces that are shown as one unit.
    #[wasm_bindgen(js_name = borderIndices)]
    pub fn border_indices(&self, level: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.border_indices(level_of(level)?))
    }

    /// Outline of the union of `ids`, as index pairs into `linePositions`.
    #[wasm_bindgen(js_name = boundaryIndices)]
    pub fn boundary_indices(&self, ids: &[u32]) -> Vec<u32> {
        self.0.boundary_indices(ids)
    }

    /// Totals over some provinces: `[provinces, land, sea, pixels, population, provinces with
    /// a population]`. Unknown ids are ignored.
    #[wasm_bindgen(js_name = provinceStats)]
    pub fn province_stats(&self, ids: &[u32]) -> Vec<f64> {
        stats::flatten_stats(&self.0.province_stats(ids))
    }

    /// The distinct states that some provinces are in, ascending; -1 stands for "in no state".
    #[wasm_bindgen(js_name = statesOf)]
    pub fn states_of(&self, ids: &[u32]) -> Vec<i32> {
        self.0.states_of(ids).into_iter().map(|s| s as i32).collect()
    }

    /// The biomes that the land provinces among `ids` have, as a bit mask: bit `i` is set for
    /// the biome number `i` (see `biomeNames`).
    #[wasm_bindgen(js_name = landBiomes)]
    pub fn land_biomes(&self, ids: &[u32]) -> u32 {
        self.0.land_biomes(ids)
    }

    /// The provinces that highlight together with `province` at a view level (0 provinces,
    /// 1 states, 2 countries, 3 strategic regions): everything in the unit it is shown as.
    #[wasm_bindgen(js_name = groupProvinces)]
    pub fn group_provinces(&self, province: u32, level: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.group_provinces(province, level_of(level)?))
    }

    /// The units, at a view level, that `provinces` belong to, as `[kind, id, kind, id, ...]`
    /// with kind 0 province, 1 state, 2 country, 3 strategic region; each once.
    #[wasm_bindgen(js_name = unitsOf)]
    pub fn units_of(&self, provinces: &[u32], level: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.units_of(provinces, level_of(level)?))
    }

    /// The layer objects that `provinces` belong to at a view level, as `[kind, id, ...]`:
    /// provinces at level 0, states at 1, countries at 2, strategic regions at 3. Provinces
    /// that belong to no such object (a province in no state, at level 1) are skipped. This is
    /// what a click or a box can select in a view.
    #[wasm_bindgen(js_name = layerUnitsOf)]
    pub fn layer_units_of(&self, provinces: &[u32], level: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.layer_units_of(provinces, level_of(level)?))
    }

    /// The provinces of the layer object `province` belongs to at a view level; empty when it
    /// belongs to none. This is what lights up under the pointer in a view.
    #[wasm_bindgen(js_name = layerProvinces)]
    pub fn layer_provinces(&self, province: u32, level: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.layer_provinces(province, level_of(level)?))
    }

    /// The provinces of some units given as `[kind, id, ...]` (see `unitsOf`), ascending.
    #[wasm_bindgen(js_name = provincesOfUnits)]
    pub fn provinces_of_units(&self, units: &[u32]) -> Vec<u32> {
        self.0.provinces_of_units(units)
    }
}
