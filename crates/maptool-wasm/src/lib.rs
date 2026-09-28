use maptool_core::{Options, PixelFormat};
use wasm_bindgen::prelude::*;

/// A vectorized map. Data stays in WASM memory until read, so a viewer can pull
/// paths one province at a time.
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
        Ok(self.get(id)?.path.clone())
    }

    /// Province color as 0xRRGGBB.
    pub fn color(&self, id: usize) -> Result<u32, JsError> {
        let [r, g, b] = self.get(id)?.color;
        Ok(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    #[wasm_bindgen(js_name = pixelCount)]
    pub fn pixel_count(&self, id: usize) -> Result<u32, JsError> {
        Ok(self.get(id)?.pixel_count)
    }

    /// `[x0, y0, x1, y1]` in pixels, upper bound exclusive.
    pub fn bbox(&self, id: usize) -> Result<Vec<u32>, JsError> {
        Ok(self.get(id)?.bbox.to_vec())
    }

    /// The whole map as a standalone SVG document.
    #[wasm_bindgen(js_name = toSvg)]
    pub fn to_svg(&self) -> String {
        self.0.to_svg()
    }

    fn get(&self, id: usize) -> Result<&maptool_core::Province, JsError> {
        self.0.provinces.get(id).ok_or_else(|| JsError::new(&format!("no province {id}")))
    }
}

/// Vectorize RGBA pixels (e.g. `ImageData.data`). Omitted settings use the defaults.
#[wasm_bindgen]
pub fn vectorize(
    rgba: &[u8],
    width: u32,
    height: u32,
    tolerance: Option<f64>,
    corner_angle: Option<f64>,
    min_chain_len: Option<usize>,
    precision: Option<usize>,
    corner_run: Option<f64>,
    validate: Option<bool>,
) -> Result<VectorMap, JsError> {
    let d = Options::default();
    let opts = Options {
        tolerance: tolerance.unwrap_or(d.tolerance),
        corner_angle: corner_angle.unwrap_or(d.corner_angle),
        min_chain_len: min_chain_len.unwrap_or(d.min_chain_len),
        precision: precision.unwrap_or(d.precision),
        corner_run: corner_run.unwrap_or(d.corner_run),
        validate: validate.unwrap_or(d.validate),
    };
    maptool_core::vectorize(rgba, width, height, PixelFormat::Rgba, &opts)
        .map(VectorMap)
        .map_err(|e| JsError::new(&e.to_string()))
}
