#[cfg(not(feature = "serde"))]
fn main() {
    eprintln!("export-serde-fixtures requires the `serde` feature to be enabled.");
    std::process::exit(1);
}

#[cfg(feature = "serde")]
use std::{env, path::PathBuf};

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
use serde::Serialize;

#[cfg(feature = "serde")]
use xi_rope::serde_fixtures::{fixtures, Fixture};

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
use xi_rope::{
    tree::{TreeBuilder, TreeBuilderEvent, TreeBuilderEventKind, TreeBuilderTracer},
    Interval, Rope, RopeInfo,
};

#[cfg(feature = "serde")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut output_dir: Option<PathBuf> = None;
    let mut trace_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" | "--output-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--dir requires a value specifying the output directory")?;
                output_dir = Some(PathBuf::from(value));
            }
            "--tree-builder-trace" | "--tree-builder-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--tree-builder-trace requires a value specifying the output directory")?;
                trace_dir = Some(PathBuf::from(value));
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

    if output_dir.is_none() && trace_dir.is_none() {
        print_usage();
        return Err("missing required --dir <PATH> or --tree-builder-trace <PATH> argument".into());
    }

    if let Some(dir) = output_dir {
        export_to_directory(dir.as_path(), fixtures())?;
    }

    if let Some(dir) = trace_dir {
        handle_tree_builder_trace(dir)?;
    }

    Ok(())
}

#[cfg(feature = "serde")]
fn print_usage() {
    eprintln!(
        "Usage: cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --dir <PATH> [--tree-builder-trace <PATH>]\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --tree-builder-trace <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --list"
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
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;

    for fixture in fixtures {
        let mut content = fixture.json.to_owned();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        let path = dir.join(fixture.name);
        std::fs::write(path, content)?;
    }

    Ok(())
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
fn handle_tree_builder_trace(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    export_tree_builder_trace(dir.as_path())
}

#[cfg(all(feature = "serde", not(feature = "tree_builder_slice_trace")))]
fn handle_tree_builder_trace(_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    Err("--tree-builder-trace requires the `tree_builder_slice_trace` feature to be enabled".into())
}

#[cfg(all(feature = "serde", feature = "tree_builder_slice_trace"))]
fn export_tree_builder_trace(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
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

    std::fs::create_dir_all(dir)?;
    let path = dir.join("basic_slice_plan.json");
    let mut json = serde_json::to_string_pretty(&serializable_events)?;
    if !json.ends_with('\n') {
        json.push('\n');
    }
    std::fs::write(path, json)?;

    Ok(())
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
            TreeBuilderEventKind::EnterChild { requested, translated } => SerializableEventKind::EnterChild {
                requested: SerializableInterval::from(*requested),
                translated: SerializableInterval::from(*translated),
            },
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
