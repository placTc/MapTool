//! `pub(crate)` lookup helpers on `MapDocument`, used from the other impl files. Kept
//! `pub(crate)` rather than private: an inherent `fn` here is only visible to *this*
//! module by default, and every other `impl MapDocument` block lives in a sibling
//! module, not a descendant of it.

use wasm_bindgen::prelude::*;

use crate::MapDocument;
use crate::support::{js_err, kind_of};

impl MapDocument {
    pub(crate) fn info(&self, id: usize) -> Result<&maptool_core::ProvinceInfo, JsError> {
        self.0.mesh.provinces.get(id).ok_or_else(|| js_err(format!("no province {id}")))
    }

    pub(crate) fn state(&self, id: u32) -> Result<&maptool_core::State, JsError> {
        self.0.states.get(id).ok_or_else(|| js_err(format!("no state {id}")))
    }

    pub(crate) fn group(&self, kind: u8, id: u32) -> Result<&maptool_core::Group, JsError> {
        let group_kind = kind_of(kind)?;
        self.0.groups(group_kind).get(id).ok_or_else(|| js_err(format!("no {} {id}", group_kind.noun())))
    }
}
