//! Tracing of the crack-edge graph between differently-labelled pixels.
//!
//! Vertices are pixel corners (lattice points `0..=w` x `0..=h`); edges are the
//! unit segments that separate two pixels with different labels. Vertices where
//! three or more labels meet (and image corners) are *anchors*. The edges are
//! cut into *chains* running between anchors; every edge belongs to exactly one
//! chain, so a border shared by two provinces is traced exactly once.

use crate::geom::LatticePoint;
use crate::label::Labels;

pub struct RawChain {
    /// Label on the left-hand side when walking `pts` in order (screen coordinates, y down).
    pub left: u32,
    pub right: u32,
    pub pts: Vec<LatticePoint>,
    /// A loop with no anchor on it. `pts.first() == pts.last()`.
    pub closed: bool,
}

struct Tracer<'a> {
    labels: &'a Labels,
    w: usize,
    h: usize,
    /// Number of horizontal edges; vertical edge ids start here.
    nh: usize,
    visited: Vec<u64>,
}

impl Tracer<'_> {
    #[inline]
    fn h_id(&self, x: i32, y: i32) -> usize {
        y as usize * self.w + x as usize
    }

    #[inline]
    fn v_id(&self, x: i32, y: i32) -> usize {
        self.nh + y as usize * (self.w + 1) + x as usize
    }

    #[inline]
    fn seen(&self, id: usize) -> bool {
        self.visited[id >> 6] >> (id & 63) & 1 != 0
    }

    #[inline]
    fn mark(&mut self, id: usize) {
        self.visited[id >> 6] |= 1 << (id & 63);
    }

    /// Labels of the four pixels around a lattice point: (nw, ne, sw, se).
    #[inline]
    fn corners(&self, x: i32, y: i32) -> (u32, u32, u32, u32) {
        let l = self.labels;
        (l.at(x - 1, y - 1), l.at(x, y - 1), l.at(x - 1, y), l.at(x, y))
    }

    #[inline]
    fn degree(&self, x: i32, y: i32) -> u32 {
        let (nw, ne, sw, se) = self.corners(x, y);
        (nw != ne) as u32 + (sw != se) as u32 + (nw != sw) as u32 + (ne != se) as u32
    }

    fn is_anchor(&self, x: i32, y: i32) -> bool {
        let deg = self.degree(x, y);
        let corner = (x == 0 || x as usize == self.w) && (y == 0 || y as usize == self.h);
        deg >= 3 || (corner && deg > 0)
    }

    /// Existing edges at a vertex as (edge id, other endpoint).
    fn incident(&self, x: i32, y: i32) -> ([(usize, LatticePoint); 4], usize) {
        let (nw, ne, sw, se) = self.corners(x, y);
        let mut out = [(0, (0, 0)); 4];
        let mut n = 0;
        if ne != se {
            out[n] = (self.h_id(x, y), (x + 1, y));
            n += 1;
        }
        if nw != sw {
            out[n] = (self.h_id(x - 1, y), (x - 1, y));
            n += 1;
        }
        if sw != se {
            out[n] = (self.v_id(x, y), (x, y + 1));
            n += 1;
        }
        if nw != ne {
            out[n] = (self.v_id(x, y - 1), (x, y - 1));
            n += 1;
        }
        (out, n)
    }

    /// (left, right) labels when walking the edge `u -> v`.
    fn left_right(&self, u: LatticePoint, v: LatticePoint) -> (u32, u32) {
        let l = self.labels;
        if u.1 == v.1 {
            let x = u.0.min(v.0);
            let (above, below) = (l.at(x, u.1 - 1), l.at(x, u.1));
            if v.0 > u.0 { (above, below) } else { (below, above) }
        } else {
            let y = u.1.min(v.1);
            let (west, east) = (l.at(u.0 - 1, y), l.at(u.0, y));
            if v.1 > u.1 { (east, west) } else { (west, east) }
        }
    }

    fn walk(&mut self, start: LatticePoint, first: (usize, LatticePoint)) -> RawChain {
        let anchored = self.is_anchor(start.0, start.1);
        let (left, right) = self.left_right(start, first.1);
        self.mark(first.0);
        let mut pts = vec![start, first.1];
        let mut prev = first.0;
        let mut cur = first.1;
        while cur != start && !self.is_anchor(cur.0, cur.1) {
            let (inc, n) = self.incident(cur.0, cur.1);
            let next = inc[..n]
                .iter()
                .copied()
                .find(|e| e.0 != prev)
                .expect("non-anchor vertex has degree 2");
            self.mark(next.0);
            pts.push(next.1);
            prev = next.0;
            cur = next.1;
        }
        RawChain { left, right, pts, closed: !anchored }
    }
}

/// Trace every border in the image, calling `emit` once per chain.
pub fn trace_chains(labels: &Labels, mut emit: impl FnMut(RawChain)) {
    let (w, h) = (labels.width, labels.height);
    let nh = (h + 1) * w;
    let nv = (w + 1) * h;
    let mut t = Tracer { labels, w, h, nh, visited: vec![0; (nh + nv).div_ceil(64)] };

    // Chains that touch an anchor.
    for y in 0..=h as i32 {
        for x in 0..=w as i32 {
            if !t.is_anchor(x, y) {
                continue;
            }
            let (inc, n) = t.incident(x, y);
            for &edge in &inc[..n] {
                if !t.seen(edge.0) {
                    let chain = t.walk((x, y), edge);
                    emit(chain);
                }
            }
        }
    }

    // Whatever is left are anchor-free loops (islands, whole-image borders of
    // a single province, ...).
    for y in 0..=h as i32 {
        for x in 0..w as i32 {
            let id = t.h_id(x, y);
            if !t.seen(id) && labels.at(x, y - 1) != labels.at(x, y) {
                let chain = t.walk((x, y), (id, (x + 1, y)));
                emit(chain);
            }
        }
    }
    for y in 0..h as i32 {
        for x in 0..=w as i32 {
            let id = t.v_id(x, y);
            if !t.seen(id) && labels.at(x - 1, y) != labels.at(x, y) {
                let chain = t.walk((x, y), (id, (x, y + 1)));
                emit(chain);
            }
        }
    }
}
