use maptool_core::{Biome, Document, Error, Kind, Level, Options, PixelFormat, StateSet, ViewMode, is_map_file, mesh};

fn rgb(colors: &[u32]) -> Vec<u8> {
    colors.iter().flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect()
}

fn doc_from(colors: &[u32], w: u32, h: u32, o: &Options) -> Document {
    // Block maps have four-way junctions, and noise maps have lone pixels.
    let lax = Options { validate: false, ..o.clone() };
    Document::new(mesh(&rgb(colors), w, h, PixelFormat::Rgb, &lax, 0.05).unwrap())
}

/// `cols` x `rows` blocks of `b` x `b` pixels, one province per block; province ids run row by row.
fn grid(cols: u32, rows: u32, b: u32, o: &Options) -> Document {
    let px: Vec<u32> = (0..cols * b * rows * b)
        .map(|i| {
            let (x, y) = (i % (cols * b), i / (cols * b));
            0x010203 * ((y / b) * cols + x / b + 1)
        })
        .collect();
    doc_from(&px, cols * b, rows * b, o)
}

fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state >> 33
}

/// Jagged Voronoi-like map, like the one in the other test file.
fn blobby(w: u32, h: u32, seeds: u32, rng: u64) -> Vec<u32> {
    let mut s = rng;
    let pts: Vec<(f64, f64)> = (0..seeds).map(|_| ((lcg(&mut s) % w as u64) as f64, (lcg(&mut s) % h as u64) as f64)).collect();
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let mut best = (f64::MAX, 0);
            for (i, &(sx, sy)) in pts.iter().enumerate() {
                let hash = ((x as u64 * 73856093) ^ (y as u64 * 19349663) ^ (i as u64 * 83492791)) % 13;
                let d = (x as f64 - sx).powi(2) + (y as f64 - sy).powi(2) + hash as f64;
                if d < best.0 {
                    best = (d, i as u32);
                }
            }
            0x010101 * (best.1 + 1)
        })
        .collect()
}

fn exact() -> Options {
    Options::exact()
}

// ------------------------------------------------------------------ states

#[test]
fn creating_and_moving_provinces_between_states() {
    let mut s = StateSet::new(9);
    let a = s.create("Alpha", &[0, 1, 4]).unwrap();
    assert_eq!(s.state_of(1), Some(a));
    assert_eq!(s.state_of(2), None);
    assert_eq!(s.get(a).unwrap().provinces, vec![0, 1, 4]);

    // A province is in one state at a time: assigning it elsewhere moves it.
    let b = s.create("Beta", &[4, 5, 5]).unwrap();
    assert_ne!(a, b);
    assert_eq!(s.get(a).unwrap().provinces, vec![0, 1]);
    assert_eq!(s.get(b).unwrap().provinces, vec![4, 5]);
    assert_eq!(s.state_of(4), Some(b));

    s.assign(a, &[5, 8]).unwrap();
    assert_eq!(s.get(a).unwrap().provinces, vec![0, 1, 5, 8]);
    assert_eq!(s.get(b).unwrap().provinces, vec![4]);

    s.unassign(&[0, 4, 7]).unwrap();
    assert_eq!(s.state_of(0), None);
    assert_eq!(s.get(b).unwrap().provinces, Vec::<u32>::new());
    assert_eq!(s.len(), 2, "a state that lost all its provinces stays until deleted");
}

#[test]
fn deleting_states_frees_provinces_and_never_reuses_ids() {
    let mut s = StateSet::new(4);
    let a = s.create("A", &[0, 1]).unwrap();
    s.delete(a).unwrap();
    assert_eq!(s.state_of(0), None);
    assert!(s.get(a).is_none());
    let b = s.create("B", &[0]).unwrap();
    assert!(b > a);
    assert!(matches!(s.delete(a), Err(Error::Edit(_))));
}

#[test]
fn names_are_trimmed_and_default_to_the_id() {
    let mut s = StateSet::new(3);
    let a = s.create("   ", &[0]).unwrap();
    assert_eq!(s.get(a).unwrap().name, format!("State {a}"));
    s.rename(a, "  Northmark ").unwrap();
    assert_eq!(s.get(a).unwrap().name, "Northmark");
    s.rename(a, "").unwrap();
    assert_eq!(s.get(a).unwrap().name, format!("State {a}"));
    s.set_color(a, [1, 2, 3]).unwrap();
    assert_eq!(s.get(a).unwrap().color, [1, 2, 3]);
}

#[test]
fn refused_edits_change_nothing() {
    let mut s = StateSet::new(3);
    let a = s.create("A", &[0, 1]).unwrap();
    let before = s.clone();
    assert!(s.create("B", &[2, 3]).is_err(), "province 3 does not exist");
    assert!(s.assign(a, &[2, 99]).is_err());
    assert!(s.assign(77, &[2]).is_err());
    assert!(s.unassign(&[0, 99]).is_err());
    assert!(s.rename(77, "x").is_err());
    assert_eq!(s, before);
}

#[test]
fn state_colors_are_distinct() {
    let colors: Vec<_> = (1..=40).map(maptool_core::auto_color).collect();
    for i in 0..colors.len() {
        for j in i + 1..colors.len() {
            assert_ne!(colors[i], colors[j]);
        }
    }
}

#[test]
fn states_survive_saving() {
    let mut s = StateSet::new(20);
    let a = s.create("Ostmark — östlich", &[3, 4, 19]).unwrap();
    let b = s.create("B", &[0]).unwrap();
    s.set_color(b, [9, 8, 7]).unwrap();
    s.delete(a).unwrap();
    s.create("C", &[3, 4]).unwrap();
    let back = StateSet::from_bytes(&s.to_bytes(), 20).unwrap();
    assert_eq!(back, s);
}

#[test]
fn damaged_states_are_rejected_without_panicking() {
    let mut s = StateSet::new(10);
    s.create("A", &[1, 2]).unwrap();
    s.create("B", &[5]).unwrap();
    let good = s.to_bytes();

    assert!(StateSet::from_bytes(&good, 4).is_err(), "province 5 is out of range for a 4 province map");
    for len in 0..good.len() {
        assert!(StateSet::from_bytes(&good[..len], 10).is_err(), "truncated to {len}");
    }
    for i in 0..good.len() {
        for bit in 0..8 {
            let mut bad = good.clone();
            bad[i] ^= 1 << bit;
            let _ = StateSet::from_bytes(&bad, 10); // must not panic
        }
    }
    // The same province in two states.
    let mut dup = good.clone();
    let tail = dup.len() - 4;
    dup[tail..].copy_from_slice(&1u32.to_le_bytes());
    assert!(StateSet::from_bytes(&dup, 10).is_err());
}

// ------------------------------------------------------- province metadata

#[test]
fn provinces_start_as_unnamed_land() {
    let doc = grid(2, 2, 4, &exact());
    let m = doc.provinces.get(3).unwrap();
    assert_eq!((m.name.clone(), m.description.clone(), m.kind, m.biome(), m.population), (None, None, Kind::Land, Biome::Plains, None));
    assert_eq!(doc.provinces.display_name(3), "3");
}

#[test]
fn editing_province_metadata() {
    let mut doc = grid(3, 1, 4, &exact());
    let t = &mut doc.provinces;
    t.set_name(1, "  Saltmere ").unwrap();
    assert_eq!(t.display_name(1), "Saltmere");
    t.set_name(1, "   ").unwrap();
    assert_eq!(t.display_name(1), "1", "blank goes back to the number");
    t.set_description(2, "Line one\nLine two").unwrap();
    assert_eq!(t.get(2).unwrap().description.as_deref(), Some("Line one\nLine two"));
    t.set_description(2, "").unwrap();
    assert_eq!(t.get(2).unwrap().description, None);
    t.set_population(0, Some(12_500)).unwrap();
    assert_eq!(t.get(0).unwrap().population, Some(12_500));
    t.set_population(0, None).unwrap();
    assert_eq!(t.get(0).unwrap().population, None);

    t.set_kind(&[0, 2], Kind::Sea).unwrap();
    t.set_biome(&[1, 2], Biome::Mountains).unwrap();
    assert_eq!(t.get(0).unwrap().kind, Kind::Sea);
    assert_eq!(t.get(1).unwrap().kind, Kind::Land);
    assert_eq!(t.get(1).unwrap().biome(), Biome::Mountains);
    assert_eq!(t.get(0).unwrap().biome(), Biome::Sea, "a sea province's biome is Sea");

    assert!(t.set_kind(&[0, 9], Kind::Sea).is_err());
    assert_eq!(t.get(0).unwrap().kind, Kind::Sea, "a refused bulk edit changes nothing");
    assert!(t.set_name(9, "x").is_err());
}

#[test]
fn long_text_is_capped() {
    let mut doc = grid(1, 1, 4, &exact());
    doc.provinces.set_name(0, &"n".repeat(1000)).unwrap();
    doc.provinces.set_description(0, &"d".repeat(20_000)).unwrap();
    assert_eq!(doc.provinces.get(0).unwrap().name.as_ref().unwrap().chars().count(), 200);
    assert_eq!(doc.provinces.get(0).unwrap().description.as_ref().unwrap().chars().count(), 5000);
}

#[test]
fn province_metadata_survives_saving_and_rejects_damage() {
    let mut doc = grid(3, 2, 4, &exact());
    doc.provinces.set_name(0, "Ærø").unwrap();
    doc.provinces.set_description(1, "multi\nline — text").unwrap();
    doc.provinces.set_population(2, Some(u64::MAX)).unwrap();
    doc.provinces.set_kind(&[3], Kind::Sea).unwrap();
    doc.provinces.set_biome(&[4, 5], Biome::Arctic).unwrap();
    let good = doc.provinces.to_bytes();
    assert_eq!(maptool_core::ProvinceTable::from_bytes(&good, 6).unwrap(), doc.provinces);

    assert!(maptool_core::ProvinceTable::from_bytes(&good, 7).is_err(), "wrong province count");
    for len in 0..good.len() {
        assert!(maptool_core::ProvinceTable::from_bytes(&good[..len], 6).is_err(), "truncated to {len}");
    }
    for i in 0..good.len() {
        for bit in 0..8 {
            let mut bad = good.clone();
            bad[i] ^= 1 << bit;
            let _ = maptool_core::ProvinceTable::from_bytes(&bad, 6); // must not panic
        }
    }
    // An unknown biome code.
    let mut bad = good.clone();
    bad[8 + 1] = 200;
    assert!(maptool_core::ProvinceTable::from_bytes(&bad, 6).is_err());
}

// ------------------------------------------------------------ the container

fn edited_document() -> Document {
    let mut doc = doc_from(&blobby(96, 72, 14, 5), 96, 72, &Options::default());
    let a = doc.states.create("Coast", &[0, 1, 2]).unwrap();
    doc.states.create("Inland", &[3, 4]).unwrap();
    doc.states.assign(a, &[4]).unwrap();
    doc.provinces.set_name(0, "Harbor").unwrap();
    doc.provinces.set_kind(&[5, 6], Kind::Sea).unwrap();
    doc.provinces.set_biome(&[7], Biome::Desert).unwrap();
    doc.provinces.set_population(3, Some(4200)).unwrap();
    doc
}

#[test]
fn a_saved_map_reopens_identically() {
    let doc = edited_document();
    let bytes = doc.to_bytes();
    assert!(is_map_file(&bytes));
    let back = Document::from_bytes(&bytes).unwrap();
    assert_eq!(back, doc);
    // Hit testing on the reloaded mesh agrees with the original everywhere.
    for y in 0..72 {
        for x in 0..96 {
            let p = (x as f64 + 0.5, y as f64 + 0.5);
            assert_eq!(back.mesh.pick(p.0, p.1), doc.mesh.pick(p.0, p.1));
        }
    }
}

#[test]
fn saved_maps_are_much_smaller_than_the_raw_mesh() {
    let doc = edited_document();
    let raw = (doc.mesh.positions.len() + doc.mesh.indices.len() + doc.mesh.line_positions.len()) * 4;
    let saved = doc.to_bytes().len();
    assert!(saved * 2 < raw, "saved {saved} bytes vs {raw} raw");
}

#[test]
fn edits_can_be_saved_and_restored_without_the_mesh() {
    let doc = edited_document();
    let mut fresh = Document::new(doc.mesh.clone());
    fresh.set_edits_from_bytes(&doc.edits_to_bytes()).unwrap();
    assert_eq!(fresh, doc);

    // Refused edits leave the document as it was.
    let before = fresh.clone();
    let mut bad = doc.edits_to_bytes();
    let last = bad.len() - 1;
    bad.truncate(last);
    assert!(fresh.set_edits_from_bytes(&bad).is_err());
    assert_eq!(fresh, before);
    let other = grid(2, 2, 4, &exact());
    assert!(fresh.set_edits_from_bytes(&other.edits_to_bytes()).is_err(), "edits of a different map");
    assert_eq!(fresh, before);
}

#[test]
fn damaged_map_files_are_rejected_without_panicking() {
    let doc = grid(3, 2, 6, &Options::default());
    let good = doc.to_bytes();
    for len in 0..good.len() {
        assert!(Document::from_bytes(&good[..len]).is_err(), "truncated to {len}");
    }
    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0x55;
        if let Ok(d) = Document::from_bytes(&bad) {
            // Anything that still loads must be safe to draw and query.
            let n = d.mesh.positions.len() as u32 / 2;
            assert!(d.mesh.indices.iter().all(|&i| i < n));
            let _ = d.mesh.pick(3.0, 3.0);
            let _ = d.state_border_indices();
            let _ = d.palette(ViewMode::States, &[0], &[1]);
        }
    }
    assert!(Document::from_bytes(b"not a map").is_err());
    assert!(!is_map_file(b"\x89PNG"));
}

#[test]
fn a_newer_format_is_refused_with_a_clear_message() {
    let mut bytes = grid(2, 1, 4, &exact()).to_bytes();
    bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
    let err = Document::from_bytes(&bytes).unwrap_err().to_string();
    assert!(err.contains("version 99"), "{err}");
}

// ----------------------------------------------------- outlines and state view

fn on_image_edge(d: &Document, pair: &[u32], w: f32, h: f32) -> bool {
    let p = |i: u32| (d.mesh.line_positions[i as usize * 2], d.mesh.line_positions[i as usize * 2 + 1]);
    let (a, b) = (p(pair[0]), p(pair[1]));
    (a.0 == 0.0 && b.0 == 0.0) || (a.0 == w && b.0 == w) || (a.1 == 0.0 && b.1 == 0.0) || (a.1 == h && b.1 == h)
}

#[test]
fn state_view_hides_borders_inside_a_state() {
    let mut doc = grid(3, 3, 8, &exact());
    let all = doc.mesh.line_indices.len();
    assert_eq!(doc.state_border_indices().len(), all, "without states every border is drawn");

    // Provinces 0 and 1 are neighbours: their shared border (one segment per side) disappears.
    doc.states.create("A", &[0, 1]).unwrap();
    assert_eq!(doc.state_border_indices().len(), all - 2 * 2);

    // Two provinces that only touch at a corner share no border, so nothing is hidden.
    let mut diag = grid(3, 3, 8, &exact());
    diag.states.create("D", &[0, 4]).unwrap();
    assert_eq!(diag.state_border_indices().len(), all);
}

#[test]
fn one_state_covering_everything_leaves_only_the_image_edge() {
    for opts in [exact(), Options::default()] {
        let mut doc = doc_from(&blobby(120, 90, 18, 11), 120, 90, &opts);
        let everything: Vec<u32> = (0..doc.mesh.provinces.len() as u32).collect();
        doc.states.create("All", &everything).unwrap();
        let kept = doc.state_border_indices();
        assert!(!kept.is_empty());
        assert!(kept.chunks(2).all(|p| on_image_edge(&doc, p, 120.0, 90.0)), "an inner border survived");
    }
}

#[test]
fn unassigned_provinces_keep_all_their_borders_in_the_state_view() {
    let mut doc = grid(3, 1, 8, &exact());
    doc.states.create("Left", &[0]).unwrap();
    // Province 2 is unassigned and province 1 is unassigned: the border between them stays.
    let kept = doc.state_border_indices();
    assert_eq!(kept.len(), doc.mesh.line_indices.len());
}

#[test]
fn group_provinces_is_the_whole_state_or_the_province_itself() {
    let mut doc = grid(3, 2, 4, &exact());
    doc.states.create("S", &[1, 2, 5]).unwrap();
    assert_eq!(doc.group_provinces(2, Level::States), vec![1, 2, 5]);
    assert_eq!(doc.group_provinces(0, Level::States), vec![0]);
    assert!(doc.group_provinces(50, Level::States).is_empty());
}

#[test]
fn selection_outline_covers_only_the_edge_of_the_group() {
    let doc = grid(3, 3, 8, &exact());
    let of = |ids: &[u32]| doc.mesh.boundary_indices(ids).len() / 2;
    let one = of(&[4]);
    assert_eq!(one, doc.mesh.line_ranges[4][1] as usize / 2, "a single province: all of its segments");
    // Two neighbours: the border they share is inside the group, so it is left out.
    assert_eq!(of(&[0, 1]), doc.mesh.line_ranges[0][1] as usize / 2 + doc.mesh.line_ranges[1][1] as usize / 2 - 2);
    assert_eq!(of(&[0, 0, 99]), of(&[0]), "repeats and unknown ids are ignored");
    assert_eq!(of(&[]), 0);
    // Everything selected: only the image edge remains.
    let all: Vec<u32> = (0..9).collect();
    assert!(doc.mesh.boundary_indices(&all).chunks(2).all(|p| on_image_edge(&doc, p, 24.0, 24.0)));
}

#[test]
fn state_totals() {
    let mut doc = grid(3, 1, 4, &exact());
    let s = doc.states.create("S", &[0, 1, 2]).unwrap();
    doc.provinces.set_kind(&[2], Kind::Sea).unwrap();
    doc.provinces.set_population(0, Some(100)).unwrap();
    doc.provinces.set_population(1, Some(250)).unwrap();
    let t = doc.state_stats(s).unwrap();
    assert_eq!((t.provinces, t.land, t.sea, t.pixels, t.population, t.populated), (3, 2, 1, 48, 350, 2));
    assert!(doc.state_stats(99).is_none());
}

// ------------------------------------------------------------------ colors

fn color_of(palette: &[u8], id: usize) -> [u8; 3] {
    [palette[id * 4], palette[id * 4 + 1], palette[id * 4 + 2]]
}

#[test]
fn palette_modes() {
    let mut doc = grid(3, 1, 4, &exact());
    let original: Vec<[u8; 3]> = doc.mesh.provinces.iter().map(|p| p.color).collect();
    let p = doc.palette(ViewMode::Original, &[], &[]);
    assert_eq!((0..3).map(|i| color_of(&p, i)).collect::<Vec<_>>(), original);
    assert!(p.chunks(4).all(|c| c[3] == 255));

    // No states yet: the state view shows the original colors.
    let p = doc.palette(ViewMode::States, &[], &[]);
    assert_eq!(color_of(&p, 0), original[0]);

    let s = doc.states.create("S", &[0, 1]).unwrap();
    let state_color = doc.states.get(s).unwrap().color;
    let p = doc.palette(ViewMode::States, &[], &[]);
    assert_eq!((color_of(&p, 0), color_of(&p, 1)), (state_color, state_color));
    assert_ne!(color_of(&p, 2), original[2], "unassigned provinces are dimmed once states exist");

    doc.provinces.set_kind(&[1], Kind::Sea).unwrap();
    doc.provinces.set_biome(&[0], Biome::Desert).unwrap();
    let biome = doc.palette(ViewMode::Biome, &[], &[]);
    assert_eq!(color_of(&biome, 0), Biome::Desert.color());
    assert_ne!(color_of(&biome, 1), color_of(&biome, 2), "sea differs from land");
    let kind = doc.palette(ViewMode::Type, &[], &[]);
    assert_eq!(color_of(&kind, 0), color_of(&kind, 2), "land provinces share a color in the type view");
    assert_eq!(color_of(&biome, 1), color_of(&kind, 1), "sea looks the same in both");

    let base = doc.palette(ViewMode::Original, &[], &[]);
    let tinted = doc.palette(ViewMode::Original, &[0], &[1]);
    assert_ne!(color_of(&tinted, 0), color_of(&base, 0));
    assert_ne!(color_of(&tinted, 1), color_of(&base, 1));
    assert_eq!(color_of(&tinted, 2), color_of(&base, 2));
    assert_eq!(doc.palette(ViewMode::Original, &[99], &[99]), base, "unknown ids are ignored");
}

// ------------------------------------------------------------ the Sea biome

#[test]
fn sea_provinces_are_locked_to_the_sea_biome() {
    let mut doc = grid(3, 1, 4, &exact());
    let t = &mut doc.provinces;
    t.set_biome(&[0, 1], Biome::Forest).unwrap();
    t.set_kind(&[0], Kind::Sea).unwrap();
    assert_eq!(t.get(0).unwrap().biome(), Biome::Sea);
    assert_eq!(t.get(1).unwrap().biome(), Biome::Forest);

    // A biome edit reaches the land provinces of a selection and skips the sea ones.
    t.set_biome(&[0, 1, 2], Biome::Desert).unwrap();
    assert_eq!(t.get(0).unwrap().biome(), Biome::Sea);
    assert_eq!(t.get(1).unwrap().biome(), Biome::Desert);
    assert_eq!(t.get(2).unwrap().biome(), Biome::Desert);

    // Sea is not a choice: a province becomes Sea by being made sea.
    let before = t.clone();
    assert!(matches!(t.set_biome(&[1], Biome::Sea), Err(Error::Edit(_))));
    assert_eq!(*t, before);

    // Making it land again brings back the biome it had, not one applied while it was sea.
    t.set_kind(&[0], Kind::Land).unwrap();
    assert_eq!(t.get(0).unwrap().biome(), Biome::Forest);
}

#[test]
fn only_sea_provinces_have_the_sea_biome() {
    let names = maptool_core::biome_names();
    assert_eq!(names.len(), 11);
    assert_eq!(names[10], "Sea");
    assert!(Biome::ALL[..10].iter().all(|b| *b != Biome::Sea));

    let mut doc = grid(2, 1, 4, &exact());
    doc.provinces.set_kind(&[1], Kind::Sea).unwrap();
    let p = doc.palette(ViewMode::Biome, &[], &[]);
    assert_eq!(color_of(&p, 1), Biome::Sea.color());
    assert_eq!(color_of(&p, 0), Biome::Plains.color());
}

#[test]
fn the_sea_lock_survives_saving_and_older_data() {
    let mut doc = grid(2, 1, 4, &exact());
    doc.provinces.set_biome(&[0], Biome::Forest).unwrap();
    doc.provinces.set_kind(&[0], Kind::Sea).unwrap();
    let back = maptool_core::ProvinceTable::from_bytes(&doc.provinces.to_bytes(), 2).unwrap();
    assert_eq!(back, doc.provinces);
    let mut back = back;
    back.set_kind(&[0], Kind::Land).unwrap();
    assert_eq!(back.get(0).unwrap().biome(), Biome::Forest, "the hidden land biome was saved too");

    // Data written before the Sea biome existed: a sea province with an ordinary biome byte.
    let mut old = doc.provinces.to_bytes();
    old[8 + 1] = Biome::Hills as u8; // province 0: kind Sea, biome byte Hills
    let loaded = maptool_core::ProvinceTable::from_bytes(&old, 2).unwrap();
    assert_eq!(loaded.get(0).unwrap().biome(), Biome::Sea);

    // A land province can never carry the Sea biome, whatever the bytes say.
    let mut bad = doc.provinces.to_bytes();
    bad[8 + 4 + 1] = Biome::Sea as u8; // province 1 is land
    assert!(maptool_core::ProvinceTable::from_bytes(&bad, 2).is_err());
    // A sea province with the Sea code is tolerated and gets a default land biome.
    let mut sea = doc.provinces.to_bytes();
    sea[8 + 1] = Biome::Sea as u8;
    let loaded = maptool_core::ProvinceTable::from_bytes(&sea, 2).unwrap();
    assert_eq!(loaded.get(0).unwrap().biome(), Biome::Sea);
}

// ---------------------------------------------------------------- map name

#[test]
fn a_map_can_be_named() {
    let mut doc = grid(2, 1, 4, &exact());
    assert_eq!(doc.name(), "");
    doc.set_name("  Europe 1444  ");
    assert_eq!(doc.name(), "Europe 1444");
    doc.set_name(&"x".repeat(500));
    assert_eq!(doc.name().chars().count(), 100);
    doc.set_name("   ");
    assert_eq!(doc.name(), "", "blank clears the name");
}

#[test]
fn the_map_name_is_saved_with_the_edits_and_the_file() {
    let mut doc = edited_document();
    doc.set_name("Ærø & Fanø — map");
    assert_eq!(Document::from_bytes(&doc.to_bytes()).unwrap(), doc);

    let mut fresh = Document::new(doc.mesh.clone());
    fresh.set_edits_from_bytes(&doc.edits_to_bytes()).unwrap();
    assert_eq!(fresh.name(), "Ærø & Fanø — map");
    assert_eq!(fresh, doc);
}

#[test]
fn edits_saved_before_maps_had_names_still_load() {
    let doc = edited_document();
    // Version 1: version, states length, states, then the province data to the end.
    let states = doc.states.to_bytes();
    let mut v1 = 1u32.to_le_bytes().to_vec();
    v1.extend((states.len() as u32).to_le_bytes());
    v1.extend(&states);
    v1.extend(doc.provinces.to_bytes());

    let mut fresh = Document::new(doc.mesh.clone());
    fresh.set_name("kept?");
    fresh.set_edits_from_bytes(&v1).unwrap();
    assert_eq!(fresh.name(), "", "old edits carry no name");
    assert_eq!(fresh.states, doc.states);
    assert_eq!(fresh.provinces, doc.provinces);
}

#[test]
fn damaged_edits_with_a_name_are_rejected() {
    let mut doc = edited_document();
    doc.set_name("Some name");
    let good = doc.edits_to_bytes();
    let mut target = Document::new(doc.mesh.clone());
    for len in 0..good.len() {
        assert!(target.set_edits_from_bytes(&good[..len]).is_err(), "truncated to {len}");
    }
    let mut extra = good.clone();
    extra.push(0);
    assert!(target.set_edits_from_bytes(&extra).is_err(), "trailing data");
    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0x40;
        let _ = target.set_edits_from_bytes(&bad); // must not panic
    }
    assert!(target.set_edits_from_bytes(&good).is_ok());
    assert_eq!(target.name(), "Some name");
}
