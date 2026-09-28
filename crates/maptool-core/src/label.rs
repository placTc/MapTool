use std::collections::HashMap;

use crate::PixelFormat;

/// Label of pixels that belong to no province (outside the image, or fully transparent).
pub const NONE: u32 = u32::MAX;

/// Per-pixel province ids plus per-province statistics.
///
/// A province is identified by its exact RGB color, so disconnected regions of
/// the same color are one province.
pub struct Labels {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u32>,
    pub colors: Vec<[u8; 3]>,
    pub counts: Vec<u32>,
    /// `[x0, y0, x1, y1]`, exclusive upper bound.
    pub bboxes: Vec<[u32; 4]>,
}

impl Labels {
    pub fn build(pixels: &[u8], width: usize, height: usize, format: PixelFormat) -> Labels {
        let ch = format.channels();
        let mut data = Vec::with_capacity(width * height);
        let mut ids: HashMap<u32, u32> = HashMap::new();
        let mut colors: Vec<[u8; 3]> = Vec::new();
        let mut counts: Vec<u32> = Vec::new();
        let mut bboxes: Vec<[u32; 4]> = Vec::new();
        // Provinces are mostly made of long runs, so remember the last lookup.
        let mut last: Option<(u32, u32)> = None;

        for y in 0..height {
            for x in 0..width {
                let p = &pixels[(y * width + x) * ch..][..ch];
                if ch == 4 && p[3] == 0 {
                    data.push(NONE);
                    continue;
                }
                let key = ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32;
                let id = match last {
                    Some((k, id)) if k == key => id,
                    _ => {
                        let id = *ids.entry(key).or_insert_with(|| {
                            colors.push([p[0], p[1], p[2]]);
                            counts.push(0);
                            bboxes.push([u32::MAX, u32::MAX, 0, 0]);
                            (colors.len() - 1) as u32
                        });
                        last = Some((key, id));
                        id
                    }
                };
                data.push(id);
                let i = id as usize;
                counts[i] += 1;
                let b = &mut bboxes[i];
                b[0] = b[0].min(x as u32);
                b[1] = b[1].min(y as u32);
                b[2] = b[2].max(x as u32 + 1);
                b[3] = b[3].max(y as u32 + 1);
            }
        }

        Labels { width, height, data, colors, counts, bboxes }
    }

    /// Label at a pixel; anything outside the image is `NONE`.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            NONE
        } else {
            self.data[y as usize * self.width + x as usize]
        }
    }
}
