//! Small helpers shared by every `impl MapDocument` file in this crate.

use maptool_core::{GroupKind, Level};
use wasm_bindgen::prelude::*;

pub(crate) fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

pub(crate) fn level_of(level: u8) -> Result<Level, JsError> {
    Level::from_u8(level).ok_or_else(|| js_err(format!("unknown view level {level}")))
}

pub(crate) fn kind_of(kind: u8) -> Result<GroupKind, JsError> {
    GroupKind::from_u8(kind).ok_or_else(|| js_err(format!("unknown group kind {kind}")))
}
