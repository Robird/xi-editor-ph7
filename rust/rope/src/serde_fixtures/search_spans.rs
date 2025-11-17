use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::find::{find, CaseMatching};
use crate::rope::Rope;
use crate::tree::Cursor;

use super::detect_git_commit;
use super::snapshots::RangeSnapshot;

pub const SEARCH_SPANS_FILENAME: &str = "search_spans.json";
const SEARCH_SPANS_SCHEMA_VERSION: &str = "1.0.0";
const CONTEXT_WINDOW: usize = 40;
const DEFAULT_STYLE_ID: i32 = 7;
const DEFAULT_PRIORITY: i32 = 10;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSpansFile {
    pub metadata: SearchSpansMetadata,
    pub search_cases: Vec<SearchCaseSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSpansMetadata {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_commit: Option<String>,
    pub generated_at_unix_millis: u128,
    pub case_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchCaseSnapshot {
    pub sample: String,
    pub query: String,
    pub is_regex: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_options: Option<String>,
    pub case_matching: CaseMatchingSnapshot,
    pub text_len: usize,
    pub hits: Vec<SearchHitSnapshot>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub span_windows: Vec<SpanSegmentSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseMatchingSnapshot {
    Exact,
    CaseInsensitive,
}

impl From<CaseMatching> for CaseMatchingSnapshot {
    fn from(value: CaseMatching) -> Self {
        match value {
            CaseMatching::Exact => CaseMatchingSnapshot::Exact,
            CaseMatching::CaseInsensitive => CaseMatchingSnapshot::CaseInsensitive,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchHitSnapshot {
    pub index: usize,
    pub range: RangeSnapshot,
    pub line: usize,
    pub context_before: String,
    pub context_after: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpanSegmentSnapshot {
    pub range: RangeSnapshot,
    pub style_id: i32,
    pub style_tag: String,
    pub priority: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSpansExportReport {
    pub file_path: PathBuf,
    pub case_count: usize,
}

struct SearchSample {
    name: &'static str,
    text: &'static str,
    query: &'static str,
    is_regex: bool,
    regex_flags: &'static [&'static str],
    case_matching: CaseMatching,
    notes: &'static str,
}

pub fn export_search_spans(dir: &Path) -> Result<SearchSpansExportReport, Box<dyn Error>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(SEARCH_SPANS_FILENAME);
    let existing_timestamp = read_existing_generated_at(&path);
    let payload = build_search_payload(existing_timestamp)?;
    let mut json = serde_json::to_string_pretty(&payload)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(&path, json)?;
    Ok(SearchSpansExportReport { file_path: path, case_count: payload.search_cases.len() })
}

fn build_search_payload(
    existing_timestamp: Option<u128>,
) -> Result<SearchSpansFile, Box<dyn Error>> {
    let cases = search_samples()
        .into_iter()
        .map(build_case_snapshot)
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let metadata = SearchSpansMetadata {
        schema_version: SEARCH_SPANS_SCHEMA_VERSION.to_string(),
        rust_commit: detect_git_commit(),
        generated_at_unix_millis: existing_timestamp.unwrap_or_else(current_millis),
        case_count: cases.len(),
    };
    Ok(SearchSpansFile { metadata, search_cases: cases })
}

fn build_case_snapshot(sample: SearchSample) -> Result<SearchCaseSnapshot, Box<dyn Error>> {
    let rope = Rope::from(sample.text);
    let rope_text = String::from(&rope);
    let text_len = rope.len();
    let regex =
        if sample.is_regex { Some(build_regex(sample.query, sample.regex_flags)?) } else { None };
    let mut cursor = Cursor::new(&rope, 0);
    let mut hits = Vec::new();
    let mut span_windows = Vec::new();
    loop {
        let mut lines_raw = rope.lines_raw(cursor.pos()..);
        let start = match find(
            &mut cursor,
            &mut lines_raw,
            sample.case_matching,
            sample.query,
            regex.as_ref(),
        ) {
            Some(pos) => pos,
            None => break,
        };
        let end = cursor.pos();
        if end == start {
            if end >= text_len {
                break;
            }
            cursor.set((end + 1).min(text_len));
            continue;
        }
        let range = RangeSnapshot { start, end };
        let line = rope.line_of_offset(start);
        let before = context_before(&rope_text, start, CONTEXT_WINDOW);
        let after = context_after(&rope_text, end, CONTEXT_WINDOW);
        let index = hits.len();
        hits.push(SearchHitSnapshot {
            index,
            range: range.clone(),
            line,
            context_before: before,
            context_after: after,
        });
        span_windows.push(SpanSegmentSnapshot {
            range,
            style_id: DEFAULT_STYLE_ID,
            style_tag: "search-match".to_string(),
            priority: DEFAULT_PRIORITY,
        });
    }

    let regex_options =
        if sample.regex_flags.is_empty() { None } else { Some(sample.regex_flags.join("|")) };

    Ok(SearchCaseSnapshot {
        sample: sample.name.to_string(),
        query: sample.query.to_string(),
        is_regex: sample.is_regex,
        regex_options,
        case_matching: sample.case_matching.into(),
        text_len: rope.len(),
        hits,
        span_windows,
        notes: Some(sample.notes.to_string()),
    })
}

fn search_samples() -> Vec<SearchSample> {
    vec![
        SearchSample {
            name: "literal_case_insensitive",
            text: "Search spans ensure Stage D parity stays honest. stage lines repeat to test folding.",
            query: "stage",
            is_regex: false,
            regex_flags: &[],
            case_matching: CaseMatching::CaseInsensitive,
            notes: "Case insensitive literal search across repeated tokens.",
        },
        SearchSample {
            name: "regex_multiline_warnings",
            text: "INFO: startup complete\nWARN: retry pending\nWARN: exceeded threshold\nERROR: final line",
            query: "^WARN:.*$",
            is_regex: true,
            regex_flags: &["multi_line"],
            case_matching: CaseMatching::Exact,
            notes: "Regex search captures all WARN-prefixed lines via multi-line anchors.",
        },
        SearchSample {
            name: "emoji_literal_hits",
            text: "Rocket 🚀 boosters ready. Backup 🚀 stage stays idle. Emoji hits require utf8 aware spans.",
            query: "🚀",
            is_regex: false,
            regex_flags: &[],
            case_matching: CaseMatching::Exact,
            notes: "Emoji literal demonstrates multi-byte span capture.",
        },
    ]
}

fn build_regex(pattern: &str, flags: &[&str]) -> Result<regex::Regex, Box<dyn Error>> {
    let mut builder = RegexBuilder::new(pattern);
    for flag in flags {
        match *flag {
            "multi_line" => {
                builder.multi_line(true);
            }
            "case_insensitive" => {
                builder.case_insensitive(true);
            }
            "unicode" => {
                builder.unicode(true);
            }
            _ => {}
        }
    }
    Ok(builder.build()?)
}

fn context_before(text: &str, end: usize, limit: usize) -> String {
    if end == 0 {
        return String::new();
    }
    let mut count = 0;
    let mut idx = end;
    while idx > 0 && count < limit {
        idx -= 1;
        while idx > 0 && !text.is_char_boundary(idx) {
            idx -= 1;
        }
        count += 1;
    }
    text[idx..end].to_string()
}

fn context_after(text: &str, start: usize, limit: usize) -> String {
    if start >= text.len() {
        return String::new();
    }
    let mut idx = start;
    let mut count = 0;
    while idx < text.len() && count < limit {
        if let Some(ch) = text[idx..].chars().next() {
            idx += ch.len_utf8();
            count += 1;
        } else {
            break;
        }
    }
    text[start..idx].to_string()
}

fn current_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|dur| dur.as_millis()).unwrap_or_default()
}

fn read_existing_generated_at(path: &Path) -> Option<u128> {
    #[derive(Deserialize)]
    struct MetadataEnvelope {
        metadata: SearchSpansMetadata,
    }

    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<MetadataEnvelope>(&contents)
        .map(|payload| payload.metadata.generated_at_unix_millis)
        .ok()
}
