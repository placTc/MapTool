//! Importing province numbers and types from a CSV: a hex color per row, matched
//! against the provinces' source colors, with the province's type (land or sea)
//! and its own number (ID).

use std::collections::HashMap;

use crate::provinces::Kind;
use crate::{Document, Error};

/// How many problems are kept as text; the rest are only counted.
const MAX_PROBLEMS: usize = 200;
/// Rows looked at when working out which column is which.
const SAMPLE_ROWS: usize = 200;

/// What an import did.
#[derive(Clone, Debug, PartialEq)]
pub struct CsvReport {
    /// Data rows read (not counting a header).
    pub rows: usize,
    /// Rows applied to a province.
    pub matched: usize,
    /// Of those, how many set land or sea (zero without a type column).
    pub land: usize,
    pub sea: usize,
    /// Provinces of the map that no applied row mentioned.
    pub unlisted: usize,
    /// Whether the CSV had a type column, and an ID column.
    pub has_types: bool,
    pub has_ids: bool,
    /// Every problem found, one line each, at most 200.
    pub problems: Vec<String>,
    /// The true number of problems, which may be larger than `problems.len()`.
    pub problem_count: usize,
}

fn fail(why: impl Into<String>) -> Error {
    Error::Edit(why.into())
}

/// Split CSV text into records of fields. Fields may be quoted (`""` is a quote inside
/// one). Blank lines are skipped and unquoted fields are trimmed.
fn records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false; // inside quotes
    let mut was_quoted = false; // this field had quotes, so keep its spaces
    let mut chars = text.chars().peekable();

    let end_field = |field: &mut String, was_quoted: &mut bool, record: &mut Vec<String>| {
        record.push(if *was_quoted { std::mem::take(field) } else { std::mem::take(field).trim().to_string() });
        *was_quoted = false;
    };
    let end_record = |record: &mut Vec<String>, out: &mut Vec<Vec<String>>| {
        if record.iter().any(|f| !f.is_empty()) {
            out.push(std::mem::take(record));
        } else {
            record.clear();
        }
    };

    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.trim().is_empty() {
            field.clear();
            quoted = true;
            was_quoted = true;
        } else if c == delimiter {
            end_field(&mut field, &mut was_quoted, &mut record);
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            end_field(&mut field, &mut was_quoted, &mut record);
            end_record(&mut record, &mut out);
        } else {
            field.push(c);
        }
    }
    end_field(&mut field, &mut was_quoted, &mut record);
    end_record(&mut record, &mut out);
    out
}

/// The delimiter used by the first few lines: the one of `,` `;` and tab that occurs most
/// outside quotes. Comma when none does.
fn detect_delimiter(text: &str) -> char {
    let mut counts = [0usize; 3];
    let mut quoted = false;
    let mut lines = 0;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => counts[0] += 1,
            ';' if !quoted => counts[1] += 1,
            '\t' if !quoted => counts[2] += 1,
            '\n' if !quoted => {
                lines += 1;
                if lines >= 5 {
                    break;
                }
            }
            _ => {}
        }
    }
    let best = counts.iter().enumerate().max_by_key(|&(i, &n)| (n, std::cmp::Reverse(i))).unwrap();
    if *best.1 == 0 { ',' } else { [',', ';', '\t'][best.0] }
}

/// `#1a2b3c`, `1A2B3C` or `0x1a2b3c`.
fn hex_color(text: &str) -> Option<[u8; 3]> {
    let t = text.trim();
    let digits = t.strip_prefix('#').or_else(|| t.strip_prefix("0x")).or_else(|| t.strip_prefix("0X")).unwrap_or(t);
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(digits, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// True when the text is unmistakably a hex color rather than a number: it has a prefix
/// or a letter digit.
fn looks_like_hex(text: &str) -> bool {
    let t = text.trim();
    hex_color(t).is_some() && (t.starts_with('#') || t.starts_with("0x") || t.starts_with("0X") || t.bytes().any(|b| b.is_ascii_alphabetic()))
}

fn kind_of(text: &str) -> Option<Kind> {
    match text.trim().to_ascii_lowercase().as_str() {
        "land" => Some(Kind::Land),
        "sea" => Some(Kind::Sea),
        _ => None,
    }
}

fn id_of(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

fn rgb_text(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Color,
    Kind,
    Id,
}

/// A header cell's meaning, ignoring case, spaces, underscores and hyphens.
fn header_meaning(cell: &str) -> Option<Column> {
    let key: String = cell.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    match key.as_str() {
        "color" | "colour" | "hex" | "hexcolor" | "hexcolour" | "rgb" => Some(Column::Color),
        "type" | "kind" | "provincetype" | "terrain" => Some(Column::Kind),
        "id" | "number" | "no" | "provinceid" | "provincenumber" | "province" => Some(Column::Id),
        _ => None,
    }
}

impl Document {
    /// Apply a CSV that gives, per row, a hex color, a province type (`land` or `sea`) and
    /// the province's own number (ID). Rows are matched to provinces by their source color.
    ///
    /// The columns may come in any order and be separated by commas, semicolons or tabs.
    /// A header row is used if there is one; otherwise the columns are worked out from the
    /// data. Either the type or the ID column may be missing.
    ///
    /// A row with a bad value, a color that is not in the map, or a color or ID that an
    /// earlier row already used is skipped and reported; the rest are applied. If no row
    /// can be applied, nothing changes and an error explains why. Importing replaces all
    /// earlier IDs; types only change for provinces the CSV lists.
    pub fn import_csv(&mut self, text: &str) -> Result<CsvReport, Error> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let rows = records(text, detect_delimiter(text));
        let Some(first) = rows.first() else {
            return Err(fail("the CSV is empty"));
        };

        // A first row with no hex color in it is a header.
        let header = !first.iter().any(|f| hex_color(f).is_some());
        let data = &rows[usize::from(header)..];
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);

        let mut by_header: HashMap<usize, Column> = HashMap::new();
        if header {
            for (i, cell) in first.iter().enumerate() {
                if let Some(c) = header_meaning(cell) {
                    by_header.entry(i).or_insert(c);
                }
            }
        }
        let named = |want: Column| (0..width).find(|i| by_header.get(i) == Some(&want));

        // Work out the rest from the data itself.
        let sample = &data[..data.len().min(SAMPLE_ROWS)];
        let rate = |col: usize, test: &dyn Fn(&str) -> bool| -> f64 {
            let cells: Vec<_> = sample.iter().filter_map(|r| r.get(col)).collect();
            if cells.is_empty() { 0.0 } else { cells.iter().filter(|c| test(c)).count() as f64 / cells.len() as f64 }
        };
        let color_col = named(Column::Color).or_else(|| {
            // Prefer columns that are clearly hex (a `#` or a letter) over ones that just have six digits.
            (0..width)
                .map(|c| (c, rate(c, &|s| hex_color(s).is_some()), rate(c, &|s| looks_like_hex(s))))
                .filter(|&(_, any, _)| any >= 0.9)
                .max_by(|a, b| (a.2 + a.1).total_cmp(&(b.2 + b.1)).then(b.0.cmp(&a.0)))
                .map(|(c, _, _)| c)
        });
        let Some(color_col) = color_col else {
            return Err(fail("could not find a column of hex colors (like #1a2b3c)"));
        };
        let kind_col = named(Column::Kind)
            .or_else(|| (0..width).find(|&c| c != color_col && rate(c, &|s| kind_of(s).is_some()) >= 0.9));
        let id_col = named(Column::Id)
            .or_else(|| (0..width).find(|&c| c != color_col && Some(c) != kind_col && rate(c, &|s| id_of(s).is_some()) >= 0.9));
        if kind_col.is_none() && id_col.is_none() {
            return Err(fail("could not find a province type column (land or sea) or an ID column next to the colors"));
        }

        // Check every row before changing anything.
        let mut province_of: HashMap<[u8; 3], u32> = HashMap::new();
        for p in &self.mesh.provinces {
            province_of.insert(p.color, p.id);
        }
        let mut problems: Vec<String> = Vec::new();
        let mut problem_count = 0;
        let mut problem = |problems: &mut Vec<String>, text: String| {
            problem_count += 1;
            if problems.len() < MAX_PROBLEMS {
                problems.push(text);
            }
        };
        let mut seen_colors: HashMap<[u8; 3], usize> = HashMap::new();
        let mut seen_ids: HashMap<u32, usize> = HashMap::new();
        let mut accepted: Vec<(u32, Option<Kind>, Option<u32>)> = Vec::new();

        for (n, row) in data.iter().enumerate() {
            let line = n + 1 + usize::from(header);
            let cell = |col: usize| row.get(col).map_or("", |s| s.as_str());
            let Some(color) = hex_color(cell(color_col)) else {
                problem(&mut problems, format!("row {line}: \"{}\" is not a hex color (like #1a2b3c)", cell(color_col)));
                continue;
            };
            let kind = match kind_col {
                None => None,
                Some(c) => match kind_of(cell(c)) {
                    Some(k) => Some(k),
                    None => {
                        problem(&mut problems, format!("row {line}: \"{}\" is not a province type (land or sea)", cell(c)));
                        continue;
                    }
                },
            };
            let id = match id_col {
                None => None,
                Some(c) => match id_of(cell(c)) {
                    Some(i) => Some(i),
                    None => {
                        problem(&mut problems, format!("row {line}: \"{}\" is not a whole number for an ID", cell(c)));
                        continue;
                    }
                },
            };
            let Some(&province) = province_of.get(&color) else {
                problem(&mut problems, format!("row {line}: {} is not a color in this map", rgb_text(color)));
                continue;
            };
            if let Some(first) = seen_colors.get(&color) {
                problem(&mut problems, format!("row {line}: {} appears again (first in row {first})", rgb_text(color)));
                continue;
            }
            if let Some(first) = id.and_then(|i| seen_ids.get(&i)) {
                problem(&mut problems, format!("row {line}: ID {} is used again (first in row {first})", id.unwrap()));
                continue;
            }
            seen_colors.insert(color, line);
            if let Some(i) = id {
                seen_ids.insert(i, line);
            }
            accepted.push((province, kind, id));
        }

        if accepted.is_empty() {
            let why = problems.first().map(|p| format!(" (first problem: {p})")).unwrap_or_default();
            return Err(fail(format!("none of the {} rows could be applied to this map{why}", data.len())));
        }

        if id_col.is_some() {
            self.provinces.clear_numbers();
        }
        let (mut land, mut sea) = (0, 0);
        for &(province, kind, id) in &accepted {
            if let Some(i) = id {
                self.provinces.set_number(province, Some(i))?;
            }
            if let Some(k) = kind {
                self.provinces.set_kind(&[province], k)?;
                match k {
                    Kind::Land => land += 1,
                    Kind::Sea => sea += 1,
                }
            }
        }
        Ok(CsvReport {
            rows: data.len(),
            matched: accepted.len(),
            land,
            sea,
            unlisted: self.mesh.provinces.len() - accepted.len(),
            has_types: kind_col.is_some(),
            has_ids: id_col.is_some(),
            problems,
            problem_count,
        })
    }
}
