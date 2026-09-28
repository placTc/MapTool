//! Linking shared border chains into per-province rings, then writing them as SVG
//! path data or as flattened polygons.

use std::fmt::Write;

use crate::geom::{self, CurvePoint, LatticePoint};
use crate::label::{Labels, NONE};
use crate::smooth::Curve;
use crate::{Options, smooth, trace};

struct Chain {
    left: u32,
    right: u32,
    start: LatticePoint,
    end: LatticePoint,
    /// Direction of the first / last crack edge, walking forward.
    first_dir: LatticePoint,
    last_dir: LatticePoint,
    curve: Curve,
}

#[derive(Clone, Copy)]
struct Arc {
    chain: u32,
    rev: bool,
}

struct ArcInfo {
    start: LatticePoint,
    end: LatticePoint,
    /// Direction leaving the start / arriving at the end, in walking order.
    out_dir: LatticePoint,
    in_dir: LatticePoint,
}

fn info(c: &Chain, rev: bool) -> ArcInfo {
    let neg = |d: LatticePoint| (-d.0, -d.1);
    if rev {
        ArcInfo { start: c.end, end: c.start, out_dir: neg(c.last_dir), in_dir: neg(c.first_dir) }
    } else {
        ArcInfo { start: c.start, end: c.end, out_dir: c.first_dir, in_dir: c.last_dir }
    }
}

/// Signed turn from `d_in` to `d_out`: larger means a sharper left turn
/// (screen coordinates, y down).
fn turn(d_in: LatticePoint, d_out: LatticePoint) -> f64 {
    let (in_f, out_f) = (geom::to_curve_point(d_in), geom::to_curve_point(d_out));
    geom::cross2(out_f, in_f).atan2(geom::dot2(out_f, in_f))
}

/// The smoothed borders of a whole map: chains shared by neighbouring provinces,
/// and for every province the arcs (chain plus direction) that bound it.
pub struct Geometry {
    chains: Vec<Chain>,
    arcs: Vec<Vec<Arc>>,
}

pub fn geometry(labels: &Labels, opts: &Options) -> Geometry {
    let mut chains: Vec<Chain> = Vec::new();
    trace::trace_chains(labels, |raw| {
        let n = raw.pts.len();
        let curve = smooth::build_curve(&raw.pts, raw.closed, opts);
        chains.push(Chain {
            left: raw.left,
            right: raw.right,
            start: raw.pts[0],
            end: raw.pts[n - 1],
            first_dir: (raw.pts[1].0 - raw.pts[0].0, raw.pts[1].1 - raw.pts[0].1),
            last_dir: (raw.pts[n - 1].0 - raw.pts[n - 2].0, raw.pts[n - 1].1 - raw.pts[n - 2].1),
            curve,
        });
    });

    // Every chain is used by both neighbours, walked in opposite directions.
    let mut arcs: Vec<Vec<Arc>> = vec![Vec::new(); labels.colors.len()];
    for (i, c) in chains.iter().enumerate() {
        if c.left != NONE {
            arcs[c.left as usize].push(Arc { chain: i as u32, rev: false });
        }
        if c.right != NONE {
            arcs[c.right as usize].push(Arc { chain: i as u32, rev: true });
        }
    }
    Geometry { chains, arcs }
}

impl Geometry {
    /// SVG path data of every province, indexed by province id.
    pub fn paths(&self, precision: usize) -> Vec<String> {
        self.arcs.iter().map(|list| self.path(list, precision)).collect()
    }

    /// Flattened rings of every province: `[province][ring][point]`. A ring is
    /// implicitly closed (its last point is not repeated). Rings run with the
    /// province on the left, so holes have the opposite winding to outer rings.
    ///
    /// Each border is flattened once, in its forward direction, and the
    /// neighbour on the other side reuses the same points reversed, so shared
    /// borders match bit for bit and the two provinces leave no cracks.
    pub fn rings(&self, tolerance: f64) -> Vec<Vec<Vec<[f32; 2]>>> {
        let flats: Vec<Vec<CurvePoint>> = self.chains.iter().map(|c| flatten(&c.curve, tolerance)).collect();
        self.arcs
            .iter()
            .map(|list| {
                link_rings(list, &self.chains)
                    .into_iter()
                    .map(|ring| {
                        let mut pts: Vec<[f32; 2]> = Vec::new();
                        for arc in ring {
                            let f = &flats[arc.chain as usize];
                            // Skip each arc's end point: it is the next arc's start.
                            if arc.rev {
                                pts.extend(f.iter().rev().take(f.len() - 1).map(|p| [p.0 as f32, p.1 as f32]));
                            } else {
                                pts.extend(f[..f.len() - 1].iter().map(|p| [p.0 as f32, p.1 as f32]));
                            }
                        }
                        pts
                    })
                    .filter(|ring| ring.len() >= 3)
                    .collect()
            })
            .collect()
    }

    fn path(&self, list: &[Arc], prec: usize) -> String {
        let mut out = String::new();
        for ring in link_rings(list, &self.chains) {
            let first = ring[0];
            push_move(&mut out, self.chains[first.chain as usize].curve.begin(first.rev), prec);
            for arc in ring {
                write_arc(&mut out, &self.chains[arc.chain as usize].curve, arc.rev, prec);
            }
            out.push('Z');
        }
        out
    }
}

/// Returns the SVG path data of every province, indexed by province id.
pub fn build_paths(labels: &Labels, opts: &Options) -> Vec<String> {
    geometry(labels, opts).paths(opts.precision)
}

/// Join a province's arcs end to start into closed rings.
fn link_rings(list: &[Arc], chains: &[Chain]) -> Vec<Vec<Arc>> {
    let infos: Vec<ArcInfo> = list.iter().map(|a| info(&chains[a.chain as usize], a.rev)).collect();
    let mut by_start: Vec<(LatticePoint, usize)> = infos.iter().enumerate().map(|(i, a)| (a.start, i)).collect();
    by_start.sort_unstable();
    let mut used = vec![false; list.len()];
    let mut rings = Vec::new();

    for first in 0..list.len() {
        if used[first] {
            continue;
        }
        let mut ring = Vec::new();
        let mut cur = first;
        loop {
            used[cur] = true;
            ring.push(list[cur]);

            // Continue with the arc leaving this vertex that turns left the
            // most; that keeps pieces touching only at a point separate.
            let end = infos[cur].end;
            let in_dir = infos[cur].in_dir;
            let lo = by_start.partition_point(|&(v, _)| v < end);
            let next = by_start[lo..]
                .iter()
                .take_while(|&&(v, _)| v == end)
                .map(|&(_, j)| j)
                .filter(|&j| !used[j] || j == first)
                .max_by(|&x, &y| turn(in_dir, infos[x].out_dir).total_cmp(&turn(in_dir, infos[y].out_dir)));
            match next {
                Some(j) if j != first => cur = j,
                _ => break,
            }
        }
        rings.push(ring);
    }
    rings
}

/// Points along a curve such that no point of the true curve is farther than
/// `tol` from the polyline. Includes the start and end points exactly.
fn flatten(curve: &Curve, tol: f64) -> Vec<CurvePoint> {
    let mut out = vec![curve.start];
    let mut from = curve.start;
    for s in &curve.segs {
        if s.line {
            out.push(s.to);
        } else {
            // Wang's bound for a cubic: n = sqrt(3/4 * max second difference / tol).
            let d1 = (from.0 - 2.0 * s.c1.0 + s.c2.0, from.1 - 2.0 * s.c1.1 + s.c2.1);
            let d2 = (s.c1.0 - 2.0 * s.c2.0 + s.to.0, s.c1.1 - 2.0 * s.c2.1 + s.to.1);
            let m = d1.0.hypot(d1.1).max(d2.0.hypot(d2.1));
            let n = ((0.75 * m / tol.max(1e-6)).sqrt().ceil() as usize).clamp(1, 256);
            for i in 1..n {
                out.push(cubic(from, s.c1, s.c2, s.to, i as f64 / n as f64));
            }
            out.push(s.to);
        }
        from = s.to;
    }
    out
}

fn cubic(p0: CurvePoint, p1: CurvePoint, p2: CurvePoint, p3: CurvePoint, t: f64) -> CurvePoint {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
}

fn push_num(s: &mut String, v: f64, prec: usize) {
    let mark = s.len();
    let _ = write!(s, "{v:.prec$}");
    if s[mark..].contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.').len();
        s.truncate(trimmed);
    }
    if &s[mark..] == "-0" {
        s.truncate(mark);
        s.push('0');
    }
}

fn push_pt(s: &mut String, pt: CurvePoint, prec: usize) {
    push_num(s, pt.0, prec);
    s.push(' ');
    push_num(s, pt.1, prec);
}

fn push_move(s: &mut String, pt: CurvePoint, prec: usize) {
    s.push('M');
    push_pt(s, pt, prec);
}

fn write_arc(s: &mut String, curve: &Curve, rev: bool, prec: usize) {
    for (c1, c2, to, line) in curve.walk(rev) {
        if line {
            s.push('L');
        } else {
            s.push('C');
            push_pt(s, c1, prec);
            s.push(' ');
            push_pt(s, c2, prec);
            s.push(' ');
        }
        push_pt(s, to, prec);
    }
}
