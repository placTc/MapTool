//! Turning a staircase of pixel-corner points into a short chain of curves.

use crate::Options;
use crate::geom::{self, CurvePoint, LatticePoint};

#[derive(Clone, Copy)]
pub struct Seg {
    pub c1: CurvePoint,
    pub c2: CurvePoint,
    pub to: CurvePoint,
    /// Straight line; `c1`/`c2` are unused.
    pub line: bool,
}

#[derive(Clone)]
pub struct Curve {
    pub start: CurvePoint,
    pub segs: Vec<Seg>,
}

impl Curve {
    /// Where the curve starts when walked forward or backward.
    pub fn begin(&self, reversed: bool) -> CurvePoint {
        if reversed { self.segs.last().map_or(self.start, |s| s.to) } else { self.start }
    }

    /// Segments in walking order, each as (control 1, control 2, target, is_line).
    pub fn walk(&self, reversed: bool) -> impl Iterator<Item = (CurvePoint, CurvePoint, CurvePoint, bool)> + '_ {
        let n = self.segs.len();
        (0..n).map(move |k| {
            if reversed {
                let i = n - 1 - k;
                let target = if i == 0 { self.start } else { self.segs[i - 1].to };
                let s = &self.segs[i];
                (s.c2, s.c1, target, s.line)
            } else {
                let s = &self.segs[k];
                (s.c1, s.c2, s.to, s.line)
            }
        })
    }
}

pub fn build_curve(pts: &[LatticePoint], closed: bool, o: &Options) -> Curve {
    let n_edges = pts.len() - 1;
    let mut simp = None;
    if o.tolerance > 0.0 && n_edges >= o.min_chain_len {
        let s = simplify(pts, closed, o.tolerance);
        // A loop needs at least a triangle to keep any area.
        if !(closed && s.len() < 4) {
            simp = Some(s);
        }
    }
    match simp {
        Some(s) => fit(&s, closed, o.corner_angle, o.corner_run),
        None => lines(&drop_collinear(pts)),
    }
}

fn lines(pts: &[LatticePoint]) -> Curve {
    let segs = pts[1..]
        .iter()
        .map(|&v| {
            let p = geom::to_curve_point(v);
            Seg { c1: p, c2: p, to: p, line: true }
        })
        .collect();
    Curve { start: geom::to_curve_point(pts[0]), segs }
}

/// Remove points that lie on a straight run, keeping both ends.
fn drop_collinear(pts: &[LatticePoint]) -> Vec<LatticePoint> {
    let mut out: Vec<LatticePoint> = Vec::with_capacity(pts.len());
    for &v in pts {
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            if geom::signed_area2(a, b, v) == 0 { out.pop(); } else { break; }
        }
        out.push(v);
    }
    out
}

/// Douglas-Peucker on an open polyline (iterative; chains can be very long).
fn douglas_peucker(pts: &[LatticePoint], tol: f64) -> Vec<LatticePoint> {
    let n = pts.len();
    if n <= 2 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut far = (0.0, a);
        for i in a + 1..b {
            let d = geom::distance_to_segment(
                geom::to_curve_point(pts[i]),
                geom::to_curve_point(pts[a]),
                geom::to_curve_point(pts[b]),
            );
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > tol {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    pts.iter().zip(keep).filter(|&(_, k)| k).map(|(&v, _)| v).collect()
}

fn simplify(pts: &[LatticePoint], closed: bool, tol: f64) -> Vec<LatticePoint> {
    if !closed {
        return douglas_peucker(pts, tol);
    }
    // Split the loop at the point farthest from its start so both halves are open.
    let s = pts[0];
    let far = (1..pts.len() - 1)
        .max_by_key(|&i| {
            let (dx, dy) = ((pts[i].0 - s.0) as i64, (pts[i].1 - s.1) as i64);
            dx * dx + dy * dy
        })
        .unwrap_or(0);
    if far == 0 {
        return pts.to_vec();
    }
    let mut out = douglas_peucker(&pts[..=far], tol);
    out.extend_from_slice(&douglas_peucker(&pts[far..], tol)[1..]);
    out
}

/// A turn at least this sharp is a corner when both sides are long straight runs.
const RUN_CORNER_DEG: f64 = 75.0;

/// (tangent in, tangent out, smooth) for one vertex of the fitted curve.
type VertexTangent = (CurvePoint, CurvePoint, bool);

/// Tangent in/out and corner-vs-smooth decision for every vertex of `points`.
///
/// A vertex stays a corner when the turn is beyond `corner_deg`, or when it is
/// at least `RUN_CORNER_DEG` and both neighbouring segments are `corner_run`
/// pixels or longer. The second rule is what keeps squares square: after
/// simplification a square's 4 vertices look like a coarse circle, and only
/// the long straight sides tell them apart. Chain ends are always corners.
fn vertex_tangents(points: &[CurvePoint], closed: bool, corner_deg: f64, corner_run: f64) -> Vec<VertexTangent> {
    let count = points.len();
    let prev = |i: usize| (i + count - 1) % count;
    let next = |i: usize| (i + 1) % count;

    (0..count)
        .map(|i| {
            let has_prev = closed || i > 0;
            let has_next = closed || i + 1 < count;
            match (has_prev, has_next) {
                (true, true) => {
                    let a = geom::direction(points[prev(i)], points[i]);
                    let b = geom::direction(points[i], points[next(i)]);
                    let turn = (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos().to_degrees();
                    let shortest =
                        geom::distance(points[prev(i)], points[i]).min(geom::distance(points[i], points[next(i)]));
                    if turn > corner_deg || (turn >= RUN_CORNER_DEG && shortest >= corner_run) {
                        return (a, b, false);
                    }
                    let m = (a.0 + b.0, a.1 + b.1);
                    let l = (m.0 * m.0 + m.1 * m.1).sqrt();
                    let t = if l < 1e-9 { a } else { (m.0 / l, m.1 / l) };
                    (t, t, true)
                }
                (false, true) => {
                    let b = geom::direction(points[i], points[next(i)]);
                    (b, b, false)
                }
                (true, false) => {
                    let a = geom::direction(points[prev(i)], points[i]);
                    (a, a, false)
                }
                (false, false) => ((0.0, 0.0), (0.0, 0.0), false),
            }
        })
        .collect()
}

/// Build one cubic-Bézier segment per edge of `points`, from each endpoint's
/// tangent. Control points sit a third of the way along each segment so the
/// curve cannot overshoot; an edge whose tangents run along its own chord
/// collapses to a straight line instead of an unnecessary curve.
fn build_segments(points: &[CurvePoint], closed: bool, tangents: &[VertexTangent]) -> Vec<Seg> {
    let count = points.len();
    let next = |i: usize| (i + 1) % count;
    let segment_count = if closed { count } else { count - 1 };

    (0..segment_count)
        .map(|i| {
            let j = next(i);
            let (a, b) = (points[i], points[j]);
            if !tangents[i].2 && !tangents[j].2 {
                return Seg { c1: b, c2: b, to: b, line: true };
            }
            let (tangent_out, tangent_in) = (tangents[i].1, tangents[j].0);
            // Tangents along the chord: a curve that is really a straight line.
            let chord = geom::direction(a, b);
            let along = |t: CurvePoint| geom::cross2(t, chord).abs() < 1e-9 && geom::dot2(t, chord) > 0.0;
            if along(tangent_out) && along(tangent_in) {
                return Seg { c1: b, c2: b, to: b, line: true };
            }
            let l = geom::distance(a, b) / 3.0;
            Seg {
                c1: (a.0 + tangent_out.0 * l, a.1 + tangent_out.1 * l),
                c2: (b.0 - tangent_in.0 * l, b.1 - tangent_in.1 * l),
                to: b,
                line: false,
            }
        })
        .collect()
}

/// Fit cubic Béziers through the simplified points. Tangents come from the
/// bisector of the neighbouring segments (see `vertex_tangents`).
fn fit(simp: &[LatticePoint], closed: bool, corner_deg: f64, corner_run: f64) -> Curve {
    let mut points: Vec<CurvePoint> = simp.iter().map(|&v| geom::to_curve_point(v)).collect();
    if closed {
        points.pop();
    }
    let tangents = vertex_tangents(&points, closed, corner_deg, corner_run);
    let segs = build_segments(&points, closed, &tangents);
    Curve { start: points[0], segs: merge_lines(points[0], segs) }
}

/// Join consecutive straight segments that point the same way.
fn merge_lines(start: CurvePoint, segs: Vec<Seg>) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    let mut from = start;
    let mut last_from = start;
    for s in segs {
        if let Some(last) = out.last_mut() {
            if last.line && s.line {
                let (d1, d2) = ((last.to.0 - last_from.0, last.to.1 - last_from.1), (s.to.0 - from.0, s.to.1 - from.1));
                let cross = geom::cross2(d1, d2);
                let dot = geom::dot2(d1, d2);
                if cross.abs() <= 1e-9 * (geom::distance((0.0, 0.0), d1) * geom::distance((0.0, 0.0), d2)) && dot > 0.0
                {
                    last.to = s.to;
                    from = s.to;
                    continue;
                }
            }
        }
        last_from = from;
        from = s.to;
        out.push(s);
    }
    out
}
