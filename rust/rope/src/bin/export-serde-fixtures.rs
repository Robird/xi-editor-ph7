#[cfg(not(feature = "serde"))]
fn main() {
    eprintln!("export-serde-fixtures requires the `serde` feature to be enabled.");
    std::process::exit(1);
}

#[cfg(feature = "serde")]
use std::{env, path::PathBuf};

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(feature = "serde")]
use serde_json::Value;

#[cfg(feature = "serde")]
use sha2::{Digest, Sha256};

#[cfg(feature = "serde")]
use xi_rope::serde_fixtures::{
    export_breaks_descriptors, export_chunk_descriptors, export_cursor_descriptor_fixtures,
    export_diff_regions, export_grapheme_descriptors, export_search_spans, fixtures,
    BreaksDescriptorExportReport, ChunkDescriptorExportReport, DiffRegionsExportReport, Fixture,
    GraphemeDescriptorExportReport, SearchSpansExportReport, BREAKS_DESCRIPTOR_FILENAME,
    CHUNK_DESCRIPTOR_FILENAME, CURSOR_DESCRIPTOR_FILENAME, DIFF_REGIONS_FILENAME,
    GRAPHEME_DESCRIPTOR_FILENAME, SEARCH_SPANS_FILENAME,
};

#[cfg(feature = "serde")]
const DEFAULT_MANIFEST_RELATIVE: &str =
    "../../../tests/xi.Core.Tests/Fixtures/fixtures.manifest.json";
#[cfg(feature = "serde")]
const SUBSET_SCHEMA_HASH: &str = "serde_fixtures::subset";
#[cfg(feature = "serde")]
const DELTA_SCHEMA_HASH: &str = "serde_fixtures::delta";
#[cfg(feature = "serde")]
const ENGINE_SCHEMA_HASH: &str = "serde_fixtures::engine";
#[cfg(feature = "serde")]
const CURSOR_SCHEMA_HASH: &str = "cursor_descriptors@1.1.0";
#[cfg(feature = "serde")]
const CHUNK_SCHEMA_HASH: &str = "chunk_descriptors@1.0.0";
#[cfg(feature = "serde")]
const GRAPHEME_SCHEMA_HASH: &str = "grapheme_descriptors@1.0.0";
#[cfg(feature = "serde")]
const BREAKS_SCHEMA_HASH: &str = "breaks_descriptors@1.0.0";
#[cfg(feature = "serde")]
const DIFF_REGIONS_SCHEMA_HASH: &str = "diff_regions@1.0.0";
#[cfg(feature = "serde")]
const SEARCH_SPANS_SCHEMA_HASH: &str = "search_spans@1.0.0";
#[cfg(feature = "serde")]
const TREE_BUILDER_TRACE_SCHEMA_HASH: &str = "tree_builder_slice_trace@1.0.0";

#[cfg(feature = "serde")]
#[derive(Serialize)]
struct FixtureManifest {
    rust_commit: String,
    cli_rev: String,
    feature_gates: Vec<String>,
    fixtures: Vec<ManifestFixture>,
}

#[cfg(feature = "serde")]
#[derive(Serialize)]
struct ManifestFixture {
    name: String,
    path: String,
    count: usize,
    schema_hash: String,
    payload_hash: String,
}

#[cfg(feature = "serde")]
struct FixtureFileReport {
    name: String,
    path: PathBuf,
}

#[cfg(feature = "serde")]
struct TreeBuilderTraceExportReport {
    file_name: String,
    file_path: PathBuf,
    event_count: usize,
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
use xi_rope::{
    tree::{TreeBuilder, TreeBuilderEvent, TreeBuilderEventKind, TreeBuilderTracer},
    Interval, Rope, RopeInfo,
};

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
const TREE_BUILDER_TRACE_FILENAME: &str = "basic_slice_plan.json";

#[cfg(feature = "serde")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut output_dir: Option<PathBuf> = None;
    let mut trace_dir: Option<PathBuf> = None;
    let mut cursor_dir: Option<PathBuf> = None;
    let mut chunk_dir: Option<PathBuf> = None;
    let mut grapheme_dir: Option<PathBuf> = None;
    let mut breaks_dir: Option<PathBuf> = None;
    let mut diff_dir: Option<PathBuf> = None;
    let mut search_dir: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = Some(default_manifest_path());
    let mut manifest_entries: Vec<ManifestFixture> = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" | "--output-dir" => {
                let value =
                    args.next().ok_or("--dir requires a value specifying the output directory")?;
                output_dir = Some(PathBuf::from(value));
            }
            "--tree-builder-trace" | "--tree-builder-dir" => {
                let value = args.next().ok_or(
                    "--tree-builder-trace requires a value specifying the output directory",
                )?;
                trace_dir = Some(PathBuf::from(value));
            }
            "--cursor-descriptors" => {
                let value = args.next().ok_or(
                    "--cursor-descriptors requires a value specifying the output directory",
                )?;
                cursor_dir = Some(PathBuf::from(value));
            }
            "--chunk-descriptors" => {
                let value = args.next().ok_or(
                    "--chunk-descriptors requires a value specifying the output directory",
                )?;
                chunk_dir = Some(PathBuf::from(value));
            }
            "--grapheme-descriptors" => {
                let value = args.next().ok_or(
                    "--grapheme-descriptors requires a value specifying the output directory",
                )?;
                grapheme_dir = Some(PathBuf::from(value));
            }
            "--breaks-descriptors" => {
                let value = args.next().ok_or(
                    "--breaks-descriptors requires a value specifying the output directory",
                )?;
                breaks_dir = Some(PathBuf::from(value));
            }
            "--diff-regions" => {
                let value = args
                    .next()
                    .ok_or("--diff-regions requires a value specifying the output directory")?;
                diff_dir = Some(PathBuf::from(value));
            }
            "--search-spans" => {
                let value = args
                    .next()
                    .ok_or("--search-spans requires a value specifying the output directory")?;
                search_dir = Some(PathBuf::from(value));
            }
            "--emit-manifest" => {
                let value = args
                    .next()
                    .ok_or("--emit-manifest requires a value specifying the manifest path")?;
                manifest_path = Some(PathBuf::from(value));
            }
            "--list" => {
                list_fixtures();
                return Ok(());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => {
                return Err(format!("unrecognised argument: {other}").into());
            }
        }
    }

    if output_dir.is_none()
        && trace_dir.is_none()
        && cursor_dir.is_none()
        && chunk_dir.is_none()
        && grapheme_dir.is_none()
        && breaks_dir.is_none()
        && diff_dir.is_none()
        && search_dir.is_none()
    {
        print_usage();
        return Err(
            "missing required --dir <PATH>, --tree-builder-trace <PATH>, --cursor-descriptors <PATH>, --chunk-descriptors <PATH>, --grapheme-descriptors <PATH>, --breaks-descriptors <PATH>, --diff-regions <PATH>, or --search-spans <PATH> argument"
                .into(),
        );
    }

    if let Some(dir) = output_dir {
        let reports = export_to_directory(dir.as_path(), fixtures())?;
        for report in reports {
            let payload_hash = compute_payload_hash(report.path.as_path())?;
            manifest_entries.push(ManifestFixture {
                name: report.name.clone(),
                path: manifest_display_path(report.path.as_path()),
                count: 1,
                schema_hash: schema_hash_for_regression(&report.name),
                payload_hash,
            });
        }
    }

    if let Some(dir) = trace_dir {
        let report = handle_tree_builder_trace(dir)?;
        report_tree_builder_trace_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: report.file_name.clone(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.event_count,
            schema_hash: TREE_BUILDER_TRACE_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = cursor_dir {
        let report = export_cursor_descriptor_fixtures(dir.as_path())?;
        println!(
            "exported {count} cursor descriptor samples to {path}",
            count = report.sample_count,
            path = report.file_path.display()
        );
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: CURSOR_DESCRIPTOR_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.sample_count,
            schema_hash: CURSOR_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = chunk_dir {
        let report = export_chunk_descriptors(dir.as_path())?;
        report_chunk_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: CHUNK_DESCRIPTOR_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.chunk_count + report.line_count,
            schema_hash: CHUNK_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = grapheme_dir {
        let report = export_grapheme_descriptors(dir.as_path())?;
        report_grapheme_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: GRAPHEME_DESCRIPTOR_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.descriptor_count,
            schema_hash: GRAPHEME_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = breaks_dir {
        let report = export_breaks_descriptors(dir.as_path())?;
        report_breaks_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: BREAKS_DESCRIPTOR_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.descriptor_count,
            schema_hash: BREAKS_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = diff_dir {
        let report = export_diff_regions(dir.as_path())?;
        report_diff_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: DIFF_REGIONS_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.case_count,
            schema_hash: DIFF_REGIONS_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(dir) = search_dir {
        let report = export_search_spans(dir.as_path())?;
        report_search_export(&report);
        let payload_hash = compute_payload_hash(report.file_path.as_path())?;
        manifest_entries.push(ManifestFixture {
            name: SEARCH_SPANS_FILENAME.to_string(),
            path: manifest_display_path(report.file_path.as_path()),
            count: report.case_count,
            schema_hash: SEARCH_SPANS_SCHEMA_HASH.to_string(),
            payload_hash,
        });
    }

    if let Some(path) = manifest_path {
        if !manifest_entries.is_empty() {
            manifest_entries.sort_by(|a, b| a.name.cmp(&b.name));
            write_manifest(path.as_path(), manifest_entries)?;
        }
    }

    Ok(())
}

#[cfg(feature = "serde")]
fn print_usage() {
    eprintln!(
        "Usage: cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --dir <PATH> [--tree-builder-trace <PATH>] [--cursor-descriptors <PATH>] [--chunk-descriptors <PATH>] [--grapheme-descriptors <PATH>] [--breaks-descriptors <PATH>] [--diff-regions <PATH>] [--search-spans <PATH>] [--emit-manifest <PATH>]\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --tree-builder-trace <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --cursor-descriptors <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --chunk-descriptors <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --grapheme-descriptors <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --breaks-descriptors <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --diff-regions <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --search-spans <PATH>\n        cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --list"
    );
}

#[cfg(feature = "serde")]
fn list_fixtures() {
    for fixture in fixtures() {
        println!("{}", fixture.name);
    }
}

#[cfg(feature = "serde")]
fn export_to_directory(
    dir: &std::path::Path,
    fixtures: &[Fixture],
) -> Result<Vec<FixtureFileReport>, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let mut reports = Vec::with_capacity(fixtures.len());

    for fixture in fixtures {
        let mut content = fixture.json.to_owned();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        let path = dir.join(fixture.name);
        std::fs::write(&path, content)?;
        reports.push(FixtureFileReport { name: fixture.name.to_string(), path });
    }

    Ok(reports)
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
fn handle_tree_builder_trace(
    dir: PathBuf,
) -> Result<TreeBuilderTraceExportReport, Box<dyn std::error::Error>> {
    export_tree_builder_trace(dir.as_path())
}

#[cfg(all(feature = "serde", not(feature = "tree_builder_slice_trace")))]
fn handle_tree_builder_trace(
    _dir: PathBuf,
) -> Result<TreeBuilderTraceExportReport, Box<dyn std::error::Error>> {
    Err("--tree-builder-trace requires the `tree_builder_slice_trace` feature to be enabled".into())
}

#[cfg(feature = "serde")]
fn report_chunk_export(report: &ChunkDescriptorExportReport) {
    println!(
        "exported {chunk_count} chunk descriptors and {line_count} line descriptors to {path}",
        chunk_count = report.chunk_count,
        line_count = report.line_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn report_grapheme_export(report: &GraphemeDescriptorExportReport) {
    println!(
        "exported {count} grapheme descriptors to {path}",
        count = report.descriptor_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn report_breaks_export(report: &BreaksDescriptorExportReport) {
    println!(
        "exported {count} breaks descriptor samples to {path}",
        count = report.descriptor_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn report_diff_export(report: &DiffRegionsExportReport) {
    println!(
        "exported {count} diff regions to {path}",
        count = report.case_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn report_search_export(report: &SearchSpansExportReport) {
    println!(
        "exported {count} search span cases to {path}",
        count = report.case_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn report_tree_builder_trace_export(report: &TreeBuilderTraceExportReport) {
    println!(
        "exported {count} tree builder slice trace events to {path}",
        count = report.event_count,
        path = report.file_path.display()
    );
}

#[cfg(feature = "serde")]
fn write_manifest(
    path: &std::path::Path,
    fixtures: Vec<ManifestFixture>,
) -> Result<(), Box<dyn std::error::Error>> {
    if fixtures.is_empty() {
        return Ok(());
    }

    let rust_commit = xi_rope::serde_fixtures::detect_git_commit()
        .ok_or("failed to detect git commit for manifest emission")?;
    let manifest = FixtureManifest {
        rust_commit,
        cli_rev: env!("CARGO_PKG_VERSION").to_string(),
        feature_gates: collect_feature_gates(),
        fixtures,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string_pretty(&manifest)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(path, json)?;
    println!("exported manifest to {}", manifest_display_path(path));
    Ok(())
}

#[cfg(feature = "serde")]
fn collect_feature_gates() -> Vec<String> {
    let mut gates = Vec::new();
    if cfg!(feature = "serde") {
        gates.push("serde".to_string());
    }
    if cfg!(feature = "cursor_state") {
        gates.push("cursor_state".to_string());
    }
    if cfg!(feature = "tree_builder_slice_trace") {
        gates.push("tree_builder_slice_trace".to_string());
    }
    gates.sort();
    gates
}

#[cfg(feature = "serde")]
fn compute_payload_hash(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&data)?;
    Ok(hash_value(&value))
}

#[cfg(feature = "serde")]
fn hash_value(value: &Value) -> String {
    let mut canonical = String::new();
    write_canonical_json(value, &mut canonical);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest)
}

#[cfg(feature = "serde")]
fn write_canonical_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(num) => out.push_str(&num.to_string()),
        Value::String(s) => {
            out.push_str(&serde_json::to_string(s).expect("string serialization should succeed"));
        }
        Value::Array(items) => {
            out.push('[');
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_canonical_json(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (idx, key) in keys.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str(
                    &serde_json::to_string(key).expect("object key serialization should succeed"),
                );
                out.push(':');
                if let Some(value) = map.get(*key) {
                    write_canonical_json(value, out);
                }
            }
            out.push('}');
        }
    }
}

#[cfg(feature = "serde")]
fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write;
    for byte in bytes {
        let _ = write!(&mut output, "{:02x}", byte);
    }
    output
}

#[cfg(feature = "serde")]
fn schema_hash_for_regression(name: &str) -> String {
    match name {
        "subset_regression.json" => SUBSET_SCHEMA_HASH.to_string(),
        "delta_regression.json" => DELTA_SCHEMA_HASH.to_string(),
        "engine_regression.json" => ENGINE_SCHEMA_HASH.to_string(),
        other => format!("serde_fixtures::{}", other.trim_end_matches(".json")),
    }
}

#[cfg(feature = "serde")]
fn manifest_display_path(path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let relative = workspace_root()
        .canonicalize()
        .ok()
        .and_then(|root| canonical.strip_prefix(&root).ok().map(|p| p.to_owned()));
    normalize_path(relative.unwrap_or(canonical))
}

#[cfg(feature = "serde")]
fn normalize_path(path: std::path::PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(feature = "serde")]
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[cfg(feature = "serde")]
fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_MANIFEST_RELATIVE)
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
fn export_tree_builder_trace(
    dir: &std::path::Path,
) -> Result<TreeBuilderTraceExportReport, Box<dyn std::error::Error>> {
    let events_store = Rc::new(RefCell::new(Vec::new()));
    let tracer = RecordingTracer::new(events_store.clone());
    let mut builder = TreeBuilder::<RopeInfo, String>::with_tracer(Box::new(tracer));

    builder.push_leaf("abc".to_owned());
    let source = Rope::from("012345");
    builder.push_slice(&source, Interval::new(1, 4));
    let _ = builder.build();

    let events_cell = Rc::try_unwrap(events_store)
        .map_err(|_| "failed to capture tree builder events due to outstanding references")?;
    let events = events_cell.into_inner();

    let serializable_events = convert_events(&events);

    let event_count = serializable_events.len();
    std::fs::create_dir_all(dir)?;
    let path = dir.join(TREE_BUILDER_TRACE_FILENAME);
    let mut json = serde_json::to_string_pretty(&serializable_events)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(&path, json)?;

    Ok(TreeBuilderTraceExportReport {
        file_name: TREE_BUILDER_TRACE_FILENAME.to_string(),
        file_path: path,
        event_count,
    })
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
fn convert_events(events: &[TreeBuilderEvent]) -> Vec<SerializableEvent> {
    let mut mapper = NodeIdMapper::new();
    events.iter().map(|event| SerializableEvent::from_event(event, &mut mapper)).collect()
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
struct NodeIdMapper {
    next: u64,
    map: HashMap<usize, u64>,
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl NodeIdMapper {
    fn new() -> Self {
        NodeIdMapper { next: 1, map: HashMap::new() }
    }

    fn map(&mut self, ptr: usize) -> u64 {
        if let Some(id) = self.map.get(&ptr) {
            *id
        } else {
            let id = self.next;
            self.next += 1;
            self.map.insert(ptr, id);
            id
        }
    }
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
#[derive(Serialize)]
struct SerializableEvent {
    kind: SerializableEventKind,
    depth: usize,
    node_height: usize,
    node_len: usize,
    node_id: u64,
    reuse: bool,
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl SerializableEvent {
    fn from_event(event: &TreeBuilderEvent, mapper: &mut NodeIdMapper) -> Self {
        SerializableEvent {
            kind: SerializableEventKind::from(&event.kind),
            depth: event.depth,
            node_height: event.node_height,
            node_len: event.node_len,
            node_id: mapper.map(event.node_ptr),
            reuse: event.reuse,
        }
    }
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
#[derive(Serialize)]
#[serde(tag = "kind")]
enum SerializableEventKind {
    PushFrame,
    ExtendFrame,
    MergePop { merged_children: usize },
    LeafSlice { interval: SerializableInterval },
    EnterChild { requested: SerializableInterval, translated: SerializableInterval },
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl From<&TreeBuilderEventKind> for SerializableEventKind {
    fn from(kind: &TreeBuilderEventKind) -> Self {
        match kind {
            TreeBuilderEventKind::PushFrame => SerializableEventKind::PushFrame,
            TreeBuilderEventKind::ExtendFrame => SerializableEventKind::ExtendFrame,
            TreeBuilderEventKind::MergePop { merged_children } => {
                SerializableEventKind::MergePop { merged_children: *merged_children }
            }
            TreeBuilderEventKind::LeafSlice { interval } => {
                SerializableEventKind::LeafSlice { interval: SerializableInterval::from(*interval) }
            }
            TreeBuilderEventKind::EnterChild { requested, translated } => {
                SerializableEventKind::EnterChild {
                    requested: SerializableInterval::from(*requested),
                    translated: SerializableInterval::from(*translated),
                }
            }
        }
    }
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
#[derive(Serialize)]
struct SerializableInterval {
    start: usize,
    end: usize,
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl From<Interval> for SerializableInterval {
    fn from(interval: Interval) -> Self {
        SerializableInterval { start: interval.start(), end: interval.end() }
    }
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
struct RecordingTracer {
    events: Rc<RefCell<Vec<TreeBuilderEvent>>>,
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl RecordingTracer {
    fn new(events: Rc<RefCell<Vec<TreeBuilderEvent>>>) -> Self {
        RecordingTracer { events }
    }
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
impl TreeBuilderTracer<RopeInfo, String> for RecordingTracer {
    fn record(&mut self, event: TreeBuilderEvent) {
        self.events.borrow_mut().push(event);
    }
}
