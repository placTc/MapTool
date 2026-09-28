import init, { Settings, meshFromImage, type MapMesh } from 'maptool-wasm';

export type { MapMesh };

/** How long the last `loadMesh` spent inside WASM (decoding, smoothing, triangulating). */
export const loadTimings = { wasmMs: 0 };

let ready: Promise<unknown> | undefined;

/**
 * Decode a PNG/BMP file and build the map mesh, all inside WASM. The browser never
 * touches the pixels, so province colors cannot be altered by its color pipeline.
 */
export async function loadMesh(file: Blob, opts: { tolerance: number; validate: boolean }): Promise<MapMesh> {
  ready ??= init();
  await ready;
  const bytes = new Uint8Array(await file.arrayBuffer());
  const settings = new Settings();
  settings.tolerance = opts.tolerance;
  settings.validate = opts.validate;
  try {
    const t = performance.now();
    const mesh = meshFromImage(bytes, settings);
    loadTimings.wasmMs = performance.now() - t;
    return mesh;
  } finally {
    settings.free();
  }
}
