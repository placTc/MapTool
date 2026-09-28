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
use crate::{Error, GroupKind, GroupSet, MapMesh, ProvinceTable, StateSet};

const MAGIC: &[u8; 4] = b"MTMP";
const FORMAT_VERSION: u32 = 1;
/// Edits version 1 had no map name and stored the province data to the end; version 2
/// added the name; version 3 added countries and strategic regions.
const EDITS_VERSION: u32 = 3;
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
    /// Provinces in a state that is in a country take its color.
    Countries = 4,
    /// Provinces in a state that is in a strategic region take its color.
    Regions = 5,
}

impl ViewMode {
    pub fn from_u8(v: u8) -> Option<ViewMode> {
        match v {
            0 => Some(ViewMode::Original),
            1 => Some(ViewMode::States),
            2 => Some(ViewMode::Type),
            3 => Some(ViewMode::Biome),
            4 => Some(ViewMode::Countries),
            5 => Some(ViewMode::Regions),
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

/// How much of the map's structure is shown as single units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Level {
    /// Every province on its own.
    Provinces = 0,
    /// States as units; provinces in no state stay single.
    States = 1,
    /// Countries as units; states in no country stay single, then provinces in no state.
    Countries = 2,
    /// Strategic regions as units, falling back the same way.
    Regions = 3,
}

impl Level {
    pub fn from_u8(v: u8) -> Option<Level> {
        match v {
            0 => Some(Level::Provinces),
            1 => Some(Level::States),
            2 => Some(Level::Countries),
            3 => Some(Level::Regions),
            _ => None,
        }
    }
}

/// The thing a province is shown, picked and outlined as, at some [`Level`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Province(u32),
    State(u32),
    Country(u32),
    Region(u32),
}

impl Unit {
    /// 0 province, 1 state, 2 country, 3 region.
    pub fn kind(self) -> u8 {
        match self {
            Unit::Province(_) => 0,
            Unit::State(_) => 1,
            Unit::Country(_) => 2,
            Unit::Region(_) => 3,
        }
    }

    pub fn id(self) -> u32 {
        match self {
            Unit::Province(i) | Unit::State(i) | Unit::Country(i) | Unit::Region(i) => i,
        }
    }
}

/// Which provinces `filter_provinces` keeps.
pub mod filter {
    /// Drop provinces that are already in a state.
    pub const SKIP_IN_STATES: u32 = 1;
    /// Keep only land provinces.
    pub const LAND_ONLY: u32 = 2;
    /// Keep only sea provinces.
    pub const SEA_ONLY: u32 = 4;
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

/// Totals over a set of provinces (a state, country or region), for display.
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
    /// Groups of states. A state is in at most one country and at most one region.
    pub countries: GroupSet,
    pub regions: GroupSet,
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
        Document {
            mesh,
            states: StateSet::new(n),
            countries: GroupSet::new(GroupKind::Country),
            regions: GroupSet::new(GroupKind::Region),
            provinces: ProvinceTable::new(n),
            name: String::new(),
            segment_province,
            segment_mate,
        }
    }

    /// The map's name, or an empty string when it has none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Name the map. Blank clears it; long names are cut.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.trim().chars().take(MAX_MAP_NAME_CHARS).collect();
    }

    /// What `province` is shown as at `level`: itself, its state, its country or its
    /// strategic region. Where it has no group at that level it falls back to the level
    /// below: a state in no country shows as the state, a province in no state as itself.
    pub fn unit(&self, province: u32, level: Level) -> Unit {
        let Some(state) = self.states.state_of(province) else {
            return Unit::Province(province);
        };
        match level {
            Level::Provinces => Unit::Province(province),
            Level::States => Unit::State(state),
            Level::Countries => self.countries.group_of(state).map_or(Unit::State(state), Unit::Country),
            Level::Regions => self.regions.group_of(state).map_or(Unit::State(state), Unit::Region),
        }
    }

    /// The provinces that highlight together with `province` at `level`: everything in its
    /// unit. Empty if there is no such province.
    pub fn group_provinces(&self, province: u32, level: Level) -> Vec<u32> {
        if province as usize >= self.mesh.provinces.len() {
            return Vec::new();
        }
        match self.unit(province, level) {
            Unit::Province(p) => vec![p],
            Unit::State(s) => self.states.get(s).map(|s| s.provinces.clone()).unwrap_or_default(),
            Unit::Country(c) => self.provinces_of_group(GroupKind::Country, c),
            Unit::Region(r) => self.provinces_of_group(GroupKind::Region, r),
        }
    }

    /// All provinces in the states of a country or region, ascending. Empty if there is
    /// no such group.
    pub fn provinces_of_group(&self, kind: GroupKind, id: u32) -> Vec<u32> {
        let Some(group) = self.groups(kind).get(id) else { return Vec::new() };
        let mut out: Vec<u32> = group.states.iter().filter_map(|&s| self.states.get(s)).flat_map(|s| s.provinces.iter().copied()).collect();
        out.sort_unstable();
        out
    }

    pub fn groups(&self, kind: GroupKind) -> &GroupSet {
        match kind {
            GroupKind::Country => &self.countries,
            GroupKind::Region => &self.regions,
        }
    }

    /// For renaming, recoloring, describing and deleting groups. To change which states a
    /// group holds, use [`Document::create_group`], [`Document::assign_to_group`] and
    /// [`Document::unassign_from_groups`], which check the states exist.
    pub fn groups_mut(&mut self, kind: GroupKind) -> &mut GroupSet {
        match kind {
            GroupKind::Country => &mut self.countries,
            GroupKind::Region => &mut self.regions,
        }
    }

    /// Create a country or region from `states`, taking them out of the group of that
    /// kind they were in. Returns its id.
    pub fn create_group(&mut self, kind: GroupKind, name: &str, states: &[u32]) -> Result<u32, Error> {
        let known = &self.states;
        let set = match kind {
            GroupKind::Country => &mut self.countries,
            GroupKind::Region => &mut self.regions,
        };
        set.create(name, states, |s| known.get(s).is_some())
    }

    /// Put `states` into a country or region, taking them out of the group of that kind
    /// they were in.
    pub fn assign_to_group(&mut self, kind: GroupKind, id: u32, states: &[u32]) -> Result<(), Error> {
        let known = &self.states;
        let set = match kind {
            GroupKind::Country => &mut self.countries,
            GroupKind::Region => &mut self.regions,
        };
        set.assign(id, states, |s| known.get(s).is_some())
    }

    /// Take `states` out of their country or region (whichever `kind` says).
    pub fn unassign_from_groups(&mut self, kind: GroupKind, states: &[u32]) -> Result<(), Error> {
        let known = &self.states;
        let set = match kind {
            GroupKind::Country => &mut self.countries,
            GroupKind::Region => &mut self.regions,
        };
        set.unassign(states, |s| known.get(s).is_some())
    }

    /// Delete a state. It also leaves its country and its strategic region.
    pub fn delete_state(&mut self, id: u32) -> Result<(), Error> {
        self.states.delete(id)?;
        self.countries.remove_state(id);
        self.regions.remove_state(id);
        Ok(())
    }

    /// Border segments to draw at `level`: every border except those between two provinces
    /// that are the same unit. Index pairs into `mesh.line_positions`.
    pub fn border_indices(&self, level: Level) -> Vec<u32> {
        let mut out = Vec::new();
        for (k, pair) in self.mesh.line_indices.as_chunks::<2>().0.iter().enumerate() {
            let (p, m) = (self.segment_province[k], self.segment_mate[k]);
            if m != NO_PROVINCE && self.unit(p, level) == self.unit(m, level) {
                continue;
            }
            out.extend_from_slice(pair);
        }
        out
    }

    /// Border segments for the state view.
    pub fn state_border_indices(&self) -> Vec<u32> {
        self.border_indices(Level::States)
    }

    /// The units, at `level`, that `provinces` belong to, as `[kind, id, kind, id, ...]`
    /// (kinds as in [`Unit::kind`]), each once, in order of first appearance. Unknown
    /// provinces are skipped.
    pub fn units_of(&self, provinces: &[u32], level: Level) -> Vec<u32> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for &p in provinces {
            if p as usize >= self.mesh.provinces.len() {
                continue;
            }
            let u = self.unit(p, level);
            if seen.insert(u.kind() as u64 | (u.id() as u64) << 8) {
                out.push(u.kind() as u32);
                out.push(u.id());
            }
        }
        out
    }

    /// The provinces of `ids` that pass `flags` (see [`filter`]), ascending, each once.
    /// Unknown ids are dropped. Asking for land only and sea only at once keeps nothing.
    pub fn filter_provinces(&self, ids: &[u32], flags: u32) -> Vec<u32> {
        let mut out: Vec<u32> = ids.iter().copied().filter(|&p| (p as usize) < self.mesh.provinces.len()).collect();
        out.sort_unstable();
        out.dedup();
        out.retain(|&p| {
            if flags & filter::SKIP_IN_STATES != 0 && self.states.state_of(p).is_some() {
                return false;
            }
            let kind = self.provinces.get(p).map(|m| m.kind);
            !(flags & filter::LAND_ONLY != 0 && kind != Some(Kind::Land) || flags & filter::SEA_ONLY != 0 && kind != Some(Kind::Sea))
        });
        out
    }

    /// The provinces in the rectangle `x0,y0`-`x1,y1` (image pixels, any corner order):
    /// those it touches, or with `whole` only those that lie entirely inside it, filtered
    /// by `flags` (see [`filter`]).
    pub fn provinces_in_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64, whole: bool, flags: u32) -> Vec<u32> {
        self.filter_provinces(&self.mesh.provinces_in_rect(x0, y0, x1, y1, whole), flags)
    }

    /// The provinces of some units, given as `[kind, id, kind, id, ...]` (kinds as in
    /// [`Unit::kind`]), ascending, each once. Unknown units are ignored.
    pub fn provinces_of_units(&self, units: &[u32]) -> Vec<u32> {
        let mut out = Vec::new();
        for pair in units.as_chunks::<2>().0 {
            match pair[0] {
                0 if (pair[1] as usize) < self.mesh.provinces.len() => out.push(pair[1]),
                1 => out.extend(self.states.get(pair[1]).map(|s| s.provinces.iter().copied()).into_iter().flatten()),
                2 => out.extend(self.provinces_of_group(GroupKind::Country, pair[1])),
                3 => out.extend(self.provinces_of_group(GroupKind::Region, pair[1])),
                _ => {}
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Provinces that are in no state.
    pub fn unassigned_provinces(&self) -> Vec<u32> {
        (0..self.mesh.provinces.len() as u32).filter(|&p| self.states.state_of(p).is_none()).collect()
    }

    /// Totals over some provinces. Unknown ids are ignored.
    pub fn province_stats(&self, ids: &[u32]) -> StateStats {
        let mut stats = StateStats { provinces: 0, land: 0, sea: 0, pixels: 0, population: 0, populated: 0 };
        for &p in ids {
            let (Some(meta), Some(info)) = (self.provinces.get(p), self.mesh.provinces.get(p as usize)) else { continue };
            stats.provinces += 1;
            match meta.kind {
                Kind::Land => stats.land += 1,
                Kind::Sea => stats.sea += 1,
            }
            stats.pixels += info.pixel_count as u64;
            if let Some(pop) = meta.population {
                stats.population += pop;
                stats.populated += 1;
            }
        }
        stats
    }

    /// Totals for state `id`, or `None` if there is no such state.
    pub fn state_stats(&self, id: u32) -> Option<StateStats> {
        Some(self.province_stats(&self.states.get(id)?.provinces))
    }

    /// Totals for a country or region (over the provinces of its states), or `None` if
    /// there is no such group.
    pub fn group_stats(&self, kind: GroupKind, id: u32) -> Option<StateStats> {
        self.groups(kind).get(id)?;
        Some(self.province_stats(&self.provinces_of_group(kind, id)))
    }

    /// The color of the country or region holding `province`'s state, if it has one.
    fn group_color(&self, set: &GroupSet, province: u32) -> Option<[u8; 3]> {
        let state = self.states.state_of(province)?;
        set.get(set.group_of(state)?).map(|g| g.color)
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
                ViewMode::Countries | ViewMode::Regions => {
                    let set = if mode == ViewMode::Countries { &self.countries } else { &self.regions };
                    match self.group_color(set, p.id) {
                        Some(c) => c,
                        None if set.is_empty() => p.color,
                        None => mix(p.color, DIM, 0.6),
                    }
                }
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
    ///   name length, name (UTF-8), countries length, countries, regions length,
    ///   regions                                                u32 lengths
    pub fn edits_to_bytes(&self) -> Vec<u8> {
        let states = self.states.to_bytes();
        let provinces = self.provinces.to_bytes();
        let (countries, regions) = (self.countries.to_bytes(), self.regions.to_bytes());
        let mut out = Vec::with_capacity(24 + states.len() + provinces.len() + self.name.len() + countries.len() + regions.len());
        out.extend_from_slice(&EDITS_VERSION.to_le_bytes());
        out.extend_from_slice(&(states.len() as u32).to_le_bytes());
        out.extend_from_slice(&states);
        out.extend_from_slice(&(provinces.len() as u32).to_le_bytes());
        out.extend_from_slice(&provinces);
        out.extend_from_slice(&(self.name.len() as u32).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out.extend_from_slice(&(countries.len() as u32).to_le_bytes());
        out.extend_from_slice(&countries);
        out.extend_from_slice(&(regions.len() as u32).to_le_bytes());
        out.extend_from_slice(&regions);
        out
    }

    /// Replace the name, states, countries, regions and province metadata. Nothing
    /// changes if `bytes` are invalid. Older edits still load: those from before maps
    /// had names (version 1) leave the map unnamed, and those from before countries and
    /// regions (versions 1 and 2) leave both empty.
    pub fn set_edits_from_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let n = self.mesh.provinces.len();
        let mut r = Reader { data: bytes, pos: 0 };
        let version = r.u32()?;
        if !(1..=EDITS_VERSION).contains(&version) {
            return Err(bad(format!("edits version {version}, this build reads versions 1 to {EDITS_VERSION}")));
        }
        let states_len = r.u32()? as usize;
        let states = StateSet::from_bytes(r.take(states_len)?, n)?;
        let mut groups = None;
        let (provinces, name) = if version == 1 {
            (ProvinceTable::from_bytes(&bytes[r.pos..], n)?, String::new())
        } else {
            let len = r.u32()? as usize;
            let provinces = ProvinceTable::from_bytes(r.take(len)?, n)?;
            let name_len = r.u32()? as usize;
            let name = std::str::from_utf8(r.take(name_len)?).map_err(|_| bad("the map name is not valid UTF-8"))?.to_string();
            if name.chars().count() > MAX_MAP_NAME_CHARS {
                return Err(bad("the map name is too long"));
            }
            if version >= 3 {
                let known = |s: u32| states.get(s).is_some();
                let len = r.u32()? as usize;
                let countries = GroupSet::from_bytes(r.take(len)?, GroupKind::Country, known)?;
                let len = r.u32()? as usize;
                let regions = GroupSet::from_bytes(r.take(len)?, GroupKind::Region, known)?;
                groups = Some((countries, regions));
            }
            if r.pos != bytes.len() {
                return Err(bad("unexpected trailing data"));
            }
            (provinces, name)
        };
        let (countries, regions) = groups.unwrap_or_else(|| (GroupSet::new(GroupKind::Country), GroupSet::new(GroupKind::Region)));
        self.countries = countries;
        self.regions = regions;
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
