use maptool_core::{Document, Options, PixelFormat};
use wasm_bindgen::prelude::*;

mod color;
mod document_io;
mod generate;
mod groups;
mod lookup;
mod mesh_buffers;
mod metadata;
mod selection;
mod stats;
mod states;
mod support;
mod view;

use support::js_err;

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
        let defaults = Options::default();
        Settings {
            tolerance: defaults.tolerance,
            corner_angle: defaults.corner_angle,
            corner_run: defaults.corner_run,
            min_chain_len: defaults.min_chain_len as u32,
            precision: defaults.precision as u32,
            validate: defaults.validate,
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
/// The buffer getters (see `mesh_buffers`) return views straight into WASM memory,
/// so nothing is copied. A view is only valid until WASM memory next grows, so hand
/// it to `gl.bufferData` right away and do not keep it.
///
/// The rest of `MapDocument`'s methods live in sibling modules, grouped by concern:
/// `view` (picking/drawing/grouping queries), `selection` (box/filter selection),
/// `mesh_buffers` (raw buffer getters), `metadata` (province fields + CSV import),
/// `states`, `groups` (countries/strategic regions), `document_io` (save/load).
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
