//! Marshaling `maptool_core::StateStats` into the flat arrays the JS side reads.

use maptool_core::StateStats;

/// `[provinces, land, sea, pixels, population, provinces with a population]`.
pub(crate) fn flatten_stats(s: &StateStats) -> Vec<f64> {
    vec![s.provinces as f64, s.land as f64, s.sea as f64, s.pixels as f64, s.population as f64, s.populated as f64]
}
