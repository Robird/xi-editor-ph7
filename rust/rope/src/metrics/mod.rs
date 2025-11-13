pub(crate) mod break_indices;
pub(crate) mod codepoint;
pub(crate) mod identity;
pub(crate) mod lines;

pub(crate) use break_indices::{
    count_breaks_up_to, find_next_break, find_prev_break, is_break_boundary, nth_break_offset,
};
pub(crate) use codepoint::{
    count_utf16_code_units_bytes, is_codepoint_boundary, len_utf8_from_first_byte,
    next_codepoint_boundary, prev_codepoint_boundary,
};
#[allow(unused_imports)]
pub(crate) use identity::{BaseUnitsIdentity, BreaksBaseMetric};
pub(crate) use lines::{
    count_newlines_bytes, find_next_newline, find_prev_newline, is_newline_boundary,
};
