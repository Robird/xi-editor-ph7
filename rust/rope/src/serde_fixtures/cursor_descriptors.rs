use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(feature = "cursor_state")]
use super::snapshots::frames_from_state;
use super::snapshots::PathFrameSnapshot;
#[cfg(feature = "cursor_state")]
use crate::tree::CursorState;
use crate::{
    helpers::string_leaf::{MAX_LEAF, MIN_LEAF},
    rope::{LinesMetric, Rope, RopeInfo, Utf16CodeUnitsMetric},
    tree::{Cursor, CursorDescriptor, TreeBuilder},
};

pub const CURSOR_DESCRIPTOR_FILENAME: &str = "cursor_descriptors.json";
const DEEP_TREE_LEAF_COUNT_EXP: u32 = 5;
const MIN_DEEP_PATH_DEPTH: usize = 5;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DescriptorMetric {
    Base,
    Lines,
    Utf16,
    Breaks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorDescriptorOffsets {
    pub offset_of_leaf: usize,
    pub offset_in_leaf: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_len: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorDescriptorFrame {
    pub node_height: usize,
    pub node_len: usize,
    pub child_index: usize,
    pub child_offset: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorStateSnapshot {
    pub cursor_state_enabled: bool,
    pub position: usize,
    pub offset_of_leaf: usize,
    pub is_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_len: Option<usize>,
    pub path: Vec<PathFrameSnapshot>,
    pub metric: DescriptorMetric,
    pub edit_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_version_after_edit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalidated_after_edit: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CursorDescriptorFixture {
    pub name: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_text: Option<String>,
    #[serde(default = "default_true")]
    pub expect_apply: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect_apply_after_edit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub metric: DescriptorMetric,
    pub position: usize,
    pub is_valid: bool,
    pub offsets: CursorDescriptorOffsets,
    pub leaf_path: Vec<CursorDescriptorFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_state: Option<CursorStateSnapshot>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug)]
pub struct CursorDescriptorExportReport {
    pub file_path: PathBuf,
    pub sample_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct CursorStateParams {
    edit_version: u64,
    edit_version_after_edit: Option<u64>,
    invalidated_after_edit: Option<bool>,
    metric_override: Option<DescriptorMetric>,
}

impl CursorStateParams {
    const fn new(edit_version: u64) -> Self {
        CursorStateParams {
            edit_version,
            edit_version_after_edit: None,
            invalidated_after_edit: None,
            metric_override: None,
        }
    }

    fn with_after_edit(mut self, edit_version_after_edit: u64, invalidated: bool) -> Self {
        self.edit_version_after_edit = Some(edit_version_after_edit);
        self.invalidated_after_edit = Some(invalidated);
        self
    }

    fn with_metric(mut self, metric: DescriptorMetric) -> Self {
        self.metric_override = Some(metric);
        self
    }
}

pub fn export_cursor_descriptor_fixtures(
    dir: &Path,
) -> Result<CursorDescriptorExportReport, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let fixtures = cursor_descriptor_samples();
    let mut json = serde_json::to_string_pretty(&fixtures)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    let path = dir.join(CURSOR_DESCRIPTOR_FILENAME);
    std::fs::write(&path, json)?;
    Ok(CursorDescriptorExportReport { file_path: path, sample_count: fixtures.len() })
}

pub fn cursor_descriptor_samples() -> Vec<CursorDescriptorFixture> {
    vec![
        sample_empty_base(),
        sample_single_leaf_midpoint(),
        sample_single_leaf_end(),
        sample_lines_middle(),
        sample_lines_tail_boundary(),
        sample_utf16_surrogate_midpoint(),
        sample_utf16_cluster_tail(),
        sample_breaks_metric_soft_wrap(),
        sample_split_leaf_boundary(),
        sample_deep_tree_midpoint(),
        sample_post_edit_invalidates(),
        sample_invalid_descriptor(),
    ]
}

fn sample_empty_base() -> CursorDescriptorFixture {
    let rope = Rope::from("");
    let cursor = Cursor::new(&rope, 0);
    fixture_from_descriptor(
        "empty_base_start",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "Empty document at BOF (valid descriptor).",
        true,
        None,
        None,
        Some(CursorStateParams::new(1)),
    )
}

fn sample_single_leaf_midpoint() -> CursorDescriptorFixture {
    let text = "Cursor fixtures keep parity with BaseMetric.";
    let rope = Rope::from(text);
    let cursor = Cursor::new(&rope, 7);
    fixture_from_descriptor(
        "single_leaf_midpoint_base",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "Mid-leaf descriptor anchored at ASCII offset 7.",
        true,
        None,
        None,
        Some(CursorStateParams::new(2)),
    )
}

fn sample_single_leaf_end() -> CursorDescriptorFixture {
    let text = "Trailing newline coverage\n";
    let rope = Rope::from(text);
    let cursor = Cursor::new(&rope, rope.len());
    fixture_from_descriptor(
        "single_leaf_end_base",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "Descriptor at EOF after trailing newline.",
        true,
        None,
        None,
        Some(CursorStateParams::new(3)),
    )
}

fn sample_lines_middle() -> CursorDescriptorFixture {
    let text = "zero\none bounded line\nthree\nfour";
    let rope = Rope::from(text);
    let third_line_start = rope.count_base_units::<LinesMetric>(2);
    let cursor = Cursor::new(&rope, third_line_start);
    fixture_from_descriptor(
        "lines_metric_middle",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Lines,
        "LinesMetric descriptor positioned at start of third line.",
        true,
        None,
        None,
        Some(CursorStateParams::new(10)),
    )
}

fn sample_lines_tail_boundary() -> CursorDescriptorFixture {
    let text = "l0\nl1\nl2\n";
    let rope = Rope::from(text);
    let line_count = rope.measure::<LinesMetric>();
    let final_line_offset = rope.count_base_units::<LinesMetric>(line_count);
    let cursor = Cursor::new(&rope, final_line_offset);
    fixture_from_descriptor(
        "lines_metric_tail_boundary",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Lines,
        "Descriptor pointing at final newline boundary.",
        true,
        None,
        None,
        Some(CursorStateParams::new(11)),
    )
}

fn sample_utf16_surrogate_midpoint() -> CursorDescriptorFixture {
    let text = "emoji 😀 boundary check";
    let rope = Rope::from(text);
    let utf16_offset = rope.count_base_units::<Utf16CodeUnitsMetric>(5);
    let cursor = Cursor::new(&rope, utf16_offset);
    fixture_from_descriptor(
        "utf16_metric_surrogate_midpoint",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Utf16,
        "Utf16CodeUnitsMetric descriptor landing inside surrogate pair boundary.",
        true,
        None,
        None,
        Some(CursorStateParams::new(20)),
    )
}

fn sample_utf16_cluster_tail() -> CursorDescriptorFixture {
    let text = "start 😀 emoji 😁 cluster 🚀";
    let rope = Rope::from(text);
    let total_units = rope.measure::<Utf16CodeUnitsMetric>();
    let utf16_offset = rope.count_base_units::<Utf16CodeUnitsMetric>(total_units.saturating_sub(3));
    let cursor = Cursor::new(&rope, utf16_offset);
    fixture_from_descriptor(
        "utf16_metric_cluster_tail",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Utf16,
        "Utf16CodeUnitsMetric descriptor near end of multi-emoji cluster.",
        true,
        None,
        None,
        Some(CursorStateParams::new(21)),
    )
}

fn sample_breaks_metric_soft_wrap() -> CursorDescriptorFixture {
    let text = "Soft wrap ensures BreaksMetric parity across CRLF\r\nwindows and emoji 🚀 spans.";
    let rope = Rope::from(text);
    let cursor = Cursor::new(&rope, 32);
    fixture_from_descriptor(
        "breaks_metric_soft_wrap",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "BreaksMetric-aligned cursor captured near soft wrap boundary.",
        true,
        None,
        None,
        Some(CursorStateParams::new(25).with_metric(DescriptorMetric::Breaks)),
    )
}

fn sample_split_leaf_boundary() -> CursorDescriptorFixture {
    let mut text = "A".repeat(MAX_LEAF + 12);
    text.push_str("leaf split coverage");
    let rope = Rope::from(text);
    let position = MAX_LEAF + 6;
    let cursor = Cursor::new(&rope, position);
    fixture_from_descriptor(
        "base_metric_split_leaf_boundary",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "Descriptor straddling a builder-induced leaf boundary.",
        true,
        None,
        None,
        Some(CursorStateParams::new(30)),
    )
}

fn sample_deep_tree_midpoint() -> CursorDescriptorFixture {
    let rope = build_deep_rope();
    let midpoint = rope.len() / 2;
    let cursor = Cursor::new(&rope, midpoint);
    let descriptor = cursor.to_descriptor();
    assert!(
        descriptor.frames().len() >= MIN_DEEP_PATH_DEPTH,
        "expected deep descriptor path, found {}",
        descriptor.frames().len()
    );
    fixture_from_descriptor(
        "deep_tree_midpoint",
        &rope,
        descriptor,
        DescriptorMetric::Base,
        "Deep rope descriptor exercising > cache depth path.",
        true,
        None,
        None,
        Some(CursorStateParams::new(31)),
    )
}

fn sample_post_edit_invalidates() -> CursorDescriptorFixture {
    let text = "abcdefghij";
    let rope = Rope::from(text);
    let cursor = Cursor::new(&rope, 4);
    let edited_text = {
        let mut updated = String::from(text);
        updated.replace_range(3..6, "XYZ");
        updated
    };
    fixture_from_descriptor(
        "cursor_state_invalidated_after_edit",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Base,
        "Descriptor captured pre-edit; edited_text inserts XYZ causing invalidation.",
        true,
        Some(edited_text),
        Some(false),
        Some(CursorStateParams::new(40).with_after_edit(41, true)),
    )
}

fn sample_invalid_descriptor() -> CursorDescriptorFixture {
    let text = "abc";
    let rope = Rope::from(text);
    let mut cursor = Cursor::new(&rope, rope.len());
    let _ = cursor.next::<LinesMetric>();
    fixture_from_descriptor(
        "already_invalid_descriptor",
        &rope,
        cursor.to_descriptor(),
        DescriptorMetric::Lines,
        "Cursor was invalidated after next() returning None.",
        false,
        None,
        None,
        Some(CursorStateParams::new(50)),
    )
}

#[allow(clippy::too_many_arguments)]
fn fixture_from_descriptor(
    name: &str,
    rope: &Rope,
    descriptor: CursorDescriptor<RopeInfo, String>,
    metric: DescriptorMetric,
    notes: &str,
    expect_apply: bool,
    edited_text: Option<String>,
    expect_apply_after_edit: Option<bool>,
    cursor_state_params: Option<CursorStateParams>,
) -> CursorDescriptorFixture {
    let offsets = CursorDescriptorOffsets {
        offset_of_leaf: descriptor.offset_of_leaf(),
        offset_in_leaf: descriptor.position().saturating_sub(descriptor.offset_of_leaf()),
        leaf_len: descriptor.leaf_len(),
    };
    let leaf_path = descriptor
        .frames()
        .iter()
        .map(|frame| CursorDescriptorFrame {
            node_height: frame.node_height(),
            node_len: frame.node_len(),
            child_index: frame.child_index(),
            child_offset: frame.child_offset(),
        })
        .collect();
    let cursor_state = cursor_state_snapshot(&descriptor, metric, cursor_state_params);
    CursorDescriptorFixture {
        name: name.to_string(),
        text: String::from(rope),
        edited_text,
        expect_apply,
        expect_apply_after_edit,
        notes: if notes.is_empty() { None } else { Some(notes.to_string()) },
        metric,
        position: descriptor.position(),
        is_valid: descriptor.is_valid(),
        offsets,
        leaf_path,
        cursor_state,
    }
}

fn build_deep_rope() -> Rope {
    let mut builder = TreeBuilder::<RopeInfo, String>::new();
    let payload = generate_leaf_payload();
    let leaf_count = 8usize.pow(DEEP_TREE_LEAF_COUNT_EXP);
    for idx in 0..leaf_count {
        let mut leaf = payload.clone();
        // Encode a small counter near the end to make leaves unique.
        let marker = format!("{:06}", idx % 1_000_000);
        let marker_len = marker.len();
        let base_len = leaf.len();
        leaf.replace_range(base_len - marker_len..base_len, &marker);
        builder.push_leaf(leaf);
    }
    builder.build()
}

fn generate_leaf_payload() -> String {
    let mut payload = String::with_capacity(MIN_LEAF);
    while payload.len() < MIN_LEAF {
        payload.push_str("DeepNodePayload-");
    }
    payload.truncate(MIN_LEAF);
    payload
}

fn cursor_state_snapshot(
    descriptor: &CursorDescriptor<RopeInfo, String>,
    fixture_metric: DescriptorMetric,
    params: Option<CursorStateParams>,
) -> Option<CursorStateSnapshot> {
    #[cfg(feature = "cursor_state")]
    {
        return params.map(|params| {
            let state = CursorState::from_descriptor(descriptor);
            let metric = params.metric_override.unwrap_or(fixture_metric);
            CursorStateSnapshot {
                cursor_state_enabled: true,
                position: state.position(),
                offset_of_leaf: state.offset_of_leaf(),
                is_valid: state.is_valid(),
                leaf_len: descriptor.leaf_len(),
                path: frames_from_state(&state),
                metric,
                edit_version: params.edit_version,
                edit_version_after_edit: params.edit_version_after_edit,
                invalidated_after_edit: params.invalidated_after_edit,
            }
        });
    }
    #[cfg(not(feature = "cursor_state"))]
    {
        let _ = (descriptor, fixture_metric, params);
        return None;
    }
}
