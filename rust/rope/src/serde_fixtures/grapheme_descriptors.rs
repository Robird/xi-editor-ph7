use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::helpers::string_leaf::{MAX_LEAF, MIN_LEAF};
use crate::rope::Rope;
use crate::tree::{Cursor, CursorDescriptor, TreeBuilder};
use unicode_segmentation::UnicodeSegmentation;

use super::chunk_descriptors::{PathFrameSnapshot, RangeSnapshot};
use super::detect_git_commit;
use crate::rope::RopeInfo;

pub const GRAPHEME_DESCRIPTOR_FILENAME: &str = "grapheme_descriptors.json";
const GRAPHEME_SCHEMA_VERSION: &str = "1.0.0";
const GRAPHEME_CONTEXT_WINDOW: usize = 24;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphemeDescriptorFile {
    pub metadata: GraphemeDescriptorMetadata,
    pub grapheme_descriptors: Vec<GraphemeDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphemeDescriptorMetadata {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_commit: Option<String>,
    pub generated_at_unix_millis: u128,
    pub descriptor_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphemeDescriptor {
    pub sample: String,
    pub cluster_index: usize,
    pub cluster: String,
    pub byte_range: RangeSnapshot,
    pub utf16_range: RangeSnapshot,
    pub scalar_count: usize,
    pub contains_zwj: bool,
    pub is_ascii: bool,
    pub crosses_leaf: bool,
    pub requires_fallback: bool,
    pub tags: Vec<String>,
    pub context: GraphemeContext,
    pub leaf: LeafSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphemeContext {
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeafSnapshot {
    pub range: RangeSnapshot,
    pub path: Vec<PathFrameSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphemeDescriptorExportReport {
    pub file_path: PathBuf,
    pub descriptor_count: usize,
}

struct GraphemeSample {
    name: &'static str,
    rope: Rope,
    tags: &'static [&'static str],
    max_clusters: usize,
}

pub fn export_grapheme_descriptors(
    dir: &Path,
) -> Result<GraphemeDescriptorExportReport, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let payload = grapheme_descriptor_fixtures();
    let mut json = serde_json::to_string_pretty(&payload)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    let path = dir.join(GRAPHEME_DESCRIPTOR_FILENAME);
    std::fs::write(&path, json)?;
    Ok(GraphemeDescriptorExportReport {
        file_path: path,
        descriptor_count: payload.grapheme_descriptors.len(),
    })
}

pub fn grapheme_descriptor_fixtures() -> GraphemeDescriptorFile {
    let samples = grapheme_samples();
    let grapheme_descriptors = build_grapheme_descriptors(&samples);
    let metadata = GraphemeDescriptorMetadata {
        schema_version: GRAPHEME_SCHEMA_VERSION.to_string(),
        rust_commit: detect_git_commit(),
        generated_at_unix_millis: current_millis(),
        descriptor_count: grapheme_descriptors.len(),
    };
    GraphemeDescriptorFile { metadata, grapheme_descriptors }
}

fn build_grapheme_descriptors(samples: &[GraphemeSample]) -> Vec<GraphemeDescriptor> {
    let mut descriptors = Vec::new();
    for sample in samples {
        let rope_text = String::from(&sample.rope);
        let mut cluster_index = 0usize;
        for (byte_offset, cluster) in rope_text.grapheme_indices(true) {
            if cluster_index >= sample.max_clusters {
                break;
            }
            let start = byte_offset;
            let end = start + cluster.len();
            let descriptor = snapshot_grapheme(
                sample,
                cluster_index,
                cluster,
                start,
                end,
                &sample.rope,
                &rope_text,
            );
            descriptors.push(descriptor);
            cluster_index += 1;
        }
    }
    descriptors
}

fn snapshot_grapheme(
    sample: &GraphemeSample,
    cluster_index: usize,
    cluster_text: &str,
    start: usize,
    end: usize,
    rope: &Rope,
    rope_text: &str,
) -> GraphemeDescriptor {
    let byte_range = RangeSnapshot { start, end };
    let utf16_range = RangeSnapshot {
        start: rope.convert_utf16_from_bytes(start),
        end: rope.convert_utf16_from_bytes(end),
    };
    let scalar_count = cluster_text.chars().count();
    let contains_zwj = cluster_text.contains('\u{200D}');
    let is_ascii = cluster_text.chars().all(|c| c.is_ascii());
    let leaf = capture_leaf_snapshot(rope, start);
    let leaf_len = leaf.range.end.saturating_sub(leaf.range.start);
    let start_in_leaf = start.saturating_sub(leaf.range.start);
    let crosses_leaf = start_in_leaf + (end - start) > leaf_len;
    let requires_fallback = infer_fallback(contains_zwj, crosses_leaf, cluster_text);
    let tags =
        compose_grapheme_tags(sample.tags, contains_zwj, is_ascii, crosses_leaf, cluster_text);
    let context = grapheme_context_from_text(rope_text, start, end);

    GraphemeDescriptor {
        sample: sample.name.to_string(),
        cluster_index,
        cluster: cluster_text.to_string(),
        byte_range,
        utf16_range,
        scalar_count,
        contains_zwj,
        is_ascii,
        crosses_leaf,
        requires_fallback,
        tags,
        context,
        leaf,
    }
}

fn capture_leaf_snapshot(rope: &Rope, offset: usize) -> LeafSnapshot {
    let cursor = Cursor::new(rope, offset);
    let descriptor = cursor.to_descriptor();
    let leaf_offset = descriptor.offset_of_leaf();
    let leaf_len = descriptor.leaf_len().unwrap_or_else(|| rope.len().saturating_sub(leaf_offset));
    let range = RangeSnapshot { start: leaf_offset, end: leaf_offset + leaf_len };
    let path = frames_from_descriptor(&descriptor);
    LeafSnapshot { range, path }
}

fn grapheme_context_from_text(text: &str, start: usize, end: usize) -> GraphemeContext {
    let before_start =
        clamp_prev_boundary_in_text(text, start.saturating_sub(GRAPHEME_CONTEXT_WINDOW));
    let after_end =
        clamp_next_boundary_in_text(text, (end + GRAPHEME_CONTEXT_WINDOW).min(text.len()));
    let before = text[before_start..start].to_string();
    let after = text[end..after_end].to_string();
    GraphemeContext { before, after }
}

fn clamp_prev_boundary_in_text(text: &str, offset: usize) -> usize {
    let mut idx = offset.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn clamp_next_boundary_in_text(text: &str, offset: usize) -> usize {
    let mut idx = offset.min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn frames_from_descriptor(
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

fn infer_fallback(contains_zwj: bool, crosses_leaf: bool, cluster_text: &str) -> bool {
    if contains_zwj || crosses_leaf {
        return true;
    }
    cluster_text.chars().any(|ch| (ch as u32) >= 0x1_0000)
}

fn compose_grapheme_tags(
    base: &[&str],
    contains_zwj: bool,
    is_ascii: bool,
    crosses_leaf: bool,
    cluster_text: &str,
) -> Vec<String> {
    let mut tags: Vec<String> = base.iter().map(|t| t.to_string()).collect();
    tags.push("grapheme".to_string());
    if contains_zwj {
        tags.push("zwj".to_string());
    }
    if crosses_leaf {
        tags.push("cross_leaf".to_string());
    }
    if cluster_text.chars().any(|c| (c as u32) >= 0x1_0000) {
        tags.push("surrogate".to_string());
    }
    if is_ascii {
        tags.push("ascii".to_string());
    }
    tags.sort();
    tags.dedup();
    tags
}

fn grapheme_samples() -> Vec<GraphemeSample> {
    vec![
        GraphemeSample {
            name: "ascii_sentence",
            rope: Rope::from("ASCII sample covers words."),
            tags: &["ascii"],
            max_clusters: 5,
        },
        GraphemeSample {
            name: "combining_marks",
            rope: Rope::from("a\u{0301} tone e\u{0301}"),
            tags: &["combining"],
            max_clusters: 4,
        },
        GraphemeSample {
            name: "hangul_chain",
            rope: Rope::from("\u{110B}\u{1161}\u{11AB}\u{110C}\u{1165}\u{11AB}"),
            tags: &["hangul"],
            max_clusters: 4,
        },
        GraphemeSample {
            name: "emoji_palette",
            rope: Rope::from("Mixed 😀😃😄 👍🏽 crew"),
            tags: &["emoji", "surrogate"],
            max_clusters: 5,
        },
        GraphemeSample {
            name: "zwj_family",
            rope: Rope::from("Family 👩‍👩‍👧‍👦 + crew 👨‍🚀"),
            tags: &["zwj", "emoji"],
            max_clusters: 12,
        },
        GraphemeSample {
            name: "cross_leaf_flag",
            rope: build_cross_leaf_flag_sample(),
            tags: &["emoji", "cross_leaf"],
            max_clusters: 640,
        },
    ]
}

fn build_cross_leaf_flag_sample() -> Rope {
    let mut builder = TreeBuilder::<RopeInfo, String>::new();
    builder.push_leaf(flag_leaf_left());
    builder.push_leaf(flag_leaf_right());
    builder.build()
}

fn flag_leaf_left() -> String {
    let mut payload = String::new();
    while payload.len() < MIN_LEAF + 8 {
        payload.push_str("flag-left-");
    }
    let target_len = (MIN_LEAF + 16).min(MAX_LEAF - 32);
    payload.truncate(target_len);
    payload.push_str("\u{1F1FA}");
    payload
}

fn flag_leaf_right() -> String {
    let mut payload = String::new();
    payload.push_str("\u{1F1F8}");
    while payload.len() < MIN_LEAF + 12 {
        payload.push_str("-flag-right");
    }
    let target_len = (MIN_LEAF + 24).min(MAX_LEAF - 16);
    payload.truncate(target_len);
    payload
}

fn current_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|dur| dur.as_millis()).unwrap_or_default()
}
