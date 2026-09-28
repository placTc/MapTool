//! Saving and loading a `MapDocument`'s bytes.

use wasm_bindgen::prelude::*;

use crate::MapDocument;
use crate::support::js_err;

#[wasm_bindgen]
impl MapDocument {
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
}
