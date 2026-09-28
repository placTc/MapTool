//! Zero-copy views into the mesh's GPU-ready buffers.
//!
//! Each getter returns a JS typed array that aliases WASM linear memory directly, so
//! nothing is copied. A view is only valid until WASM memory next grows, so the caller
//! must hand it to `gl.bufferData` right away and not keep it.

use js_sys::{Float32Array, Uint32Array};
use wasm_bindgen::prelude::*;

use crate::MapDocument;

#[wasm_bindgen]
impl MapDocument {
    /// Triangle vertices, x and y interleaved.
    pub fn positions(&self) -> Float32Array {
        // SAFETY: the view is consumed by the caller before any WASM allocation.
        unsafe { Float32Array::view(&self.0.mesh.positions) }
    }

    /// Province id of each triangle vertex.
    #[wasm_bindgen(js_name = vertexProvince)]
    pub fn vertex_province(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.vertex_province) }
    }

    pub fn indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.indices) }
    }

    /// Border points, x and y interleaved.
    #[wasm_bindgen(js_name = linePositions)]
    pub fn line_positions(&self) -> Float32Array {
        unsafe { Float32Array::view(&self.0.mesh.line_positions) }
    }

    /// Every border segment as index pairs into `linePositions`, for `gl.LINES`.
    #[wasm_bindgen(js_name = lineIndices)]
    pub fn line_indices(&self) -> Uint32Array {
        unsafe { Uint32Array::view(&self.0.mesh.line_indices) }
    }
}
