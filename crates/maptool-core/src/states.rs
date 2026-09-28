//! States: named, colored groups of provinces, edited by the user and saved with the map.
//!
//! A province belongs to at most one state; assigning it to another state moves it.

use crate::Error;
use crate::mesh::{Reader, bad};

const NO_STATE: u32 = u32::MAX;
const MAX_NAME_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub id: u32,
    pub name: String,
    /// Free text; empty when there is none.
    pub description: String,
    pub color: [u8; 3],
    /// Province ids in ascending order.
    pub provinces: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateSet {
    states: Vec<State>,
    /// A new state gets the smallest id not currently in use, so deleting one and creating
    /// another does not grow the numbering forever. Kept as a monotonic upper bound (it only
    /// grows) so saved data can be checked without scanning it first.
    next_id: u32,
    /// State id of every province, or `NO_STATE`. Derived from `states`.
    of_province: Vec<u32>,
}

fn invalid(why: impl Into<String>) -> Error {
    Error::Edit(why.into())
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A distinct, readable color per state id (golden-angle steps around the hue wheel).
pub fn auto_color(id: u32) -> [u8; 3] {
    let hue = (id as f64 * 137.508) % 360.0;
    let (s, l) = (0.55, 0.55);
    let c = (1.0 - (2.0 * l - 1.0f64).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [((r + m) * 255.0).round() as u8, ((g + m) * 255.0).round() as u8, ((b + m) * 255.0).round() as u8]
}

fn clean_name(name: &str, id: u32) -> String {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() { format!("State {id}") } else { name }
}

impl StateSet {
    pub fn new(province_count: usize) -> StateSet {
        StateSet { states: Vec::new(), next_id: 1, of_province: vec![NO_STATE; province_count] }
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn province_count(&self) -> usize {
        self.of_province.len()
    }

    /// States in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &State> {
        self.states.iter()
    }

    pub fn get(&self, id: u32) -> Option<&State> {
        self.states.iter().find(|s| s.id == id)
    }

    pub fn state_of(&self, province: u32) -> Option<u32> {
        self.of_province.get(province as usize).copied().filter(|&s| s != NO_STATE)
    }

    fn index(&self, id: u32) -> Result<usize, Error> {
        self.states.iter().position(|s| s.id == id).ok_or_else(|| invalid(format!("no state {id}")))
    }

    fn check(&self, provinces: &[u32]) -> Result<Vec<u32>, Error> {
        let mut ids = provinces.to_vec();
        ids.sort_unstable();
        ids.dedup();
        match ids.last() {
            Some(&last) if last as usize >= self.of_province.len() => Err(invalid(format!("no province {last}"))),
            _ => Ok(ids),
        }
    }

    /// Create a state holding `provinces` (moved out of any state they were in). Its id is the
    /// smallest one not already in use, so deleting a state and creating another reuses the
    /// gap instead of growing past it. An empty name becomes "State <id>". Returns the new id.
    pub fn create(&mut self, name: &str, provinces: &[u32]) -> Result<u32, Error> {
        self.check(provinces)?;
        let id = (1..).find(|i| !self.states.iter().any(|s| s.id == *i)).unwrap();
        self.next_id = self.next_id.max(id + 1);
        self.states.push(State { id, name: clean_name(name, id), description: String::new(), color: auto_color(id), provinces: Vec::new() });
        self.assign(id, provinces)?;
        Ok(id)
    }

    /// Put `provinces` into state `id`, taking them out of whichever state held them.
    pub fn assign(&mut self, id: u32, provinces: &[u32]) -> Result<(), Error> {
        let target = self.index(id)?;
        let ids = self.check(provinces)?;
        let mut old_states = Vec::new();
        let mut added = Vec::new();
        for &p in &ids {
            let old = self.of_province[p as usize];
            if old == id {
                continue;
            }
            if old != NO_STATE && !old_states.contains(&old) {
                old_states.push(old);
            }
            self.of_province[p as usize] = id;
            added.push(p);
        }
        for old in old_states {
            let of = &self.of_province;
            let i = self.states.iter().position(|s| s.id == old).unwrap();
            self.states[i].provinces.retain(|&p| of[p as usize] == old);
        }
        let state = &mut self.states[target];
        state.provinces.extend(added);
        state.provinces.sort_unstable();
        Ok(())
    }

    /// Take `provinces` out of their states. Provinces in no state are ignored.
    pub fn unassign(&mut self, provinces: &[u32]) -> Result<(), Error> {
        let ids = self.check(provinces)?;
        let mut touched = Vec::new();
        for &p in &ids {
            let old = std::mem::replace(&mut self.of_province[p as usize], NO_STATE);
            if old != NO_STATE && !touched.contains(&old) {
                touched.push(old);
            }
        }
        for old in touched {
            let of = &self.of_province;
            let i = self.states.iter().position(|s| s.id == old).unwrap();
            self.states[i].provinces.retain(|&p| of[p as usize] == old);
        }
        Ok(())
    }

    pub fn rename(&mut self, id: u32, name: &str) -> Result<(), Error> {
        let i = self.index(id)?;
        self.states[i].name = clean_name(name, id);
        Ok(())
    }

    /// Blank text clears the description.
    pub fn set_description(&mut self, id: u32, text: &str) -> Result<(), Error> {
        let i = self.index(id)?;
        self.states[i].description = text.trim().chars().take(MAX_DESCRIPTION_CHARS).collect();
        Ok(())
    }

    pub fn set_color(&mut self, id: u32, color: [u8; 3]) -> Result<(), Error> {
        let i = self.index(id)?;
        self.states[i].color = color;
        Ok(())
    }

    /// Delete a state; its provinces become unassigned.
    pub fn delete(&mut self, id: u32) -> Result<(), Error> {
        let i = self.index(id)?;
        for &p in &self.states[i].provinces {
            self.of_province[p as usize] = NO_STATE;
        }
        self.states.remove(i);
        Ok(())
    }

    // ---------------------------------------------------------------- saving
    //
    //   next id, state count                                   2 x u32
    //   per state: id, r g b 0, name length, name, description
    //              length, description, province count,
    //              province ids                                u32 each (text is UTF-8)

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, self.next_id);
        put(&mut out, self.states.len() as u32);
        for s in &self.states {
            put(&mut out, s.id);
            put(&mut out, u32::from_le_bytes([s.color[0], s.color[1], s.color[2], 0]));
            put(&mut out, s.name.len() as u32);
            out.extend_from_slice(s.name.as_bytes());
            put(&mut out, s.description.len() as u32);
            out.extend_from_slice(s.description.as_bytes());
            put(&mut out, s.provinces.len() as u32);
            for &p in &s.provinces {
                put(&mut out, p);
            }
        }
        out
    }

    /// Read states saved by [`StateSet::to_bytes`] for a map of `province_count`
    /// provinces. Never panics on bad input; rejects anything inconsistent.
    pub fn from_bytes(bytes: &[u8], province_count: usize) -> Result<StateSet, Error> {
        let mut r = Reader { data: bytes, pos: 0 };
        let next_id = r.u32()?;
        let count = r.u32()? as usize;
        if count > bytes.len() / 12 {
            return Err(bad("state count exceeds the data"));
        }
        let mut set = StateSet::new(province_count);
        set.next_id = next_id;
        for _ in 0..count {
            let id = r.u32()?;
            let c = r.u32()?.to_le_bytes();
            let name_len = r.u32()? as usize;
            let name = std::str::from_utf8(r.take(name_len)?).map_err(|_| bad("a state name is not valid UTF-8"))?.to_string();
            let len = r.u32()? as usize;
            let description = std::str::from_utf8(r.take(len)?).map_err(|_| bad("a state description is not valid UTF-8"))?.to_string();
            let n = r.u32()? as usize;
            let provinces = r.u32s(n)?;
            if id >= next_id || set.states.iter().any(|s| s.id == id) {
                return Err(bad(format!("state id {id} is repeated or not below the next id")));
            }
            if !provinces.windows(2).all(|w| w[0] < w[1]) {
                return Err(bad("a state's provinces are not sorted and unique"));
            }
            for &p in &provinces {
                match set.of_province.get_mut(p as usize) {
                    None => return Err(bad(format!("a state refers to missing province {p}"))),
                    Some(slot) if *slot != NO_STATE => return Err(bad(format!("province {p} is in two states"))),
                    Some(slot) => *slot = id,
                }
            }
            set.states.push(State { id, name, description, color: [c[0], c[1], c[2]], provinces });
        }
        if r.pos != bytes.len() {
            return Err(bad("unexpected trailing data"));
        }
        Ok(set)
    }
}
