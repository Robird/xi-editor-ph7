use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::breaks::{BreakBuilder, Breaks};
use crate::rope::Rope;
use crate::tree::Cursor;

use super::detect_git_commit;
use super::snapshots::{frames_from_descriptor, PathFrameSnapshot, RangeSnapshot};

pub const BREAKS_DESCRIPTOR_FILENAME: &str = "breaks_descriptors.json";
const BREAKS_SCHEMA_VERSION: &str = "1.0.0";
const EXCERPT_CODEPOINT_LIMIT: usize = 160;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BreaksDescriptorFile {
    pub metadata: BreaksDescriptorMetadata,
    pub break_sets: Vec<BreakSetDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BreaksDescriptorMetadata {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_commit: Option<String>,
    pub generated_at_unix_millis: u128,
    pub descriptor_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BreakSetDescriptor {
    pub sample: String,
    pub rope_len: usize,
    pub wrap_width_units: usize,
    pub metric: BreakMetricKind,
    pub break_offsets: Vec<usize>,
    pub break_count: usize,
    pub leaf_runs: Vec<LeafRunSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_excerpt: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BreakMetricKind {
    #[serde(rename = "BreaksMetric")]
    BreaksMetric,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeafRunSnapshot {
    pub range: RangeSnapshot,
    pub break_count: usize,
    pub path: Vec<PathFrameSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BreaksDescriptorExportReport {
    pub file_path: PathBuf,
    pub descriptor_count: usize,
}

struct BreaksSample {
    name: &'static str,
    text: &'static str,
    wrap_width: usize,
    tags: &'static [&'static str],
}

pub fn export_breaks_descriptors(
    dir: &Path,
) -> Result<BreaksDescriptorExportReport, Box<dyn Error>> {
    std::fs::create_dir_all(dir)?;
    let payload = build_breaks_descriptor_file();
    let mut json = serde_json::to_string_pretty(&payload)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    let path = dir.join(BREAKS_DESCRIPTOR_FILENAME);
    std::fs::write(&path, json)?;
    Ok(BreaksDescriptorExportReport { file_path: path, descriptor_count: payload.break_sets.len() })
}

fn build_breaks_descriptor_file() -> BreaksDescriptorFile {
    let samples = breaks_samples();
    let break_sets = samples.iter().map(build_break_set).collect::<Vec<_>>();
    let metadata = BreaksDescriptorMetadata {
        schema_version: BREAKS_SCHEMA_VERSION.to_string(),
        rust_commit: detect_git_commit(),
        generated_at_unix_millis: current_millis(),
        descriptor_count: break_sets.len(),
    };

    BreaksDescriptorFile { metadata, break_sets }
}

fn build_break_set(sample: &BreaksSample) -> BreakSetDescriptor {
    let rope = Rope::from(sample.text);
    let text = String::from(&rope);
    let mut break_offsets = greedy_break_offsets(&text, sample.wrap_width);
    if rope.len() > 0 && break_offsets.is_empty() {
        break_offsets.push(rope.len());
    }
    let breaks = build_breaks_tree(rope.len(), &break_offsets);
    debug_assert_eq!(breaks.len(), rope.len());
    let leaf_runs = capture_leaf_runs(&rope, &breaks);
    let tags = compose_break_tags(sample.tags, &text);
    let excerpt = text_excerpt(&text);

    BreakSetDescriptor {
        sample: sample.name.to_string(),
        rope_len: rope.len(),
        wrap_width_units: sample.wrap_width,
        metric: BreakMetricKind::BreaksMetric,
        break_count: break_offsets.len(),
        break_offsets,
        leaf_runs,
        text_excerpt: excerpt,
        tags,
    }
}

fn breaks_samples() -> Vec<BreaksSample> {
    vec![
        BreaksSample {
            name: "ascii_guidance",
            wrap_width: 38,
            tags: &["ascii", "narrative"],
            text: "Stage D exporter now emits real breaks fixtures so downstream automation can trust the data. The sample paragraph intentionally keeps a mix of short and medium sentences to exercise greedy wrapping.",
        },
        BreaksSample {
            name: "crlf_emoji_mix",
            wrap_width: 26,
            tags: &["crlf", "emoji"],
            text: "Symbols 😀 keep CRLF\r\nspacing flowing across emoji-rich lines🚀 and sparkles ✨ for coverage.",
        },
        BreaksSample {
            name: "delimited_tables",
            wrap_width: 24,
            tags: &["table", "tabs"],
            text: "columns:alpha,beta,gamma\nrow0:aaaaabbbbbcccccdddddeeeee\nrow1:tabs\tvs spaces for wrap",
        },
    ]
}

fn greedy_break_offsets(text: &str, wrap: usize) -> Vec<usize> {
    if text.is_empty() || wrap == 0 {
        return Vec::new();
    }
    let mut offsets = Vec::new();
    let mut cursor = 0;
    let len = text.len();
    while cursor < len {
        if len - cursor <= wrap {
            offsets.push(len);
            break;
        }
        let mut preferred: Option<usize> = None;
        for (idx, ch) in text[cursor..].char_indices() {
            let abs = cursor + idx;
            if idx >= wrap {
                break;
            }
            if ch == '\n' {
                preferred = Some(abs + ch.len_utf8());
                break;
            }
            if ch.is_whitespace() || ch == '-' {
                preferred = Some(abs + ch.len_utf8());
            }
        }
        let mut next = preferred.unwrap_or_else(|| {
            let mut candidate = cursor + wrap;
            while candidate > cursor && !text.is_char_boundary(candidate) {
                candidate -= 1;
            }
            if candidate == cursor {
                candidate = cursor
                    + text[cursor..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(len - cursor);
            }
            candidate
        });
        next = next.min(len);
        if next == cursor {
            next = (cursor + 1).min(len);
        }
        offsets.push(next);
        cursor = next;
    }
    offsets.sort();
    offsets.dedup();
    offsets
}

fn build_breaks_tree(text_len: usize, offsets: &[usize]) -> Breaks {
    if text_len == 0 {
        return Breaks::new_no_break(0);
    }
    if offsets.is_empty() {
        let mut builder = BreakBuilder::new();
        builder.add_no_break(text_len);
        return builder.build();
    }
    let mut builder = BreakBuilder::new();
    let mut prev = 0;
    for &offset in offsets {
        let bounded = offset.min(text_len);
        if bounded < prev {
            continue;
        }
        let delta = bounded.saturating_sub(prev);
        if delta > 0 {
            builder.add_break(delta);
            prev = bounded;
        }
    }
    if prev < text_len {
        builder.add_no_break(text_len - prev);
    }
    builder.build()
}

fn capture_leaf_runs(rope: &Rope, breaks: &Breaks) -> Vec<LeafRunSnapshot> {
    let mut runs = Vec::new();
    let mut cursor = Cursor::new(rope, 0);
    loop {
        if cursor.get_leaf().is_none() {
            break;
        }
        let descriptor = cursor.to_descriptor();
        if let Some(leaf_len) = descriptor.leaf_len() {
            let start = descriptor.offset_of_leaf();
            let end = start + leaf_len;
            let break_count =
                if end > start { breaks.count_breaks_in_range(start..end) } else { 0 };
            runs.push(LeafRunSnapshot {
                range: RangeSnapshot { start, end },
                break_count,
                path: frames_from_descriptor(&descriptor),
            });
        }
        if cursor.next_leaf().is_none() {
            break;
        }
    }
    runs
}

fn compose_break_tags(base: &[&str], text: &str) -> Vec<String> {
    let mut tags: Vec<String> = base.iter().map(|t| t.to_string()).collect();
    tags.push("breaks".to_string());
    if text.contains("\r\n") {
        tags.push("crlf".to_string());
    } else if text.contains('\n') {
        tags.push("lf".to_string());
    }
    if text.contains('\t') {
        tags.push("tab".to_string());
    }
    if text.chars().any(|ch| ch as u32 >= 0x1_0000) {
        tags.push("surrogate".to_string());
    }
    tags.sort();
    tags.dedup();
    tags
}

fn text_excerpt(text: &str) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(truncate_codepoints(text, EXCERPT_CODEPOINT_LIMIT))
    }
}

fn truncate_codepoints(text: &str, limit: usize) -> String {
    let mut count = 0;
    let mut end = text.len();
    for (idx, _) in text.char_indices() {
        if count == limit {
            end = idx;
            break;
        }
        count += 1;
    }
    if count <= limit {
        text.to_string()
    } else {
        text[..end].to_string()
    }
}

fn current_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|dur| dur.as_millis()).unwrap_or_default()
}
