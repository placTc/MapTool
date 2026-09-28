import init, { vectorize, type VectorMap } from 'maptool-wasm';

export interface Province {
  id: number;
  color: string;
  pixelCount: number;
  bbox: [number, number, number, number];
  path: string;
}

export interface LoadedMap {
  width: number;
  height: number;
  provinces: Province[];
}

let ready: Promise<unknown> | undefined;

/** Vectorize RGBA pixels into plain objects the UI can render reactively. */
export async function vectorizeRgba(data: Uint8Array, width: number, height: number, tolerance: number, validate: boolean): Promise<LoadedMap> {
  ready ??= init();
  await ready;
  const map: VectorMap = vectorize(data, width, height, tolerance, undefined, undefined, undefined, undefined, validate);
  try {
    const provinces: Province[] = [];
    for (let id = 0; id < map.len; id++) {
      const [x0, y0, x1, y1] = map.bbox(id);
      provinces.push({
        id,
        color: '#' + map.color(id).toString(16).padStart(6, '0'),
        pixelCount: map.pixelCount(id),
        bbox: [x0, y0, x1, y1],
        path: map.path(id),
      });
    }
    return { width: map.width, height: map.height, provinces };
  } finally {
    map.free();
  }
}
