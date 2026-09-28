use js_sys::{Float32Array, Uint32Array};
use maptool_core::{Options, PixelFormat};
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

// ----------------------------------------------------------------- mesh

/// A map as triangle meshes for GPU rendering, plus hit testing.
///
/// The buffer getters return views straight into WASM memory, so nothing is
/// copied. A view is only valid until WASM memory next grows, so hand it to
/// `gl.bufferData` right away and do not keep it.
#[wasm_bindgen]
pub struct MapMesh(maptool_core::MapMesh);

#[wasm_bindgen]
impl MapMesh {
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

    /// Province color as 0xRRGGBB.
    pub fn color(&self, id: usize) -> Result<u32, JsError> {
        let [r, g, b] = self.province(id)?.color;
        Ok(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    /// All province colors as RGBA bytes, four per province, for a palette texture.
    pub fn palette(&self) -> Vec<u8> {
        self.0.provinces.iter().flat_map(|p| [p.color[0], p.color[1], p.color[2], 255]).collect()
    }

    #[wasm_bindgen(js_name = pixelCount)]
    pub fn pixel_count(&self, id: usize) -> Result<u32, JsError> {
        Ok(self.province(id)?.pixel_count)
    }

    /// `[x0, y0, x1, y1]` in pixels, upper bound exclusive.
    pub fn bbox(&self, id: usize) -> Result<Vec<u32>, JsError> {
        Ok(self.province(id)?.bbox.to_vec())
    }

    /// The province under the image point `(x, y)`, or -1.
    pub fn pick(&self, x: f64, y: f64) -> i32 {
        self.0.pick(x, y).map_or(-1, |id| id as i32)
    }

    /// Triangle vertices, x and y interleaved.
    pub fn positions(&self) -> Float32Array {
        // SAFETY: the view is consumed by the caller before any WASM allocation.
        unsafe { Float32Array::view(&self.0.positions) }
    }

    /// Province id of each triangle vertex.
    #[wasm_bindgen(js_name = vertexProvince)]
    pub fn vertex_province(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.vertex_province) }
    }

    pub fn indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.indices) }
    }

    /// Border points, x and y interleaved.
    #[wasm_bindgen(js_name = linePositions)]
    pub fn line_positions(&self) -> Float32Array {
        unsafe { Float32Array::view(&self.0.line_positions) }
    }

    /// Border segments as index pairs into `linePositions`, for `gl.LINES`.
    #[wasm_bindgen(js_name = lineIndices)]
    pub fn line_indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.line_indices) }
    }

    /// `[first index, index count]` of a province's border segments in `lineIndices`.
    #[wasm_bindgen(js_name = lineRange)]
    pub fn line_range(&self, id: usize) -> Result<Vec<u32>, JsError> {
        self.0.line_ranges.get(id).map(|r| r.to_vec()).ok_or_else(|| js_err(format!("no province {id}")))
    }

    fn province(&self, id: usize) -> Result<&maptool_core::ProvinceInfo, JsError> {
        self.0.provinces.get(id).ok_or_else(|| js_err(format!("no province {id}")))
    }
}

/// Build a mesh from RGBA pixels (e.g. `ImageData.data`).
#[wasm_bindgen(js_name = meshFromRgba)]
pub fn mesh_from_rgba(rgba: &[u8], width: u32, height: u32, settings: &Settings) -> Result<MapMesh, JsError> {
    maptool_core::mesh(rgba, width, height, PixelFormat::Rgba, &settings.options(), settings.flatten_tolerance)
        .map(MapMesh)
        .map_err(js_err)
}

/// Decode a PNG or BMP file and build a mesh from it.
#[wasm_bindgen(js_name = meshFromImage)]
pub fn mesh_from_image(bytes: &[u8], settings: &Settings) -> Result<MapMesh, JsError> {
    let (rgba, w, h) = maptool_core::decode_image(bytes).map_err(js_err)?;
    mesh_from_rgba(&rgba, w, h, settings)
}
