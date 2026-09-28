/**
 * Decode an image to raw RGBA without any color management or alpha
 * premultiplication: provinces are identified by exact color, so the browser
 * must not touch the pixel values.
 */
export async function decodeImage(source: Blob): Promise<{ data: Uint8Array; width: number; height: number }> {
  const bitmap = await createImageBitmap(source, {
    colorSpaceConversion: 'none',
    premultiplyAlpha: 'none',
  });
  const { width, height } = bitmap;
  const canvas = new OffscreenCanvas(width, height);
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  const img = ctx.getImageData(0, 0, width, height);
  return { data: new Uint8Array(img.data.buffer), width, height };
}
