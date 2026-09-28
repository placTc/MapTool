//! The map's name and per-province metadata, including CSV import.

use maptool_core::{Biome, Kind};
use wasm_bindgen::prelude::*;

use crate::MapDocument;
use crate::support::js_err;

#[wasm_bindgen]
impl MapDocument {
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
