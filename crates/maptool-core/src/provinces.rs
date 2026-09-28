//! Per-province metadata: name, description, land or sea, biome, population.

use crate::mesh::{Reader, bad};
use crate::Error;

const TABLE_VERSION: u32 = 1;
const MAX_NAME_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Land = 0,
    Sea = 1,
}

impl Kind {
    pub fn from_u8(v: u8) -> Option<Kind> {
        match v {
            0 => Some(Kind::Land),
            1 => Some(Kind::Sea),
            _ => None,
        }
    }
}

/// Biomes. The numeric codes are part of the file format: never reorder or reuse
/// them, only append.
///
/// `Sea` is not a choice: a sea province's biome is always `Sea`, and only sea
/// provinces have it. A province's land biome is kept underneath, so turning a
/// province into sea and back restores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Biome {
    Plains = 0,
    Forest = 1,
    Jungle = 2,
    Marsh = 3,
    Hills = 4,
    Mountains = 5,
    Desert = 6,
    Steppe = 7,
    Tundra = 8,
    Arctic = 9,
    Sea = 10,
}

impl Biome {
    /// Every biome, in code order. The last one is `Sea`.
    pub const ALL: [Biome; 11] = [
        Biome::Plains,
        Biome::Forest,
        Biome::Jungle,
        Biome::Marsh,
        Biome::Hills,
        Biome::Mountains,
        Biome::Desert,
        Biome::Steppe,
        Biome::Tundra,
        Biome::Arctic,
        Biome::Sea,
    ];

    pub fn from_u8(v: u8) -> Option<Biome> {
        Biome::ALL.get(v as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Biome::Plains => "Plains",
            Biome::Forest => "Forest",
            Biome::Jungle => "Jungle",
            Biome::Marsh => "Marsh",
            Biome::Hills => "Hills",
            Biome::Mountains => "Mountains",
            Biome::Desert => "Desert",
            Biome::Steppe => "Steppe",
            Biome::Tundra => "Tundra",
            Biome::Arctic => "Arctic",
            Biome::Sea => "Sea",
        }
    }

    /// Color used when the map is drawn by biome.
    pub fn color(self) -> [u8; 3] {
        match self {
            Biome::Plains => [156, 196, 92],
            Biome::Forest => [46, 125, 70],
            Biome::Jungle => [20, 96, 54],
            Biome::Marsh => [98, 140, 122],
            Biome::Hills => [176, 150, 98],
            Biome::Mountains => [128, 122, 116],
            Biome::Desert => [222, 196, 122],
            Biome::Steppe => [196, 184, 104],
            Biome::Tundra => [170, 190, 176],
            Biome::Arctic => [232, 240, 246],
            Biome::Sea => [58, 108, 168],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProvinceMeta {
    /// `None` means "show the province number".
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Kind,
    /// The biome it has as land. Kept while the province is sea, so making it land
    /// again brings it back; read the real biome through [`ProvinceMeta::biome`].
    land_biome: Biome,
    pub population: Option<u64>,
    /// The province's own number, from an imported CSV. `None` means "use its position
    /// in the map"; read the effective number through [`ProvinceTable::number`].
    number: Option<u32>,
}

impl ProvinceMeta {
    /// `Sea` for a sea province, otherwise its land biome.
    pub fn biome(&self) -> Biome {
        if self.kind == Kind::Sea { Biome::Sea } else { self.land_biome }
    }
}

impl Default for ProvinceMeta {
    fn default() -> Self {
        ProvinceMeta { name: None, description: None, kind: Kind::Land, land_biome: Biome::Plains, population: None, number: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProvinceTable {
    items: Vec<ProvinceMeta>,
}

fn invalid(why: impl Into<String>) -> Error {
    Error::Edit(why.into())
}

/// Trim, cap the length, and turn blank text into `None`.
fn optional_text(text: &str, max_chars: usize) -> Option<String> {
    let t = text.trim();
    if t.is_empty() { None } else { Some(t.chars().take(max_chars).collect()) }
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn read_text(r: &mut Reader) -> Result<String, Error> {
    let len = r.u32()? as usize;
    std::str::from_utf8(r.take(len)?).map(str::to_string).map_err(|_| bad("province text is not valid UTF-8"))
}

impl ProvinceTable {
    pub fn new(province_count: usize) -> ProvinceTable {
        ProvinceTable { items: vec![ProvinceMeta::default(); province_count] }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&ProvinceMeta> {
        self.items.get(id as usize)
    }

    /// The province's number: the one imported from a CSV, or else its position in the map.
    pub fn number(&self, id: u32) -> u32 {
        self.get(id).and_then(|m| m.number).unwrap_or(id)
    }

    /// The name, or the province number when it has none.
    pub fn display_name(&self, id: u32) -> String {
        self.get(id).and_then(|m| m.name.clone()).unwrap_or_else(|| self.number(id).to_string())
    }

    /// Give a province its own number, or `None` to go back to its position in the map.
    pub fn set_number(&mut self, id: u32, number: Option<u32>) -> Result<(), Error> {
        self.at(id)?.number = number;
        Ok(())
    }

    /// Forget every imported number.
    pub fn clear_numbers(&mut self) {
        for m in &mut self.items {
            m.number = None;
        }
    }

    fn at(&mut self, id: u32) -> Result<&mut ProvinceMeta, Error> {
        self.items.get_mut(id as usize).ok_or_else(|| invalid(format!("no province {id}")))
    }

    fn check(&self, ids: &[u32]) -> Result<(), Error> {
        match ids.iter().find(|&&i| i as usize >= self.items.len()) {
            Some(i) => Err(invalid(format!("no province {i}"))),
            None => Ok(()),
        }
    }

    /// Blank text clears the name.
    pub fn set_name(&mut self, id: u32, name: &str) -> Result<(), Error> {
        self.at(id)?.name = optional_text(name, MAX_NAME_CHARS);
        Ok(())
    }

    /// Blank text clears the description.
    pub fn set_description(&mut self, id: u32, text: &str) -> Result<(), Error> {
        self.at(id)?.description = optional_text(text, MAX_DESCRIPTION_CHARS);
        Ok(())
    }

    pub fn set_population(&mut self, id: u32, population: Option<u64>) -> Result<(), Error> {
        self.at(id)?.population = population;
        Ok(())
    }

    pub fn set_kind(&mut self, ids: &[u32], kind: Kind) -> Result<(), Error> {
        self.check(ids)?;
        for &i in ids {
            self.items[i as usize].kind = kind;
        }
        Ok(())
    }

    /// Set the biome of the land provinces among `ids`. Sea provinces are skipped:
    /// their biome is locked to `Sea`. `Sea` itself cannot be chosen; make the
    /// province sea instead.
    pub fn set_biome(&mut self, ids: &[u32], biome: Biome) -> Result<(), Error> {
        if biome == Biome::Sea {
            return Err(invalid("a province gets the Sea biome by being made sea"));
        }
        self.check(ids)?;
        for &i in ids {
            let m = &mut self.items[i as usize];
            if m.kind == Kind::Land {
                m.land_biome = biome;
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------- saving
    //
    //   version, province count                                  2 x u32
    //   per province: kind, land biome, flags, 0                 4 x u8
    //     flags: 1 = name, 2 = description, 4 = population, 8 = number
    //     then, in that order: name (u32 length + UTF-8), description
    //     (same), population (u64), number (u32)

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.items.len() * 4);
        out.extend_from_slice(&TABLE_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.items.len() as u32).to_le_bytes());
        for m in &self.items {
            let flags = m.name.is_some() as u8
                | (m.description.is_some() as u8) << 1
                | (m.population.is_some() as u8) << 2
                | (m.number.is_some() as u8) << 3;
            out.extend_from_slice(&[m.kind as u8, m.land_biome as u8, flags, 0]);
            if let Some(n) = &m.name {
                put_text(&mut out, n);
            }
            if let Some(d) = &m.description {
                put_text(&mut out, d);
            }
            if let Some(p) = m.population {
                out.extend_from_slice(&p.to_le_bytes());
            }
            if let Some(n) = m.number {
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
        out
    }

    /// Read a table saved by [`ProvinceTable::to_bytes`] for a map of
    /// `province_count` provinces. Never panics on bad input.
    pub fn from_bytes(bytes: &[u8], province_count: usize) -> Result<ProvinceTable, Error> {
        let mut r = Reader { data: bytes, pos: 0 };
        let version = r.u32()?;
        if version != TABLE_VERSION {
            return Err(bad(format!("province data version {version}, this build reads version {TABLE_VERSION}")));
        }
        if r.u32()? as usize != province_count {
            return Err(bad("province data does not match the map's province count"));
        }
        let mut items = Vec::with_capacity(province_count);
        for _ in 0..province_count {
            let head = r.take(4)?;
            let (kind, biome, flags) = (head[0], head[1], head[2]);
            if flags & !15 != 0 {
                return Err(bad("unknown province data flags"));
            }
            let kind = Kind::from_u8(kind).ok_or_else(|| bad("unknown province type"))?;
            let mut land_biome = Biome::from_u8(biome).ok_or_else(|| bad("unknown biome"))?;
            if land_biome == Biome::Sea {
                if kind == Kind::Land {
                    return Err(bad("a land province cannot have the Sea biome"));
                }
                land_biome = Biome::Plains; // a sea province; its land biome was never recorded
            }
            let name = if flags & 1 != 0 { Some(read_text(&mut r)?) } else { None };
            let description = if flags & 2 != 0 { Some(read_text(&mut r)?) } else { None };
            let population = if flags & 4 != 0 { Some(u64::from_le_bytes(r.take(8)?.try_into().unwrap())) } else { None };
            let number = if flags & 8 != 0 { Some(r.u32()?) } else { None };
            items.push(ProvinceMeta { name, description, kind, land_biome, population, number });
        }
        if r.pos != bytes.len() {
            return Err(bad("unexpected trailing data"));
        }
        Ok(ProvinceTable { items })
    }
}
