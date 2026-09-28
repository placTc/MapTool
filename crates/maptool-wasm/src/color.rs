//! Packing between the `0xRRGGBB` colors the JS side sends/receives and `[r, g, b]`.

pub(crate) fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

pub(crate) fn unpack_rgb(rgb: u32) -> [u8; 3] {
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}
