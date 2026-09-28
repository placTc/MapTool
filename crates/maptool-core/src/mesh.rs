//! Triangle meshes for GPU rendering, plus hit testing on the same geometry.

use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};

use crate::Error;

/// Per-province facts, without any geometry.
#[derive(Clone, Debug)]
pub struct ProvinceInfo {
    pub id: u32,
    pub color: [u8; 3],
    pub pixel_count: u32,
    /// `[x0, y0, x1, y1]` in pixels, upper bound exclusive.
    pub bbox: [u32; 4],
}

/// Everything a renderer needs. Coordinates are image pixels, y down.
///
/// * Triangles: `positions` (x, y pairs), `vertex_province` (one id per vertex)
///   and `indices`. Each province is a separate set of vertices.
/// * Borders: `line_positions` (x, y pairs of every ring point), `line_indices`
///   (pairs, for `LINES`) and `line_ranges` (per province: first index, count).
#[derive(Clone, Debug)]
pub struct MapMesh {
    pub width: u32,
    pub height: u32,
    pub provinces: Vec<ProvinceInfo>,
    pub positions: Vec<f32>,
    pub vertex_province: Vec<u32>,
    pub indices: Vec<u32>,
    pub line_positions: Vec<f32>,
    pub line_indices: Vec<u32>,
    pub line_ranges: Vec<[u32; 2]>,
    /// Start of each ring in `line_positions` (in points), with a final end sentinel.
    ring_starts: Vec<u32>,
    /// Per province: first ring, ring count.
    province_rings: Vec<[u32; 2]>,
}

impl MapMesh {
    /// The province covering the image point `(x, y)`, if any.
    ///
    /// Uses the same rings that are drawn, so it agrees with the picture. A point
    /// exactly on a shared border may belong to either neighbour.
    pub fn pick(&self, x: f64, y: f64) -> Option<u32> {
        for (id, info) in self.provinces.iter().enumerate() {
            let [x0, y0, x1, y1] = info.bbox;
            // Smoothed borders can bulge slightly beyond the pixel bounds.
            if x < x0 as f64 - 1.0 || x > x1 as f64 + 1.0 || y < y0 as f64 - 1.0 || y > y1 as f64 + 1.0 {
                continue;
            }
            let [first, count] = self.province_rings[id];
            let mut inside = false;
            for ring in first..first + count {
                let (a, b) = (self.ring_starts[ring as usize] as usize, self.ring_starts[ring as usize + 1] as usize);
                let pts = &self.line_positions[a * 2..b * 2];
                let n = b - a;
                let mut j = n - 1;
                for i in 0..n {
                    let (xi, yi) = (pts[i * 2] as f64, pts[i * 2 + 1] as f64);
                    let (xj, yj) = (pts[j * 2] as f64, pts[j * 2 + 1] as f64);
                    if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                        inside = !inside;
                    }
                    j = i;
                }
            }
            if inside {
                return Some(id as u32);
            }
        }
        None
    }
}

pub fn build(
    width: u32,
    height: u32,
    provinces: Vec<ProvinceInfo>,
    rings: Vec<Vec<Vec<[f32; 2]>>>,
) -> Result<MapMesh, Error> {
    let mut mesh = MapMesh {
        width,
        height,
        provinces,
        positions: Vec::new(),
        vertex_province: Vec::new(),
        indices: Vec::new(),
        line_positions: Vec::new(),
        line_indices: Vec::new(),
        line_ranges: Vec::new(),
        ring_starts: vec![0],
        province_rings: Vec::new(),
    };
    let mut tess = FillTessellator::new();
    let options = FillOptions::default().with_fill_rule(FillRule::EvenOdd);

    for (id, province_rings) in rings.iter().enumerate() {
        // Borders.
        let ring_base = (mesh.ring_starts.len() - 1) as u32;
        let line_start = mesh.line_indices.len() as u32;
        for ring in province_rings {
            let base = (mesh.line_positions.len() / 2) as u32;
            let n = ring.len() as u32;
            for p in ring {
                mesh.line_positions.extend_from_slice(p);
            }
            for i in 0..n {
                mesh.line_indices.push(base + i);
                mesh.line_indices.push(base + (i + 1) % n);
            }
            mesh.ring_starts.push(base + n);
        }
        mesh.province_rings.push([ring_base, province_rings.len() as u32]);
        mesh.line_ranges.push([line_start, mesh.line_indices.len() as u32 - line_start]);

        // Fill: one path per province, even-odd, so holes and exclaves just work.
        let mut builder = Path::builder();
        for ring in province_rings {
            builder.begin(point(ring[0][0], ring[0][1]));
            for p in &ring[1..] {
                builder.line_to(point(p[0], p[1]));
            }
            builder.end(true);
        }
        let path = builder.build();
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        tess.tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()),
        )
        .map_err(|e| Error::Tessellation { province: id as u32, message: format!("{e:?}") })?;

        let base = mesh.vertex_province.len() as u32;
        for p in &buffers.vertices {
            mesh.positions.extend_from_slice(p);
        }
        mesh.vertex_province.extend(std::iter::repeat_n(id as u32, buffers.vertices.len()));
        mesh.indices.extend(buffers.indices.iter().map(|i| i + base));
    }
    Ok(mesh)
}
