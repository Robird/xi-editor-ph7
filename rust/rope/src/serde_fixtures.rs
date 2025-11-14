//! Shared serde regression fixtures for xi-rope.
//!
//! These constants centralise the golden JSON payloads used by tests and
//! external tooling so that they remain a single source of truth.

#![cfg(feature = "serde")]

/// Describes a single serde regression fixture.
#[derive(Copy, Clone, Debug)]
pub struct Fixture {
    pub name: &'static str,
    pub json: &'static str,
}

pub const SUBSET_FIXTURE: Fixture = Fixture {
    name: "subset_regression.json",
    json: r#"{"segments":[{"len":2,"count":0},{"len":3,"count":3},{"len":1,"count":0},{"len":1,"count":1},{"len":2,"count":0}]}"#,
};

pub const DELTA_FIXTURE: Fixture = Fixture {
    name: "delta_regression.json",
    json: r#"{"els":[{"copy":[0,3]},{"insert":"[ins]"},{"copy":[8,10]},{"insert":"!"},{"copy":[15,62]}],"base_len":62}"#,
};

pub const ENGINE_FIXTURE: Fixture = Fixture {
    name: "engine_regression.json",
    json: r#"{"text":"Hi there","tombstones":"Well, ","deletes_from_union":{"segments":[{"len":6,"count":1},{"len":8,"count":0}]},"undone_groups":[2],"revs":[{"rev_id":{"session1":0,"session2":0,"num":0},"max_undo_so_far":0,"edit":{"Undo":{"toggled_groups":[],"deletes_bitxor":{"segments":[]}}}},{"rev_id":{"session1":1,"session2":0,"num":1},"max_undo_so_far":0,"edit":{"Edit":{"priority":0,"undo_group":0,"inserts":{"segments":[{"len":2,"count":1}]},"deletes":{"segments":[{"len":2,"count":0}]}}}},{"rev_id":{"session1":1,"session2":0,"num":2},"max_undo_so_far":1,"edit":{"Edit":{"priority":1,"undo_group":1,"inserts":{"segments":[{"len":2,"count":0},{"len":6,"count":1}]},"deletes":{"segments":[{"len":8,"count":0}]}}}},{"rev_id":{"session1":1,"session2":0,"num":3},"max_undo_so_far":2,"edit":{"Edit":{"priority":0,"undo_group":2,"inserts":{"segments":[{"len":6,"count":1},{"len":8,"count":0}]},"deletes":{"segments":[{"len":14,"count":0}]}}}},{"rev_id":{"session1":1,"session2":0,"num":4},"max_undo_so_far":2,"edit":{"Undo":{"toggled_groups":[2],"deletes_bitxor":{"segments":[{"len":6,"count":1},{"len":8,"count":0}]}}}}]}"#,
};

pub const FIXTURES: [Fixture; 3] = [SUBSET_FIXTURE, DELTA_FIXTURE, ENGINE_FIXTURE];

/// Returns the registered fixtures as a slice for iteration.
pub const fn fixtures() -> &'static [Fixture] {
    &FIXTURES
}

/// Attempts to lookup a fixture by file name.
pub fn get_fixture(name: &str) -> Option<&'static Fixture> {
    let mut i = 0;
    while i < FIXTURES.len() {
        let fixture = &FIXTURES[i];
        if fixture.name == name {
            return Some(fixture);
        }
        i += 1;
    }
    None
}
