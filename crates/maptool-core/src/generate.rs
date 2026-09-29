//! Generate a province map from a hand-painted "border map": white pixels are
//! land, `#00FF00` pixels are sea/lake, black pixels are a border line to
//! respect now and interpret later (country/subdivision/natural — the color
//! alone doesn't say which). Land and, optionally, sea/lake are carved into
//! province-sized blobs; black pixels are absorbed into whichever province
//! is nearest, splitting a line's width between the two sides it separates.
//!
//! Every disconnected land or sea/lake region always becomes its own
//! province, even when `split_seas` is off: that flag only controls whether
//! a *connected* sea/lake body is further divided.
//!
//! Growth is best-first from jittered seed points, ordered by squared
//! Euclidean distance to each province's own reference point, not a plain
//! breadth-first search: a 4-connectivity BFS orders pixels by hop count
//! (Manhattan distance), which would make every province a diamond. 4
//! connectivity is still used for adjacency (matching the rest of the crate),
//! it just is not what orders the frontier.
//!
//! Growth happens in two passes. The first ([`grow`] with `allow_wall:
//! false`) only ever crosses land-to-land or sea/lake-to-sea/lake: a border
//! line is a hard barrier here, exactly like a coastline, so a province can
//! never walk onto a line and back off the other side onto more of its own
//! terrain — that would defeat the line's entire purpose. Only once every
//! land and sea/lake pixel is settled does a second pass, seeded by
//! [`seed_wall_frontier`] and run through [`grow`] again with `allow_wall:
//! true`, resolve the line's own pixels, splitting its width between
//! whichever neighbouring province reaches each of them first.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::{Error, PixelFormat};

const NONE: u32 = u32::MAX;

/// Tuning knobs for [`generate_labels`]. `land_radius`/`water_radius` describe
/// province size, not a count: provinces grow from seeds spaced roughly
/// `2 * radius` apart, jittered so borders are organic rather than a grid.
#[derive(Clone, Copy, Debug)]
pub struct GenerateOptions {
    pub land_radius: f64,
    /// Only used when `split_seas` is true; otherwise every connected
    /// sea/lake body is one province regardless of size.
    pub water_radius: f64,
    pub split_seas: bool,
    /// The same image and options with the same seed always produce the same result.
    pub seed: u32,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        GenerateOptions { land_radius: 24.0, water_radius: 24.0, split_seas: false, seed: 0 }
    }
}

/// Direct output of [`generate_labels`], in the same shape as the crate's
/// internal label map, so it can be turned into one without going through a
/// color image.
pub struct GeneratedLabels {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u32>,
    pub colors: Vec<[u8; 3]>,
    pub counts: Vec<u32>,
    /// `[x0, y0, x1, y1]`, exclusive upper bound.
    pub bboxes: Vec<[u32; 4]>,
    /// Ids of the provinces grown from sea/lake pixels; every other id is land.
    pub sea_provinces: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Terrain {
    Land,
    Water,
    Wall,
}

/// Classify every pixel as land, sea/lake or a border line. Alpha is ignored
/// even for [`PixelFormat::Rgba`] — border maps aren't expected to carry
/// transparency. Any color other than white, `#00FF00` or black is a hard
/// input error, never a panic.
fn classify(pixels: &[u8], width: usize, height: usize, format: PixelFormat) -> Result<Vec<Terrain>, Error> {
    let ch = format.channels();
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let p = &pixels[(y * width + x) * ch..][..ch];
            let (r, g, b) = (p[0], p[1], p[2]);
            let terrain = match (r, g, b) {
                (255, 255, 255) => Terrain::Land,
                (0, 255, 0) => Terrain::Water,
                (0, 0, 0) => Terrain::Wall,
                _ => return Err(Error::InvalidBorderColor { x: x as u32, y: y as u32, color: [r, g, b] }),
            };
            out.push(terrain);
        }
    }
    Ok(out)
}

/// 4-connectivity flood fill: every pixel where `is_target` holds gets a
/// component id shared with its same-target orthogonal neighbours; every
/// other pixel gets [`NONE`]. Returns the ids and the number of components.
fn label_components(width: usize, height: usize, is_target: impl Fn(usize) -> bool) -> (Vec<u32>, u32) {
    let mut ids = vec![NONE; width * height];
    let mut next_id = 0u32;
    let mut queue: VecDeque<usize> = VecDeque::new();

    for start in 0..width * height {
        if ids[start] != NONE || !is_target(start) {
            continue;
        }
        let id = next_id;
        next_id += 1;
        ids[start] = id;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % width, i / width);
            let mut neighbors = [None; 4];
            if x > 0 {
                neighbors[0] = Some(i - 1);
            }
            if x + 1 < width {
                neighbors[1] = Some(i + 1);
            }
            if y > 0 {
                neighbors[2] = Some(i - width);
            }
            if y + 1 < height {
                neighbors[3] = Some(i + width);
            }
            for j in neighbors.into_iter().flatten() {
                if ids[j] == NONE && is_target(j) {
                    ids[j] = id;
                    queue.push_back(j);
                }
            }
        }
    }
    (ids, next_id)
}

/// The pixel-index bounding box and an arbitrary "first seen" pixel of every
/// component id produced by [`label_components`], for seed placement.
struct ComponentBounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    first_pixel: usize,
}

fn component_bounds(ids: &[u32], count: u32, width: usize) -> Vec<ComponentBounds> {
    let mut bounds: Vec<ComponentBounds> = (0..count)
        .map(|_| ComponentBounds { min_x: u32::MAX, min_y: u32::MAX, max_x: 0, max_y: 0, first_pixel: usize::MAX })
        .collect();
    for (i, &id) in ids.iter().enumerate() {
        if id == NONE {
            continue;
        }
        let (x, y) = ((i % width) as u32, (i / width) as u32);
        let b = &mut bounds[id as usize];
        b.min_x = b.min_x.min(x);
        b.min_y = b.min_y.min(y);
        b.max_x = b.max_x.max(x);
        b.max_y = b.max_y.max(y);
        if b.first_pixel == usize::MAX {
            b.first_pixel = i;
        }
    }
    bounds
}

/// A deterministic, dependency-free hash, so seed jitter is reproducible from
/// `seed` alone (splitmix64-style mixing). Returns a value in `0..range.max(1)`.
fn hash_jitter(seed: u32, stream: u32, cx: i64, cy: i64, range: u32) -> u32 {
    let mut h = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(stream as u64);
    h ^= (cx as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h = h.wrapping_mul(0xD6E8_FEB8_6659_FD93).wrapping_add(cy as u64);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h % range.max(1) as u64) as u32
}

/// Seed points for one component: a jittered grid over its bounding box,
/// spaced `~2 * radius_at(..)` apart, kept only where a candidate lands on a
/// pixel of that same component. Falls back to the component's first pixel
/// if none survive, so every component (even a single pixel) gets a seed.
///
/// `radius_at` takes a location and returns the local target radius; today
/// every caller passes a closure that ignores its argument and always
/// returns the same flat radius, but the signature leaves room for a future
/// spatially-varying density without reshaping seed placement itself.
fn place_seeds(width: usize, ids: &[u32], component: u32, bounds: &ComponentBounds, seed: u32, stream: u32, radius_at: impl Fn(u32, u32) -> f64) -> Vec<usize> {
    let mut out = Vec::new();
    let (min_x, min_y, max_x, max_y) = (bounds.min_x as i64, bounds.min_y as i64, bounds.max_x as i64, bounds.max_y as i64);
    let mut y = min_y;
    while y <= max_y {
        let cell = (2.0 * radius_at(min_x as u32, y as u32)).round().max(1.0) as i64;
        let mut x = min_x;
        while x <= max_x {
            let jx = hash_jitter(seed, stream, x, y, cell as u32) as i64;
            let jy = hash_jitter(seed, stream ^ 0x5bd1_e995, x, y, cell as u32) as i64;
            let (px, py) = (x + jx, y + jy);
            if (px as usize) < width && py >= 0 {
                let idx = py as usize * width + px as usize;
                if idx < ids.len() && ids[idx] == component {
                    out.push(idx);
                }
            }
            x += cell;
        }
        y += cell;
    }
    if out.is_empty() {
        out.push(bounds.first_pixel);
    }
    out
}

fn dist2(x: u32, y: u32, rx: u32, ry: u32) -> u64 {
    let (dx, dy) = (x as i64 - rx as i64, y as i64 - ry as i64);
    (dx * dx + dy * dy) as u64
}

/// Per-pixel labels and per-province bookkeeping, built incrementally by
/// [`grow`], [`claim_unreachable_pockets`] and [`repair_junctions_and_exclaves`].
struct Growth {
    data: Vec<u32>,
    counts: Vec<u32>,
    bboxes: Vec<[u32; 4]>,
    /// Which terrain each province may grow through (besides `Wall`, always allowed).
    origin: Vec<Terrain>,
    /// The point growth distance is measured from: a seed pixel, or a water
    /// component's centroid when it is not split.
    ref_point: Vec<(u32, u32)>,
}

impl Growth {
    fn new(pixel_count: usize) -> Growth {
        Growth { data: vec![NONE; pixel_count], counts: Vec::new(), bboxes: Vec::new(), origin: Vec::new(), ref_point: Vec::new() }
    }

    fn new_province(&mut self, origin: Terrain, ref_point: (u32, u32)) -> u32 {
        let id = self.counts.len() as u32;
        self.counts.push(0);
        self.bboxes.push([u32::MAX, u32::MAX, 0, 0]);
        self.origin.push(origin);
        self.ref_point.push(ref_point);
        id
    }

    fn claim(&mut self, pixel: usize, x: u32, y: u32, province: u32) {
        self.data[pixel] = province;
        self.counts[province as usize] += 1;
        let b = &mut self.bboxes[province as usize];
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x + 1);
        b[3] = b[3].max(y + 1);
    }
}

/// A province's identity for the purpose of one growth-frontier push: its id,
/// which terrain it may enter, and its reference point.
#[derive(Clone, Copy)]
struct ProvinceRef {
    id: u32,
    origin: Terrain,
    x: u32,
    y: u32,
}

/// Push `(nx, ny)` onto the frontier if it is still unclaimed and its raw
/// terrain is the province's own origin terrain, or — only when `allow_wall`
/// — a border line. `allow_wall` must be `false` for the primary growth pass
/// (a border line is a hard barrier there, exactly like a coastline: without
/// this, a province could walk onto a border-line pixel and back off the
/// other side onto more of its own terrain, crossing a line it was supposed
/// to respect) and `true` only for the later pass that resolves border-line
/// pixels themselves (see [`seed_wall_frontier`]).
#[allow(clippy::too_many_arguments)] // each parameter is load-bearing; bundling them would just move the count into a one-off struct
fn push_if_open(terrain: &[Terrain], growth: &Growth, heap: &mut BinaryHeap<Reverse<(u64, u32, usize)>>, width: u32, nx: u32, ny: u32, p: ProvinceRef, allow_wall: bool) {
    let npixel = (ny * width + nx) as usize;
    let t = terrain[npixel];
    if growth.data[npixel] == NONE && (t == p.origin || (allow_wall && t == Terrain::Wall)) {
        heap.push(Reverse((dist2(nx, ny, p.x, p.y), p.id, npixel)));
    }
}

/// Grow every province from the seeds already pushed onto `heap`: pop the
/// closest pending claim, skip it if the pixel was claimed since it was
/// pushed, otherwise claim it and offer its open neighbours. With
/// `allow_wall: false` this is what makes both coastlines and border lines a
/// hard barrier during primary growth (a province can never enter another
/// terrain, nor cross a border line to reach more of its own). Called a
/// second time with `allow_wall: true` (seeded by [`seed_wall_frontier`]) to
/// resolve border-line pixels afterward, splitting a line's width between
/// whichever side's frontier reaches each of its pixels first.
fn grow(terrain: &[Terrain], width: usize, height: usize, growth: &mut Growth, mut heap: BinaryHeap<Reverse<(u64, u32, usize)>>, allow_wall: bool) {
    let (width, height) = (width as u32, height as u32);
    while let Some(Reverse((_, province, pixel))) = heap.pop() {
        if growth.data[pixel] != NONE {
            continue;
        }
        let (x, y) = (pixel as u32 % width, pixel as u32 / width);
        growth.claim(pixel, x, y, province);
        let p = ProvinceRef { id: province, origin: growth.origin[province as usize], x: growth.ref_point[province as usize].0, y: growth.ref_point[province as usize].1 };
        if x > 0 {
            push_if_open(terrain, growth, &mut heap, width, x - 1, y, p, allow_wall);
        }
        if x + 1 < width {
            push_if_open(terrain, growth, &mut heap, width, x + 1, y, p, allow_wall);
        }
        if y > 0 {
            push_if_open(terrain, growth, &mut heap, width, x, y - 1, p, allow_wall);
        }
        if y + 1 < height {
            push_if_open(terrain, growth, &mut heap, width, x, y + 1, p, allow_wall);
        }
    }
}

/// Seed every still-unclaimed border-line pixel adjacent to an already-claimed
/// pixel, so a following `grow(.., allow_wall: true)` pass can resolve every
/// border-line pixel without ever letting the original growth cross a line to
/// reach more of its own terrain. Run only after the primary growth pass has
/// fully claimed every land and sea/lake pixel (each component is internally
/// connected through its own terrain alone, so it needs no border-line
/// shortcut to be fully covered) — by then every pixel this function's
/// `push_if_open` calls can still open is a border-line pixel, never land or
/// sea/lake belonging to some other, now-unreachable component.
fn seed_wall_frontier(terrain: &[Terrain], width: usize, height: usize, growth: &Growth) -> BinaryHeap<Reverse<(u64, u32, usize)>> {
    let mut heap = BinaryHeap::new();
    let (width_u32, height_u32) = (width as u32, height as u32);
    for i in 0..width * height {
        let province = growth.data[i];
        if province == NONE {
            continue;
        }
        let p = ProvinceRef { id: province, origin: growth.origin[province as usize], x: growth.ref_point[province as usize].0, y: growth.ref_point[province as usize].1 };
        let (x, y) = ((i % width) as u32, (i / width) as u32);
        if x > 0 {
            push_if_open(terrain, growth, &mut heap, width_u32, x - 1, y, p, true);
        }
        if x + 1 < width_u32 {
            push_if_open(terrain, growth, &mut heap, width_u32, x + 1, y, p, true);
        }
        if y > 0 {
            push_if_open(terrain, growth, &mut heap, width_u32, x, y - 1, p, true);
        }
        if y + 1 < height_u32 {
            push_if_open(terrain, growth, &mut heap, width_u32, x, y + 1, p, true);
        }
    }
    heap
}

/// After [`grow`], any pixel still unclaimed is a border-line pocket with no
/// reachable land or sea/lake seed (this includes an all-black image). Give
/// each such connected pocket its own new province rather than leaving gaps
/// or panicking.
fn claim_unreachable_pockets(width: usize, height: usize, growth: &mut Growth) {
    let (pocket_ids, pocket_count) = label_components(width, height, |i| growth.data[i] == NONE);
    if pocket_count == 0 {
        return;
    }
    let mut province_of_pocket = vec![NONE; pocket_count as usize];
    for (i, &pocket) in pocket_ids.iter().enumerate() {
        if pocket == NONE {
            continue;
        }
        let (x, y) = ((i % width) as u32, (i / width) as u32);
        let province = province_of_pocket[pocket as usize];
        let province = if province == NONE {
            let id = growth.new_province(Terrain::Wall, (x, y));
            province_of_pocket[pocket as usize] = id;
            id
        } else {
            province
        };
        growth.claim(i, x, y, province);
    }
}

/// Whether pixel `(x, y)` is a single-pixel exclave: its province has other
/// pixels elsewhere (`counts >= 2`) but none of its 4 neighbours share its label.
/// Mirrors `validate::check`'s exact condition.
fn is_exclave(data: &[u32], counts: &[u32], width: usize, height: usize, x: usize, y: usize) -> bool {
    let i = y * width + x;
    let l = data[i];
    if l == NONE || counts[l as usize] < 2 {
        return false;
    }
    let joined =
        (x > 0 && data[i - 1] == l) || (x + 1 < width && data[i + 1] == l) || (y > 0 && data[i - width] == l) || (y + 1 < height && data[i + width] == l);
    !joined
}

/// Whether the corner at `(x, y)` (both `>= 1`) is a four-way junction: the
/// 2x2 block of pixels around it has four pairwise-distinct labels. Mirrors
/// `validate::check`'s exact condition.
fn junction_labels(data: &[u32], width: usize, x: usize, y: usize) -> bool {
    let (a, b, c, e) = (data[(y - 1) * width + x - 1], data[(y - 1) * width + x], data[y * width + x - 1], data[y * width + x]);
    if [a, b, c, e].contains(&NONE) {
        return false;
    }
    a != b && a != c && a != e && b != c && b != e && c != e
}

fn relabel(growth: &mut Growth, width: usize, x: usize, y: usize, new_label: u32) {
    let i = y * width + x;
    let old = growth.data[i];
    if old != NONE {
        growth.counts[old as usize] -= 1;
    }
    growth.data[i] = new_label;
    growth.counts[new_label as usize] += 1;
    // Grow the new province's bbox; the old province's bbox is left alone
    // (a bbox that is larger than the true extent is always a safe fast-reject,
    // per `MapMesh::near`/`provinces_in_rect` — only a too-small one would be a bug).
    let b = &mut growth.bboxes[new_label as usize];
    b[0] = b[0].min(x as u32);
    b[1] = b[1].min(y as u32);
    b[2] = b[2].max(x as u32 + 1);
    b[3] = b[3].max(y as u32 + 1);
}

/// Re-check the corners and pixels a relabel at `(x, y)` could have affected,
/// and queue whichever are still (or newly) violations.
fn requeue_around(width: usize, height: usize, growth: &Growth, x: usize, y: usize, junctions: &mut VecDeque<(usize, usize)>, exclaves: &mut VecDeque<(usize, usize)>) {
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let (px, py) = (x as i64 + dx, y as i64 + dy);
            if px < 0 || py < 0 || px as usize >= width || py as usize >= height {
                continue;
            }
            let (px, py) = (px as usize, py as usize);
            if is_exclave(&growth.data, &growth.counts, width, height, px, py) {
                exclaves.push_back((px, py));
            }
        }
    }
    for &(cx, cy) in &[(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
        if cx == 0 || cy == 0 || cx >= width || cy >= height {
            continue;
        }
        if junction_labels(&growth.data, width, cx, cy) {
            junctions.push_back((cx, cy));
        }
    }
}

/// Fix the four-way junction at `(x, y)`, if it is still one: relabel one of
/// its four corner pixels to match an orthogonal partner within the same 2x2
/// block, preferring `(x, y)` itself, then `(x-1, y)`, `(x, y-1)`, `(x-1, y-1)`
/// in turn, skipping any whose current province would be emptied by the move
/// (`counts < 2`). If all four would be emptied, the junction is left as is —
/// `validate::check` remains the final gate on the caller's own `validate` flag.
fn repair_one_junction(width: usize, height: usize, growth: &mut Growth, x: usize, y: usize, junctions: &mut VecDeque<(usize, usize)>, exclaves: &mut VecDeque<(usize, usize)>) {
    if !junction_labels(&growth.data, width, x, y) {
        return; // an earlier repair in this pass already resolved it
    }
    // a=(x-1,y-1), b=(x,y-1), c=(x-1,y), e=(x,y); orthogonal in-block pairs are a-b and c-e.
    let corners = [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)];
    let partner_of = [1usize, 0, 3, 2];
    for &src in &[3usize, 2, 1, 0] {
        let (sx, sy) = corners[src];
        let label = growth.data[sy * width + sx];
        if growth.counts[label as usize] < 2 {
            continue;
        }
        let (tx, ty) = corners[partner_of[src]];
        let target = growth.data[ty * width + tx];
        relabel(growth, width, sx, sy, target);
        requeue_around(width, height, growth, sx, sy, junctions, exclaves);
        return;
    }
}

/// Fix the single-pixel exclave at `(x, y)`, if it is still one: relabel it
/// to match its first available 4-neighbour (there always is one once past
/// `claim_unreachable_pockets`, since no pixel is `NONE` by then).
fn repair_one_exclave(width: usize, height: usize, growth: &mut Growth, x: usize, y: usize, junctions: &mut VecDeque<(usize, usize)>, exclaves: &mut VecDeque<(usize, usize)>) {
    if !is_exclave(&growth.data, &growth.counts, width, height, x, y) {
        return;
    }
    let i = y * width + x;
    let neighbor = if x > 0 {
        Some(growth.data[i - 1])
    } else if x + 1 < width {
        Some(growth.data[i + 1])
    } else if y > 0 {
        Some(growth.data[i - width])
    } else if y + 1 < height {
        Some(growth.data[i + width])
    } else {
        None
    };
    let Some(new_label) = neighbor else { return };
    relabel(growth, width, x, y, new_label);
    requeue_around(width, height, growth, x, y, junctions, exclaves);
}

/// Best-effort repair of `validate.rs`'s two rejected shapes, in case jittered
/// seed placement still produced an exact tie somewhere. Worklist-based: after
/// the first full scan, a repair only re-checks the handful of pixels/corners
/// it could have affected, not the whole image. `generate_labels`'s caller
/// still runs `validate::check` afterward (when its own `validate` option asks
/// for it), so a repair that does not fully converge surfaces as an ordinary
/// `Error::Invalid`, never a silently broken map.
fn repair_junctions_and_exclaves(width: usize, height: usize, growth: &mut Growth) {
    let mut junctions: VecDeque<(usize, usize)> = VecDeque::new();
    let mut exclaves: VecDeque<(usize, usize)> = VecDeque::new();
    for y in 1..height {
        for x in 1..width {
            if junction_labels(&growth.data, width, x, y) {
                junctions.push_back((x, y));
            }
        }
    }
    for y in 0..height {
        for x in 0..width {
            if is_exclave(&growth.data, &growth.counts, width, height, x, y) {
                exclaves.push_back((x, y));
            }
        }
    }

    let limit = width.saturating_mul(height).saturating_add(1);
    let mut steps = 0usize;
    while steps <= limit {
        if let Some((x, y)) = junctions.pop_front() {
            repair_one_junction(width, height, growth, x, y, &mut junctions, &mut exclaves);
        } else if let Some((x, y)) = exclaves.pop_front() {
            repair_one_exclave(width, height, growth, x, y, &mut junctions, &mut exclaves);
        } else {
            break;
        }
        steps += 1;
    }
}

/// A province color, injective (not just visually distinct) over the
/// realistic id range: the multiplier is odd, hence a unit mod 2^24, so this
/// map has no collisions for any `id` under 2^24 — required because
/// `Labels::build`'s province-identity model is "exact same color = same
/// province," so two unrelated blobs must never collide if this is ever
/// re-encoded to a color image.
fn province_color(id: u32) -> [u8; 3] {
    let v = id.wrapping_mul(0x9E37_79B1) & 0x00FF_FFFF;
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Generate a province labeling from a hand-painted border map. See the
/// module docs for the color contract. Never panics on bad input.
pub fn generate_labels(pixels: &[u8], width: u32, height: u32, format: PixelFormat, opts: &GenerateOptions) -> Result<GeneratedLabels, Error> {
    crate::check_dims(pixels, width, height, format)?;
    let (w, h) = (width as usize, height as usize);
    let terrain = classify(pixels, w, h, format)?;

    let (land_ids, land_count) = label_components(w, h, |i| terrain[i] == Terrain::Land);
    let (water_ids, water_count) = label_components(w, h, |i| terrain[i] == Terrain::Water);
    let land_bounds = component_bounds(&land_ids, land_count, w);
    let water_bounds = component_bounds(&water_ids, water_count, w);

    let mut growth = Growth::new(w * h);
    let mut heap: BinaryHeap<Reverse<(u64, u32, usize)>> = BinaryHeap::new();
    let mut sea_provinces = Vec::new();

    for component in 0..land_count {
        let bounds = &land_bounds[component as usize];
        let seeds = place_seeds(w, &land_ids, component, bounds, opts.seed, component, |_, _| opts.land_radius);
        for &seed_pixel in &seeds {
            let (sx, sy) = ((seed_pixel % w) as u32, (seed_pixel / w) as u32);
            let province = growth.new_province(Terrain::Land, (sx, sy));
            heap.push(Reverse((0, province, seed_pixel)));
        }
    }

    for component in 0..water_count {
        let bounds = &water_bounds[component as usize];
        if opts.split_seas {
            // Distinct stream per component, offset past every land stream used above.
            let seeds = place_seeds(w, &water_ids, component, bounds, opts.seed, land_count.wrapping_add(component), |_, _| opts.water_radius);
            for &seed_pixel in &seeds {
                let (sx, sy) = ((seed_pixel % w) as u32, (seed_pixel / w) as u32);
                let province = growth.new_province(Terrain::Water, (sx, sy));
                sea_provinces.push(province);
                heap.push(Reverse((0, province, seed_pixel)));
            }
        } else {
            claim_whole_water_component(&water_ids, component, w, &mut growth, &mut sea_provinces);
        }
    }

    // Primary growth: a border line is a hard barrier, exactly like a
    // coastline, so land and sea/lake never cross one to reach more of their
    // own terrain. Every land/sea component is fully claimed by this point —
    // each is internally connected through its own terrain alone.
    grow(&terrain, w, h, &mut growth, heap, false);
    // Resolve the border-line pixels themselves, now that every neighbouring
    // province is settled: split between whichever side's frontier reaches
    // each of them first.
    let wall_heap = seed_wall_frontier(&terrain, w, h, &growth);
    grow(&terrain, w, h, &mut growth, wall_heap, true);
    claim_unreachable_pockets(w, h, &mut growth);
    repair_junctions_and_exclaves(w, h, &mut growth);

    let colors: Vec<[u8; 3]> = (0..growth.counts.len() as u32).map(province_color).collect();
    Ok(GeneratedLabels { width, height, data: growth.data, colors, counts: growth.counts, bboxes: growth.bboxes, sea_provinces })
}

/// Claim every pixel of one whole, un-split water component as a single
/// province, around its centroid. Its border-line neighbours are resolved
/// later, by [`seed_wall_frontier`]/the second [`grow`] pass, exactly like
/// every other province's — this function only needs to claim the
/// component's own (already fully connected) interior.
fn claim_whole_water_component(water_ids: &[u32], component: u32, width: usize, growth: &mut Growth, sea_provinces: &mut Vec<u32>) {
    let (mut sum_x, mut sum_y, mut n) = (0u64, 0u64, 0u64);
    for (i, &id) in water_ids.iter().enumerate() {
        if id == component {
            sum_x += (i % width) as u64;
            sum_y += (i / width) as u64;
            n += 1;
        }
    }
    if n == 0 {
        return;
    }
    let centroid = ((sum_x / n) as u32, (sum_y / n) as u32);
    let province = growth.new_province(Terrain::Water, centroid);
    sea_provinces.push(province);

    for (i, &id) in water_ids.iter().enumerate() {
        if id != component {
            continue;
        }
        let (x, y) = ((i % width) as u32, (i / width) as u32);
        growth.claim(i, x, y, province);
    }
}
