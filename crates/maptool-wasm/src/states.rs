//! State management on `MapDocument`.

use wasm_bindgen::prelude::*;

use crate::support::js_err;
use crate::{MapDocument, color, stats};

#[wasm_bindgen]
impl MapDocument {
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
        let totals = self.0.state_stats(id).ok_or_else(|| js_err(format!("no state {id}")))?;
        Ok(stats::flatten_stats(&totals))
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
}
