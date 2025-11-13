#[inline]
pub(crate) fn nth_break_offset(data: &[usize], leaf_len: usize, in_measured_units: usize) -> usize {
    if in_measured_units > data.len() {
        leaf_len + 1
    } else if in_measured_units == 0 {
        0
    } else {
        data[in_measured_units - 1]
    }
}

#[inline]
pub(crate) fn count_breaks_up_to(data: &[usize], offset: usize) -> usize {
    match data.binary_search(&offset) {
        Ok(n) => n + 1,
        Err(n) => n,
    }
}

#[inline]
pub(crate) fn find_prev_break(data: &[usize], offset: usize) -> Option<usize> {
    if data.is_empty() {
        return None;
    }
    let mut lo = 0;
    let mut hi = data.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if data[mid] < offset {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        None
    } else {
        Some(data[lo - 1])
    }
}

#[inline]
pub(crate) fn find_next_break(data: &[usize], offset: usize) -> Option<usize> {
    let idx = match data.binary_search(&offset) {
        Ok(n) => n + 1,
        Err(n) => n,
    };
    data.get(idx).copied()
}

#[inline]
pub(crate) fn is_break_boundary(data: &[usize], offset: usize) -> bool {
    data.binary_search(&offset).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_empty_breaks() {
        assert_eq!(nth_break_offset(&[], 0, 0), 0);
        assert_eq!(nth_break_offset(&[], 5, 1), 6);
        assert_eq!(count_breaks_up_to(&[], 10), 0);
        assert!(find_prev_break(&[], 3).is_none());
        assert!(find_next_break(&[], 3).is_none());
        assert!(!is_break_boundary(&[], 0));
    }

    #[test]
    fn navigates_breaks() {
        let data = [3, 7, 10, 12];
        assert_eq!(nth_break_offset(&data, 20, 0), 0);
        assert_eq!(nth_break_offset(&data, 20, 2), 7);
        assert_eq!(nth_break_offset(&data, 20, 5), 21);
        assert_eq!(count_breaks_up_to(&data, 7), 2);
        assert_eq!(count_breaks_up_to(&data, 8), 2);
        assert_eq!(find_prev_break(&data, 0), None);
        assert_eq!(find_prev_break(&data, 3), None);
        assert_eq!(find_prev_break(&data, 4), Some(3));
        assert_eq!(find_prev_break(&data, 11), Some(10));
        assert_eq!(find_next_break(&data, 0), Some(3));
        assert_eq!(find_next_break(&data, 3), Some(7));
        assert_eq!(find_next_break(&data, 11), Some(12));
        assert_eq!(find_next_break(&data, 12), None);
        assert!(is_break_boundary(&data, 10));
        assert!(!is_break_boundary(&data, 11));
    }

    #[test]
    fn handles_duplicates() {
        let data = [5, 5, 8];
        assert_eq!(find_prev_break(&data, 5), None);
        assert_eq!(find_next_break(&data, 5), Some(8));
        assert_eq!(count_breaks_up_to(&data, 5), 2);
    }
}
