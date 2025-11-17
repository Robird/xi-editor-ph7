use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::helpers::string_leaf::{MAX_LEAF, MIN_LEAF};
use crate::rope::Rope;
use crate::tree::{Cursor, TreeBuilder};

use super::detect_git_commit;
use super::snapshots::{frames_from_descriptor, PathFrameSnapshot, RangeSnapshot};
use crate::rope::RopeInfo;

pub const CHUNK_DESCRIPTOR_FILENAME: &str = "chunk_descriptors.json";
const CHUNK_SCHEMA_VERSION: &str = "1.0.0";
const DEFAULT_CONTEXT_WINDOW: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkDescriptorFile {
    pub metadata: ChunkDescriptorMetadata,
    pub chunk_descriptors: Vec<ChunkDescriptor>,
    pub line_descriptors: Vec<LineDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkDescriptorMetadata {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_commit: Option<String>,
    pub generated_at_unix_millis: u128,
    pub chunk_descriptor_count: usize,
    pub line_descriptor_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub sample: String,
    pub chunk_index: usize,
    pub text: String,
    pub byte_range: RangeSnapshot,
    pub utf16_range: RangeSnapshot,
    pub leaf_range: RangeSnapshot,
    pub contains_crlf: bool,
    pub is_empty: bool,
    pub tags: Vec<String>,
    pub path: Vec<PathFrameSnapshot>,
    pub context: ChunkContext,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineDescriptor {
    pub sample: String,
    pub line_index: usize,
    pub raw: String,
    pub logical: String,
    pub byte_range: RangeSnapshot,
    pub utf16_range: RangeSnapshot,
    pub newline_kind: LineEndingKind,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkContext {
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineEndingKind {
    None,
    Lf,
    Cr,
    CrLf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkDescriptorExportReport {
    pub file_path: PathBuf,
    pub chunk_count: usize,
    pub line_count: usize,
}

struct RopeFixtureSample {
    name: &'static str,
    rope: Rope,
    tags: &'static [&'static str],
    include_in_lines: bool,
    max_chunks: usize,
    max_lines: usize,
}

pub fn export_chunk_descriptors(
    dir: &Path,
) -> Result<ChunkDescriptorExportReport, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(CHUNK_DESCRIPTOR_FILENAME);
    let existing_timestamp = read_existing_generated_at(&path);
    let payload = chunk_descriptor_fixtures(existing_timestamp);
    let mut json = serde_json::to_string_pretty(&payload)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(&path, json)?;
    Ok(ChunkDescriptorExportReport {
        file_path: path,
        chunk_count: payload.chunk_descriptors.len(),
        line_count: payload.line_descriptors.len(),
    })
}

fn chunk_context(rope: &Rope, start: usize, end: usize) -> ChunkContext {
    let before_start = clamp_prev_boundary(rope, start.saturating_sub(DEFAULT_CONTEXT_WINDOW));
    let after_end = clamp_next_boundary(rope, (end + DEFAULT_CONTEXT_WINDOW).min(rope.len()));
    let before = rope.slice_to_cow(before_start..start).into_owned();
    let after = rope.slice_to_cow(end..after_end).into_owned();
    ChunkContext { before, after }
}

fn clamp_prev_boundary(rope: &Rope, offset: usize) -> usize {
    rope.at_or_prev_codepoint_boundary(offset).unwrap_or(0)
}

fn clamp_next_boundary(rope: &Rope, offset: usize) -> usize {
    rope.at_or_next_codepoint_boundary(offset).unwrap_or_else(|| rope.len())
}
pub fn chunk_descriptor_fixtures(existing_timestamp: Option<u128>) -> ChunkDescriptorFile {
    let samples = chunk_samples();
    let chunk_descriptors = build_chunk_descriptors(&samples);
    let line_descriptors = build_line_descriptors(&samples);
    let metadata = ChunkDescriptorMetadata {
        schema_version: CHUNK_SCHEMA_VERSION.to_string(),
        rust_commit: detect_git_commit(),
        generated_at_unix_millis: existing_timestamp.unwrap_or_else(current_millis),
        chunk_descriptor_count: chunk_descriptors.len(),
        line_descriptor_count: line_descriptors.len(),
    };

    ChunkDescriptorFile { metadata, chunk_descriptors, line_descriptors }
}

fn build_chunk_descriptors(samples: &[RopeFixtureSample]) -> Vec<ChunkDescriptor> {
    let mut descriptors = Vec::new();
    for sample in samples {
        let mut cursor = Cursor::new(&sample.rope, 0);
        let mut absolute_start = 0usize;
        let mut chunk_index = 0usize;
        for chunk in sample.rope.iter_chunks(..).take(sample.max_chunks) {
            let text = chunk.to_string();
            cursor.set(absolute_start);
            let descriptor =
                snapshot_chunk(sample, chunk_index, &text, absolute_start, &sample.rope, &cursor);
            descriptors.push(descriptor);
            absolute_start += text.len();
            chunk_index += 1;
            let _ = cursor.next_leaf();
        }
        if chunk_index == 0 && sample.rope.is_empty() {
            descriptors.push(empty_chunk_descriptor(sample));
        }
    }
    descriptors
}

fn build_line_descriptors(samples: &[RopeFixtureSample]) -> Vec<LineDescriptor> {
    let mut descriptors = Vec::new();
    for sample in samples {
        if !sample.include_in_lines || sample.max_lines == 0 {
            continue;
        }
        let mut raw_iter = sample.rope.lines_raw(..);
        let mut logical_iter = sample.rope.lines(..);
        let mut offset = 0usize;
        let mut line_index = 0usize;
        while line_index < sample.max_lines {
            match (raw_iter.next(), logical_iter.next()) {
                (Some(raw), Some(logical)) => {
                    let raw_owned: String = match raw {
                        Cow::Borrowed(s) => s.to_string(),
                        Cow::Owned(s) => s,
                    };
                    let logical_owned: String = match logical {
                        Cow::Borrowed(s) => s.to_string(),
                        Cow::Owned(s) => s,
                    };
                    let len = raw_owned.len();
                    let start = offset;
                    let end = start + len;
                    let newline_kind = detect_newline_kind(&raw_owned);
                    let utf16_range = RangeSnapshot {
                        start: sample.rope.convert_utf16_from_bytes(start),
                        end: sample.rope.convert_utf16_from_bytes(end),
                    };
                    let tags = compose_line_tags(sample.tags, newline_kind);
                    descriptors.push(LineDescriptor {
                        sample: sample.name.to_string(),
                        line_index,
                        raw: raw_owned,
                        logical: logical_owned,
                        byte_range: RangeSnapshot { start, end },
                        utf16_range,
                        newline_kind,
                        tags,
                    });
                    offset = end;
                    line_index += 1;
                }
                _ => break,
            }
        }
    }
    descriptors
}

fn snapshot_chunk(
    sample: &RopeFixtureSample,
    chunk_index: usize,
    chunk_text: &str,
    absolute_start: usize,
    rope: &Rope,
    cursor: &Cursor<'_, RopeInfo, String>,
) -> ChunkDescriptor {
    let byte_end = absolute_start + chunk_text.len();
    let utf16_range = RangeSnapshot {
        start: rope.convert_utf16_from_bytes(absolute_start),
        end: rope.convert_utf16_from_bytes(byte_end),
    };
    let descriptor = cursor.to_descriptor();
    let leaf_offset = descriptor.offset_of_leaf();
    let leaf_len = descriptor
        .leaf_len()
        .unwrap_or_else(|| chunk_text.len() + absolute_start.saturating_sub(leaf_offset));
    let leaf_range = RangeSnapshot { start: leaf_offset, end: leaf_offset + leaf_len };
    let path = frames_from_descriptor(&descriptor);
    let contains_crlf = chunk_text.contains("\r\n");
    let tags = compose_chunk_tags(sample.tags, chunk_text);
    let context = chunk_context(rope, absolute_start, byte_end);

    ChunkDescriptor {
        sample: sample.name.to_string(),
        chunk_index,
        text: chunk_text.to_string(),
        byte_range: RangeSnapshot { start: absolute_start, end: byte_end },
        utf16_range,
        leaf_range,
        contains_crlf,
        is_empty: chunk_text.is_empty(),
        tags,
        path,
        context,
    }
}

fn empty_chunk_descriptor(sample: &RopeFixtureSample) -> ChunkDescriptor {
    ChunkDescriptor {
        sample: sample.name.to_string(),
        chunk_index: 0,
        text: String::new(),
        byte_range: RangeSnapshot { start: 0, end: 0 },
        utf16_range: RangeSnapshot { start: 0, end: 0 },
        leaf_range: RangeSnapshot { start: 0, end: 0 },
        contains_crlf: false,
        is_empty: true,
        tags: compose_chunk_tags(sample.tags, ""),
        path: Vec::new(),
        context: ChunkContext { before: String::new(), after: String::new() },
    }
}

fn compose_chunk_tags(base: &[&str], chunk_text: &str) -> Vec<String> {
    let mut tags: Vec<String> = base.iter().map(|t| t.to_string()).collect();
    tags.push("chunk".to_string());
    if chunk_text.is_empty() {
        tags.push("empty".to_string());
    }
    if chunk_text.contains("\r\n") {
        tags.push("crlf".to_string());
    } else if chunk_text.contains('\n') {
        tags.push("lf".to_string());
    }
    if chunk_text.contains('\u{200D}') {
        tags.push("zwj".to_string());
    }
    if chunk_text.chars().any(|c| c as u32 >= 0x1_0000) {
        tags.push("surrogate".to_string());
    }
    tags.sort();
    tags.dedup();
    tags
}

fn compose_line_tags(base: &[&str], newline_kind: LineEndingKind) -> Vec<String> {
    let mut tags: Vec<String> = base.iter().map(|t| t.to_string()).collect();
    tags.push("line".to_string());
    match newline_kind {
        LineEndingKind::Lf => tags.push("lf".to_string()),
        LineEndingKind::Cr => tags.push("cr".to_string()),
        LineEndingKind::CrLf => tags.push("crlf".to_string()),
        LineEndingKind::None => {}
    }
    tags.sort();
    tags.dedup();
    tags
}

fn detect_newline_kind(raw: &str) -> LineEndingKind {
    if raw.ends_with("\r\n") {
        LineEndingKind::CrLf
    } else if raw.ends_with('\n') {
        LineEndingKind::Lf
    } else if raw.ends_with('\r') {
        LineEndingKind::Cr
    } else {
        LineEndingKind::None
    }
}

fn current_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|dur| dur.as_millis()).unwrap_or_default()
}

fn chunk_samples() -> Vec<RopeFixtureSample> {
    vec![
        RopeFixtureSample {
            name: "empty_document",
            rope: Rope::from(""),
            tags: &["empty"],
            include_in_lines: false,
            max_chunks: 1,
            max_lines: 0,
        },
        RopeFixtureSample {
            name: "ascii_sketch",
            rope: Rope::from("Chunk iterator baseline\nspans ascii text."),
            tags: &["ascii"],
            include_in_lines: true,
            max_chunks: 1,
            max_lines: 2,
        },
        RopeFixtureSample {
            name: "crlf_mixed_log",
            rope: Rope::from("zero\r\none\r\ntwo\r\nthree\r\n"),
            tags: &["crlf"],
            include_in_lines: true,
            max_chunks: 2,
            max_lines: 4,
        },
        RopeFixtureSample {
            name: "emoji_cluster_block",
            rope: Rope::from("😀 emoji block 😁🚀✨\nnext line 😀😃😄"),
            tags: &["emoji", "surrogate"],
            include_in_lines: true,
            max_chunks: 2,
            max_lines: 2,
        },
        RopeFixtureSample {
            name: "surrogate_phrase",
            rope: Rope::from("Music 𝄞 motif and math 𝟘 sample."),
            tags: &["surrogate"],
            include_in_lines: true,
            max_chunks: 1,
            max_lines: 1,
        },
        RopeFixtureSample {
            name: "zwj_family_story",
            rope: Rope::from("Family 👩‍👩‍👧‍👦 lounge\nCrew 👨‍🚀 ties"),
            tags: &["zwj", "emoji"],
            include_in_lines: true,
            max_chunks: 1,
            max_lines: 2,
        },
        RopeFixtureSample {
            name: "deep_tree_payload",
            rope: build_deep_tree_sample(),
            tags: &["deep_tree", "builder"],
            include_in_lines: false,
            max_chunks: 3,
            max_lines: 0,
        },
    ]
}

fn build_deep_tree_sample() -> Rope {
    let mut builder = TreeBuilder::<RopeInfo, String>::new();
    for idx in 0..4 {
        builder.push_leaf(deep_leaf_payload(idx));
    }
    builder.build()
}

fn deep_leaf_payload(idx: usize) -> String {
    let mut payload = String::new();
    while payload.len() < MIN_LEAF + 32 {
        payload.push_str("deep-tree-chunk-");
    }
    let target_len = (MIN_LEAF + 64).min(MAX_LEAF - 8);
    payload.truncate(target_len);
    let marker = format!("[leaf-{idx:02}]");
    let start = payload.len().saturating_sub(marker.len());
    payload.replace_range(start..start + marker.len(), &marker);
    payload
}

fn read_existing_generated_at(path: &Path) -> Option<u128> {
    #[derive(Deserialize)]
    struct MetadataEnvelope {
        metadata: ChunkDescriptorMetadata,
    }

    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<MetadataEnvelope>(&contents)
        .map(|payload| payload.metadata.generated_at_unix_millis)
        .ok()
}
