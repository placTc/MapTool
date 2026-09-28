//! Country and strategic-region ("group") management on `MapDocument`.
//!
//! Both are groups of states, and every method takes `kind`: 0 for countries, 1 for
//! strategic regions. A state is in at most one group of each kind.

use wasm_bindgen::prelude::*;

use crate::support::{js_err, kind_of};
use crate::{MapDocument, color, stats};

#[wasm_bindgen]
impl MapDocument {
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
        let group_kind = kind_of(kind)?;
        let totals = self.0.group_stats(group_kind, id).ok_or_else(|| js_err(format!("no {} {id}", group_kind.noun())))?;
        let states = self.group(kind, id)?.states.len();
        Ok(std::iter::once(states as f64).chain(stats::flatten_stats(&totals)).collect())
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
}
