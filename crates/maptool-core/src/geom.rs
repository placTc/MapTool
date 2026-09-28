//! Shared 2D point/vector types and arithmetic for the tracing, smoothing and
//! ring-building stages, which all work on the same plane but at different
//! precisions.

/// A pixel-corner lattice coordinate (`trace`'s vertices, `smooth`'s input).
pub type LatticePoint = (i32, i32);

/// A coordinate once curves are fitted (`smooth`'s output, `build`'s flattened rings).
pub type CurvePoint = (f64, f64);

pub fn to_curve_point(v: LatticePoint) -> CurvePoint {
    (v.0 as f64, v.1 as f64)
}

/// Twice the signed area of triangle a-b-c; exactly zero when the three lattice
/// points are collinear. Kept as exact i64 arithmetic, not routed through
/// `CurvePoint`/f64: `drop_collinear` relies on an exact zero test, and this feeds
/// directly into the shared border vertices neighbours must match bit-for-bit.
pub fn signed_area2(a: LatticePoint, b: LatticePoint, c: LatticePoint) -> i64 {
    (b.0 - a.0) as i64 * (c.1 - b.1) as i64 - (b.1 - a.1) as i64 * (c.0 - b.0) as i64
}

pub fn cross2(a: CurvePoint, b: CurvePoint) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

pub fn dot2(a: CurvePoint, b: CurvePoint) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

pub fn distance(a: CurvePoint, b: CurvePoint) -> f64 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

pub fn distance_to_segment(q: CurvePoint, a: CurvePoint, b: CurvePoint) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (px, py) = (q.0 - a.0, q.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return (px * px + py * py).sqrt();
    }
    let t = ((px * dx + py * dy) / len2).clamp(0.0, 1.0);
    let (cx, cy) = (dx * t - px, dy * t - py);
    (cx * cx + cy * cy).sqrt()
}

/// Unit vector from `a` to `b`.
pub fn direction(a: CurvePoint, b: CurvePoint) -> CurvePoint {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    (dx / len, dy / len)
}
