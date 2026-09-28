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
    /// Bit pattern of border-point vertex `i`'s position, for exact-match hashing.
    /// Neighbours share bit-identical coordinates on shared borders, so bit
    /// comparison — not float equality — is a safe and exact way to find them.
    pub(crate) fn border_point_bits(&self, i: u32) -> (u32, u32) {
        (self.line_positions[i as usize * 2].to_bits(), self.line_positions[i as usize * 2 + 1].to_bits())
    }

    /// Border segments (index pairs into `line_positions`, for `LINES`) of the outline
    /// of the union of `ids`: segments shared by two selected provinces are left
    /// out, so only the outer edge of the whole group remains.
    pub fn boundary_indices(&self, ids: &[u32]) -> Vec<u32> {
        use std::collections::HashSet;
        let point = |i: u32| self.border_point_bits(i);
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
        (0..self.provinces.len()).find(|&id| self.near(id, x, y, x, y) && self.contains(id, x, y)).map(|id| id as u32)
    }

    /// Whether the pixel bounds of province `id`, grown by a pixel (smoothed borders can
    /// bulge slightly beyond them), overlap the rectangle.
    fn near(&self, id: usize, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
        let [bx0, by0, bx1, by1] = self.provinces[id].bbox;
        !(x1 < bx0 as f64 - 1.0 || x0 > bx1 as f64 + 1.0 || y1 < by0 as f64 - 1.0 || y0 > by1 as f64 + 1.0)
    }

    /// The rings of province `id`, as slices of x, y pairs.
    fn rings(&self, id: usize) -> impl Iterator<Item = &[f32]> {
        let [first, count] = self.province_rings[id];
        (first..first + count).map(move |ring| {
            let (a, b) = (self.ring_starts[ring as usize] as usize, self.ring_starts[ring as usize + 1] as usize);
            &self.line_positions[a * 2..b * 2]
        })
    }

    /// Whether the point is inside province `id` (even-odd over its rings, so holes count).
    fn contains(&self, id: usize, x: f64, y: f64) -> bool {
        let mut inside = false;
        for pts in self.rings(id) {
            let n = pts.len() / 2;
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
        inside
    }

    /// The provinces in the rectangle `x0,y0`-`x1,y1` (image pixels, any corner order),
    /// ascending: those whose shape touches it, or with `whole` only those whose pixel
    /// bounds lie entirely inside it.
    pub fn provinces_in_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64, whole: bool) -> Vec<u32> {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let mut out = Vec::new();
        for (id, info) in self.provinces.iter().enumerate() {
            let hit = if whole {
                let [bx0, by0, bx1, by1] = info.bbox;
                bx0 as f64 >= x0 && bx1 as f64 <= x1 && by0 as f64 >= y0 && by1 as f64 <= y1
            } else {
                self.near(id, x0, y0, x1, y1)
                    && (self.rings(id).any(|pts| {
                        let n = pts.len() / 2;
                        (0..n).any(|i| {
                            let j = (i + 1) % n;
                            segment_hits_rect(
                                (pts[i * 2] as f64, pts[i * 2 + 1] as f64),
                                (pts[j * 2] as f64, pts[j * 2 + 1] as f64),
                                (x0, y0, x1, y1),
                            )
                        })
                    }) || self.contains(id, (x0 + x1) / 2.0, (y0 + y1) / 2.0))
            };
            if hit {
                out.push(id as u32);
            }
        }
        out
    }
}

fn empty_mesh(width: u32, height: u32, provinces: Vec<ProvinceInfo>) -> MapMesh {
    MapMesh {
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
    }
}

/// Push one province's ring outlines into the mesh's border buffers.
fn add_province_borders(mesh: &mut MapMesh, province_rings: &[Vec<[f32; 2]>]) {
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
}

/// Tessellate one province's rings (even-odd fill, so holes and exclaves just work)
/// into a fresh triangle vertex/index buffer.
fn triangulate_province(
    tess: &mut FillTessellator,
    options: &FillOptions,
    province_rings: &[Vec<[f32; 2]>],
    id: u32,
) -> Result<VertexBuffers<[f32; 2], u32>, Error> {
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
    tess.tessellate_path(&path, options, &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()))
        .map_err(|e| Error::Tessellation { province: id, message: format!("{e:?}") })?;
    Ok(buffers)
}

/// Append one province's triangles to the mesh's flat vertex/index buffers.
fn append_triangles(mesh: &mut MapMesh, id: u32, buffers: &VertexBuffers<[f32; 2], u32>) {
    let base = mesh.vertex_province.len() as u32;
    for p in &buffers.vertices {
        mesh.positions.extend_from_slice(p);
    }
    mesh.vertex_province.extend(std::iter::repeat_n(id, buffers.vertices.len()));
    mesh.indices.extend(buffers.indices.iter().map(|i| i + base));
}

pub fn build(
    width: u32,
    height: u32,
    provinces: Vec<ProvinceInfo>,
    rings: Vec<Vec<Vec<[f32; 2]>>>,
) -> Result<MapMesh, Error> {
    let mut mesh = empty_mesh(width, height, provinces);
    let mut tess = FillTessellator::new();
    let options = FillOptions::default().with_fill_rule(FillRule::EvenOdd);

    for (id, province_rings) in rings.iter().enumerate() {
        add_province_borders(&mut mesh, province_rings);
        let buffers = triangulate_province(&mut tess, &options, province_rings, id as u32)?;
        append_triangles(&mut mesh, id as u32, &buffers);
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

/// Whether the segment `a`-`b` has any point in the rectangle `(x0, y0, x1, y1)` (borders
/// count). Liang-Barsky clipping.
fn segment_hits_rect(a: (f64, f64), b: (f64, f64), (x0, y0, x1, y1): (f64, f64, f64, f64)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0 - x0), (dx, x1 - a.0), (-dy, a.1 - y0), (dy, y1 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return false;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return false;
                }
                t1 = t1.min(r);
            }
        }
    }
    true
}

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

/// Per-province vertex/index counts, and each province's local index base
/// (where its vertices start in the flat, rebased index stream) — computed
/// once and shared by `write_header_and_provinces` and `write_geometry`.
struct EncodedCounts {
    vertices: Vec<u32>,
    indices: Vec<u32>,
    base: Vec<u32>,
}

fn count_per_province(mesh: &MapMesh) -> EncodedCounts {
    let n = mesh.provinces.len();
    let mut vertices = vec![0u32; n];
    for &p in &mesh.vertex_province {
        vertices[p as usize] += 1;
    }
    let mut indices = vec![0u32; n];
    for tri in mesh.indices.chunks_exact(3) {
        indices[mesh.vertex_province[tri[0] as usize] as usize] += 3;
    }
    let mut base = vec![0u32; n];
    let mut acc = 0;
    for i in 0..n {
        base[i] = acc;
        acc += vertices[i];
    }
    EncodedCounts { vertices, indices, base }
}

fn write_header_and_provinces(out: &mut Vec<u8>, mesh: &MapMesh, counts: &EncodedCounts) {
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    let ring_count = mesh.ring_starts.len() - 1;
    put(mesh.width);
    put(mesh.height);
    put(mesh.provinces.len() as u32);
    put(ring_count as u32);
    for (i, p) in mesh.provinces.iter().enumerate() {
        put(u32::from_le_bytes([p.color[0], p.color[1], p.color[2], 0]));
        put(p.pixel_count);
        for b in p.bbox {
            put(b);
        }
        put(counts.vertices[i]);
        put(counts.indices[i]);
        put(mesh.province_rings[i][1]);
    }
}

fn write_geometry(out: &mut Vec<u8>, mesh: &MapMesh, counts: &EncodedCounts) {
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    for w in mesh.ring_starts.windows(2) {
        put(w[1] - w[0]);
    }
    for &v in &mesh.positions {
        put(v.to_bits());
    }
    let mut idx = mesh.indices.iter();
    for i in 0..mesh.provinces.len() {
        for _ in 0..counts.indices[i] {
            put(idx.next().unwrap() - counts.base[i]);
        }
    }
    for &v in &mesh.line_positions {
        put(v.to_bits());
    }
}

struct MeshHeader {
    width: u32,
    height: u32,
    province_count: usize,
    ring_count: usize,
}

fn read_header(r: &mut Reader, payload_len: usize) -> Result<MeshHeader, Error> {
    let (width, height) = (r.u32()?, r.u32()?);
    let province_count = r.u32()? as usize;
    let ring_count = r.u32()? as usize;
    if width == 0 || height == 0 {
        return Err(bad("zero-sized image"));
    }
    // Each province takes 36 bytes, so a count the payload cannot hold is corrupt.
    if province_count > payload_len / 36 || ring_count > payload_len / 4 {
        return Err(bad("counts exceed the data"));
    }
    Ok(MeshHeader { width, height, province_count, ring_count })
}

/// Per-province counts read from the payload, before geometry is read.
struct RawProvinceCounts {
    provinces: Vec<ProvinceInfo>,
    vertices: Vec<usize>,
    indices: Vec<usize>,
    rings: Vec<usize>,
    total_vertices: usize,
    total_indices: usize,
}

fn read_provinces(r: &mut Reader, header: &MeshHeader) -> Result<RawProvinceCounts, Error> {
    let n = header.province_count;
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
    if total_rings != header.ring_count {
        return Err(bad("ring counts disagree"));
    }
    Ok(RawProvinceCounts { provinces, vertices, indices, rings, total_vertices, total_indices })
}

/// Ring lengths for the whole payload, and the total number of border points they sum to.
fn read_ring_lengths(r: &mut Reader, ring_count: usize) -> Result<(Vec<u32>, usize), Error> {
    let ring_lens = r.u32s(ring_count)?;
    let total_points: usize =
        ring_lens.iter().try_fold(0usize, |a, &b| a.checked_add(b as usize)).ok_or_else(|| bad("counts overflow"))?;
    if ring_lens.iter().any(|&l| l < 3) {
        return Err(bad("a ring has fewer than 3 points"));
    }
    Ok((ring_lens, total_points))
}

struct GeometryArrays {
    positions: Vec<f32>,
    /// Triangle indices, local to their own province (see `rebuild_triangle_indices`).
    local_indices: Vec<u32>,
    line_positions: Vec<f32>,
}

/// Triangle positions, local (per-province) triangle indices, and border points.
fn read_geometry_arrays(
    r: &mut Reader,
    counts: &RawProvinceCounts,
    total_points: usize,
    payload_len: usize,
) -> Result<GeometryArrays, Error> {
    let positions = r.f32s(counts.total_vertices.checked_mul(2).ok_or_else(|| bad("too large"))?)?;
    let local_indices = r.u32s(counts.total_indices)?;
    let line_positions = r.f32s(total_points.checked_mul(2).ok_or_else(|| bad("too large"))?)?;
    if r.pos != payload_len {
        return Err(bad("unexpected trailing data"));
    }
    if u32::try_from(counts.total_vertices).is_err() || u32::try_from(total_points).is_err() {
        return Err(bad("too many vertices"));
    }
    Ok(GeometryArrays { positions, local_indices, line_positions })
}

struct TriangleIndices {
    vertex_province: Vec<u32>,
    indices: Vec<u32>,
}

/// Per-vertex province ids, and triangle indices rebased from per-province-local
/// to global (`local` is not stored globally, to keep the format small).
fn rebuild_triangle_indices(counts: &RawProvinceCounts, local: &[u32]) -> Result<TriangleIndices, Error> {
    let n = counts.provinces.len();
    let mut vertex_province = Vec::with_capacity(counts.total_vertices);
    let mut indices = Vec::with_capacity(counts.total_indices);
    let (mut base, mut next) = (0u32, 0usize);
    for id in 0..n {
        vertex_province.extend(std::iter::repeat_n(id as u32, counts.vertices[id]));
        for &i in &local[next..next + counts.indices[id]] {
            if i as usize >= counts.vertices[id] {
                return Err(bad("a triangle refers to a missing vertex"));
            }
            indices.push(base + i);
        }
        next += counts.indices[id];
        base += counts.vertices[id] as u32;
    }
    Ok(TriangleIndices { vertex_province, indices })
}

struct BorderIndex {
    ring_starts: Vec<u32>,
    line_indices: Vec<u32>,
    line_ranges: Vec<[u32; 2]>,
    province_rings: Vec<[u32; 2]>,
}

/// Border segment indices and per-province ring/line bookkeeping, rebuilt from
/// each province's ring count and each ring's point count.
fn rebuild_borders(counts: &RawProvinceCounts, ring_lens: &[u32]) -> BorderIndex {
    let n = counts.provinces.len();
    let total_points: usize = ring_lens.iter().map(|&l| l as usize).sum();
    let mut ring_starts = Vec::with_capacity(ring_lens.len() + 1);
    ring_starts.push(0u32);
    let mut line_indices = Vec::with_capacity(total_points * 2);
    let mut line_ranges = Vec::with_capacity(n);
    let mut province_rings = Vec::with_capacity(n);
    let mut ring = 0usize;
    for id in 0..n {
        let first_ring = ring as u32;
        let line_start = line_indices.len() as u32;
        for _ in 0..counts.rings[id] {
            let start = *ring_starts.last().unwrap();
            let len = ring_lens[ring];
            for i in 0..len {
                line_indices.push(start + i);
                line_indices.push(start + (i + 1) % len);
            }
            ring_starts.push(start + len);
            ring += 1;
        }
        province_rings.push([first_ring, counts.rings[id] as u32]);
        line_ranges.push([line_start, line_indices.len() as u32 - line_start]);
    }
    BorderIndex { ring_starts, line_indices, line_ranges, province_rings }
}

impl MapMesh {
    /// Serialize to the compressed section format described above.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let counts = count_per_province(self);
        let ring_count = self.ring_starts.len() - 1;
        let mut out: Vec<u8> = Vec::with_capacity(
            16 + self.provinces.len() * 36
                + ring_count * 4
                + self.positions.len() * 4
                + self.indices.len() * 4
                + self.line_positions.len() * 4,
        );
        write_header_and_provinces(&mut out, self, &counts);
        write_geometry(&mut out, self, &counts);
        miniz_oxide::deflate::compress_to_vec(&out, COMPRESSION_LEVEL)
    }

    /// Read a section written by [`MapMesh::encode`]. Never panics on bad input.
    pub(crate) fn decode(bytes: &[u8]) -> Result<MapMesh, Error> {
        let payload = miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, MAX_PAYLOAD)
            .map_err(|e| bad(format!("damaged data ({e:?})")))?;
        let mut r = Reader { data: &payload, pos: 0 };

        let header = read_header(&mut r, payload.len())?;
        let counts = read_provinces(&mut r, &header)?;
        let (ring_lens, total_points) = read_ring_lengths(&mut r, header.ring_count)?;
        let geometry = read_geometry_arrays(&mut r, &counts, total_points, payload.len())?;
        let triangles = rebuild_triangle_indices(&counts, &geometry.local_indices)?;
        let borders = rebuild_borders(&counts, &ring_lens);

        Ok(MapMesh {
            width: header.width,
            height: header.height,
            provinces: counts.provinces,
            positions: geometry.positions,
            vertex_province: triangles.vertex_province,
            indices: triangles.indices,
            line_positions: geometry.line_positions,
            line_indices: borders.line_indices,
            line_ranges: borders.line_ranges,
            ring_starts: borders.ring_starts,
            province_rings: borders.province_rings,
        })
    }
}
