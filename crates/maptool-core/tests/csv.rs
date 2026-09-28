use maptool_core::{Biome, Document, Error, Kind, Options, PixelFormat, mesh};

/// A grid of `cols` x `rows` blocks of `b` x `b` pixels, one province per block, colored by `code(index)`.
fn grid_by(cols: u32, rows: u32, b: u32, code: impl Fn(u32) -> u32) -> Document {
    let px: Vec<u8> = (0..cols * b * rows * b)
        .flat_map(|i| {
            let (x, y) = (i % (cols * b), i / (cols * b));
            let c = code((y / b) * cols + x / b);
            [(c >> 16) as u8, (c >> 8) as u8, c as u8]
        })
        .collect();
    let opts = Options { validate: false, ..Options::exact() };
    Document::new(mesh(&px, cols * b, rows * b, PixelFormat::Rgb, &opts, 0.05).unwrap())
}

fn grid(cols: u32, rows: u32, b: u32) -> Document {
    grid_by(cols, rows, b, |i| 0x010203 * (i + 1))
}

/// The colors of a small grid map's provinces, as `#rrggbb`: #010203, #020406, #030609, ...
fn color(i: u32) -> String {
    format!("#{:06x}", 0x010203 * (i + 1))
}

fn three() -> Document {
    grid(3, 1, 4)
}

fn number(d: &Document, id: u32) -> u32 {
    d.provinces.number(id)
}

fn kind(d: &Document, id: u32) -> Kind {
    d.provinces.get(id).unwrap().kind
}

#[test]
fn colors_types_and_ids_are_applied() {
    let mut d = three();
    let csv = format!("color,type,id\n{},land,10\n{},sea,20\n{},land,30\n", color(0), color(1), color(2));
    let r = d.import_csv(&csv).unwrap();
    assert_eq!((r.rows, r.matched, r.land, r.sea, r.unlisted, r.problem_count), (3, 3, 2, 1, 0, 0));
    assert!(r.has_types && r.has_ids);
    assert_eq!([number(&d, 0), number(&d, 1), number(&d, 2)], [10, 20, 30]);
    assert_eq!([kind(&d, 0), kind(&d, 1), kind(&d, 2)], [Kind::Land, Kind::Sea, Kind::Land]);
    // The number is what an unnamed province is called, and an explicit name still wins.
    assert_eq!(d.provinces.display_name(1), "20");
    d.provinces.set_name(1, "Gulf").unwrap();
    assert_eq!(d.provinces.display_name(1), "Gulf");
    assert_eq!(number(&d, 1), 20);
}

#[test]
fn a_sea_province_from_a_csv_is_locked_to_the_sea_biome() {
    let mut d = three();
    d.provinces.set_biome(&[1], Biome::Forest).unwrap();
    d.import_csv(&format!("{},sea,1\n", color(1))).unwrap();
    assert_eq!(d.provinces.get(1).unwrap().biome(), Biome::Sea);
}

#[test]
fn many_formats_are_understood() {
    let want = |d: &Document| {
        assert_eq!([number(d, 0), number(d, 1), number(d, 2)], [7, 8, 9]);
        assert_eq!([kind(d, 0), kind(d, 1), kind(d, 2)], [Kind::Sea, Kind::Land, Kind::Sea]);
    };
    let (a, b, c) = (color(0), color(1), color(2));
    let bare = |s: &str| s.trim_start_matches('#').to_string();
    let variants = [
        // Semicolons, CRLF line ends, upper case, a header with odd names.
        format!("Colour;Province Type;Province ID\r\n{};SEA;7\r\n{};Land;8\r\n{};sea;9\r\n", a.to_uppercase(), b.to_uppercase(), c.to_uppercase()),
        // Tabs, no header, no '#', mixed case type.
        format!("{}\tsea\t7\n{}\tLAND\t8\n{}\tSea\t9\n", bare(&a), bare(&b), bare(&c)),
        // 0x prefixes and quoted fields, with a byte order mark and a blank line.
        format!("\u{feff}\"0x{}\",\"sea\",\"7\"\n\n\"0x{}\",\"land\",\"8\"\n\"0x{}\",\"sea\",\"9\"\n", bare(&a), bare(&b), bare(&c)),
        // Other column orders, with and without a header.
        format!("type,id,hex\nsea,7,{a}\nland,8,{b}\nsea,9,{c}\n"),
        format!("sea,{a},7\nland,{b},8\nsea,{c},9\n"),
        format!("7;sea;{a}\n8;land;{b}\n9;sea;{c}"),
        // Spaces around fields, and extra columns nobody asked for.
        format!("id , name , color , type\n 7 , x , {a} , sea\n 8 , y , {b} , land\n 9 , z , {c} , sea\n"),
    ];
    for (i, csv) in variants.iter().enumerate() {
        let mut d = three();
        let r = d.import_csv(csv).unwrap_or_else(|e| panic!("variant {i}: {e}"));
        assert_eq!((r.matched, r.problem_count), (3, 0), "variant {i}: {:?}", r.problems);
        want(&d);
    }
}

#[test]
fn a_quoted_field_may_hold_the_delimiter() {
    let mut d = three();
    let csv = format!("id,name,color,type\n5,\"Ostmark, east\",{},land\n", color(0));
    let r = d.import_csv(&csv).unwrap();
    assert_eq!(r.matched, 1);
    assert_eq!(number(&d, 0), 5);
}

#[test]
fn either_the_type_or_the_id_may_be_missing() {
    let mut d = three();
    let r = d.import_csv(&format!("{},40\n{},50\n", color(0), color(1))).unwrap();
    assert!(r.has_ids && !r.has_types);
    assert_eq!((number(&d, 0), number(&d, 1), number(&d, 2)), (40, 50, 2));
    assert_eq!(kind(&d, 0), Kind::Land, "types are left alone without a type column");

    let mut d = three();
    let r = d.import_csv(&format!("type,color\nsea,{}\n", color(2))).unwrap();
    assert!(r.has_types && !r.has_ids);
    assert_eq!(kind(&d, 2), Kind::Sea);
    assert_eq!(number(&d, 2), 2, "ids are left alone without an id column");
}

#[test]
fn bad_rows_are_skipped_and_reported_and_the_rest_applied() {
    let mut d = three();
    let csv = format!(
        "color,type,id\n\
         {},land,1\n\
         nothex,land,2\n\
         {},lake,3\n\
         {},sea,notanumber\n\
         #ffffff,land,4\n\
         {},sea,5\n\
         {},land,1\n\
         {},sea,6\n",
        color(0),
        color(1),
        color(1),
        color(0), // a repeat of the first row's color
        color(2), // valid, but its ID 1 is already taken
        color(2),
    );
    // Rows: 1 ok; 2 bad color; 3 bad type; 4 bad id; 5 color not in map; 6 repeats color 0;
    //       7 ID 1 taken; 8 ok.
    let r = d.import_csv(&csv).unwrap();
    assert_eq!((r.rows, r.matched, r.problem_count), (8, 2, 6), "{:#?}", r.problems);
    assert_eq!(r.problems.len(), 6);
    let joined = r.problems.join("\n");
    for expect in [
        "row 3: \"nothex\" is not a hex color",
        "row 4: \"lake\" is not a province type",
        "row 5: \"notanumber\" is not a whole number",
        "row 6: #ffffff is not a color in this map",
        "row 7: #010203 appears again (first in row 2)",
        "row 8: ID 1 is used again (first in row 2)",
    ] {
        assert!(joined.contains(expect), "missing {expect:?} in:\n{joined}");
    }
    assert_eq!((number(&d, 0), number(&d, 2)), (1, 6));
    assert_eq!(number(&d, 1), 1, "province 1 was never applied, so it keeps its position");
    assert_eq!(kind(&d, 2), Kind::Sea);
    assert_eq!(r.unlisted, 1);
}

#[test]
fn nothing_changes_when_no_row_applies() {
    let mut d = three();
    d.provinces.set_name(0, "Keep me").unwrap();
    let before = d.clone();
    let err = d.import_csv("color,type,id\n#ffffff,land,1\n#eeeeee,sea,2\n").unwrap_err();
    assert!(matches!(err, Error::Edit(_)));
    assert!(err.to_string().contains("none of the 2 rows"), "{err}");
    assert_eq!(d, before);
}

#[test]
fn files_it_cannot_read_are_refused_with_a_reason() {
    for (csv, expect) in [
        ("", "empty"),
        ("\n\n  \n", "empty"),
        ("name,population\nAlpha,10\n", "hex colors"),
        (&*format!("{}\n{}\n", color(0), color(1)), "type column"),
    ] {
        let mut d = three();
        let before = d.clone();
        let err = d.import_csv(csv).unwrap_err().to_string();
        assert!(err.contains(expect), "{csv:?} gave {err}");
        assert_eq!(d, before);
    }
}

#[test]
fn importing_again_replaces_ids_but_keeps_types_it_does_not_mention() {
    let mut d = three();
    d.import_csv(&format!("{},sea,10\n{},sea,20\n{},land,30\n", color(0), color(1), color(2))).unwrap();
    d.import_csv(&format!("{},land,99\n", color(2))).unwrap();
    assert_eq!(number(&d, 2), 99);
    assert_eq!(number(&d, 0), 0, "old ids are gone");
    assert_eq!(number(&d, 1), 1);
    assert_eq!(kind(&d, 0), Kind::Sea, "a type the new file does not mention stays");
}

#[test]
fn ids_and_types_survive_saving() {
    let mut d = three();
    d.import_csv(&format!("{},sea,1001\n{},land,1002\n", color(0), color(1))).unwrap();
    let back = Document::from_bytes(&d.to_bytes()).unwrap();
    assert_eq!(back, d);
    assert_eq!((number(&back, 0), number(&back, 1)), (1001, 1002));
    let mut fresh = Document::new(d.mesh.clone());
    fresh.set_edits_from_bytes(&d.edits_to_bytes()).unwrap();
    assert_eq!(fresh, d);
}

#[test]
fn many_problems_are_counted_but_only_some_are_listed() {
    let mut d = three();
    let mut csv = format!("color,type,id\n{},land,1\n", color(0));
    for i in 0..1000 {
        csv.push_str(&format!("bad{i},land,{}\n", i + 2));
    }
    let r = d.import_csv(&csv).unwrap();
    assert_eq!((r.matched, r.problem_count, r.problems.len()), (1, 1000, 200));
}

#[test]
fn junk_never_panics() {
    let mut seed = 12345u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    let pieces = ["#", "0x", ",", ";", "\t", "\n", "\r\n", "\"", "\"\"", "land", "sea", "id", "color", "#010203", "010203", " ", "9999999999", "-1", "é", "\u{feff}", "🙂"];
    for _ in 0..500 {
        let text: String = (0..(next() % 40)).map(|_| pieces[next() % pieces.len()]).collect();
        let mut d = three();
        let before = d.clone();
        if d.import_csv(&text).is_err() {
            assert_eq!(d, before, "a refused import changed the document: {text:?}");
        }
    }
}

#[test]
fn a_large_csv_is_quick() {
    let mut d = grid_by(60, 40, 3, |i| i + 1); // 2400 provinces, colors #000001, #000002, ...
    let mut csv = String::from("color,type,id\n");
    for i in 0..2400u32 {
        csv.push_str(&format!("#{:06x},{},{}\n", i + 1, if i % 3 == 0 { "sea" } else { "land" }, 100_000 + i));
    }
    let t = std::time::Instant::now();
    let r = d.import_csv(&csv).unwrap();
    assert_eq!((r.matched, r.sea, r.land, r.problem_count), (2400, 800, 1600, 0));
    assert!(t.elapsed().as_millis() < 500, "took {:?}", t.elapsed());
    assert_eq!(number(&d, 2399), 102_399);
}
