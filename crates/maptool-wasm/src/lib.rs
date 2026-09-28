use js_sys::{Float32Array, Uint32Array};
use maptool_core::{Biome, Document, GroupKind, Kind, Level, Options, PixelFormat, ViewMode};
use wasm_bindgen::prelude::*;

mod color;
mod stats;

/// Tuning knobs. Create with `new Settings()` and assign the fields you care about.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Settings {
    /// Simplification in pixels; 0 gives exact pixel edges.
    pub tolerance: f64,
    pub corner_angle: f64,
    pub corner_run: f64,
    pub min_chain_len: u32,
    /// Decimal places in SVG output.
    pub precision: u32,
    /// Reject single-pixel exclaves and four-way junctions.
    pub validate: bool,
    /// Meshes only: how far flattened curves may stray from the true border, in pixels.
    pub flatten_tolerance: f64,
}

#[wasm_bindgen]
impl Settings {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Settings {
        let d = Options::default();
        Settings {
            tolerance: d.tolerance,
            corner_angle: d.corner_angle,
            corner_run: d.corner_run,
            min_chain_len: d.min_chain_len as u32,
            precision: d.precision as u32,
            validate: d.validate,
            flatten_tolerance: 0.03,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::new()
    }
}

impl Settings {
    fn options(&self) -> Options {
        Options {
            tolerance: self.tolerance,
            corner_angle: self.corner_angle,
            min_chain_len: self.min_chain_len as usize,
            precision: self.precision as usize,
            corner_run: self.corner_run,
            validate: self.validate,
        }
    }
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

// ------------------------------------------------------------------ SVG

/// A vectorized map as SVG paths.
#[wasm_bindgen]
pub struct VectorMap(maptool_core::VectorMap);

#[wasm_bindgen]
impl VectorMap {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.0.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// Number of provinces; valid ids are `0..len`.
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.0.provinces.len()
    }

    #[wasm_bindgen(getter, js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.0.provinces.is_empty()
    }

    /// SVG path data of a province.
    pub fn path(&self, id: usize) -> Result<String, JsError> {
        self.0.provinces.get(id).map(|p| p.path.clone()).ok_or_else(|| js_err(format!("no province {id}")))
    }

    /// The whole map as a standalone SVG document.
    #[wasm_bindgen(js_name = toSvg)]
    pub fn to_svg(&self) -> String {
        self.0.to_svg()
    }
}

/// Vectorize RGBA pixels (e.g. `ImageData.data`) into SVG paths.
#[wasm_bindgen]
pub fn vectorize(rgba: &[u8], width: u32, height: u32, settings: &Settings) -> Result<VectorMap, JsError> {
    maptool_core::vectorize(rgba, width, height, PixelFormat::Rgba, &settings.options()).map(VectorMap).map_err(js_err)
}

/// Decode a PNG or BMP file and vectorize it into SVG paths.
#[wasm_bindgen(js_name = vectorizeImage)]
pub fn vectorize_image(bytes: &[u8], settings: &Settings) -> Result<VectorMap, JsError> {
    let (rgba, w, h) = maptool_core::decode_image(bytes).map_err(js_err)?;
    vectorize(&rgba, w, h, settings)
}

// ------------------------------------------------------------- the document

/// A map being worked on: GPU-ready geometry plus the user's states and province
/// metadata. Everything that can be computed is computed here in Rust.
///
/// The buffer getters return views straight into WASM memory, so nothing is
/// copied. A view is only valid until WASM memory next grows, so hand it to
/// `gl.bufferData` right away and do not keep it.
#[wasm_bindgen]
pub struct MapDocument(Document);

#[wasm_bindgen]
impl MapDocument {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.0.mesh.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.0.mesh.height
    }

    /// Number of provinces; valid ids are `0..len`.
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.0.mesh.provinces.len()
    }

    #[wasm_bindgen(getter, js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.0.mesh.provinces.is_empty()
    }

    // ---- geometry and drawing

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

    // ---- box selection

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

    /// Triangle vertices, x and y interleaved.
    pub fn positions(&self) -> Float32Array {
        // SAFETY: the view is consumed by the caller before any WASM allocation.
        unsafe { Float32Array::view(&self.0.mesh.positions) }
    }

    /// Province id of each triangle vertex.
    #[wasm_bindgen(js_name = vertexProvince)]
    pub fn vertex_province(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.vertex_province) }
    }

    pub fn indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.indices) }
    }

    /// Border points, x and y interleaved.
    #[wasm_bindgen(js_name = linePositions)]
    pub fn line_positions(&self) -> Float32Array {
        unsafe { Float32Array::view(&self.0.mesh.line_positions) }
    }

    /// Every border segment as index pairs into `linePositions`, for `gl.LINES`.
    #[wasm_bindgen(js_name = lineIndices)]
    pub fn line_indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.line_indices) }
    }

    // ---- the map's name

    /// The map's name, or an empty string when it has none.
    #[wasm_bindgen(getter, js_name = mapName)]
    pub fn map_name(&self) -> String {
        self.0.name().to_string()
    }

    /// Name the map. Blank clears it. The name is saved with the edits and the file.
    #[wasm_bindgen(js_name = setMapName)]
    pub fn set_map_name(&mut self, name: &str) {
        self.0.set_name(name);
    }

    // ---- province metadata

    /// The province's number: the one imported from a CSV, or else its position in the map.
    /// This is what an unnamed province is called.
    #[wasm_bindgen(js_name = provinceNumber)]
    pub fn province_number(&self, id: u32) -> u32 {
        self.0.provinces.number(id)
    }

    /// Apply a CSV of hex colors with province types and IDs (see the README for the
    /// format). Rows are matched to provinces by source color. Throws, changing nothing,
    /// if no row can be applied.
    #[wasm_bindgen(js_name = importCsv)]
    pub fn import_csv(&mut self, text: &str) -> Result<CsvReport, JsError> {
        self.0.import_csv(text).map(CsvReport).map_err(js_err)
    }

    /// The province's name, or its number when it has none.
    #[wasm_bindgen(js_name = provinceName)]
    pub fn province_name(&self, id: u32) -> String {
        self.0.provinces.display_name(id)
    }

    /// The name as typed, or undefined when the province has none.
    #[wasm_bindgen(js_name = provinceOwnName)]
    pub fn province_own_name(&self, id: u32) -> Option<String> {
        self.0.provinces.get(id).and_then(|m| m.name.clone())
    }

    #[wasm_bindgen(js_name = provinceDescription)]
    pub fn province_description(&self, id: u32) -> String {
        self.0.provinces.get(id).and_then(|m| m.description.clone()).unwrap_or_default()
    }

    /// 0 land, 1 sea.
    #[wasm_bindgen(js_name = provinceKind)]
    pub fn province_kind(&self, id: u32) -> u8 {
        self.0.provinces.get(id).map_or(0, |m| m.kind as u8)
    }

    /// Index into `biomeNames()`. Always the last one (Sea) for a sea province.
    #[wasm_bindgen(js_name = provinceBiome)]
    pub fn province_biome(&self, id: u32) -> u8 {
        self.0.provinces.get(id).map_or(0, |m| m.biome() as u8)
    }

    /// The population, or undefined when it is not set.
    #[wasm_bindgen(js_name = provincePopulation)]
    pub fn province_population(&self, id: u32) -> Option<f64> {
        self.0.provinces.get(id).and_then(|m| m.population).map(|p| p as f64)
    }

    /// Blank text clears the name (the province shows its number again).
    #[wasm_bindgen(js_name = setProvinceName)]
    pub fn set_province_name(&mut self, id: u32, name: &str) -> Result<(), JsError> {
        self.0.provinces.set_name(id, name).map_err(js_err)
    }

    #[wasm_bindgen(js_name = setProvinceDescription)]
    pub fn set_province_description(&mut self, id: u32, text: &str) -> Result<(), JsError> {
        self.0.provinces.set_description(id, text).map_err(js_err)
    }

    /// `undefined` clears the population; otherwise it must be a whole number of at least 0.
    #[wasm_bindgen(js_name = setProvincePopulation)]
    pub fn set_province_population(&mut self, id: u32, population: Option<f64>) -> Result<(), JsError> {
        let value = match population {
            None => None,
            Some(p) if p.is_finite() && p >= 0.0 && p.fract() == 0.0 && p <= 9_007_199_254_740_991.0 => Some(p as u64),
            Some(_) => return Err(js_err("population must be a whole number, 0 or more")),
        };
        self.0.provinces.set_population(id, value).map_err(js_err)
    }

    /// Set land (0) or sea (1) on several provinces at once.
    #[wasm_bindgen(js_name = setKind)]
    pub fn set_kind(&mut self, ids: &[u32], kind: u8) -> Result<(), JsError> {
        let kind = Kind::from_u8(kind).ok_or_else(|| js_err(format!("unknown type {kind}")))?;
        self.0.provinces.set_kind(ids, kind).map_err(js_err)
    }

    /// Set the biome (an index into `biomeNames()`) of the land provinces among `ids`.
    /// Sea provinces are skipped: their biome is locked to Sea. Sea itself cannot be
    /// chosen; a province gets it by being made sea with `setKind`.
    #[wasm_bindgen(js_name = setBiome)]
    pub fn set_biome(&mut self, ids: &[u32], biome: u8) -> Result<(), JsError> {
        let biome = Biome::from_u8(biome).ok_or_else(|| js_err(format!("unknown biome {biome}")))?;
        self.0.provinces.set_biome(ids, biome).map_err(js_err)
    }

    // ---- states

    #[wasm_bindgen(getter, js_name = stateCount)]
    pub fn state_count(&self) -> usize {
        self.0.states.len()
    }

    /// State ids in creation order.
    #[wasm_bindgen(js_name = stateIds)]
    pub fn state_ids(&self) -> Vec<u32> {
        self.0.states.iter().map(|s| s.id).collect()
    }

    #[wasm_bindgen(js_name = stateName)]
    pub fn state_name(&self, id: u32) -> Result<String, JsError> {
        Ok(self.state(id)?.name.clone())
    }

    /// State color as 0xRRGGBB.
    #[wasm_bindgen(js_name = stateColor)]
    pub fn state_color(&self, id: u32) -> Result<u32, JsError> {
        let [r, g, b] = self.state(id)?.color;
        Ok(color::pack_rgb(r, g, b))
    }

    /// The province ids in a state, ascending.
    #[wasm_bindgen(js_name = stateProvinces)]
    pub fn state_provinces(&self, id: u32) -> Result<Vec<u32>, JsError> {
        Ok(self.state(id)?.provinces.clone())
    }

    /// The id of the state holding a province, or -1.
    #[wasm_bindgen(js_name = stateOf)]
    pub fn state_of(&self, province: u32) -> i32 {
        self.0.states.state_of(province).map_or(-1, |s| s as i32)
    }

    /// `[provinces, land, sea, pixels, population, provinces with a population]`.
    #[wasm_bindgen(js_name = stateStats)]
    pub fn state_stats(&self, id: u32) -> Result<Vec<f64>, JsError> {
        let s = self.0.state_stats(id).ok_or_else(|| js_err(format!("no state {id}")))?;
        Ok(stats::flatten_stats(&s))
    }

    /// Create a state from `provinces`, taking them out of any state they were in.
    /// Returns the new state's id.
    #[wasm_bindgen(js_name = createState)]
    pub fn create_state(&mut self, name: &str, provinces: &[u32]) -> Result<u32, JsError> {
        self.0.states.create(name, provinces).map_err(js_err)
    }

    /// Put `provinces` into a state, taking them out of any other state.
    #[wasm_bindgen(js_name = assignToState)]
    pub fn assign_to_state(&mut self, id: u32, provinces: &[u32]) -> Result<(), JsError> {
        self.0.states.assign(id, provinces).map_err(js_err)
    }

    /// Take `provinces` out of their states.
    pub fn unassign(&mut self, provinces: &[u32]) -> Result<(), JsError> {
        self.0.states.unassign(provinces).map_err(js_err)
    }

    #[wasm_bindgen(js_name = renameState)]
    pub fn rename_state(&mut self, id: u32, name: &str) -> Result<(), JsError> {
        self.0.states.rename(id, name).map_err(js_err)
    }

    /// `rgb` is 0xRRGGBB.
    #[wasm_bindgen(js_name = setStateColor)]
    pub fn set_state_color(&mut self, id: u32, rgb: u32) -> Result<(), JsError> {
        self.0.states.set_color(id, color::unpack_rgb(rgb)).map_err(js_err)
    }

    /// Delete a state; its provinces become unassigned, and it leaves its country and
    /// strategic region.
    #[wasm_bindgen(js_name = deleteState)]
    pub fn delete_state(&mut self, id: u32) -> Result<(), JsError> {
        self.0.delete_state(id).map_err(js_err)
    }

    #[wasm_bindgen(js_name = stateDescription)]
    pub fn state_description(&self, id: u32) -> Result<String, JsError> {
        Ok(self.state(id)?.description.clone())
    }

    /// Blank text clears the description.
    #[wasm_bindgen(js_name = setStateDescription)]
    pub fn set_state_description(&mut self, id: u32, text: &str) -> Result<(), JsError> {
        self.0.states.set_description(id, text).map_err(js_err)
    }

    // ---- countries and strategic regions
    //
    // Both are groups of states, and every method takes `kind`: 0 for countries, 1 for
    // strategic regions. A state is in at most one group of each kind.

    /// Group ids in creation order.
    #[wasm_bindgen(js_name = groupIds)]
    pub fn group_ids(&self, kind: u8) -> Result<Vec<u32>, JsError> {
        Ok(self.0.groups(kind_of(kind)?).iter().map(|g| g.id).collect())
    }

    #[wasm_bindgen(js_name = groupName)]
    pub fn group_name(&self, kind: u8, id: u32) -> Result<String, JsError> {
        Ok(self.group(kind, id)?.name.clone())
    }

    #[wasm_bindgen(js_name = groupDescription)]
    pub fn group_description(&self, kind: u8, id: u32) -> Result<String, JsError> {
        Ok(self.group(kind, id)?.description.clone())
    }

    /// Group color as 0xRRGGBB.
    #[wasm_bindgen(js_name = groupColor)]
    pub fn group_color(&self, kind: u8, id: u32) -> Result<u32, JsError> {
        let [r, g, b] = self.group(kind, id)?.color;
        Ok(color::pack_rgb(r, g, b))
    }

    /// A country's three-letter tag, or "" if none is set. Always "" for a strategic region.
    #[wasm_bindgen(js_name = groupTag)]
    pub fn group_tag(&self, kind: u8, id: u32) -> Result<String, JsError> {
        Ok(self.group(kind, id)?.tag.clone())
    }

    /// Set a country's tag (three letters, case-insensitively unique among countries), or
    /// clear it with a blank string. Errors for a strategic region.
    #[wasm_bindgen(js_name = setGroupTag)]
    pub fn set_group_tag(&mut self, kind: u8, id: u32, tag: &str) -> Result<(), JsError> {
        self.0.groups_mut(kind_of(kind)?).set_tag(id, tag).map_err(js_err)
    }

    /// The state ids in a group, ascending.
    #[wasm_bindgen(js_name = groupStates)]
    pub fn group_states(&self, kind: u8, id: u32) -> Result<Vec<u32>, JsError> {
        Ok(self.group(kind, id)?.states.clone())
    }

    /// The id of the group holding a state, or -1.
    #[wasm_bindgen(js_name = groupOfState)]
    pub fn group_of_state(&self, kind: u8, state: u32) -> Result<i32, JsError> {
        Ok(self.0.groups(kind_of(kind)?).group_of(state).map_or(-1, |g| g as i32))
    }

    /// All provinces in the states of a group, ascending.
    #[wasm_bindgen(js_name = provincesOfGroup)]
    pub fn provinces_of_group(&self, kind: u8, id: u32) -> Result<Vec<u32>, JsError> {
        Ok(self.0.provinces_of_group(kind_of(kind)?, id))
    }

    /// `[states, provinces, land, sea, pixels, population, provinces with a population]`.
    #[wasm_bindgen(js_name = groupStats)]
    pub fn group_stats(&self, kind: u8, id: u32) -> Result<Vec<f64>, JsError> {
        let k = kind_of(kind)?;
        let s = self.0.group_stats(k, id).ok_or_else(|| js_err(format!("no {} {id}", k.noun())))?;
        let states = self.group(kind, id)?.states.len();
        Ok(std::iter::once(states as f64).chain(stats::flatten_stats(&s)).collect())
    }

    /// Create a group from `states`, taking them out of the group of this kind they were
    /// in. Returns its id.
    #[wasm_bindgen(js_name = createGroup)]
    pub fn create_group(&mut self, kind: u8, name: &str, states: &[u32]) -> Result<u32, JsError> {
        self.0.create_group(kind_of(kind)?, name, states).map_err(js_err)
    }

    /// Put `states` into a group, taking them out of the group of this kind they were in.
    #[wasm_bindgen(js_name = assignToGroup)]
    pub fn assign_to_group(&mut self, kind: u8, id: u32, states: &[u32]) -> Result<(), JsError> {
        self.0.assign_to_group(kind_of(kind)?, id, states).map_err(js_err)
    }

    /// Take `states` out of their group of this kind.
    #[wasm_bindgen(js_name = unassignFromGroups)]
    pub fn unassign_from_groups(&mut self, kind: u8, states: &[u32]) -> Result<(), JsError> {
        self.0.unassign_from_groups(kind_of(kind)?, states).map_err(js_err)
    }

    /// A blank name is ignored.
    #[wasm_bindgen(js_name = renameGroup)]
    pub fn rename_group(&mut self, kind: u8, id: u32, name: &str) -> Result<(), JsError> {
        self.0.groups_mut(kind_of(kind)?).rename(id, name).map_err(js_err)
    }

    /// Blank text clears the description.
    #[wasm_bindgen(js_name = setGroupDescription)]
    pub fn set_group_description(&mut self, kind: u8, id: u32, text: &str) -> Result<(), JsError> {
        self.0.groups_mut(kind_of(kind)?).set_description(id, text).map_err(js_err)
    }

    /// `rgb` is 0xRRGGBB.
    #[wasm_bindgen(js_name = setGroupColor)]
    pub fn set_group_color(&mut self, kind: u8, id: u32, rgb: u32) -> Result<(), JsError> {
        self.0.groups_mut(kind_of(kind)?).set_color(id, color::unpack_rgb(rgb)).map_err(js_err)
    }

    /// Delete a group; its states become ungrouped.
    #[wasm_bindgen(js_name = deleteGroup)]
    pub fn delete_group(&mut self, kind: u8, id: u32) -> Result<(), JsError> {
        self.0.groups_mut(kind_of(kind)?).delete(id).map_err(js_err)
    }

    // ---- saving

    /// The whole map as a file: geometry plus states and province metadata.
    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes()
    }

    /// Only the states and province metadata: small, for autosaving edits.
    #[wasm_bindgen(js_name = editsBytes)]
    pub fn edits_bytes(&self) -> Vec<u8> {
        self.0.edits_to_bytes()
    }

    /// Replace the states and province metadata with saved edits. Nothing changes
    /// if the bytes are invalid or belong to a different map.
    #[wasm_bindgen(js_name = setEditsBytes)]
    pub fn set_edits_bytes(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        self.0.set_edits_from_bytes(bytes).map_err(js_err)
    }

    fn info(&self, id: usize) -> Result<&maptool_core::ProvinceInfo, JsError> {
        self.0.mesh.provinces.get(id).ok_or_else(|| js_err(format!("no province {id}")))
    }

    fn state(&self, id: u32) -> Result<&maptool_core::State, JsError> {
        self.0.states.get(id).ok_or_else(|| js_err(format!("no state {id}")))
    }

    fn group(&self, kind: u8, id: u32) -> Result<&maptool_core::Group, JsError> {
        let k = kind_of(kind)?;
        self.0.groups(k).get(id).ok_or_else(|| js_err(format!("no {} {id}", k.noun())))
    }
}

fn level_of(level: u8) -> Result<Level, JsError> {
    Level::from_u8(level).ok_or_else(|| js_err(format!("unknown view level {level}")))
}

fn kind_of(kind: u8) -> Result<GroupKind, JsError> {
    GroupKind::from_u8(kind).ok_or_else(|| js_err(format!("unknown group kind {kind}")))
}

/// The bits that `provincesInRect` and `filterProvinces` take: `[skip provinces already
/// in a state, land only, sea only]`.
#[wasm_bindgen(js_name = filterFlags)]
pub fn filter_flags() -> Vec<u32> {
    vec![maptool_core::filter::SKIP_IN_STATES, maptool_core::filter::LAND_ONLY, maptool_core::filter::SEA_ONLY]
}

/// What a CSV import did.
#[wasm_bindgen]
pub struct CsvReport(maptool_core::CsvReport);

#[wasm_bindgen]
impl CsvReport {
    /// Data rows read, not counting a header.
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> usize {
        self.0.rows
    }

    /// Rows applied to a province.
    #[wasm_bindgen(getter)]
    pub fn matched(&self) -> usize {
        self.0.matched
    }

    #[wasm_bindgen(getter)]
    pub fn land(&self) -> usize {
        self.0.land
    }

    #[wasm_bindgen(getter)]
    pub fn sea(&self) -> usize {
        self.0.sea
    }

    /// Provinces of the map that no applied row mentioned.
    #[wasm_bindgen(getter)]
    pub fn unlisted(&self) -> usize {
        self.0.unlisted
    }

    #[wasm_bindgen(getter, js_name = hasTypes)]
    pub fn has_types(&self) -> bool {
        self.0.has_types
    }

    #[wasm_bindgen(getter, js_name = hasIds)]
    pub fn has_ids(&self) -> bool {
        self.0.has_ids
    }

    /// The true number of problems, which may be larger than `problems().length`.
    #[wasm_bindgen(getter, js_name = problemCount)]
    pub fn problem_count(&self) -> usize {
        self.0.problem_count
    }

    /// The problems found, one line each (at most 200).
    pub fn problems(&self) -> Vec<String> {
        self.0.problems.clone()
    }
}

/// Names of the biomes, in the order `setBiome` and `provinceBiome` number them. The
/// last one is Sea, which only sea provinces have.
#[wasm_bindgen(js_name = biomeNames)]
pub fn biome_names() -> Vec<String> {
    maptool_core::biome_names().into_iter().map(String::from).collect()
}

/// Open a map from a PNG or BMP image (built from scratch with `settings`), or from
/// a file saved by `toBytes` (which ignores `settings`).
#[wasm_bindgen(js_name = openMap)]
pub fn open_map(bytes: &[u8], settings: &Settings) -> Result<MapDocument, JsError> {
    if maptool_core::is_map_file(bytes) {
        return Document::from_bytes(bytes).map(MapDocument).map_err(js_err);
    }
    let (rgba, w, h) = maptool_core::decode_image(bytes).map_err(js_err)?;
    let mesh = maptool_core::mesh(&rgba, w, h, PixelFormat::Rgba, &settings.options(), settings.flatten_tolerance).map_err(js_err)?;
    Ok(MapDocument(Document::new(mesh)))
}
