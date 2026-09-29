use maptool_core::{Error, GenerateOptions, Options, PixelFormat, generate_labels, generate_mesh};

const LAND: u32 = 0xffffff;
const WATER: u32 = 0x0000ff;
const WALL: u32 = 0x000000;

fn rgb(colors: &[u32], w: u32, h: u32) -> Vec<u8> {
    assert_eq!(colors.len(), (w * h) as usize);
    colors.iter().flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect()
}

fn opts(land_radius: f64, water_radius: f64, split_seas: bool, seed: u32) -> GenerateOptions {
    GenerateOptions { land_radius, water_radius, split_seas, seed }
}

fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state >> 33
}

/// A land image speckled with occasional sea/lake and border-line pixels, so
/// most components are large but many single-pixel ones exist too.
fn noisy_border_map(w: u32, h: u32, seed: u64) -> Vec<u32> {
    let mut s = seed;
    (0..w * h)
        .map(|_| match lcg(&mut s) % 20 {
            0..=16 => LAND,
            17..=18 => WATER,
            _ => WALL,
        })
        .collect()
}

/// A hand-painted-looking border map: a few Voronoi-ish land/sea regions (most
/// of them land), with thin, periodic border lines drawn over them — a much
/// closer stand-in for a real input than per-pixel noise, which produces so
/// many adjacent single-pixel provinces of both terrains that a legitimate,
/// documented corner case (a junction whose all 4 pixels are already
/// singleton provinces) becomes common instead of rare.
fn organic_border_map(w: u32, h: u32, rng: u64, wall_every: u32) -> Vec<u32> {
    let mut s = rng;
    let seeds = (w * h).min(6);
    let pts: Vec<(f64, f64)> = (0..seeds).map(|_| ((lcg(&mut s) % w as u64) as f64, (lcg(&mut s) % h as u64) as f64)).collect();
    let region_is_water: Vec<bool> = (0..seeds).map(|_| lcg(&mut s).is_multiple_of(4)).collect();
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            if wall_every > 0 && (x % wall_every == 0 || y % wall_every == 0) {
                return WALL;
            }
            let mut best = (f64::MAX, 0usize);
            for (i, &(sx, sy)) in pts.iter().enumerate() {
                let d = (x as f64 - sx).powi(2) + (y as f64 - sy).powi(2);
                if d < best.0 {
                    best = (d, i);
                }
            }
            if region_is_water[best.1] { WATER } else { LAND }
        })
        .collect()
}

#[test]
fn every_pixel_labeled_once_and_areas_sum() {
    for &(w, h) in &[(12u32, 9u32), (30, 24), (1, 1), (5, 1), (1, 5)] {
        let px = rgb(&noisy_border_map(w, h, w as u64 * 31 + h as u64), w, h);
        let g = generate_labels(&px, w, h, PixelFormat::Rgb, &opts(4.0, 4.0, false, 1)).unwrap();
        assert!(g.data.iter().all(|&id| id != u32::MAX), "{w}x{h}: a pixel was left unlabeled");
        let total: u64 = g.counts.iter().map(|&c| c as u64).sum();
        assert_eq!(total, (w * h) as u64, "{w}x{h}: counts do not sum to the image");
        assert!(g.counts.iter().all(|&c| c > 0), "{w}x{h}: an empty province was produced");
    }
}

#[test]
fn generated_output_passes_validation_across_seeds_and_radii() {
    let (w, h) = (30u32, 24u32);
    for seed in 1..14u32 {
        for &radius in &[3.0, 8.0, 15.0] {
            let px = rgb(&organic_border_map(w, h, seed as u64 * 97 + radius as u64, 7), w, h);
            let mesh_opts = Options { validate: true, ..Options::default() };
            let result = generate_mesh(&px, w, h, PixelFormat::Rgb, &opts(radius, radius, seed % 2 == 0, seed), &mesh_opts, 0.05);
            assert!(result.is_ok(), "seed {seed} radius {radius}: {:?}", result.err());
        }
    }
}

/// A grid of small, wall-separated land squares. Each square is far smaller
/// than the jitter cell for a large radius, so its seed always falls back to
/// its own top-left pixel: a perfectly symmetric layout, engineered to make
/// several provinces' frontiers arrive at shared corners simultaneously.
fn checkerboard_of_squares(n: u32, square: u32) -> (Vec<u32>, u32) {
    let side = n * square + (n - 1); // one wall pixel between squares
    let mut px = vec![WALL; (side * side) as usize];
    for by in 0..n {
        for bx in 0..n {
            for dy in 0..square {
                for dx in 0..square {
                    let (x, y) = (bx * (square + 1) + dx, by * (square + 1) + dy);
                    px[(y * side + x) as usize] = LAND;
                }
            }
        }
    }
    (px, side)
}

#[test]
fn adversarial_symmetric_grid_produces_no_violations_or_empty_provinces() {
    let (px, side) = checkerboard_of_squares(5, 3);
    let bytes = rgb(&px, side, side);
    let g = generate_labels(&bytes, side, side, PixelFormat::Rgb, &opts(1000.0, 1000.0, false, 0)).unwrap();
    assert!(g.counts.iter().all(|&c| c > 0), "a province was left empty by junction repair");
    assert!(g.data.iter().all(|&id| id != u32::MAX));

    let mesh_opts = Options { validate: true, ..Options::default() };
    let result = generate_mesh(&bytes, side, side, PixelFormat::Rgb, &opts(1000.0, 1000.0, false, 0), &mesh_opts, 0.05);
    assert!(result.is_ok(), "adversarial grid still had violations after repair: {:?}", result.err());
}

#[test]
fn province_count_roughly_tracks_area_over_radius_squared() {
    let (w, h) = (200u32, 150u32);
    let px = rgb(&vec![LAND; (w * h) as usize], w, h);
    let radius = 10.0;
    let g = generate_labels(&px, w, h, PixelFormat::Rgb, &opts(radius, radius, false, 3)).unwrap();
    let expected = (w * h) as f64 / (std::f64::consts::PI * radius * radius);
    let count = g.colors.len() as f64;
    assert!(count > expected * 0.3 && count < expected * 3.0, "count {count} vs expected ~{expected}");
}

#[test]
fn disconnected_water_bodies_stay_separate_even_when_not_split() {
    // Two lakes, far apart, on a sea of land; split_seas is off.
    let (w, h) = (30u32, 10u32);
    let mut px = vec![LAND; (w * h) as usize];
    for y in 2..8 {
        for x in 2..6 {
            px[(y * w + x) as usize] = WATER;
        }
        for x in 24..28 {
            px[(y * w + x) as usize] = WATER;
        }
    }
    let bytes = rgb(&px, w, h);
    let g = generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(6.0, 6.0, false, 5)).unwrap();
    let lake_a = g.data[(4 * w + 4) as usize];
    let lake_b = g.data[(4 * w + 26) as usize];
    assert_ne!(lake_a, lake_b, "two disconnected lakes ended up as one province");
    assert!(g.sea_provinces.contains(&lake_a) && g.sea_provinces.contains(&lake_b));
}

#[test]
fn wall_line_is_split_between_both_neighbouring_provinces() {
    // land (2 cols) | wall (21 cols) | water (2 cols): the wall column touching
    // land is far closer to the land seed than to the water centroid, and vice
    // versa for the column touching water, regardless of jitter within each
    // tiny 2-column region.
    let (w, h) = (25u32, 5u32);
    let mut px = vec![WALL; (w * h) as usize];
    for y in 0..h {
        px[(y * w) as usize] = LAND;
        px[(y * w + 1) as usize] = LAND;
        px[(y * w + 23) as usize] = WATER;
        px[(y * w + 24) as usize] = WATER;
    }
    let bytes = rgb(&px, w, h);
    let g = generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(3.0, 3.0, false, 9)).unwrap();

    assert!((0..w).all(|x| (0..h).all(|y| g.data[(y * w + x) as usize] != u32::MAX)), "a wall pixel was left unlabeled");

    let near_land = g.data[(2 * w + 2) as usize];
    let near_water = g.data[(2 * w + 22) as usize];
    assert!(!g.sea_provinces.contains(&near_land), "the land-facing wall pixel ended up sea");
    assert!(g.sea_provinces.contains(&near_water), "the water-facing wall pixel did not end up sea");
}

#[test]
fn a_wall_line_is_never_crossed_by_a_single_province() {
    // A vertical wall splits an otherwise uniform land area into a left half
    // and a right half. The radius is generous enough that `place_seeds`
    // falls back to one seed per component, each placed at that component's
    // first pixel in row-major scan order — i.e. right next to the wall on
    // the right half. That makes some far-left pixels' straight-line
    // distance to the *right* seed shorter than to the left seed, which is
    // exactly the scenario that leaked across the wall before growth treated
    // a border line as a hard barrier: regardless of relative distance, no
    // province may ever include pixels from both halves.
    let (w, h) = (40u32, 20u32);
    let mut px = vec![LAND; (w * h) as usize];
    for y in 0..h {
        px[(y * w + w / 2) as usize] = WALL;
    }
    let bytes = rgb(&px, w, h);
    let g = generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(1000.0, 1000.0, false, 11)).unwrap();

    let half = w / 2;
    let ids_in = |xs: std::ops::Range<u32>| -> std::collections::HashSet<u32> {
        (0..h).flat_map(|y| xs.clone().map(move |x| (x, y))).map(|(x, y)| g.data[(y * w + x) as usize]).collect()
    };
    let left = ids_in(0..half);
    let right = ids_in(half + 1..w);
    assert!(left.is_disjoint(&right), "a province spans both sides of the wall: {left:?} / {right:?}");
}

#[test]
fn sea_provinces_match_water_origin() {
    let (w, h) = (20u32, 10u32);
    let mut px = vec![LAND; (w * h) as usize];
    for y in 0..h {
        for x in 10..w {
            px[(y * w + x) as usize] = WATER;
        }
    }
    let bytes = rgb(&px, w, h);
    let g = generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(4.0, 4.0, true, 2)).unwrap();
    for (i, &id) in g.data.iter().enumerate() {
        let x = i as u32 % w;
        let is_sea = g.sea_provinces.contains(&id);
        assert_eq!(is_sea, x >= 10, "pixel {i} (x={x}) province {id} sea-ness disagrees with its source terrain");
    }
}

#[test]
fn all_black_image_and_enclosed_pocket_do_not_panic() {
    let (w, h) = (10u32, 8u32);
    let all_black = rgb(&vec![WALL; (w * h) as usize], w, h);
    let g = generate_labels(&all_black, w, h, PixelFormat::Rgb, &opts(4.0, 4.0, false, 0)).unwrap();
    assert!(g.data.iter().all(|&id| id != u32::MAX));
    assert!(g.counts.iter().all(|&c| c > 0));

    // A wall pocket fully enclosed by land, with no water/land pixel inside.
    let (w, h) = (11u32, 11u32);
    let mut px = vec![LAND; (w * h) as usize];
    for y in 3..8 {
        for x in 3..8 {
            px[(y * w + x) as usize] = WALL;
        }
    }
    let bytes = rgb(&px, w, h);
    let g = generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(2.0, 2.0, false, 0)).unwrap();
    assert!(g.data.iter().all(|&id| id != u32::MAX));
    assert!(g.counts.iter().all(|&c| c > 0));
}

#[test]
fn invalid_pixel_color_is_a_hard_error() {
    let (w, h) = (4u32, 4u32);
    let mut px = vec![LAND; (w * h) as usize];
    px[6] = 0x0a1420; // neither white, green nor black
    let bytes = rgb(&px, w, h);
    match generate_labels(&bytes, w, h, PixelFormat::Rgb, &opts(4.0, 4.0, false, 0)) {
        Err(Error::InvalidBorderColor { x, y, color }) => {
            assert_eq!((x, y, color), (2, 1, [0x0a, 0x14, 0x20]));
        }
        other => panic!("expected InvalidBorderColor, got {:?}", other.map(|g| g.data.len())),
    }
}

#[test]
fn same_seed_is_deterministic_different_seed_differs() {
    let (w, h) = (100u32, 80u32);
    let px = rgb(&vec![LAND; (w * h) as usize], w, h);
    let a1 = generate_labels(&px, w, h, PixelFormat::Rgb, &opts(15.0, 15.0, false, 42)).unwrap();
    let a2 = generate_labels(&px, w, h, PixelFormat::Rgb, &opts(15.0, 15.0, false, 42)).unwrap();
    assert_eq!(a1.data, a2.data, "same seed produced different output");

    let b = generate_labels(&px, w, h, PixelFormat::Rgb, &opts(15.0, 15.0, false, 43)).unwrap();
    assert_ne!(a1.data, b.data, "different seeds produced identical output");
}
