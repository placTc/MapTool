//! A map being worked on: the immutable mesh plus everything the user edits.
//!
//! Saved as a container:
//!
//!   `MTMP`, format version (u32), mesh section length (u32), mesh section, edits
//!
//! The mesh never changes after creation, while edits (states, province metadata)
//! change all the time. So callers that keep the two apart can rewrite only the small
//! edits blob ([`Document::edits_to_bytes`]) and put it back with
//! [`Document::set_edits_from_bytes`].

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::mesh::{Reader, bad};
use crate::provinces::{Biome, Kind};
use crate::{Error, MapMesh, ProvinceTable, StateSet};

const MAGIC: &[u8; 4] = b"MTMP";
const FORMAT_VERSION: u32 = 1;
/// Edits version 1 had no map name, and stored the province data to the end.
const EDITS_VERSION: u32 = 2;
const MAX_MAP_NAME_CHARS: usize = 100;

/// Whether `bytes` look like a saved map (as opposed to an image file).
pub fn is_map_file(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// How the map is colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ViewMode {
    /// The colors of the source image.
    Original = 0,
    /// Provinces in a state take its color; the rest are dimmed once any state exists.
    States = 1,
    /// Sea and land.
    Type = 2,
    /// By biome; sea provinces are blue.
    Biome = 3,
}

impl ViewMode {
    pub fn from_u8(v: u8) -> Option<ViewMode> {
        match v {
            0 => Some(ViewMode::Original),
            1 => Some(ViewMode::States),
            2 => Some(ViewMode::Type),
            3 => Some(ViewMode::Biome),
            _ => None,
        }
    }
}

const SEA: [u8; 3] = [58, 108, 168];
const LAND: [u8; 3] = [196, 184, 132];
const DIM: [u8; 3] = [90, 90, 96];
const SELECTED: [u8; 3] = [255, 212, 0];
const HOVERED: [u8; 3] = [255, 255, 255];
const NO_PROVINCE: u32 = u32::MAX;
/// Group ids at or above this belong to a lone unassigned province (`GROUP_BASE | id`);
/// state ids are below it.
const GROUP_BASE: u32 = 0x8000_0000;

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

/// Totals for a state, for display.
#[derive(Clone, Debug, PartialEq)]
pub struct StateStats {
    pub provinces: u32,
    pub land: u32,
    pub sea: u32,
    /// Area in image pixels.
    pub pixels: u64,
    /// Sum of the populations that are set.
    pub population: u64,
    /// How many provinces have a population set.
    pub populated: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub mesh: MapMesh,
    pub states: StateSet,
    pub provinces: ProvinceTable,
    /// The map's own name; empty when it has none. Saved with the edits.
    name: String,
    /// For every border segment (a pair in `mesh.line_indices`): the province it
    /// belongs to, and the province on its other side (`NO_PROVINCE` at the image
    /// edge). Derived from the mesh, never saved.
    segment_province: Vec<u32>,
    segment_mate: Vec<u32>,
}

/// A fast hasher for the millions of small integer keys in `segment_owners`. The std
/// hasher is built to resist hostile keys, which these (our own coordinates) are not.
#[derive(Default)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u32(b as u32);
        }
    }

    fn write_u32(&mut self, v: u32) {
        self.0 = (self.0.rotate_left(5) ^ v as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A border segment as its two endpoints' coordinate bits.
type SegmentKey = ((u32, u32), (u32, u32));

/// Which province owns each border segment, and which lies across it.
fn segment_owners(mesh: &MapMesh) -> (Vec<u32>, Vec<u32>) {
    let n = mesh.line_indices.len() / 2;
    let mut province = vec![NO_PROVINCE; n];
    for (id, r) in mesh.line_ranges.iter().enumerate() {
        province[(r[0] / 2) as usize..((r[0] + r[1]) / 2) as usize].fill(id as u32);
    }
    let point = |i: u32| (mesh.line_positions[i as usize * 2].to_bits(), mesh.line_positions[i as usize * 2 + 1].to_bits());
    // Neighbouring provinces share bit-identical border points, so a segment's
    // reverse belongs to the province across the border.
    let mut owner: HashMap<SegmentKey, u32, BuildHasherDefault<FastHasher>> =
        HashMap::with_capacity_and_hasher(n, Default::default());
    for (k, pair) in mesh.line_indices.as_chunks::<2>().0.iter().enumerate() {
        owner.insert((point(pair[0]), point(pair[1])), province[k]);
    }
    let mate = mesh
        .line_indices
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| owner.get(&(point(pair[1]), point(pair[0]))).copied().unwrap_or(NO_PROVINCE))
        .collect();
    (province, mate)
}

impl Document {
    /// A fresh document: no states, every province land with default metadata.
    pub fn new(mesh: MapMesh) -> Document {
        let n = mesh.provinces.len();
        let (segment_province, segment_mate) = segment_owners(&mesh);
        Document { mesh, states: StateSet::new(n), provinces: ProvinceTable::new(n), name: String::new(), segment_province, segment_mate }
    }

    /// The map's name, or an empty string when it has none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Name the map. Blank clears it; long names are cut.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.trim().chars().take(MAX_MAP_NAME_CHARS).collect();
    }

    /// What `province` is grouped with in the state view: its state, or itself if it
    /// belongs to none.
    fn group(&self, province: u32) -> u32 {
        self.states.state_of(province).unwrap_or(GROUP_BASE | province)
    }

    /// The provinces that highlight together with `province` in the state view: all
    /// of its state, or just itself when it is in no state.
    pub fn group_provinces(&self, province: u32) -> Vec<u32> {
        match self.states.state_of(province).and_then(|id| self.states.get(id)) {
            Some(s) => s.provinces.clone(),
            None if (province as usize) < self.mesh.provinces.len() => vec![province],
            None => Vec::new(),
        }
    }

    /// Border segments for the state view: every border except those between two
    /// provinces of the same state. Index pairs into `mesh.line_positions`.
    pub fn state_border_indices(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for (k, pair) in self.mesh.line_indices.as_chunks::<2>().0.iter().enumerate() {
            let (p, m) = (self.segment_province[k], self.segment_mate[k]);
            if m != NO_PROVINCE && self.group(p) == self.group(m) {
                continue;
            }
            out.extend_from_slice(pair);
        }
        out
    }

    /// Totals for state `id`, or `None` if there is no such state.
    pub fn state_stats(&self, id: u32) -> Option<StateStats> {
        let state = self.states.get(id)?;
        let mut stats = StateStats { provinces: 0, land: 0, sea: 0, pixels: 0, population: 0, populated: 0 };
        for &p in &state.provinces {
            let meta = self.provinces.get(p)?;
            stats.provinces += 1;
            match meta.kind {
                Kind::Land => stats.land += 1,
                Kind::Sea => stats.sea += 1,
            }
            stats.pixels += self.mesh.provinces[p as usize].pixel_count as u64;
            if let Some(pop) = meta.population {
                stats.population += pop;
                stats.populated += 1;
            }
        }
        Some(stats)
    }

    /// RGBA palette (four bytes per province) for drawing: `selected` provinces are
    /// tinted yellow and `hovered` ones white.
    pub fn palette(&self, mode: ViewMode, selected: &[u32], hovered: &[u32]) -> Vec<u8> {
        let mut colors: Vec<[u8; 3]> = self
            .mesh
            .provinces
            .iter()
            .map(|p| match mode {
                ViewMode::Original => p.color,
                ViewMode::States => match self.states.state_of(p.id).and_then(|id| self.states.get(id)) {
                    Some(s) => s.color,
                    None if self.states.is_empty() => p.color,
                    None => mix(p.color, DIM, 0.6),
                },
                ViewMode::Type | ViewMode::Biome => {
                    let meta = self.provinces.get(p.id).cloned().unwrap_or_default();
                    match (meta.kind, mode) {
                        (Kind::Sea, _) => SEA,
                        (Kind::Land, ViewMode::Type) => LAND,
                        (Kind::Land, _) => meta.biome().color(),
                    }
                }
            })
            .collect();
        for (ids, tint, amount) in [(selected, SELECTED, 0.45), (hovered, HOVERED, 0.35)] {
            for &id in ids {
                if let Some(c) = colors.get_mut(id as usize) {
                    *c = mix(*c, tint, amount);
                }
            }
        }
        colors.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect()
    }

    /// The map name, states and province metadata, without the mesh.
    ///
    ///   version, states length, states, province data length, province data,
    ///   name length, name (UTF-8)                              u32 lengths
    pub fn edits_to_bytes(&self) -> Vec<u8> {
        let states = self.states.to_bytes();
        let provinces = self.provinces.to_bytes();
        let mut out = Vec::with_capacity(16 + states.len() + provinces.len() + self.name.len());
        out.extend_from_slice(&EDITS_VERSION.to_le_bytes());
        out.extend_from_slice(&(states.len() as u32).to_le_bytes());
        out.extend_from_slice(&states);
        out.extend_from_slice(&(provinces.len() as u32).to_le_bytes());
        out.extend_from_slice(&provinces);
        out.extend_from_slice(&(self.name.len() as u32).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out
    }

    /// Replace the name, states and province metadata. Nothing changes if `bytes`
    /// are invalid. Edits saved before maps had names (version 1) still load, and
    /// leave the map unnamed.
    pub fn set_edits_from_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let n = self.mesh.provinces.len();
        let mut r = Reader { data: bytes, pos: 0 };
        let version = r.u32()?;
        if version != 1 && version != EDITS_VERSION {
            return Err(bad(format!("edits version {version}, this build reads versions 1 and {EDITS_VERSION}")));
        }
        let states_len = r.u32()? as usize;
        let states = StateSet::from_bytes(r.take(states_len)?, n)?;
        let (provinces, name) = if version == 1 {
            (ProvinceTable::from_bytes(&bytes[r.pos..], n)?, String::new())
        } else {
            let len = r.u32()? as usize;
            let provinces = ProvinceTable::from_bytes(r.take(len)?, n)?;
            let name_len = r.u32()? as usize;
            let name = std::str::from_utf8(r.take(name_len)?).map_err(|_| bad("the map name is not valid UTF-8"))?.to_string();
            if r.pos != bytes.len() {
                return Err(bad("unexpected trailing data"));
            }
            if name.chars().count() > MAX_MAP_NAME_CHARS {
                return Err(bad("the map name is too long"));
            }
            (provinces, name)
        };
        self.states = states;
        self.provinces = provinces;
        self.name = name;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let section = self.mesh.encode();
        let edits = self.edits_to_bytes();
        let mut out = Vec::with_capacity(12 + section.len() + edits.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(section.len() as u32).to_le_bytes());
        out.extend_from_slice(&section);
        out.extend_from_slice(&edits);
        out
    }

    /// Read a map saved by [`Document::to_bytes`]. Never panics on bad input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Document, Error> {
        if !is_map_file(bytes) {
            return Err(bad("not a MapTool map file"));
        }
        let word = |at: usize| -> Result<u32, Error> {
            let b = bytes.get(at..at + 4).ok_or_else(|| bad("truncated"))?;
            Ok(u32::from_le_bytes(b.try_into().unwrap()))
        };
        let version = word(4)?;
        if version != FORMAT_VERSION {
            return Err(bad(format!("format version {version}, this build reads version {FORMAT_VERSION}")));
        }
        let len = word(8)? as usize;
        let end = 12usize.checked_add(len).filter(|&e| e <= bytes.len()).ok_or_else(|| bad("truncated"))?;
        let mut doc = Document::new(MapMesh::decode(&bytes[12..end])?);
        doc.set_edits_from_bytes(&bytes[end..])?;
        Ok(doc)
    }
}

/// Biome names in code order, for building menus.
pub fn biome_names() -> Vec<&'static str> {
    Biome::ALL.iter().map(|b| b.name()).collect()
}
