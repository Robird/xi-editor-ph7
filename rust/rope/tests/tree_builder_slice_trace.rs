#![cfg(feature = "tree_builder_slice_trace")]

use std::cell::RefCell;
use std::rc::Rc;

use xi_rope::tree::{TreeBuilder, TreeBuilderEvent, TreeBuilderEventKind, TreeBuilderTracer};
use xi_rope::{Interval, Rope, RopeInfo};

struct RecordingTracer {
    events: Rc<RefCell<Vec<TreeBuilderEvent>>>,
}

impl RecordingTracer {
    fn new(store: Rc<RefCell<Vec<TreeBuilderEvent>>>) -> Self {
        RecordingTracer { events: store }
    }
}

impl TreeBuilderTracer<RopeInfo, String> for RecordingTracer {
    fn record(&mut self, event: TreeBuilderEvent) {
        self.events.borrow_mut().push(event);
    }
}

#[test]
fn tree_builder_emits_events() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let tracer = RecordingTracer::new(events.clone());
    let mut builder = TreeBuilder::<RopeInfo, String>::with_tracer(Box::new(tracer));

    builder.push_leaf("abc".to_owned());
    let source = Rope::from("012345");
    builder.push_slice(&source, Interval::new(1, 4));
    let _ = builder.build();

    let collected = events.borrow();
    let has_push =
        collected.iter().any(|event| matches!(event.kind, TreeBuilderEventKind::PushFrame));
    let has_leaf =
        collected.iter().any(|event| matches!(event.kind, TreeBuilderEventKind::LeafSlice { .. }));
    assert!(has_push, "expected at least one PushFrame event");
    assert!(has_leaf, "expected at least one LeafSlice event");
}
