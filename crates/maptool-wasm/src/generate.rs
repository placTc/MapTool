//! Generating a `MapDocument` from a hand-painted border map, instead of opening
//! one already made of one-color-per-province.

use maptool_core::{Document, GenerateOptions, Kind, PixelFormat};
use wasm_bindgen::prelude::*;

use crate::MapDocument;
use crate::support::js_err;

/// Tuning knobs for generating provinces from a border map. Create with
/// `new GenerateSettings()` and assign the fields you care about.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct GenerateSettings {
    /// Target land province size, in pixels.
    pub land_radius: f64,
    /// Target sea/lake province size, in pixels; only used when `split_seas` is set.
    pub water_radius: f64,
    /// Subdivide seas/lakes into provinces too, instead of one per connected body.
    pub split_seas: bool,
    /// The same image and settings with the same seed always produce the same result.
    pub seed: u32,
}

#[wasm_bindgen]
impl GenerateSettings {
    #[wasm_bindgen(constructor)]
    pub fn new() -> GenerateSettings {
        let defaults = GenerateOptions::default();
        GenerateSettings { land_radius: defaults.land_radius, water_radius: defaults.water_radius, split_seas: defaults.split_seas, seed: defaults.seed }
    }
}

impl Default for GenerateSettings {
    fn default() -> Self {
        GenerateSettings::new()
    }
}

impl GenerateSettings {
    fn options(&self) -> GenerateOptions {
        GenerateOptions { land_radius: self.land_radius, water_radius: self.water_radius, split_seas: self.split_seas, seed: self.seed }
    }
}

/// Generate province geometry from a hand-painted border map (white = land, `#00FF00`
/// = sea/lake, black = a border line absorbed into whichever province is nearest),
/// and wrap it in a fresh document with sea/lake provinces already marked as sea.
#[wasm_bindgen(js_name = generateMap)]
pub fn generate_map(bytes: &[u8], gen_settings: &GenerateSettings, settings: &crate::Settings) -> Result<MapDocument, JsError> {
    let (rgba, w, h) = maptool_core::decode_image(bytes).map_err(js_err)?;
    let generated = maptool_core::generate_mesh(&rgba, w, h, PixelFormat::Rgba, &gen_settings.options(), &settings.options(), settings.flatten_tolerance)
        .map_err(js_err)?;
    let mut doc = Document::new(generated.mesh);
    doc.provinces.set_kind(&generated.sea_provinces, Kind::Sea).expect("generated ids are always valid for the document that owns them");
    Ok(MapDocument(doc))
}
