use maptool_core::{Error, Options, PixelFormat, VectorMap, ViolationKind, vectorize};

type P = (f64, f64);

/// Split path data into rings of points; Béziers are sampled.
fn rings(d: &str) -> Vec<Vec<P>> {
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut cmd = ' ';
    let mut nums: Vec<f64> = Vec::new();
    let mut tok = String::new();
    let mut items: Vec<(char, Vec<f64>)> = Vec::new();
    let flush = |tok: &mut String, nums: &mut Vec<f64>| {
        if !tok.is_empty() {
            nums.push(tok.parse().unwrap());
            tok.clear();
        }
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush(&mut tok, &mut nums);
            if cmd != ' ' {
                items.push((cmd, std::mem::take(&mut nums)));
            }
            cmd = ch;
        } else if ch == ' ' {
            flush(&mut tok, &mut nums);
        } else if ch == '-' && !tok.is_empty() {
            flush(&mut tok, &mut nums);
            tok.push(ch);
        } else {
            tok.push(ch);
        }
    }
    flush(&mut tok, &mut nums);
    if cmd != ' ' {
        items.push((cmd, nums));
    }

    let mut cur: P = (0.0, 0.0);
    for (c, n) in items {
        match c {
            'M' => {
                cur = (n[0], n[1]);
                out.push(vec![cur]);
            }
            'L' => {
                cur = (n[0], n[1]);
                out.last_mut().unwrap().push(cur);
            }
            'C' => {
                let (p0, p1, p2, p3) = (cur, (n[0], n[1]), (n[2], n[3]), (n[4], n[5]));
                for k in 1..=16 {
                    let t = k as f64 / 16.0;
                    let u = 1.0 - t;
                    let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
                    let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
                    out.last_mut().unwrap().push((x, y));
                }
                cur = p3;
            }
            'Z' => {
                let ring = out.last().unwrap();
                assert!(
                    (ring[0].0 - cur.0).abs() < 1e-6 && (ring[0].1 - cur.1).abs() < 1e-6,
                    "ring does not return to its start"
                );
            }
            _ => panic!("unexpected command {c}"),
        }
    }
    out
}

fn signed_area(r: &[P]) -> f64 {
    let n = r.len();
    (0..n).map(|i| r[i].0 * r[(i + 1) % n].1 - r[(i + 1) % n].0 * r[i].1).sum::<f64>() / 2.0
}

/// Rings run with the province on the left, so holes cancel out of the sum.
fn area(d: &str) -> f64 {
    rings(d).iter().map(|r| signed_area(r)).sum::<f64>().abs()
}

fn rgb(colors: &[u32], w: u32, h: u32) -> Vec<u8> {
    assert_eq!(colors.len(), (w * h) as usize);
    colors.iter().flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect()
}

/// Runs without input validation, so the noisy test images (full of lone pixels
/// and four-way junctions) still exercise the tracer.
fn run(colors: &[u32], w: u32, h: u32, o: &Options) -> VectorMap {
    let lax = Options { validate: false, ..o.clone() };
    vectorize(&rgb(colors, w, h), w, h, PixelFormat::Rgb, &lax).unwrap()
}

fn checked(colors: &[u32], w: u32, h: u32) -> Result<VectorMap, Error> {
    vectorize(&rgb(colors, w, h), w, h, PixelFormat::Rgb, &Options::default())
}

fn assert_exact_areas(m: &VectorMap) {
    let mut total = 0.0;
    for p in &m.provinces {
        let a = area(&p.path);
        assert!((a - p.pixel_count as f64).abs() < 1e-6, "province {} area {a} != {}", p.id, p.pixel_count);
        total += a;
    }
    assert!((total - (m.width * m.height) as f64).abs() < 1e-6);
}

const RED: u32 = 0xff0000;
const BLUE: u32 = 0x0000ff;
const GREEN: u32 = 0x00ff00;

#[test]
fn two_halves() {
    let m = run(&[RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE], 4, 2, &Options::exact());
    assert_eq!(m.provinces.len(), 2);
    // A plain rectangle: four corners, whatever the start vertex and winding.
    assert_eq!(m.provinces[0].path.matches('L').count(), 4);
    assert_exact_areas(&m);
}

#[test]
fn single_color_image() {
    let m = run(&[RED; 12], 4, 3, &Options::exact());
    assert_eq!(m.provinces.len(), 1);
    assert_eq!(rings(&m.provinces[0].path).len(), 1);
    assert_exact_areas(&m);
}

#[test]
fn enclave_is_a_hole() {
    let mut px = vec![RED; 81];
    for y in 3..6 {
        for x in 3..6 {
            px[y * 9 + x] = BLUE;
        }
    }
    let m = run(&px, 9, 9, &Options::exact());
    assert_eq!(rings(&m.provinces[0].path).len(), 2);
    assert_eq!(rings(&m.provinces[1].path).len(), 1);
    assert_exact_areas(&m);
}

#[test]
fn exclave_is_a_second_subpath() {
    let mut px = vec![BLUE; 100];
    for &(x0, y0) in &[(1, 1), (6, 6)] {
        for y in y0..y0 + 3 {
            for x in x0..x0 + 3 {
                px[y * 10 + x] = RED;
            }
        }
    }
    let m = run(&px, 10, 10, &Options::exact());
    let red = m.provinces.iter().find(|p| p.color == [255, 0, 0]).unwrap();
    assert_eq!(rings(&red.path).len(), 2);
    assert_eq!(red.pixel_count, 18);
    assert_exact_areas(&m);
}

#[test]
fn checkerboard_pinch_points() {
    let m = run(&[RED, BLUE, BLUE, RED], 2, 2, &Options::exact());
    assert_exact_areas(&m);
}

#[test]
fn four_way_junction_is_traced_when_validation_is_off() {
    let m = run(&[RED, BLUE, GREEN, 0xffffff], 2, 2, &Options::exact());
    assert_eq!(m.provinces.len(), 4);
    assert_exact_areas(&m);
}

#[test]
fn four_way_junction_is_rejected() {
    // Big enough that every province is a block; only the shared corner is the problem.
    let mut px = vec![RED; 36];
    for y in 0..6 {
        for x in 0..6 {
            px[y * 6 + x] = match (x < 3, y < 3) {
                (true, true) => RED,
                (false, true) => BLUE,
                (true, false) => GREEN,
                (false, false) => 0xffffff,
            };
        }
    }
    match checked(&px, 6, 6) {
        Err(Error::Invalid { violations, total }) => {
            assert_eq!(total, 1);
            assert_eq!((violations[0].kind, violations[0].x, violations[0].y), (ViolationKind::FourWayJunction, 3, 3));
        }
        other => panic!("expected rejection, got {:?}", other.map(|m| m.provinces.len())),
    }
}

#[test]
fn three_way_and_same_color_diagonals_at_a_corner_are_fine() {
    // Three colors meeting (T junction) is the normal case.
    let px = [RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE, GREEN, GREEN, GREEN, GREEN];
    assert!(checked(&px, 4, 3).is_ok());
}

#[test]
fn single_pixel_exclave_is_rejected() {
    let mut px = vec![BLUE; 100];
    px[11] = RED; // lone red pixel at (1, 1)
    for y in 6..9 {
        for x in 6..9 {
            px[y * 10 + x] = RED;
        }
    }
    match checked(&px, 10, 10) {
        Err(Error::Invalid { violations, total }) => {
            assert_eq!(total, 1);
            assert_eq!((violations[0].kind, violations[0].x, violations[0].y), (ViolationKind::SinglePixelExclave, 1, 1));
        }
        other => panic!("expected rejection, got {:?}", other.map(|m| m.provinces.len())),
    }
}

#[test]
fn diagonal_only_contact_counts_as_an_exclave() {
    // Same-colored pixels that touch only at a corner are separate components,
    // so both colors here have two lone pixels.
    assert!(matches!(checked(&[RED, BLUE, BLUE, RED], 2, 2), Err(Error::Invalid { total: 4, .. })));
}

#[test]
fn a_province_that_is_one_pixel_is_not_an_exclave() {
    let mut px = vec![BLUE; 9];
    px[4] = RED;
    assert!(checked(&px, 3, 3).is_ok());
}

#[test]
fn multi_pixel_exclaves_are_fine() {
    let mut px = vec![BLUE; 100];
    for &(x0, y0) in &[(1, 1), (6, 6)] {
        for y in y0..y0 + 2 {
            for x in x0..x0 + 2 {
                px[y * 10 + x] = RED;
            }
        }
    }
    assert!(checked(&px, 10, 10).is_ok());
}

#[test]
fn error_message_lists_the_problems() {
    let e = checked(&[RED, BLUE, BLUE, RED], 2, 2).unwrap_err().to_string();
    assert!(e.contains("4 problem(s)") && e.contains("single-pixel exclave at pixel (0, 0)"), "{e}");
}

/// A filled square (or disc) of `inner` on a `RED` background.
fn shape(size: u32, inner: u32, inside: impl Fn(f64, f64) -> bool) -> Vec<u32> {
    (0..size * size)
        .map(|i| if inside((i % size) as f64 + 0.5, (i / size) as f64 + 0.5) { inner } else { RED })
        .collect()
}

#[test]
fn square_stays_square_when_smoothed() {
    let px = shape(40, BLUE, |x, y| (14.0..26.0).contains(&x) && (14.0..26.0).contains(&y));
    let m = checked(&px, 40, 40).unwrap();
    let blue = &m.provinces[1];
    assert!(!blue.path.contains('C'), "square was curved: {}", blue.path);
    assert!(blue.path.matches('L').count() <= 5, "too many vertices: {}", blue.path);
    assert!((area(&blue.path) - 144.0).abs() < 1e-6);
}

#[test]
fn rectangle_stays_rectangular_when_smoothed() {
    let px = shape(50, BLUE, |x, y| (10.0..40.0).contains(&x) && (20.0..27.0).contains(&y));
    let blue = &checked(&px, 50, 50).unwrap().provinces[1];
    assert!(!blue.path.contains('C'), "rectangle was curved: {}", blue.path);
    assert!((area(&blue.path) - 210.0).abs() < 1e-6);
}

#[test]
fn disc_stays_round_when_smoothed() {
    let px = shape(60, BLUE, |x, y| (x - 30.0).powi(2) + (y - 30.0).powi(2) < 20.0 * 20.0);
    let m = checked(&px, 60, 60).unwrap();
    let blue = &m.provinces[1];
    assert!(blue.path.matches('C').count() >= 4, "disc has no curves: {}", blue.path);
    assert!(blue.path.matches('L').count() <= 2, "disc has corners: {}", blue.path);
    // The curve runs through the outer corners of the pixel staircase, so a
    // small disc grows by up to about half a pixel (4% of its area at r=20).
    let pixels = blue.pixel_count as f64;
    assert!((area(&blue.path) - pixels).abs() < pixels * 0.06);
}

#[test]
fn transparent_pixels_are_no_province() {
    let mut px = Vec::new();
    for i in 0..6 {
        px.extend_from_slice(if i % 3 == 0 { &[0, 0, 0, 0] } else { &[255, 0, 0, 255] });
    }
    let m = vectorize(&px, 3, 2, PixelFormat::Rgba, &Options::exact()).unwrap();
    assert_eq!(m.provinces.len(), 1);
    assert_eq!(m.provinces[0].pixel_count, 4);
    assert_exact_areas_partial(&m, 4.0);
}

fn assert_exact_areas_partial(m: &VectorMap, expected_total: f64) {
    let total: f64 = m.provinces.iter().map(|p| area(&p.path)).sum();
    assert!((total - expected_total).abs() < 1e-6, "{total} != {expected_total}");
}

fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state >> 33
}

#[test]
fn random_noise_keeps_exact_areas() {
    for seed in 1..40u64 {
        let mut s = seed;
        let (w, h) = (3 + (seed % 17) as u32, 3 + (seed % 11) as u32);
        let n = 2 + (seed % 4) as u64;
        let px: Vec<u32> = (0..w * h).map(|_| 0x101010 * (lcg(&mut s) % n) as u32 + 0x0a0a0a).collect();
        assert_exact_areas(&run(&px, w, h, &Options::exact()));
    }
}

/// Jagged Voronoi-like map: organic borders, many enclaves and pinch points.
fn blobby(w: u32, h: u32, seeds: u32, rng: u64, jitter: u64) -> Vec<u32> {
    let mut s = rng;
    let pts: Vec<(f64, f64)> = (0..seeds)
        .map(|_| ((lcg(&mut s) % w as u64) as f64, (lcg(&mut s) % h as u64) as f64))
        .collect();
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let mut best = (f64::MAX, 0);
            for (i, &(sx, sy)) in pts.iter().enumerate() {
                let hash = (x as u64 * 73856093 ^ y as u64 * 19349663 ^ i as u64 * 83492791) % jitter;
                let d = (x as f64 - sx).powi(2) + (y as f64 - sy).powi(2) + hash as f64;
                if d < best.0 {
                    best = (d, i as u32);
                }
            }
            0x010101 * (best.1 + 1)
        })
        .collect()
}

/// Returns (segments before, segments after) smoothing, checking areas and ring counts on the way.
fn compare_smoothing(jitter: u64) -> (usize, usize) {
    let (w, h) = (160, 120);
    let px = blobby(w, h, 25, 7, jitter);

    let exact = run(&px, w, h, &Options::exact());
    assert_exact_areas(&exact);

    let smooth = run(&px, w, h, &Options::default());
    assert_eq!(smooth.provinces.len(), exact.provinces.len());
    let (mut n_exact, mut n_smooth) = (0, 0);
    for (e, s) in exact.provinces.iter().zip(&smooth.provinces) {
        let a = area(&s.path);
        let tol = (e.pixel_count as f64 * 0.08).max(6.0);
        assert!((a - e.pixel_count as f64).abs() < tol, "province {} area {a} vs {}", e.id, e.pixel_count);
        assert_eq!(rings(&e.path).len(), rings(&s.path).len(), "province {} ring count", e.id);
        n_exact += e.path.matches(['L', 'C']).count();
        n_smooth += s.path.matches(['L', 'C']).count();
    }
    (n_exact, n_smooth)
}

#[test]
fn smoothing_reduces_vertices_on_organic_borders() {
    let (before, after) = compare_smoothing(13);
    assert!(after * 3 < before, "smoothing kept too many segments: {after} vs {before}");
}

#[test]
fn smoothing_keeps_areas_under_extreme_pixel_noise() {
    // Every border pixel jumps around, so there is little to simplify; only
    // the invariants are checked.
    let (before, after) = compare_smoothing(97);
    assert!(after < before);
}

#[test]
fn smoothing_survives_noise_and_tiny_provinces() {
    for seed in 1..30u64 {
        let mut s = seed;
        let (w, h) = (20, 14);
        let px: Vec<u32> = (0..w * h).map(|_| 0x101010 * (lcg(&mut s) % 5) as u32 + 0x0a0a0a).collect();
        let m = run(&px, w, h, &Options::default());
        for p in &m.provinces {
            assert!(!p.path.is_empty());
            rings(&p.path);
        }
    }
}

#[test]
fn rejects_bad_buffers() {
    assert!(vectorize(&[0; 5], 2, 2, PixelFormat::Rgb, &Options::default()).is_err());
    assert!(vectorize(&[], 0, 2, PixelFormat::Rgb, &Options::default()).is_err());
}

#[test]
fn svg_document() {
    let m = run(&[RED, BLUE], 2, 1, &Options::exact());
    let svg = m.to_svg();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("data-id=\"0\"") && svg.contains("fill=\"#ff0000\""));
}
