//! Fuzzy-filter helpers for the box list.
//!
//! Gated behind `#[cfg(feature = "tui")]` because `nucleo-matcher` is an optional
//! dep gated under the `tui` feature (D-2: `nucleo-matcher = { optional = true }`).
//!
//! The pure ranking helper `fuzzy_rank` is exposed here; the reducer calls it
//! when the filter input changes.

#[cfg(feature = "tui")]
pub use inner::fuzzy_rank;

#[cfg(feature = "tui")]
mod inner {
    use nucleo_matcher::{
        pattern::{CaseMatching, Normalization, Pattern},
        Config, Matcher,
    };

    /// Rank `names` against `query` using `nucleo-matcher`.
    ///
    /// Returns a `Vec<usize>` of indices into `names`, best-match first.
    /// Empty or whitespace-only query returns all indices in original order
    /// (identity — filter is open but matching everything).
    ///
    /// AC-FILTER-1 / AC-FILTER-2.
    pub fn fuzzy_rank(query: &str, names: &[&str]) -> Vec<usize> {
        if query.trim().is_empty() {
            return (0..names.len()).collect();
        }

        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

        let mut scored: Vec<(usize, u32)> = names
            .iter()
            .enumerate()
            .filter_map(|(i, name)| {
                let score = pattern.score(
                    nucleo_matcher::Utf32Str::new(name, &mut Vec::new()),
                    &mut matcher,
                )?;
                Some((i, score))
            })
            .collect();

        // Sort descending by score (best match first).
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        scored.into_iter().map(|(i, _)| i).collect()
    }
}
