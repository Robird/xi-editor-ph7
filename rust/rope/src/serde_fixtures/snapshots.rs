use serde::{Deserialize, Serialize};

use crate::rope::RopeInfo;
use crate::tree::CursorDescriptor;
#[cfg(feature = "cursor_state")]
use crate::tree::CursorState;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangeSnapshot {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathFrameSnapshot {
    pub node_height: usize,
    pub node_len: usize,
    pub child_index: usize,
    pub child_offset: usize,
}

pub fn frames_from_descriptor(
    descriptor: &CursorDescriptor<RopeInfo, String>,
) -> Vec<PathFrameSnapshot> {
    descriptor
        .frames()
        .iter()
        .map(|frame| PathFrameSnapshot {
            node_height: frame.node_height(),
            node_len: frame.node_len(),
            child_index: frame.child_index(),
            child_offset: frame.child_offset(),
        })
        .collect()
}

#[cfg(feature = "cursor_state")]
pub fn frames_from_state(state: &CursorState<RopeInfo, String>) -> Vec<PathFrameSnapshot> {
    state
        .frames()
        .iter()
        .map(|frame| PathFrameSnapshot {
            node_height: frame.node_height(),
            node_len: frame.node_len(),
            child_index: frame.child_index(),
            child_offset: frame.child_offset(),
        })
        .collect()
}
