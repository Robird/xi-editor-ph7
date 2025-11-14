use std::cmp::{max, min};

use memchr::memrchr;

use crate::metrics::count_utf16_code_units_bytes;

pub(crate) const MIN_LEAF: usize = 511;
pub(crate) const MAX_LEAF: usize = 1024;
pub(crate) const NEWLINE_WINDOW: usize = MAX_LEAF - MIN_LEAF;

pub(crate) fn count_utf16_code_units(s: &str) -> usize {
    count_utf16_code_units_bytes(s.as_bytes())
}

pub(crate) fn find_leaf_split_for_bulk(s: &str) -> usize {
    find_leaf_split(s, MIN_LEAF)
}

pub(crate) fn find_leaf_split_for_merge(s: &str) -> usize {
    let preferred = max(MIN_LEAF, s.len().saturating_sub(MAX_LEAF));
    find_leaf_split(s, preferred)
}

pub(crate) fn find_leaf_split(s: &str, minsplit: usize) -> usize {
    let bounded_minsplit = min(minsplit.max(MIN_LEAF), MAX_LEAF);
    let remainder_lower = min(s.len().saturating_sub(MAX_LEAF), MAX_LEAF);
    let lower_bound = max(bounded_minsplit, remainder_lower);

    let upper_window = bounded_minsplit.saturating_add(NEWLINE_WINDOW).min(MAX_LEAF);
    let upper_remaining = min(s.len().saturating_sub(MIN_LEAF), MAX_LEAF);
    let mut splitpoint = min(upper_window, upper_remaining);
    if splitpoint < lower_bound {
        splitpoint = lower_bound;
    }

    let search_start = lower_bound.saturating_sub(1);
    if splitpoint > search_start {
        if let Some(pos) = memrchr(b'\n', &s.as_bytes()[search_start..splitpoint]) {
            return search_start + pos + 1;
        }
    }

    while !s.is_char_boundary(splitpoint) {
        splitpoint -= 1;
    }
    debug_assert!(splitpoint <= MAX_LEAF, "splitpoint {} exceeds MAX_LEAF", splitpoint);
    splitpoint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_prefers_newline_within_window() {
        let prefix = "a".repeat(MIN_LEAF + 8);
        let newline_index = prefix.len();
        let payload = format!("{}\n{}", prefix, "b".repeat(MAX_LEAF));

        let split = find_leaf_split_for_bulk(&payload);

        assert_eq!(split, newline_index + 1);
        assert!(payload.is_char_boundary(split));
        assert!(split >= MIN_LEAF && split - MIN_LEAF <= NEWLINE_WINDOW);
    }

    #[test]
    fn split_avoids_surrogate_pair() {
        let left = "a".repeat(MAX_LEAF - 1);
        let payload = format!("{}😀{}", left, "b".repeat(MIN_LEAF));

        let split = find_leaf_split_for_merge(&payload);

        assert_eq!(split, MAX_LEAF - 1);
        assert!(payload.is_char_boundary(split));
    }

    #[test]
    fn bulk_split_respects_capacity_bounds() {
        let payload = "x".repeat(MAX_LEAF + MIN_LEAF + 13);

        let split = find_leaf_split_for_bulk(&payload);
        let left_len = split;
        let right_len = payload.len() - split;

        assert!(left_len >= MIN_LEAF);
        assert!(left_len <= MAX_LEAF);
        assert!(right_len >= MIN_LEAF);
        assert!(payload.is_char_boundary(split));
    }

    #[test]
    fn count_utf16_code_units_handles_mixed_scalar_values() {
        let sample = "Line🌟\nBorrowed😀Text𐍈";
        let expected = sample.encode_utf16().count();

        assert_eq!(count_utf16_code_units(sample), expected);
    }
}
