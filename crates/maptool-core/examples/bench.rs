//! Times vectorization of a synthetic Clausewitz-sized map.
//! `cargo run --release -p maptool-core --example bench -- [width height cell]`
use std::time::Instant;

use maptool_core::{Options, PixelFormat, mesh, vectorize};

fn hash(a: u64, b: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E3779B97F4A7C15) ^ b.wrapping_mul(0xC2B2AE3D27D4EB4F);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^ (x >> 32)
}

fn main() {
    let args: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let (w, h, cell) = (args.first().copied().unwrap_or(5632), args.get(1).copied().unwrap_or(2048), args.get(2).copied().unwrap_or(64));
    let (cw, ch) = (w.div_ceil(cell), h.div_ceil(cell));

    // One jittered seed per grid cell; each pixel only looks at the 3x3 cells around it.
    let seed = |cx: i64, cy: i64| -> (f64, f64) {
        let hsh = hash(cx as u64, cy as u64);
        ((cx as f64 + (hsh & 0xffff) as f64 / 65536.0) * cell as f64, (cy as f64 + (hsh >> 16 & 0xffff) as f64 / 65536.0) * cell as f64)
    };
    let t = Instant::now();
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = ((x / cell) as i64, (y / cell) as i64);
            let mut best = (f64::MAX, 0u32);
            for cy in gy - 1..=gy + 1 {
                for cx in gx - 1..=gx + 1 {
                    let (sx, sy) = seed(cx, cy);
                    let noise = (hash((x / 3) as u64 ^ (cx as u64) << 20, (y / 3) as u64 ^ (cy as u64) << 20) % 60) as f64;
                    let d = (x as f64 - sx).powi(2) + (y as f64 - sy).powi(2) + noise;
                    if d < best.0 {
                        best = (d, (cy.rem_euclid(ch as i64) as u32) * cw + cx.rem_euclid(cw as i64) as u32 + 1);
                    }
                }
            }
            px.extend_from_slice(&[(best.1 >> 16) as u8 | 1, (best.1 >> 8) as u8, best.1 as u8]);
        }
    }
    eprintln!("generated {w}x{h} in {:.2?}", t.elapsed());

    // The noise above leaves lone pixels on purpose, to stress the tracer, so the
    // timed runs skip validation. Validation is timed on its own below.
    let lax = |o: Options| Options { validate: false, ..o };
    for (name, o) in [("smooth", lax(Options::default())), ("exact", lax(Options::exact()))] {
        let t = Instant::now();
        let m = vectorize(&px, w, h, PixelFormat::Rgb, &o).unwrap();
        let took = t.elapsed();
        let bytes: usize = m.provinces.iter().map(|p| p.path.len()).sum();
        eprintln!("{name}: {} provinces, {:.1} MB of path data, {took:.2?}", m.provinces.len(), bytes as f64 / 1e6);
    }

    for tol in [0.05, 0.2] {
        let t = Instant::now();
        let m = mesh(&px, w, h, PixelFormat::Rgb, &lax(Options::default()), tol).unwrap();
        eprintln!(
            "mesh (flatten {tol}): {} provinces, {} vertices, {} triangles, {} border segments, {:.1} MB, {:.2?}",
            m.provinces.len(),
            m.vertex_province.len(),
            m.indices.len() / 3,
            m.line_indices.len() / 2,
            (m.positions.len() * 4 + m.vertex_province.len() * 4 + m.indices.len() * 4 + m.line_positions.len() * 4 + m.line_indices.len() * 4) as f64 / 1e6,
            t.elapsed()
        );
    }

    // Label map + validation scan only; this map is expected to be rejected.
    let t = Instant::now();
    match vectorize(&px, w, h, PixelFormat::Rgb, &Options::default()) {
        Err(maptool_core::Error::Invalid { total, .. }) => eprintln!("validate: rejected with {total} problems in {:.2?}", t.elapsed()),
        Err(e) => eprintln!("validate: {e}"),
        Ok(_) => eprintln!("validate: accepted (unexpected for this noisy map)"),
    }
}
