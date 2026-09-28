//! Turning a staircase of pixel-corner points into a short chain of curves.

use crate::Options;
use crate::trace::V;

pub type P = (f64, f64);

#[derive(Clone, Copy)]
pub struct Seg {
    pub c1: P,
    pub c2: P,
    pub to: P,
    /// Straight line; `c1`/`c2` are unused.
    pub line: bool,
}

#[derive(Clone)]
pub struct Curve {
    pub start: P,
    pub segs: Vec<Seg>,
}

impl Curve {
    /// Where the curve starts when walked forward or backward.
    pub fn begin(&self, reversed: bool) -> P {
        if reversed { self.segs.last().map_or(self.start, |s| s.to) } else { self.start }
    }

    /// Segments in walking order, each as (control 1, control 2, target, is_line).
    pub fn walk(&self, reversed: bool) -> impl Iterator<Item = (P, P, P, bool)> + '_ {
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

pub fn build_curve(pts: &[V], closed: bool, o: &Options) -> Curve {
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

fn p(v: V) -> P {
    (v.0 as f64, v.1 as f64)
}

fn lines(pts: &[V]) -> Curve {
    let segs = pts[1..]
        .iter()
        .map(|&v| Seg { c1: p(v), c2: p(v), to: p(v), line: true })
        .collect();
    Curve { start: p(pts[0]), segs }
}

/// Remove points that lie on a straight run, keeping both ends.
fn drop_collinear(pts: &[V]) -> Vec<V> {
    let mut out: Vec<V> = Vec::with_capacity(pts.len());
    for &v in pts {
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let cross = (b.0 - a.0) as i64 * (v.1 - b.1) as i64 - (b.1 - a.1) as i64 * (v.0 - b.0) as i64;
            if cross == 0 { out.pop(); } else { break; }
        }
        out.push(v);
    }
    out
}

fn dist_to_segment(q: V, a: V, b: V) -> f64 {
    let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
    let (px, py) = ((q.0 - a.0) as f64, (q.1 - a.1) as f64);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return (px * px + py * py).sqrt();
    }
    let t = ((px * dx + py * dy) / len2).clamp(0.0, 1.0);
    let (cx, cy) = (dx * t - px, dy * t - py);
    (cx * cx + cy * cy).sqrt()
}

/// Douglas-Peucker on an open polyline (iterative; chains can be very long).
fn douglas_peucker(pts: &[V], tol: f64) -> Vec<V> {
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
            let d = dist_to_segment(pts[i], pts[a], pts[b]);
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

fn simplify(pts: &[V], closed: bool, tol: f64) -> Vec<V> {
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

fn unit(a: P, b: P) -> P {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dy * dy).sqrt();
    (dx / l, dy / l)
}

/// A turn at least this sharp is a corner when both sides are long straight runs.
const RUN_CORNER_DEG: f64 = 75.0;

/// Fit cubic Béziers through the simplified points. Tangents come from the
/// bisector of the neighbouring segments, and control points sit a third of
/// the way along each segment so the curve cannot overshoot.
///
/// A vertex stays a corner when the turn is beyond `corner_deg`, or when it is
/// at least `RUN_CORNER_DEG` and both neighbouring segments are `corner_run`
/// pixels or longer. The second rule is what keeps squares square: after
/// simplification a square's 4 vertices look like a coarse circle, and only
/// the long straight sides tell them apart. Chain ends are always corners.
fn fit(simp: &[V], closed: bool, corner_deg: f64, corner_run: f64) -> Curve {
    let mut q: Vec<P> = simp.iter().map(|&v| p(v)).collect();
    if closed {
        q.pop();
    }
    let cnt = q.len();
    let prev = |i: usize| (i + cnt - 1) % cnt;
    let next = |i: usize| (i + 1) % cnt;

    // (tangent in, tangent out, smooth) per vertex.
    let tang: Vec<(P, P, bool)> = (0..cnt)
        .map(|i| {
            let has_prev = closed || i > 0;
            let has_next = closed || i + 1 < cnt;
            match (has_prev, has_next) {
                (true, true) => {
                    let a = unit(q[prev(i)], q[i]);
                    let b = unit(q[i], q[next(i)]);
                    let turn = (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0).acos().to_degrees();
                    let shortest = dist(q[prev(i)], q[i]).min(dist(q[i], q[next(i)]));
                    if turn > corner_deg || (turn >= RUN_CORNER_DEG && shortest >= corner_run) {
                        return (a, b, false);
                    }
                    let m = (a.0 + b.0, a.1 + b.1);
                    let l = (m.0 * m.0 + m.1 * m.1).sqrt();
                    let t = if l < 1e-9 { a } else { (m.0 / l, m.1 / l) };
                    (t, t, true)
                }
                (false, true) => {
                    let b = unit(q[i], q[next(i)]);
                    (b, b, false)
                }
                (true, false) => {
                    let a = unit(q[prev(i)], q[i]);
                    (a, a, false)
                }
                (false, false) => ((0.0, 0.0), (0.0, 0.0), false),
            }
        })
        .collect();

    let nseg = if closed { cnt } else { cnt - 1 };
    let segs = (0..nseg)
        .map(|i| {
            let j = next(i);
            let (a, b) = (q[i], q[j]);
            if !tang[i].2 && !tang[j].2 {
                return Seg { c1: b, c2: b, to: b, line: true };
            }
            let (to, ti) = (tang[i].1, tang[j].0);
            // Tangents along the chord: a curve that is really a straight line.
            let chord = unit(a, b);
            let along = |t: P| (t.0 * chord.1 - t.1 * chord.0).abs() < 1e-9 && t.0 * chord.0 + t.1 * chord.1 > 0.0;
            if along(to) && along(ti) {
                return Seg { c1: b, c2: b, to: b, line: true };
            }
            let l = dist(a, b) / 3.0;
            Seg {
                c1: (a.0 + to.0 * l, a.1 + to.1 * l),
                c2: (b.0 - ti.0 * l, b.1 - ti.1 * l),
                to: b,
                line: false,
            }
        })
        .collect();
    Curve { start: q[0], segs: merge_lines(q[0], segs) }
}

fn dist(a: P, b: P) -> f64 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// Join consecutive straight segments that point the same way.
fn merge_lines(start: P, segs: Vec<Seg>) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    let mut from = start;
    let mut last_from = start;
    for s in segs {
        if let Some(last) = out.last_mut() {
            if last.line && s.line {
                let (d1, d2) = ((last.to.0 - last_from.0, last.to.1 - last_from.1), (s.to.0 - from.0, s.to.1 - from.1));
                let cross = d1.0 * d2.1 - d1.1 * d2.0;
                let dot = d1.0 * d2.0 + d1.1 * d2.1;
                if cross.abs() <= 1e-9 * (dist((0.0, 0.0), d1) * dist((0.0, 0.0), d2)) && dot > 0.0 {
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
