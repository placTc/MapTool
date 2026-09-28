use maptool_core::{Document, Error, GroupKind, Kind, Level, Options, PixelFormat, StateSet, Unit, ViewMode, filter, mesh};

fn rgb(colors: &[u32]) -> Vec<u8> {
    colors.iter().flat_map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect()
}

fn doc_from(colors: &[u32], w: u32, h: u32, o: &Options) -> Document {
    let lax = Options { validate: false, ..o.clone() };
    Document::new(mesh(&rgb(colors), w, h, PixelFormat::Rgb, &lax, 0.05).unwrap())
}

/// `cols` x `rows` blocks of `b` x `b` pixels, one province per block; ids run row by row.
fn grid(cols: u32, rows: u32, b: u32) -> Document {
    let px: Vec<u32> = (0..cols * b * rows * b)
        .map(|i| {
            let (x, y) = (i % (cols * b), i / (cols * b));
            0x010203 * ((y / b) * cols + x / b + 1)
        })
        .collect();
    doc_from(&px, cols * b, rows * b, &Options::exact())
}

fn lcg(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state >> 33
}

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

const COUNTRY: GroupKind = GroupKind::Country;
const REGION: GroupKind = GroupKind::Region;

// ------------------------------------------------------ state descriptions

#[test]
fn states_can_have_descriptions() {
    let mut s = StateSet::new(4);
    let a = s.create("A", &[0]).unwrap();
    assert_eq!(s.get(a).unwrap().description, "");
    s.set_description(a, "  Salt flats\nand a harbor  ").unwrap();
    assert_eq!(s.get(a).unwrap().description, "Salt flats\nand a harbor");
    s.set_description(a, &"d".repeat(9000)).unwrap();
    assert_eq!(s.get(a).unwrap().description.chars().count(), 5000);
    s.set_description(a, "   ").unwrap();
    assert_eq!(s.get(a).unwrap().description, "", "blank clears it");
    assert!(s.set_description(99, "x").is_err());

    s.set_description(a, "Ærø — östlich").unwrap();
    assert_eq!(StateSet::from_bytes(&s.to_bytes(), 4).unwrap(), s);
}

#[test]
fn states_saved_without_descriptions_still_load() {
    // Version 1: id, color, name, provinces; no description.
    let put = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let mut v1 = Vec::new();
    put(&mut v1, 1); // version
    put(&mut v1, 5); // next id
    put(&mut v1, 1); // one state
    put(&mut v1, 3); // id
    put(&mut v1, u32::from_le_bytes([10, 20, 30, 0]));
    put(&mut v1, 5);
    v1.extend_from_slice(b"Alpha");
    put(&mut v1, 2);
    put(&mut v1, 1);
    put(&mut v1, 2);
    let s = StateSet::from_bytes(&v1, 4).unwrap();
    let a = s.get(3).unwrap();
    assert_eq!((a.name.as_str(), a.description.as_str(), a.color, a.provinces.clone()), ("Alpha", "", [10, 20, 30], vec![1, 2]));
}

// ----------------------------------------------- countries and regions (groups)

#[test]
fn groups_hold_states_and_a_state_is_in_one_group_of_a_kind() {
    let mut d = grid(3, 2, 4);
    let s: Vec<u32> = (0..4).map(|i| d.states.create(&format!("S{i}"), &[i]).unwrap()).collect();

    let a = d.create_group(COUNTRY, "Aria", &[s[0], s[1]]).unwrap();
    assert_eq!(d.countries.get(a).unwrap().states, vec![s[0], s[1]]);
    assert_eq!(d.countries.group_of(s[1]), Some(a));
    assert_eq!(d.countries.group_of(s[2]), None);

    // Moving: a state joins another country and leaves the first.
    let b = d.create_group(COUNTRY, "Borea", &[s[1], s[2], s[2]]).unwrap();
    assert_eq!(d.countries.get(a).unwrap().states, vec![s[0]]);
    assert_eq!(d.countries.get(b).unwrap().states, vec![s[1], s[2]]);
    d.assign_to_group(COUNTRY, a, &[s[3], s[2]]).unwrap();
    assert_eq!(d.countries.get(a).unwrap().states, vec![s[0], s[2], s[3]]);
    assert_eq!(d.countries.get(b).unwrap().states, vec![s[1]]);

    // Countries and strategic regions are independent: one state, one of each.
    let r = d.create_group(REGION, "Northlands", &[s[0], s[1]]).unwrap();
    assert_eq!(d.regions.group_of(s[0]), Some(r));
    assert_eq!(d.countries.group_of(s[0]), Some(a), "joining a region did not touch the country");
    d.unassign_from_groups(COUNTRY, &[s[0]]).unwrap();
    assert_eq!(d.countries.group_of(s[0]), None);
    assert_eq!(d.regions.group_of(s[0]), Some(r), "leaving a country did not touch the region");
    d.unassign_from_groups(COUNTRY, &[s[0]]).unwrap(); // a state in no group is ignored
}

#[test]
fn groups_can_be_renamed_recolored_described_and_deleted() {
    let mut d = grid(2, 1, 4);
    let s = d.states.create("S", &[0]).unwrap();
    let c = d.create_group(COUNTRY, "  ", &[s]).unwrap();
    assert_eq!(d.countries.get(c).unwrap().name, format!("Country {c}"));
    let r = d.create_group(REGION, "", &[s]).unwrap();
    assert_eq!(d.regions.get(r).unwrap().name, format!("Strategic region {r}"));
    assert_ne!(d.countries.get(c).unwrap().color, d.regions.get(r).unwrap().color);

    let set = d.groups_mut(COUNTRY);
    set.rename(c, "  Aria ").unwrap();
    assert_eq!(set.get(c).unwrap().name, "Aria");
    set.rename(c, "   ").unwrap();
    assert_eq!(set.get(c).unwrap().name, "Aria", "a blank name is ignored");
    set.set_color(c, [1, 2, 3]).unwrap();
    assert_eq!(set.get(c).unwrap().color, [1, 2, 3]);
    set.set_description(c, "  A country.\nWith two lines.  ").unwrap();
    assert_eq!(set.get(c).unwrap().description, "A country.\nWith two lines.");
    set.set_description(c, &"x".repeat(9000)).unwrap();
    assert_eq!(set.get(c).unwrap().description.chars().count(), 5000);
    set.set_description(c, "").unwrap();
    assert_eq!(set.get(c).unwrap().description, "");

    set.delete(c).unwrap();
    assert!(set.get(c).is_none() && set.group_of(s).is_none());
    assert!(set.delete(c).is_err());
    assert!(set.rename(c, "x").is_err());
    let again = d.create_group(COUNTRY, "Again", &[s]).unwrap();
    assert!(again > c, "ids are never reused");
    assert_eq!(d.regions.group_of(s), Some(r), "deleting a country does not touch regions");
}

#[test]
fn refused_group_edits_change_nothing() {
    let mut d = grid(3, 1, 4);
    let s = d.states.create("S", &[0, 1]).unwrap();
    let c = d.create_group(COUNTRY, "C", &[s]).unwrap();
    let before = d.clone();
    assert!(matches!(d.create_group(COUNTRY, "X", &[s, 77]), Err(Error::Edit(_))), "state 77 does not exist");
    assert!(d.assign_to_group(COUNTRY, c, &[77]).is_err());
    assert!(d.assign_to_group(COUNTRY, 99, &[s]).is_err());
    assert!(d.assign_to_group(REGION, c, &[s]).is_err(), "country id is not a region id");
    assert!(d.unassign_from_groups(COUNTRY, &[77]).is_err());
    assert_eq!(d, before);
}

#[test]
fn deleting_a_state_removes_it_from_its_country_and_region() {
    let mut d = grid(3, 1, 4);
    let s1 = d.states.create("S1", &[0]).unwrap();
    let s2 = d.states.create("S2", &[1]).unwrap();
    let c = d.create_group(COUNTRY, "C", &[s1, s2]).unwrap();
    let r = d.create_group(REGION, "R", &[s1]).unwrap();
    d.delete_state(s1).unwrap();
    assert_eq!(d.countries.get(c).unwrap().states, vec![s2]);
    assert!(d.regions.get(r).unwrap().states.is_empty());
    assert!(d.countries.group_of(s1).is_none() && d.regions.group_of(s1).is_none());
    assert!(d.delete_state(s1).is_err());
    // The edits still validate (no group refers to the missing state).
    let mut fresh = Document::new(d.mesh.clone());
    fresh.set_edits_from_bytes(&d.edits_to_bytes()).unwrap();
    assert_eq!(fresh, d);
}

// ----------------------------------------------------------------- saving

fn with_groups() -> Document {
    let mut d = grid(3, 3, 4);
    let s: Vec<u32> = [vec![0, 1], vec![2], vec![3, 4]].iter().enumerate().map(|(i, p)| d.states.create(&format!("S{i}"), p).unwrap()).collect();
    d.states.set_description(s[0], "first state").unwrap();
    let c = d.create_group(COUNTRY, "Ostmark — östlich", &[s[0], s[1]]).unwrap();
    d.groups_mut(COUNTRY).set_description(c, "a country\nwith text").unwrap();
    let r = d.create_group(REGION, "Region", &[s[0], s[2]]).unwrap();
    d.groups_mut(REGION).set_color(r, [9, 8, 7]).unwrap();
    d.set_name("Named");
    d
}

#[test]
fn countries_regions_and_descriptions_survive_saving() {
    let d = with_groups();
    assert_eq!(Document::from_bytes(&d.to_bytes()).unwrap(), d);
    let mut fresh = Document::new(d.mesh.clone());
    fresh.set_edits_from_bytes(&d.edits_to_bytes()).unwrap();
    assert_eq!(fresh, d);
}

#[test]
fn edits_from_before_countries_and_regions_still_load() {
    let d = with_groups();
    // Version 2: version, states, province data, name.
    let (states, provinces) = (d.states.to_bytes(), d.provinces.to_bytes());
    let put = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let mut v2 = Vec::new();
    put(&mut v2, 2);
    put(&mut v2, states.len() as u32);
    v2.extend(&states);
    put(&mut v2, provinces.len() as u32);
    v2.extend(&provinces);
    put(&mut v2, d.name().len() as u32);
    v2.extend(d.name().as_bytes());

    let mut fresh = Document::new(d.mesh.clone());
    fresh.set_edits_from_bytes(&v2).unwrap();
    assert_eq!(fresh.states, d.states);
    assert_eq!(fresh.name(), "Named");
    assert!(fresh.countries.is_empty() && fresh.regions.is_empty());
}

#[test]
fn damaged_group_data_is_rejected_without_changing_anything() {
    let d = with_groups();
    let good = d.edits_to_bytes();
    let mut target = Document::new(d.mesh.clone());
    target.set_edits_from_bytes(&good).unwrap();
    let before = target.clone();
    for len in 0..good.len() {
        assert!(target.set_edits_from_bytes(&good[..len]).is_err(), "truncated to {len}");
        assert_eq!(target, before);
    }
    for i in 0..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0x21;
        let _ = target.set_edits_from_bytes(&bad); // must not panic
        target = before.clone();
    }

    // A country that refers to a state which does not exist.
    let (empty_states, provinces) = (StateSet::new(9).to_bytes(), d.provinces.to_bytes());
    let put = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    let (countries, regions) = (d.countries.to_bytes(), d.regions.to_bytes());
    let mut bad = Vec::new();
    put(&mut bad, 3);
    put(&mut bad, empty_states.len() as u32);
    bad.extend(&empty_states);
    put(&mut bad, provinces.len() as u32);
    bad.extend(&provinces);
    put(&mut bad, 0);
    put(&mut bad, countries.len() as u32);
    bad.extend(&countries);
    put(&mut bad, regions.len() as u32);
    bad.extend(&regions);
    let err = target.set_edits_from_bytes(&bad).unwrap_err().to_string();
    assert!(err.contains("missing state"), "{err}");
    assert_eq!(target, before);
}

// ------------------------------------------------- levels: what is shown as one

/// Three by three provinces (ids 0..9, row by row). States: S1 = {0, 1}, S2 = {2}, S3 = {3, 4}.
/// Country C = {S1, S2}. Region R = {S1, S3}. Provinces 5..9 are in no state.
fn layered() -> (Document, [u32; 3], u32, u32) {
    let mut d = grid(3, 3, 8);
    let s1 = d.states.create("S1", &[0, 1]).unwrap();
    let s2 = d.states.create("S2", &[2]).unwrap();
    let s3 = d.states.create("S3", &[3, 4]).unwrap();
    let c = d.create_group(COUNTRY, "C", &[s1, s2]).unwrap();
    let r = d.create_group(REGION, "R", &[s1, s3]).unwrap();
    (d, [s1, s2, s3], c, r)
}

#[test]
fn units_fall_back_from_country_to_state_to_province() {
    let (d, [s1, s2, s3], c, r) = layered();
    let at = |p, l| d.unit(p, l);
    assert_eq!(at(0, Level::Provinces), Unit::Province(0));
    assert_eq!(at(0, Level::States), Unit::State(s1));
    assert_eq!(at(0, Level::Countries), Unit::Country(c));
    assert_eq!(at(0, Level::Regions), Unit::Region(r));
    assert_eq!(at(2, Level::Countries), Unit::Country(c));
    assert_eq!(at(2, Level::Regions), Unit::State(s2), "S2 is in a country but no region");
    assert_eq!(at(3, Level::Countries), Unit::State(s3), "S3 is in a region but no country");
    assert_eq!(at(3, Level::Regions), Unit::Region(r));
    assert_eq!(at(5, Level::States), Unit::Province(5));
    assert_eq!(at(5, Level::Countries), Unit::Province(5));
    assert_eq!((at(0, Level::Countries).kind(), at(0, Level::Countries).id()), (2, c));
    assert_eq!(at(0, Level::Regions).kind(), 3);
}

#[test]
fn what_highlights_together_depends_on_the_level() {
    let (d, _, c, r) = layered();
    assert_eq!(d.group_provinces(1, Level::Provinces), vec![1]);
    assert_eq!(d.group_provinces(1, Level::States), vec![0, 1]);
    assert_eq!(d.group_provinces(1, Level::Countries), vec![0, 1, 2]);
    assert_eq!(d.group_provinces(1, Level::Regions), vec![0, 1, 3, 4]);
    assert_eq!(d.group_provinces(4, Level::Countries), vec![3, 4], "a state in no country shows as the state");
    assert_eq!(d.group_provinces(7, Level::Regions), vec![7]);
    assert!(d.group_provinces(99, Level::Countries).is_empty());
    assert_eq!(d.provinces_of_group(COUNTRY, c), vec![0, 1, 2]);
    assert_eq!(d.provinces_of_group(REGION, r), vec![0, 1, 3, 4]);
    assert!(d.provinces_of_group(COUNTRY, 77).is_empty());
}

#[test]
fn borders_inside_a_unit_are_hidden() {
    let (d, ..) = layered();
    let total = d.mesh.line_indices.len();
    // Each shared border is one segment on each side, so hiding a border removes two segments.
    let hidden = |level| (total - d.border_indices(level).len()) / 4;
    assert_eq!(hidden(Level::Provinces), 0);
    assert_eq!(hidden(Level::States), 2, "0-1 and 3-4");
    assert_eq!(hidden(Level::Countries), 3, "0-1 and 1-2 (one country), 3-4 (state S3 alone)");
    assert_eq!(hidden(Level::Regions), 4, "0-1, 3-4, 0-3 and 1-4 (one region)");
    assert_eq!(d.state_border_indices(), d.border_indices(Level::States));
}

#[test]
fn the_units_of_some_provinces() {
    let (d, [s1, s2, _], c, _) = layered();
    assert_eq!(d.units_of(&[0, 1, 2, 5], Level::Countries), vec![2, c, 0, 5]);
    assert_eq!(d.units_of(&[0, 1, 2, 5, 5, 99], Level::States), vec![1, s1, 1, s2, 0, 5]);
    assert!(d.units_of(&[], Level::States).is_empty());
}

#[test]
fn country_and_region_colors() {
    let (mut d, _, c, r) = layered();
    d.groups_mut(COUNTRY).set_color(c, [200, 10, 10]).unwrap();
    d.groups_mut(REGION).set_color(r, [10, 10, 200]).unwrap();
    let color = |p: &[u8], i: usize| [p[i * 4], p[i * 4 + 1], p[i * 4 + 2]];
    let original = d.palette(ViewMode::Original, &[], &[]);

    let countries = d.palette(ViewMode::Countries, &[], &[]);
    assert_eq!(color(&countries, 0), [200, 10, 10]);
    assert_eq!(color(&countries, 2), [200, 10, 10]);
    assert_ne!(color(&countries, 3), color(&original, 3), "a province whose state is in no country is dimmed");
    assert_ne!(color(&countries, 3), [200, 10, 10]);

    let regions = d.palette(ViewMode::Regions, &[], &[]);
    assert_eq!((color(&regions, 0), color(&regions, 3)), ([10, 10, 200], [10, 10, 200]));
    assert_ne!(color(&regions, 2), color(&original, 2), "S2 is in no region");

    // With nothing defined, the country view shows the source colors.
    let bare = grid(2, 1, 4);
    assert_eq!(bare.palette(ViewMode::Countries, &[], &[]), bare.palette(ViewMode::Original, &[], &[]));
    assert_eq!(bare.palette(ViewMode::Regions, &[], &[]), bare.palette(ViewMode::Original, &[], &[]));
}

#[test]
fn totals_for_a_country() {
    let (mut d, _, c, r) = layered();
    d.provinces.set_kind(&[2], Kind::Sea).unwrap();
    d.provinces.set_population(0, Some(100)).unwrap();
    d.provinces.set_population(1, Some(50)).unwrap();
    let t = d.group_stats(COUNTRY, c).unwrap();
    assert_eq!((t.provinces, t.land, t.sea, t.pixels, t.population, t.populated), (3, 2, 1, 192, 150, 2));
    assert_eq!(d.group_stats(REGION, r).unwrap().provinces, 4);
    assert!(d.group_stats(COUNTRY, 77).is_none());
    assert_eq!(d.province_stats(&[0, 1, 99]).provinces, 2, "unknown ids are ignored");
}

// ------------------------------------------------------------ box selection

/// The province of every pixel, found by matching colors.
fn pixel_ids(d: &Document, colors: &[u32]) -> Vec<u32> {
    colors
        .iter()
        .map(|c| d.mesh.provinces.iter().find(|p| p.color == [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).unwrap().id)
        .collect()
}

#[test]
fn a_box_selects_the_provinces_it_touches_or_only_those_inside_it() {
    let d = grid(3, 3, 8); // 24 x 24, ids row by row
    let touch = |x0, y0, x1, y1| d.mesh.provinces_in_rect(x0, y0, x1, y1, false);
    let whole = |x0, y0, x1, y1| d.mesh.provinces_in_rect(x0, y0, x1, y1, true);
    assert_eq!(touch(1.5, 1.5, 6.5, 6.5), vec![0], "inside one province");
    assert_eq!(whole(1.5, 1.5, 6.5, 6.5), Vec::<u32>::new(), "no province lies wholly inside that box");
    assert_eq!(touch(0.5, 0.5, 15.5, 7.5), vec![0, 1]);
    assert_eq!(whole(-1.0, -1.0, 16.5, 8.5), vec![0, 1]);
    assert_eq!(touch(7.5, 7.5, 8.5, 8.5), vec![0, 1, 3, 4], "over a four-way corner");
    assert_eq!(touch(8.5, 0.5, 7.5, 20.5), vec![0, 1, 3, 4, 6, 7], "corner order does not matter");
    assert_eq!(touch(-50.0, -50.0, -10.0, -10.0), Vec::<u32>::new(), "outside the map");
    assert_eq!(touch(-5.0, -5.0, 99.0, 99.0), (0..9).collect::<Vec<u32>>(), "a box larger than the map");
    assert_eq!(whole(-5.0, -5.0, 99.0, 99.0), (0..9).collect::<Vec<u32>>());
}

#[test]
fn a_box_inside_an_enclave_selects_the_enclave_not_its_surroundings() {
    let mut px = vec![0xff0000u32; 81];
    for y in 3..6 {
        for x in 3..6 {
            px[y * 9 + x] = 0x0000ff;
        }
    }
    let d = doc_from(&px, 9, 9, &Options::exact());
    let blue = d.mesh.provinces.iter().find(|p| p.color == [0, 0, 255]).unwrap().id;
    assert_eq!(d.mesh.provinces_in_rect(3.5, 3.5, 5.5, 5.5, false), vec![blue]);
    // A box across the enclave's border touches both.
    assert_eq!(d.mesh.provinces_in_rect(2.5, 3.5, 3.5, 4.5, false).len(), 2);
}

#[test]
fn box_selection_agrees_with_a_pixel_by_pixel_count() {
    let (w, h) = (90u32, 70u32);
    let colors = blobby(w, h, 16, 5);
    let d = doc_from(&colors, w, h, &Options::exact());
    let owner = pixel_ids(&d, &colors);
    let mut seed = 99u64;
    let mut coord = |max: u32| (lcg(&mut seed) % (max as u64 * 100)) as f64 / 100.0 + 0.37; // never on a pixel edge
    for _ in 0..300 {
        let (x0, y0) = (coord(w), coord(h));
        let (x1, y1) = (x0 + coord(30), y0 + coord(30));
        // Touching: a province is in when one of its pixels overlaps the box.
        let mut expect_touch = std::collections::BTreeSet::new();
        for y in 0..h {
            for x in 0..w {
                if (x as f64 + 1.0) > x0 && (x as f64) < x1 && (y as f64 + 1.0) > y0 && (y as f64) < y1 {
                    expect_touch.insert(owner[(y * w + x) as usize]);
                }
            }
        }
        let got: Vec<u32> = d.mesh.provinces_in_rect(x0, y0, x1, y1, false);
        assert_eq!(got, expect_touch.into_iter().collect::<Vec<_>>(), "touching box ({x0}, {y0})-({x1}, {y1})");
        // Whole: the province's pixel bounds are inside the box.
        let expect_whole: Vec<u32> = d
            .mesh
            .provinces
            .iter()
            .filter(|p| p.bbox[0] as f64 >= x0 && p.bbox[2] as f64 <= x1 && p.bbox[1] as f64 >= y0 && p.bbox[3] as f64 <= y1)
            .map(|p| p.id)
            .collect();
        assert_eq!(d.mesh.provinces_in_rect(x0, y0, x1, y1, true), expect_whole, "whole box ({x0}, {y0})-({x1}, {y1})");
    }
}

#[test]
fn box_selection_on_a_smoothed_map_stays_close_to_the_pixels() {
    let (w, h) = (120u32, 90u32);
    let colors = blobby(w, h, 20, 8);
    let d = doc_from(&colors, w, h, &Options::default());
    // Smoothing moves a border by about a pixel, so a box a few pixels inside one province
    // still selects exactly that province.
    for p in d.mesh.provinces.iter().filter(|p| p.pixel_count > 400) {
        let owner = |x: u32, y: u32| colors[(y * w + x) as usize];
        let want = [(p.color[0] as u32) << 16 | (p.color[1] as u32) << 8 | p.color[2] as u32][0];
        // Find a pixel whose surroundings (3 px) all belong to p.
        'search: for y in 4..h - 4 {
            for x in 4..w - 4 {
                if (y - 3..=y + 3).all(|yy| (x - 3..=x + 3).all(|xx| owner(xx, yy) == want)) {
                    let got = d.mesh.provinces_in_rect(x as f64 - 1.0, y as f64 - 1.0, x as f64 + 2.0, y as f64 + 2.0, false);
                    assert_eq!(got, vec![p.id]);
                    break 'search;
                }
            }
        }
    }
}

#[test]
fn filters_keep_the_right_provinces() {
    let mut d = grid(3, 2, 4); // ids 0..6
    d.states.create("S", &[0, 1]).unwrap();
    d.provinces.set_kind(&[1, 4, 5], Kind::Sea).unwrap();
    let f = |ids: &[u32], flags| d.filter_provinces(ids, flags);
    let all = [5, 3, 1, 0, 2, 4, 3, 99];
    assert_eq!(f(&all, 0), vec![0, 1, 2, 3, 4, 5], "sorted, without repeats or unknown ids");
    assert_eq!(f(&all, filter::SKIP_IN_STATES), vec![2, 3, 4, 5]);
    assert_eq!(f(&all, filter::LAND_ONLY), vec![0, 2, 3]);
    assert_eq!(f(&all, filter::SEA_ONLY), vec![1, 4, 5]);
    assert_eq!(f(&all, filter::SKIP_IN_STATES | filter::SEA_ONLY), vec![4, 5]);
    assert_eq!(f(&all, filter::LAND_ONLY | filter::SEA_ONLY), Vec::<u32>::new(), "no province is both");
    assert_eq!(d.unassigned_provinces(), vec![2, 3, 4, 5]);
    // A box with filters.
    assert_eq!(d.provinces_in_rect(-1.0, -1.0, 99.0, 99.0, false, filter::SKIP_IN_STATES | filter::LAND_ONLY), vec![2, 3]);
}

#[test]
fn a_box_over_a_big_map_is_fast() {
    let (w, h) = (600u32, 400u32);
    let colors = blobby(w, h, 400, 3);
    let d = doc_from(&colors, w, h, &Options::default());
    let t = std::time::Instant::now();
    for i in 0..100 {
        let o = (i * 3) as f64;
        d.mesh.provinces_in_rect(50.0 + o, 40.0 + o, 350.0 + o, 250.0 + o, false);
    }
    assert!(t.elapsed().as_millis() < 1500, "100 boxes took {:?}", t.elapsed());
}
