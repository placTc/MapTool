//! Input rules. Two situations make province borders ambiguous, so they are rejected:
//!
//! * a single-pixel exclave: a lone pixel of a color that also appears elsewhere;
//! * a four-way junction: four pixels of four different colors meeting at a corner.

use crate::label::{Labels, NONE};
use crate::{Error, Violation, ViolationKind};

/// How many violations are kept for the error message; the rest are only counted.
const MAX_LISTED: usize = 64;

pub fn check(labels: &Labels) -> Result<(), Error> {
    let (w, h) = (labels.width, labels.height);
    let d = &labels.data;
    let mut listed = Vec::new();
    let mut total = 0;
    let mut report = |kind, x: usize, y: usize| {
        total += 1;
        if listed.len() < MAX_LISTED {
            listed.push(Violation { kind, x: x as u32, y: y as u32 });
        }
    };

    // A pixel with no same-colored 4-neighbour is a component of its own. It is
    // only an *exclave* if the province has other pixels.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let l = d[i];
            if l == NONE || labels.counts[l as usize] < 2 {
                continue;
            }
            let joined = (x > 0 && d[i - 1] == l)
                || (x + 1 < w && d[i + 1] == l)
                || (y > 0 && d[i - w] == l)
                || (y + 1 < h && d[i + w] == l);
            if !joined {
                report(ViolationKind::SinglePixelExclave, x, y);
            }
        }
    }

    for y in 1..h {
        for x in 1..w {
            let (a, b, c, e) = (d[(y - 1) * w + x - 1], d[(y - 1) * w + x], d[y * w + x - 1], d[y * w + x]);
            if [a, b, c, e].contains(&NONE) {
                continue;
            }
            if a != b && a != c && a != e && b != c && b != e && c != e {
                report(ViolationKind::FourWayJunction, x, y);
            }
        }
    }

    if total == 0 { Ok(()) } else { Err(Error::Invalid { violations: listed, total }) }
}
