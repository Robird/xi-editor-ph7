use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::delta::DeltaElement;
use crate::diff::{Diff, LineHashDiff};
use crate::rope::{Rope, RopeDelta};

use super::detect_git_commit;
use super::snapshots::RangeSnapshot;

pub const DIFF_REGIONS_FILENAME: &str = "diff_regions.json";
const DIFF_REGIONS_SCHEMA_VERSION: &str = "1.0.0";
const INSERT_PREVIEW_LIMIT: usize = 80;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffRegionsFile {
    pub metadata: DiffRegionsMetadata,
    pub diff_cases: Vec<DiffCase>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffRegionsMetadata {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_commit: Option<String>,
    pub generated_at_unix_millis: u128,
    pub case_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffCase {
    pub sample: String,
    pub base_path: String,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    pub ops: Vec<DiffOpSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<DiffStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DiffStats {
    pub copied_bytes: usize,
    pub inserted_bytes: usize,
    pub deleted_bytes: usize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffOpKind {
    Copy,
    Insert,
    Delete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffOpSnapshot {
    pub kind: DiffOpKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_range: Option<RangeSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_range: Option<RangeSnapshot>,
    pub byte_len: usize,
    pub line_span: LineSpanSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_preview: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineSpanSnapshot {
    pub base: [usize; 2],
    pub target: [usize; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffRegionsExportReport {
    pub file_path: PathBuf,
    pub case_count: usize,
}

struct DiffSample {
    name: &'static str,
    base_text: &'static str,
    target_text: &'static str,
    notes: &'static str,
}

pub fn export_diff_regions(dir: &Path) -> Result<DiffRegionsExportReport, Box<dyn Error>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(DIFF_REGIONS_FILENAME);
    let existing_timestamp = read_existing_generated_at(&path);
    let payload = build_diff_regions(dir, existing_timestamp)?;
    let mut json = serde_json::to_string_pretty(&payload)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(&path, json)?;
    Ok(DiffRegionsExportReport { file_path: path, case_count: payload.diff_cases.len() })
}

fn build_diff_regions(
    dir: &Path,
    existing_timestamp: Option<u128>,
) -> Result<DiffRegionsFile, Box<dyn Error>> {
    let samples = diff_samples();
    let mut diff_cases = Vec::new();
    for sample in samples {
        let base_path = write_sample_file(dir, sample.name, "base", sample.base_text)?;
        let target_path = write_sample_file(dir, sample.name, "target", sample.target_text)?;
        let base_rope = Rope::from(sample.base_text);
        let target_rope = Rope::from(sample.target_text);
        let delta = LineHashDiff::compute_delta(&base_rope, &target_rope);
        let (ops, stats) = convert_delta_to_ops(&delta, sample.base_text, sample.target_text);
        let case = DiffCase {
            sample: sample.name.to_string(),
            base_path: relative_fixture_path(&base_path),
            target_path: relative_fixture_path(&target_path),
            base_sha: None,
            target_sha: None,
            line_count: Some(LineIndex::new(sample.target_text).total_lines()),
            ops,
            stats: Some(stats),
            notes: Some(sample.notes.to_string()),
        };
        diff_cases.push(case);
    }
    let metadata = DiffRegionsMetadata {
        schema_version: DIFF_REGIONS_SCHEMA_VERSION.to_string(),
        rust_commit: detect_git_commit(),
        generated_at_unix_millis: existing_timestamp.unwrap_or_else(current_millis),
        case_count: diff_cases.len(),
    };
    Ok(DiffRegionsFile { metadata, diff_cases })
}

fn convert_delta_to_ops(
    delta: &RopeDelta,
    base_text: &str,
    target_text: &str,
) -> (Vec<DiffOpSnapshot>, DiffStats) {
    let mut ops = Vec::new();
    let mut stats = DiffStats::default();
    let mut base_cursor = 0usize;
    let mut target_cursor = 0usize;
    let base_index = LineIndex::new(base_text);
    let target_index = LineIndex::new(target_text);

    for element in delta.elements() {
        match element {
            DeltaElement::Copy(beg, end) => {
                if *beg > base_cursor {
                    emit_delete(
                        &mut ops,
                        &mut stats,
                        base_cursor,
                        *beg,
                        &base_index,
                        &target_index,
                        target_cursor,
                    );
                    base_cursor = *beg;
                }
                if *end <= base_cursor {
                    continue;
                }
                let len = end - beg;
                if len == 0 {
                    continue;
                }
                let base_range = RangeSnapshot { start: *beg, end: *end };
                let target_range = RangeSnapshot { start: target_cursor, end: target_cursor + len };
                let line_span = LineSpanSnapshot {
                    base: line_span(&base_index, Some(&base_range), base_cursor),
                    target: line_span(&target_index, Some(&target_range), target_cursor),
                };
                stats.copied_bytes += len;
                ops.push(DiffOpSnapshot {
                    kind: DiffOpKind::Copy,
                    base_range: Some(base_range),
                    target_range: Some(target_range),
                    byte_len: len,
                    line_span,
                    insert_preview: None,
                });
                base_cursor = *end;
                target_cursor += len;
            }
            DeltaElement::Insert(node) => {
                let insert_len = node.len();
                if insert_len == 0 {
                    continue;
                }
                let target_range =
                    RangeSnapshot { start: target_cursor, end: target_cursor + insert_len };
                let preview_full = String::from(node.clone());
                let preview = truncate_codepoints(&preview_full, INSERT_PREVIEW_LIMIT);
                let line_span = LineSpanSnapshot {
                    base: line_span(&base_index, None, base_cursor),
                    target: line_span(&target_index, Some(&target_range), target_cursor),
                };
                stats.inserted_bytes += insert_len;
                ops.push(DiffOpSnapshot {
                    kind: DiffOpKind::Insert,
                    base_range: None,
                    target_range: Some(target_range),
                    byte_len: insert_len,
                    line_span,
                    insert_preview: Some(preview),
                });
                target_cursor += insert_len;
            }
        }
    }

    if base_cursor < delta.base_len() {
        emit_delete(
            &mut ops,
            &mut stats,
            base_cursor,
            delta.base_len(),
            &base_index,
            &target_index,
            target_cursor,
        );
    }

    (ops, stats)
}

fn emit_delete(
    ops: &mut Vec<DiffOpSnapshot>,
    stats: &mut DiffStats,
    start: usize,
    end: usize,
    base_index: &LineIndex,
    target_index: &LineIndex,
    target_cursor: usize,
) {
    if start >= end {
        return;
    }
    let base_range = RangeSnapshot { start, end };
    let line_span = LineSpanSnapshot {
        base: line_span(base_index, Some(&base_range), start),
        target: line_span(target_index, None, target_cursor),
    };
    let len = end - start;
    stats.deleted_bytes += len;
    ops.push(DiffOpSnapshot {
        kind: DiffOpKind::Delete,
        base_range: Some(base_range),
        target_range: None,
        byte_len: len,
        line_span,
        insert_preview: None,
    });
}

fn diff_samples() -> Vec<DiffSample> {
    vec![
        DiffSample {
            name: "ascii_minimal_ops",
            base_text: "line zero\nline one\nline two\n",
            target_text: "line zero\nline one patched\nline two\nline three added\n",
            notes: "ASCII edit inserts a new line and touches an existing row.",
        },
        DiffSample {
            name: "crlf_normalization",
            base_text: "alpha\r\nbeta windows\r\ncarriage\r\n",
            target_text: "alpha\nbeta linux\ncarriage\nnormalized endings\n",
            notes: "Shows CRLF to LF normalization with additional trailing text.",
        },
        DiffSample {
            name: "emoji_patch_block",
            base_text: "emoji 😀 block baseline\nbooster 🚀 stage stable\n",
            target_text: "emoji 😀 block baseline\nbooster 🚀 stage patched with sparkles ✨\n",
            notes: "Unicode diff exercises preview truncation with emoji.",
        },
    ]
}

fn write_sample_file(
    dir: &Path,
    name: &str,
    suffix: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let file_name = format!("{}_{}.txt", name, suffix);
    let path = dir.join(file_name);
    std::fs::write(&path, contents)?;
    Ok(path)
}

fn relative_fixture_path(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    workspace_root()
        .canonicalize()
        .ok()
        .and_then(|root| canonical.strip_prefix(&root).ok().map(|p| p.to_owned()))
        .unwrap_or(canonical)
        .to_string_lossy()
        .replace('\\', "/")
}

fn line_span(index: &LineIndex, range: Option<&RangeSnapshot>, fallback: usize) -> [usize; 2] {
    match range {
        Some(range) if range.start < range.end => {
            let start_line = index.line_of_offset(range.start);
            let end_line = index.line_of_offset(range.end.saturating_sub(1)) + 1;
            [start_line, end_line]
        }
        Some(range) => {
            let line = index.line_of_offset(range.start);
            [line, line]
        }
        None => {
            let line = index.line_of_offset(fallback);
            [line, line]
        }
    }
}

struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        let bytes = text.as_bytes();
        let mut idx = 0;
        while idx < bytes.len() {
            if bytes[idx] == b'\n' {
                starts.push(idx + 1);
            } else if bytes[idx] == b'\r' {
                if idx + 1 >= bytes.len() || bytes[idx + 1] != b'\n' {
                    starts.push(idx + 1);
                }
            }
            idx += 1;
        }
        starts.sort();
        starts.dedup();
        LineIndex { starts, len: text.len() }
    }

    fn line_of_offset(&self, offset: usize) -> usize {
        if self.len == 0 {
            return 0;
        }
        let clamped = offset.min(self.len.saturating_sub(1));
        match self.starts.binary_search(&clamped) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        }
    }

    fn total_lines(&self) -> usize {
        self.starts.len().max(1)
    }
}

fn truncate_codepoints(text: &str, limit: usize) -> String {
    if text.is_empty() {
        return String::new();
    }
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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn read_existing_generated_at(path: &Path) -> Option<u128> {
    #[derive(Deserialize)]
    struct MetadataEnvelope {
        metadata: DiffRegionsMetadata,
    }

    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<MetadataEnvelope>(&contents)
        .map(|payload| payload.metadata.generated_at_unix_millis)
        .ok()
}
