use std::cmp;

const CONT_MASK: u8 = 0b1100_0000;
const CONT_TAG: u8 = 0b1000_0000;

#[inline(always)]
fn is_continuation(byte: u8) -> bool {
    (byte & CONT_MASK) == CONT_TAG
}

#[inline(always)]
pub(crate) fn len_utf8_from_first_byte(b: u8) -> usize {
    match b {
        b if b < 0x80 => 1,
        b if b < 0xe0 => 2,
        b if b < 0xf0 => 3,
        _ => 4,
    }
}

#[inline]
pub(crate) fn is_codepoint_boundary(bytes: &[u8], offset: usize) -> bool {
    debug_assert!(offset <= bytes.len());
    if offset == 0 || offset == bytes.len() {
        return true;
    }
    !is_continuation(bytes[offset])
}

#[inline]
pub(crate) fn prev_codepoint_boundary(bytes: &[u8], offset: usize) -> Option<usize> {
    debug_assert!(offset <= bytes.len());
    if offset == 0 {
        return None;
    }
    let mut cursor = offset - 1;
    while cursor > 0 && is_continuation(bytes[cursor]) {
        cursor -= 1;
    }
    Some(cursor)
}

#[inline]
pub(crate) fn next_codepoint_boundary(bytes: &[u8], offset: usize) -> Option<usize> {
    debug_assert!(offset <= bytes.len());
    if offset == bytes.len() {
        return None;
    }
    let lead = bytes[offset];
    let width = len_utf8_from_first_byte(lead);
    Some(cmp::min(offset + width, bytes.len()))
}

#[inline]
pub(crate) fn count_utf16_code_units_bytes(bytes: &[u8]) -> usize {
    let mut utf16_count = 0;
    for &b in bytes {
        if (b as i8) >= -0x40 {
            utf16_count += 1;
        }
        if b >= 0xf0 {
            utf16_count += 1;
        }
    }
    utf16_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_boundaries() {
        let text = "hello";
        let bytes = text.as_bytes();
        for offset in 0..=bytes.len() {
            assert!(is_codepoint_boundary(bytes, offset));
        }
        assert_eq!(prev_codepoint_boundary(bytes, 5), Some(4));
        assert_eq!(next_codepoint_boundary(bytes, 0), Some(1));
    }

    #[test]
    fn multibyte_boundaries() {
        let text = "a\u{00A1}\u{4E00}\u{1F4A9}";
        let bytes = text.as_bytes();
        let offsets = [0, 1, 3, 6, 10];
        for window in offsets.windows(2) {
            let start = window[0];
            let end = window[1];
            assert!(is_codepoint_boundary(bytes, start));
            assert_eq!(next_codepoint_boundary(bytes, start), Some(end));
            assert_eq!(prev_codepoint_boundary(bytes, end), Some(start));
            if end < bytes.len() && end - start > 1 {
                assert!(!is_codepoint_boundary(bytes, end - 1));
            }
        }
        assert!(is_codepoint_boundary(bytes, bytes.len()));
        assert_eq!(next_codepoint_boundary(bytes, bytes.len()), None);
        assert_eq!(prev_codepoint_boundary(bytes, 0), None);
    }

    #[test]
    fn continuation_scan_stops_at_start() {
        let text = "\u{1f600}"; // 😀
        let bytes = text.as_bytes();
        assert_eq!(prev_codepoint_boundary(bytes, bytes.len()), Some(0));
        assert_eq!(next_codepoint_boundary(bytes, 0), Some(bytes.len()));
    }

    #[test]
    fn counts_utf16_code_units() {
        let text = "hi\u{1f600}";
        let bytes = text.as_bytes();
        assert_eq!(count_utf16_code_units_bytes(bytes), text.encode_utf16().count());
    }
}
