//! Turn images made of solid-colored areas ("provinces") into smoothed SVG paths.
//!
//! Provinces are identified by exact color. Borders between provinces are traced
//! once and shared by both neighbours, so smoothing never opens gaps or creates
//! overlaps between them.

mod build;
mod csv;
mod document;
mod groups;
mod label;
mod mesh;
mod provinces;
mod smooth;
mod states;
mod trace;
mod validate;

use std::fmt::{self, Write};

pub use csv::CsvReport;
pub use document::{Document, Level, StateStats, Unit, ViewMode, biome_names, filter, is_map_file};
pub use mesh::{MapMesh, ProvinceInfo};
pub use provinces::{Biome, Kind, ProvinceMeta, ProvinceTable};
pub use groups::{Group, GroupKind, GroupSet};
pub use states::{State, StateSet, auto_color};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    /// Pixels with alpha 0 belong to no province.
    Rgba,
}

impl PixelFormat {
    pub fn channels(self) -> usize {
        match self {
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Douglas-Peucker tolerance in pixels. `0` disables simplification and
    /// smoothing and yields exact pixel-edge polygons.
    pub tolerance: f64,
    /// Turns sharper than this many degrees stay corners instead of being rounded.
    pub corner_angle: f64,
    /// Borders shorter than this many pixel edges are left unsmoothed so tiny
    /// provinces keep their shape.
    pub min_chain_len: usize,
    /// Decimal places in the output coordinates.
    pub precision: usize,
    /// Two straight borders at least this many pixels long that meet at a
    /// sharp turn (75 degrees or more) keep their corner, so squares and
    /// rectangles stay square.
    pub corner_run: f64,
    /// Reject inputs with single-pixel exclaves or four-way junctions.
    pub validate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { tolerance: 1.0, corner_angle: 100.0, min_chain_len: 6, precision: 2, corner_run: 4.0, validate: true }
    }
}

impl Options {
    /// Exact pixel-edge polygons, no smoothing.
    pub fn exact() -> Self {
        Options { tolerance: 0.0, ..Options::default() }
    }
}

#[derive(Debug)]
pub enum Error {
    BufferSize { expected: usize, actual: usize },
    EmptyImage,
    TooLarge,
    /// The image breaks an input rule; see [`Violation`].
    Invalid { violations: Vec<Violation>, total: usize },
    /// A saved map file is damaged, truncated or from an incompatible version.
    Format(String),
    /// An edit was refused (unknown state or province).
    Edit(String),
    /// The triangulation of a province failed (degenerate geometry).
    Tessellation { province: u32, message: String },
    #[cfg(feature = "io")]
    Image(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViolationKind {
    /// A lone pixel whose color also appears elsewhere in the image.
    /// `x`, `y` are the pixel.
    SinglePixelExclave,
    /// Four pixels of four different colors meet. `x`, `y` are the shared
    /// corner: the pixels are `(x-1, y-1)`, `(x, y-1)`, `(x-1, y)` and `(x, y)`.
    FourWayJunction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    pub x: u32,
    pub y: u32,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ViolationKind::SinglePixelExclave => write!(f, "single-pixel exclave at pixel ({}, {})", self.x, self.y),
            ViolationKind::FourWayJunction => write!(f, "four-way junction at corner ({}, {})", self.x, self.y),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BufferSize { expected, actual } => {
                write!(f, "pixel buffer has {actual} bytes, expected {expected}")
            }
            Error::EmptyImage => write!(f, "image has zero width or height"),
            Error::TooLarge => write!(f, "image is too large"),
            Error::Invalid { violations, total } => {
                write!(f, "input rejected, {total} problem(s): ")?;
                for (i, v) in violations.iter().take(5).enumerate() {
                    write!(f, "{}{v}", if i == 0 { "" } else { "; " })?;
                }
                if *total > 5 {
                    write!(f, "; and {} more", total - 5)?;
                }
                Ok(())
            }
            Error::Format(why) => write!(f, "invalid map file: {why}"),
            Error::Edit(why) => write!(f, "{why}"),
            Error::Tessellation { province, message } => {
                write!(f, "cannot triangulate province {province}: {message}")
            }
            #[cfg(feature = "io")]
            Error::Image(e) => write!(f, "cannot read image: {e}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Province {
    /// Index in order of first appearance in the image (row-major).
    pub id: u32,
    pub color: [u8; 3],
    pub pixel_count: u32,
    /// `[x0, y0, x1, y1]` in pixels, upper bound exclusive.
    pub bbox: [u32; 4],
    /// SVG path data. Enclaves are holes and exclaves are extra subpaths, so
    /// fill it with `evenodd` or `nonzero` alike.
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct VectorMap {
    pub width: u32,
    pub height: u32,
    pub provinces: Vec<Province>,
}

impl VectorMap {
    /// A standalone SVG document: one `<path data-id=.. fill=..>` per province.
    pub fn to_svg(&self) -> String {
        let mut s = String::with_capacity(self.provinces.iter().map(|p| p.path.len() + 64).sum::<usize>() + 256);
        let _ = writeln!(
            s,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">",
            w = self.width,
            h = self.height
        );
        for p in &self.provinces {
            let [r, g, b] = p.color;
            let _ = writeln!(
                s,
                "<path id=\"p{id}\" data-id=\"{id}\" fill=\"#{r:02x}{g:02x}{b:02x}\" fill-rule=\"evenodd\" d=\"{d}\"/>",
                id = p.id,
                d = p.path
            );
        }
        s.push_str("</svg>\n");
        s
    }
}

/// Check the input and build the label map.
fn prepare(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    opts: &Options,
) -> Result<label::Labels, Error> {
    if width == 0 || height == 0 {
        return Err(Error::EmptyImage);
    }
    let (w, h) = (width as usize, height as usize);
    // Lattice coordinates are i32 and edge ids are usize.
    let edges = (h + 1)
        .checked_mul(w)
        .and_then(|a| (w + 1).checked_mul(h).and_then(|b| a.checked_add(b)))
        .ok_or(Error::TooLarge)?;
    if width > i32::MAX as u32 - 2 || height > i32::MAX as u32 - 2 || edges > isize::MAX as usize / 8 {
        return Err(Error::TooLarge);
    }
    let expected = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(format.channels()))
        .ok_or(Error::TooLarge)?;
    if pixels.len() != expected {
        return Err(Error::BufferSize { expected, actual: pixels.len() });
    }

    let labels = label::Labels::build(pixels, w, h, format);
    if opts.validate {
        validate::check(&labels)?;
    }
    Ok(labels)
}

/// Vectorize a `width` x `height` image given as tightly packed pixels.
pub fn vectorize(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    opts: &Options,
) -> Result<VectorMap, Error> {
    let labels = prepare(pixels, width, height, format, opts)?;
    let paths = build::build_paths(&labels, opts);

    let provinces = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| Province {
            id: i as u32,
            color: labels.colors[i],
            pixel_count: labels.counts[i],
            bbox: labels.bboxes[i],
            path,
        })
        .collect();
    Ok(VectorMap { width, height, provinces })
}

/// Like [`vectorize`], but produce triangle meshes for GPU rendering instead of
/// SVG paths. `flatten_tolerance` is how far, in pixels, the flattened curves may
/// stray from the true smoothed border (0.05 is fine up to about 30x zoom).
pub fn mesh(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    opts: &Options,
    flatten_tolerance: f64,
) -> Result<MapMesh, Error> {
    let labels = prepare(pixels, width, height, format, opts)?;
    let rings = build::geometry(&labels, opts).rings(flatten_tolerance);
    let provinces = (0..labels.colors.len())
        .map(|i| ProvinceInfo {
            id: i as u32,
            color: labels.colors[i],
            pixel_count: labels.counts[i],
            bbox: labels.bboxes[i],
        })
        .collect();
    mesh::build(width, height, provinces, rings)
}

/// Decode PNG or BMP bytes to tightly packed RGBA. No color management is applied,
/// so pixel values (and therefore province colors) are exactly what the file holds.
#[cfg(feature = "io")]
pub fn decode_image(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), Error> {
    let img = image::load_from_memory(bytes).map_err(|e| Error::Image(e.to_string()))?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}

/// Decode a PNG or BMP file and vectorize it.
#[cfg(feature = "io")]
pub fn vectorize_file(path: impl AsRef<std::path::Path>, opts: &Options) -> Result<VectorMap, Error> {
    let img = image::open(path).map_err(|e| Error::Image(e.to_string()))?.to_rgba8();
    let (w, h) = img.dimensions();
    vectorize(img.as_raw(), w, h, PixelFormat::Rgba, opts)
}
