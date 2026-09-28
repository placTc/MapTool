import init, { Settings, biomeNames, openMap, type MapDocument } from 'maptool-wasm';

export type { MapDocument };

/** How long the last `openMapBytes` spent inside WASM (decoding, smoothing, triangulating, or reading a saved map). */
export const loadTimings = { wasmMs: 0 };

let ready: Promise<unknown> | undefined;

export async function ensureWasm() {
  ready ??= init();
  await ready;
}

/**
 * Open a map from a PNG/BMP image (built from scratch using `opts`) or from a saved map file
 * (which ignores `opts`). All of it happens inside WASM, so the browser never touches the
 * pixels and province colors cannot be altered by its color pipeline.
 */
export async function openMapBytes(bytes: Uint8Array, opts: { tolerance: number; validate: boolean }): Promise<MapDocument> {
  await ensureWasm();
  const settings = new Settings();
  settings.tolerance = opts.tolerance;
  settings.validate = opts.validate;
  try {
    const t = performance.now();
    const doc = openMap(bytes, settings);
    loadTimings.wasmMs = performance.now() - t;
    return doc;
  } finally {
    settings.free();
  }
}

let biomes: string[] | undefined;

/** Biome names, in the order the document numbers them. Needs WASM to be loaded. */
export function biomeList(): string[] {
  return (biomes ??= biomeNames());
}

/** 0xRRGGBB as a CSS color. */
export function hex(rgb: number): string {
  return '#' + rgb.toString(16).padStart(6, '0');
}

/** A CSS color as 0xRRGGBB. */
export function unhex(css: string): number {
  return parseInt(css.replace('#', ''), 16) & 0xffffff;
}

/** True for a saved map (as opposed to a PNG/BMP): they start with "MTMP". */
export function isMapFile(bytes: Uint8Array): boolean {
  return bytes.length >= 4 && bytes[0] === 0x4d && bytes[1] === 0x54 && bytes[2] === 0x4d && bytes[3] === 0x50;
}
