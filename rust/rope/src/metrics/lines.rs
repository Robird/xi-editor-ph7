use memchr::{memchr, memrchr};

#[inline]
pub(crate) fn count_newlines_bytes(bytes: &[u8]) -> usize {
    bytecount::count(bytes, b'\n')
}

#[inline]
pub(crate) fn is_newline_boundary(bytes: &[u8], offset: usize) -> bool {
    if offset == 0 || offset > bytes.len() {
        return false;
    }
    bytes[offset - 1] == b'\n'
}

#[inline]
pub(crate) fn find_next_newline(bytes: &[u8], offset: usize) -> Option<usize> {
    debug_assert!(offset <= bytes.len());
    memchr(b'\n', &bytes[offset..]).map(|pos| offset + pos + 1)
}

#[inline]
pub(crate) fn find_prev_newline(bytes: &[u8], offset: usize) -> Option<usize> {
    debug_assert!(offset <= bytes.len());
    if offset <= 1 {
        return None;
    }
    memrchr(b'\n', &bytes[..offset - 1]).map(|pos| pos + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_ascii_newlines() {
        let text = b"a\nb\nc\n";
        assert_eq!(count_newlines_bytes(text), 3);
        assert_eq!(count_newlines_bytes(b"no newlines"), 0);
    }

    #[test]
    fn finds_next_newline_boundaries() {
        let text = b"first\nsecond\nthird";
        assert_eq!(find_next_newline(text, 0), Some(6));
        assert_eq!(find_next_newline(text, 6), Some(13));
        assert_eq!(find_next_newline(text, 13), None);
        assert_eq!(find_next_newline(text, text.len()), None);
    }

    #[test]
    fn finds_prev_newline_boundaries() {
        let text = b"line1\nline2\n";
        assert_eq!(find_prev_newline(text, 6), None);
        assert_eq!(find_prev_newline(text, 7), Some(6));
        assert_eq!(find_prev_newline(text, 12), Some(6));
        assert_eq!(find_prev_newline(text, 1), None);
    }

    #[test]
    fn prev_handles_no_newline() {
        assert_eq!(find_prev_newline(b"abcdef", 3), None);
    }
}
