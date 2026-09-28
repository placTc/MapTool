import type { Camera } from './gl';

/** Wheel delta in pixels. Browsers report lines (Firefox mouse wheels) or pages too. */
export function wheelPixels(e: WheelEvent, viewportHeight: number): number {
  if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) return e.deltaY * 40;
  if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) return e.deltaY * viewportHeight;
  return e.deltaY;
}

/** A point in client coordinates, relative to `rect`, as image pixels under `cam`. */
export function screenToImage(cam: Camera, rect: DOMRect, clientX: number, clientY: number): [number, number] {
  return [cam.x + (clientX - rect.left) / cam.k, cam.y + (clientY - rect.top) / cam.k];
}

/** The camera after zooming by `deltaPixels` at the cursor, keeping the image point under it fixed. */
export function zoomedCamera(cam: Camera, rect: DOMRect, clientX: number, clientY: number, deltaPixels: number, fitK: number, maxK: number): Camera {
  const sx = clientX - rect.left;
  const sy = clientY - rect.top;
  const k = Math.min(Math.max(cam.k * Math.exp(-deltaPixels * 0.0015), fitK * 0.5), maxK);
  const px = cam.x + sx / cam.k;
  const py = cam.y + sy / cam.k;
  return { x: px - sx / k, y: py - sy / k, k };
}

/** A drag box between two client points, relative to `rect`, as a CSS-positionable rectangle. */
export function boxRect(startX: number, startY: number, clientX: number, clientY: number, rect: DOMRect): { x: number; y: number; w: number; h: number } {
  return { x: Math.min(startX, clientX) - rect.left, y: Math.min(startY, clientY) - rect.top, w: Math.abs(clientX - startX), h: Math.abs(clientY - startY) };
}
