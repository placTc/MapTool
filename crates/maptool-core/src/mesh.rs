//! Triangle meshes for GPU rendering, plus hit testing on the same geometry.

use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};

use crate::Error;

/// Per-province facts, without any geometry.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
    /// Border segments (index pairs into `line_positions`, for `LINES`) of the outline
    /// of the union of `ids`: segments shared by two selected provinces are left
    /// out, so only the outer edge of the whole group remains.
    pub fn boundary_indices(&self, ids: &[u32]) -> Vec<u32> {
        use std::collections::HashSet;
        let point = |i: u32| (self.line_positions[i as usize * 2].to_bits(), self.line_positions[i as usize * 2 + 1].to_bits());
        let mut segments: Vec<(u32, u32)> = Vec::new();
        let mut seen = HashSet::new();
        for &id in ids {
            let Some(&[first, count]) = self.province_rings.get(id as usize) else { continue };
            if !seen.insert(id) {
                continue;
            }
            for ring in first..first + count {
                let (a, b) = (self.ring_starts[ring as usize], self.ring_starts[ring as usize + 1]);
                for i in a..b {
                    segments.push((i, if i + 1 == b { a } else { i + 1 }));
                }
            }
        }
        // A segment whose reverse is also present is a border between two selected provinces.
        let present: HashSet<_> = segments.iter().map(|&(a, b)| (point(a), point(b))).collect();
        segments
            .into_iter()
            .filter(|&(a, b)| !present.contains(&(point(b), point(a))))
            .flat_map(|(a, b)| [a, b])
            .collect()
    }

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

// ------------------------------------------------------------- mesh section
//
// The finished mesh in compact form, so opening a saved map skips decoding,
// smoothing and triangulation. It is a raw-deflate payload (see `file.rs` for the
// container around it) of little-endian values:
//
//   width, height, province count, ring count                      4 x u32
//   per province: r g b 0, pixel count, bbox x4, vertices,
//                 indices, rings                                    9 x u32
//   ring lengths                                                    u32 each
//   triangle vertices (x, y)                                        f32 each
//   triangle indices, local to their province                       u32 each
//   border points (x, y)                                            f32 each
//
// Everything else (vertex province ids, border segment indices, ranges) is
// rebuilt on load.

// Higher levels barely shrink this data (mostly float coordinates) but are much slower.
const COMPRESSION_LEVEL: u8 = 1;
/// Refuse payloads that would inflate beyond this many bytes.
const MAX_PAYLOAD: usize = 1 << 30;

pub(crate) fn bad(why: impl Into<String>) -> Error {
    Error::Format(why.into())
}

pub(crate) struct Reader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
}

impl Reader<'_> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| bad("truncated"))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn u32s(&mut self, n: usize) -> Result<Vec<u32>, Error> {
        let bytes = self.take(n.checked_mul(4).ok_or_else(|| bad("too large"))?)?;
        Ok(bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, Error> {
        let bytes = self.take(n.checked_mul(4).ok_or_else(|| bad("too large"))?)?;
        Ok(bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }
}

impl MapMesh {
    /// Serialize to the compressed section format described above.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let n = self.provinces.len();
        let mut vertices = vec![0u32; n];
        for &p in &self.vertex_province {
            vertices[p as usize] += 1;
        }
        let mut indices = vec![0u32; n];
        for tri in self.indices.chunks_exact(3) {
            indices[self.vertex_province[tri[0] as usize] as usize] += 3;
        }
        // Local index base of each province: where its vertices start.
        let mut base = vec![0u32; n];
        let mut acc = 0;
        for i in 0..n {
            base[i] = acc;
            acc += vertices[i];
        }

        let ring_count = self.ring_starts.len() - 1;
        let mut out: Vec<u8> = Vec::with_capacity(
            16 + n * 36 + ring_count * 4 + self.positions.len() * 4 + self.indices.len() * 4 + self.line_positions.len() * 4,
        );
        let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
        put(self.width);
        put(self.height);
        put(n as u32);
        put(ring_count as u32);
        for (i, p) in self.provinces.iter().enumerate() {
            put(u32::from_le_bytes([p.color[0], p.color[1], p.color[2], 0]));
            put(p.pixel_count);
            for b in p.bbox {
                put(b);
            }
            put(vertices[i]);
            put(indices[i]);
            put(self.province_rings[i][1]);
        }
        for w in self.ring_starts.windows(2) {
            put(w[1] - w[0]);
        }
        for &v in &self.positions {
            put(v.to_bits());
        }
        let mut idx = self.indices.iter();
        for i in 0..n {
            for _ in 0..indices[i] {
                put(idx.next().unwrap() - base[i]);
            }
        }
        for &v in &self.line_positions {
            put(v.to_bits());
        }

        miniz_oxide::deflate::compress_to_vec(&out, COMPRESSION_LEVEL)
    }

    /// Read a section written by [`MapMesh::encode`]. Never panics on bad input.
    pub(crate) fn decode(bytes: &[u8]) -> Result<MapMesh, Error> {
        let payload = miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, MAX_PAYLOAD)
            .map_err(|e| bad(format!("damaged data ({e:?})")))?;
        let mut r = Reader { data: &payload, pos: 0 };

        let (width, height) = (r.u32()?, r.u32()?);
        let n = r.u32()? as usize;
        let ring_count = r.u32()? as usize;
        if width == 0 || height == 0 {
            return Err(bad("zero-sized image"));
        }
        // Each province takes 36 bytes, so a count the payload cannot hold is corrupt.
        if n > payload.len() / 36 || ring_count > payload.len() / 4 {
            return Err(bad("counts exceed the data"));
        }

        let mut provinces = Vec::with_capacity(n);
        let (mut vertices, mut indices, mut rings) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for id in 0..n {
            let c = r.u32()?.to_le_bytes();
            provinces.push(ProvinceInfo {
                id: id as u32,
                color: [c[0], c[1], c[2]],
                pixel_count: r.u32()?,
                bbox: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            });
            vertices.push(r.u32()? as usize);
            let i = r.u32()? as usize;
            if i % 3 != 0 {
                return Err(bad("index count is not a multiple of 3"));
            }
            indices.push(i);
            rings.push(r.u32()? as usize);
        }
        let sum = |v: &[usize]| v.iter().try_fold(0usize, |a, &b| a.checked_add(b)).ok_or_else(|| bad("counts overflow"));
        let (total_vertices, total_indices, total_rings) = (sum(&vertices)?, sum(&indices)?, sum(&rings)?);
        if total_rings != ring_count {
            return Err(bad("ring counts disagree"));
        }

        let ring_lens = r.u32s(ring_count)?;
        let total_points: usize = ring_lens.iter().try_fold(0usize, |a, &b| a.checked_add(b as usize)).ok_or_else(|| bad("counts overflow"))?;
        if ring_lens.iter().any(|&l| l < 3) {
            return Err(bad("a ring has fewer than 3 points"));
        }
        let positions = r.f32s(total_vertices.checked_mul(2).ok_or_else(|| bad("too large"))?)?;
        let local = r.u32s(total_indices)?;
        let line_positions = r.f32s(total_points.checked_mul(2).ok_or_else(|| bad("too large"))?)?;
        if r.pos != payload.len() {
            return Err(bad("unexpected trailing data"));
        }
        if u32::try_from(total_vertices).is_err() || u32::try_from(total_points).is_err() {
            return Err(bad("too many vertices"));
        }

        // Rebuild what was not stored.
        let mut vertex_province = Vec::with_capacity(total_vertices);
        let mut global = Vec::with_capacity(total_indices);
        let (mut base, mut next) = (0u32, 0usize);
        for id in 0..n {
            vertex_province.extend(std::iter::repeat_n(id as u32, vertices[id]));
            for &i in &local[next..next + indices[id]] {
                if i as usize >= vertices[id] {
                    return Err(bad("a triangle refers to a missing vertex"));
                }
                global.push(base + i);
            }
            next += indices[id];
            base += vertices[id] as u32;
        }

        let mut ring_starts = Vec::with_capacity(ring_count + 1);
        ring_starts.push(0u32);
        let mut line_indices = Vec::with_capacity(total_points * 2);
        let mut line_ranges = Vec::with_capacity(n);
        let mut province_rings = Vec::with_capacity(n);
        let mut ring = 0usize;
        for id in 0..n {
            let first_ring = ring as u32;
            let line_start = line_indices.len() as u32;
            for _ in 0..rings[id] {
                let start = *ring_starts.last().unwrap();
                let len = ring_lens[ring];
                for i in 0..len {
                    line_indices.push(start + i);
                    line_indices.push(start + (i + 1) % len);
                }
                ring_starts.push(start + len);
                ring += 1;
            }
            province_rings.push([first_ring, rings[id] as u32]);
            line_ranges.push([line_start, line_indices.len() as u32 - line_start]);
        }

        Ok(MapMesh {
            width,
            height,
            provinces,
            positions,
            vertex_province,
            indices: global,
            line_positions,
            line_indices,
            line_ranges,
            ring_starts,
            province_rings,
        })
    }
}
