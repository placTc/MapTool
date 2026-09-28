//! Groups of states: countries, and strategic regions.
//!
//! Both are the same idea: a named, colored group of states, where a state belongs to at
//! most one group of its kind. Countries and strategic regions are separate sets, so a
//! state can be in one country and one region at once, and neither affects the other.

use std::collections::HashMap;

use crate::mesh::{Reader, bad};
use crate::states::auto_color;
use crate::Error;

const MAX_NAME_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GroupKind {
    Country = 0,
    Region = 1,
}

impl GroupKind {
    pub fn from_u8(v: u8) -> Option<GroupKind> {
        match v {
            0 => Some(GroupKind::Country),
            1 => Some(GroupKind::Region),
            _ => None,
        }
    }

    /// What a group of this kind is called in messages.
    pub fn noun(self) -> &'static str {
        match self {
            GroupKind::Country => "country",
            GroupKind::Region => "strategic region",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// Free text; empty when there is none.
    pub description: String,
    pub color: [u8; 3],
    /// State ids in ascending order.
    pub states: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupSet {
    kind: GroupKind,
    groups: Vec<Group>,
    /// Ids are never reused within a set, even after a group is deleted.
    next_id: u32,
    /// The group of each state that is in one. Derived from `groups`.
    of_state: HashMap<u32, u32>,
}

fn invalid(why: impl Into<String>) -> Error {
    Error::Edit(why.into())
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    put(out, text.len() as u32);
    out.extend_from_slice(text.as_bytes());
}

fn read_text(r: &mut Reader, what: &str) -> Result<String, Error> {
    let len = r.u32()? as usize;
    std::str::from_utf8(r.take(len)?).map(str::to_string).map_err(|_| bad(format!("{what} is not valid UTF-8")))
}

impl GroupSet {
    pub fn new(kind: GroupKind) -> GroupSet {
        GroupSet { kind, groups: Vec::new(), next_id: 1, of_state: HashMap::new() }
    }

    pub fn kind(&self) -> GroupKind {
        self.kind
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Groups in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &Group> {
        self.groups.iter()
    }

    pub fn get(&self, id: u32) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The group holding `state`, if any.
    pub fn group_of(&self, state: u32) -> Option<u32> {
        self.of_state.get(&state).copied()
    }

    fn index(&self, id: u32) -> Result<usize, Error> {
        self.groups.iter().position(|g| g.id == id).ok_or_else(|| invalid(format!("no {} {id}", self.kind.noun())))
    }

    /// Sorted, deduplicated `states`, or an error naming the first one `exists` rejects.
    fn check(&self, states: &[u32], exists: &impl Fn(u32) -> bool) -> Result<Vec<u32>, Error> {
        let mut ids = states.to_vec();
        ids.sort_unstable();
        ids.dedup();
        match ids.iter().find(|&&s| !exists(s)) {
            Some(s) => Err(invalid(format!("no state {s}"))),
            None => Ok(ids),
        }
    }

    /// Create a group holding `states` (moved out of any group they were in). `exists` says
    /// which state ids are real. A blank name becomes "<Country|Strategic region> <id>".
    pub fn create(&mut self, name: &str, states: &[u32], exists: impl Fn(u32) -> bool) -> Result<u32, Error> {
        self.check(states, &exists)?;
        let id = self.next_id;
        self.next_id += 1;
        let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
        let name = if name.is_empty() {
            let noun = self.kind.noun();
            format!("{}{} {id}", noun[..1].to_uppercase(), &noun[1..])
        } else {
            name
        };
        let seed = match self.kind {
            GroupKind::Country => id,
            GroupKind::Region => id + 1000,
        };
        self.groups.push(Group { id, name, description: String::new(), color: auto_color(seed), states: Vec::new() });
        self.assign(id, states, exists)?;
        Ok(id)
    }

    /// Put `states` into group `id`, taking them out of whichever group held them.
    pub fn assign(&mut self, id: u32, states: &[u32], exists: impl Fn(u32) -> bool) -> Result<(), Error> {
        let target = self.index(id)?;
        let ids = self.check(states, &exists)?;
        let mut touched = Vec::new();
        let mut added = Vec::new();
        for &s in &ids {
            match self.of_state.insert(s, id) {
                Some(old) if old == id => continue,
                Some(old) if !touched.contains(&old) => touched.push(old),
                _ => {}
            }
            added.push(s);
        }
        for old in touched {
            let of = &self.of_state;
            let i = self.groups.iter().position(|g| g.id == old).unwrap();
            self.groups[i].states.retain(|s| of.get(s) == Some(&old));
        }
        let group = &mut self.groups[target];
        group.states.extend(added);
        group.states.sort_unstable();
        Ok(())
    }

    /// Take `states` out of their groups. States in no group are ignored.
    pub fn unassign(&mut self, states: &[u32], exists: impl Fn(u32) -> bool) -> Result<(), Error> {
        let ids = self.check(states, &exists)?;
        for s in ids {
            self.remove_state(s);
        }
        Ok(())
    }

    /// Forget a state, whichever group has it (used when the state itself is deleted).
    /// Does nothing if it is in none.
    pub fn remove_state(&mut self, state: u32) {
        if let Some(old) = self.of_state.remove(&state)
            && let Some(g) = self.groups.iter_mut().find(|g| g.id == old)
        {
            g.states.retain(|&s| s != state);
        }
    }

    pub fn rename(&mut self, id: u32, name: &str) -> Result<(), Error> {
        let i = self.index(id)?;
        let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
        if !name.is_empty() {
            self.groups[i].name = name;
        }
        Ok(())
    }

    /// Blank text clears the description.
    pub fn set_description(&mut self, id: u32, text: &str) -> Result<(), Error> {
        let i = self.index(id)?;
        self.groups[i].description = text.trim().chars().take(MAX_DESCRIPTION_CHARS).collect();
        Ok(())
    }

    pub fn set_color(&mut self, id: u32, color: [u8; 3]) -> Result<(), Error> {
        let i = self.index(id)?;
        self.groups[i].color = color;
        Ok(())
    }

    /// Delete a group; its states become ungrouped.
    pub fn delete(&mut self, id: u32) -> Result<(), Error> {
        let i = self.index(id)?;
        for s in &self.groups[i].states {
            self.of_state.remove(s);
        }
        self.groups.remove(i);
        Ok(())
    }

    // ---------------------------------------------------------------- saving
    //
    //   next id, group count                                     2 x u32
    //   per group: id, r g b 0, name, description (each u32 length
    //              + UTF-8), state count, state ids               u32 each

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, self.next_id);
        put(&mut out, self.groups.len() as u32);
        for g in &self.groups {
            put(&mut out, g.id);
            put(&mut out, u32::from_le_bytes([g.color[0], g.color[1], g.color[2], 0]));
            put_text(&mut out, &g.name);
            put_text(&mut out, &g.description);
            put(&mut out, g.states.len() as u32);
            for &s in &g.states {
                put(&mut out, s);
            }
        }
        out
    }

    /// Read groups saved by [`GroupSet::to_bytes`]. `exists` says which state ids are real.
    /// Never panics on bad input; rejects anything inconsistent.
    pub fn from_bytes(bytes: &[u8], kind: GroupKind, exists: impl Fn(u32) -> bool) -> Result<GroupSet, Error> {
        let mut r = Reader { data: bytes, pos: 0 };
        let next_id = r.u32()?;
        let count = r.u32()? as usize;
        if count > bytes.len() / 20 {
            return Err(bad(format!("{} count exceeds the data", kind.noun())));
        }
        let mut set = GroupSet::new(kind);
        set.next_id = next_id;
        for _ in 0..count {
            let id = r.u32()?;
            let c = r.u32()?.to_le_bytes();
            let name = read_text(&mut r, "a name")?;
            let description = read_text(&mut r, "a description")?;
            let n = r.u32()? as usize;
            let states = r.u32s(n)?;
            if id >= next_id || set.groups.iter().any(|g| g.id == id) {
                return Err(bad(format!("{} id {id} is repeated or not below the next id", kind.noun())));
            }
            if !states.windows(2).all(|w| w[0] < w[1]) {
                return Err(bad(format!("the states of a {} are not sorted and unique", kind.noun())));
            }
            for &s in &states {
                if !exists(s) {
                    return Err(bad(format!("a {} refers to missing state {s}", kind.noun())));
                }
                if set.of_state.insert(s, id).is_some() {
                    return Err(bad(format!("state {s} is in two {}s", kind.noun())));
                }
            }
            set.groups.push(Group { id, name, description, color: [c[0], c[1], c[2]], states });
        }
        if r.pos != bytes.len() {
            return Err(bad("unexpected trailing data"));
        }
        Ok(set)
    }
}
