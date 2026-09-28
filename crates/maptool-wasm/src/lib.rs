use js_sys::{Float32Array, Uint32Array};
use maptool_core::{Biome, Document, Kind, Options, PixelFormat, ViewMode};
use wasm_bindgen::prelude::*;

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
        Ok(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
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
    /// colors, 1 by state, 2 by type, 3 by biome. `selected` provinces are tinted
    /// yellow and `hovered` ones white.
    pub fn palette(&self, mode: u8, selected: &[u32], hovered: &[u32]) -> Result<Vec<u8>, JsError> {
        let mode = ViewMode::from_u8(mode).ok_or_else(|| js_err(format!("unknown view mode {mode}")))?;
        Ok(self.0.palette(mode, selected, hovered))
    }

    /// Border segments (index pairs into `linePositions`) to draw in the state view:
    /// every border except those between two provinces of the same state.
    #[wasm_bindgen(js_name = stateBorderIndices)]
    pub fn state_border_indices(&self) -> Vec<u32> {
        self.0.state_border_indices()
    }

    /// Outline of the union of `ids`, as index pairs into `linePositions`.
    #[wasm_bindgen(js_name = boundaryIndices)]
    pub fn boundary_indices(&self, ids: &[u32]) -> Vec<u32> {
        self.0.mesh.boundary_indices(ids)
    }

    /// The provinces that highlight together with `province` in the state view:
    /// its whole state, or just itself when it is in no state.
    #[wasm_bindgen(js_name = groupProvinces)]
    pub fn group_provinces(&self, province: u32) -> Vec<u32> {
        self.0.group_provinces(province)
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
        Ok(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
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
        Ok(vec![s.provinces as f64, s.land as f64, s.sea as f64, s.pixels as f64, s.population as f64, s.populated as f64])
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
        self.0.states.set_color(id, [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]).map_err(js_err)
    }

    /// Delete a state; its provinces become unassigned.
    #[wasm_bindgen(js_name = deleteState)]
    pub fn delete_state(&mut self, id: u32) -> Result<(), JsError> {
        self.0.states.delete(id).map_err(js_err)
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
