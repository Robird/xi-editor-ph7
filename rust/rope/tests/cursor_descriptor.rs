use std::ptr;

use xi_rope::tree::{Cursor, TreeBuilder};
use xi_rope::{LinesMetric, Rope, RopeInfo};

fn build_deep_rope() -> Rope {
    const LEAF_SIZE: usize = 600;
    const LEAF_COUNT_EXP: u32 = 5; // 8^5 leaves ~32k to guarantee height > cache size

    let mut builder = TreeBuilder::<RopeInfo, String>::new();
    let leaf_payload = "x".repeat(LEAF_SIZE);
    let leaf_count = 8usize.pow(LEAF_COUNT_EXP);
    for _ in 0..leaf_count {
        builder.push_leaf(leaf_payload.clone());
    }
    builder.build()
}

#[test]
fn cursor_descriptor_round_trip_basic() {
    let text = Rope::from("one line\ntwo line\nthree\n");
    let positions = [0, 4, 9, text.len()];

    for &pos in &positions {
        let cursor = Cursor::new(&text, pos);
        let descriptor = cursor.to_descriptor();
        assert!(descriptor.is_valid(), "descriptor should be valid at position {pos}", pos = pos);

        let (orig_leaf, orig_offset) = cursor.get_leaf().unwrap();

        let restored = descriptor.restore(&text).expect("restore should succeed");
        assert_eq!(restored.pos(), cursor.pos());
        let (rest_leaf, rest_offset) = restored.get_leaf().unwrap();
        assert!(ptr::eq(rest_leaf, orig_leaf));
        assert_eq!(rest_offset, orig_offset);

        let mut fresh = Cursor::new(&text, 0);
        let applied = fresh.apply_descriptor(&descriptor);
        assert!(applied, "apply_descriptor should succeed on shared root");
        assert_eq!(fresh.pos(), cursor.pos());
        assert_eq!(fresh.get_leaf().unwrap().1, orig_offset);
        let (fresh_leaf, _) = fresh.get_leaf().unwrap();
        assert!(ptr::eq(fresh_leaf, orig_leaf));
    }
}

#[test]
fn cursor_descriptor_invalidates_after_rebuild() {
    let text = Rope::from("abcdefghij");
    let cursor = Cursor::new(&text, 3);
    let descriptor = cursor.to_descriptor();
    assert!(descriptor.is_valid());

    let rebuilt = Rope::from("abcdefghijk");
    assert!(descriptor.restore(&rebuilt).is_none(), "restore should fail on rebuilt tree");

    let mut fallback = Cursor::new(&rebuilt, 0);
    let before = fallback.get_leaf().unwrap();
    let before_ptr = before.0 as *const String;
    let before_offset = before.1;
    assert!(!fallback.apply_descriptor(&descriptor));
    let after = fallback.get_leaf().unwrap();
    assert_eq!(after.0 as *const String, before_ptr);
    assert_eq!(after.1, before_offset);
    assert_eq!(fallback.pos(), 0);
}

#[test]
fn cursor_descriptor_handles_deep_paths() {
    let rope = build_deep_rope();
    let midpoint = rope.len() / 2;
    let cursor = Cursor::new(&rope, midpoint);
    let descriptor = cursor.to_descriptor();

    assert!(descriptor.is_valid());
    assert!(descriptor.depth() > 4, "expected depth > cache size, got {}", descriptor.depth());

    let restored = descriptor.restore(&rope).expect("restore should succeed for deep rope");
    assert_eq!(restored.pos(), cursor.pos());
    assert_eq!(restored.get_leaf().unwrap().1, cursor.get_leaf().unwrap().1);

    let mut fresh = Cursor::new(&rope, 0);
    assert!(fresh.apply_descriptor(&descriptor));
    assert_eq!(fresh.pos(), cursor.pos());
}

#[test]
fn cursor_descriptor_rejects_invalid_snapshot() {
    let text = Rope::from("abc");
    let mut cursor = Cursor::new(&text, text.len());
    assert!(cursor.next::<LinesMetric>().is_none());

    let descriptor = cursor.to_descriptor();
    assert!(!descriptor.is_valid());
    assert!(descriptor.restore(&text).is_none());

    let mut fresh = Cursor::new(&text, 0);
    let start_pos = fresh.pos();
    assert!(!fresh.apply_descriptor(&descriptor));
    assert_eq!(fresh.pos(), start_pos);
}

#[cfg(feature = "serde")]
#[test]
fn exporter_creates_cursor_descriptor_file() {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;
    use xi_rope::serde_fixtures::cursor_descriptors::{
        CursorDescriptorFixture, DescriptorMetric, CURSOR_DESCRIPTOR_FILENAME,
    };

    let temp_dir = tempdir().expect("create tempdir");
    let output_dir = temp_dir.path().join("cursor_fixtures");
    let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
        .args(["--cursor-descriptors", output_dir.to_str().expect("path to str")])
        .status()
        .expect("failed to run exporter");
    assert!(status.success(), "exporter exited with {:?}", status);

    let export_path = output_dir.join(CURSOR_DESCRIPTOR_FILENAME);
    assert!(export_path.exists(), "missing cursor descriptor export");
    let data = fs::read_to_string(&export_path).expect("read export");
    let fixtures: Vec<CursorDescriptorFixture> =
        serde_json::from_str(&data).expect("parse cursor fixtures");
    assert!(fixtures.len() >= 10, "expected >=10 fixtures, got {}", fixtures.len());
    assert!(fixtures.iter().any(|f| f.metric == DescriptorMetric::Lines));
    assert!(fixtures.iter().any(|f| f.metric == DescriptorMetric::Utf16));
}

#[cfg(feature = "cursor_state")]
mod cursor_state_tests {
    use super::*;
    use std::any::type_name;

    use xi_rope::rope::{BaseMetric, Utf16CodeUnitsMetric};
    use xi_rope::tree::{CursorState, Metric};

    const SAMPLE_TEXT: &str = "zero\none\u{1F600}two\nthree\u{1F4A9}four\nlast line";

    fn sample_rope() -> Rope {
        Rope::from(SAMPLE_TEXT)
    }

    fn collect_test_positions<M>(rope: &Rope) -> Vec<usize>
    where
        M: Metric<RopeInfo, String>,
    {
        let mut cursor = Cursor::new(rope, 0);
        let mut positions = vec![cursor.pos()];
        while let Some(next) = cursor.next::<M>() {
            positions.push(next);
        }
        if positions.last().copied() != Some(rope.len()) {
            positions.push(rope.len());
        }

        let mut enriched = positions.clone();
        for &pos in &positions {
            if pos > 0 {
                enriched.push(pos - 1);
            }
            if pos + 1 < rope.len() {
                enriched.push(pos + 1);
            }
            if pos + 2 < rope.len() {
                enriched.push(pos + 2);
            }
        }

        enriched.sort_unstable();
        enriched.dedup();
        enriched
    }

    fn assert_state_navigation_parity<M>(rope: &Rope, metric_name: &str)
    where
        M: Metric<RopeInfo, String>,
    {
        let positions = collect_test_positions::<M>(rope);
        for &pos in &positions {
            let cursor_at_pos = Cursor::new(rope, pos);
            let state = cursor_at_pos.state();
            let leaf_offset_at_pos = cursor_at_pos.get_leaf().map(|(_, offset)| offset);
            assert_eq!(
                state.position(),
                pos,
                "state position mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );
            assert!(
                state.is_valid(),
                "state should be valid for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );

            let descriptor = state.to_descriptor();
            let mut from_descriptor = Cursor::new(rope, 0);
            assert!(
                from_descriptor.apply_descriptor(&descriptor),
                "descriptor apply failed for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );
            assert_eq!(
                from_descriptor.pos(),
                pos,
                "descriptor restore pos mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );
            assert_eq!(
                from_descriptor.get_leaf().map(|(_, offset)| offset),
                leaf_offset_at_pos,
                "descriptor leaf offset mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );

            // Verify next navigation parity.
            let mut baseline_next = Cursor::new(rope, pos);
            let expected_next = baseline_next.next::<M>();
            let expected_next_pos = baseline_next.pos();
            let expected_next_leaf_offset = baseline_next.get_leaf().map(|(_, offset)| offset);

            let mut restored_next = state.restore(rope).expect("state restore should succeed");
            let actual_next = restored_next.next::<M>();
            assert_eq!(
                actual_next,
                expected_next,
                "next mismatch for {metric}: {info}",
                metric = metric_name,
                info = pos
            );
            assert_eq!(
                restored_next.pos(),
                expected_next_pos,
                "next pos mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );
            assert_eq!(
                restored_next.get_leaf().map(|(_, offset)| offset),
                expected_next_leaf_offset,
                "next leaf offset mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );

            let restored_state_after_next = restored_next.state();
            let baseline_state_after_next = baseline_next.state();
            assert_eq!(
                restored_state_after_next.position(),
                baseline_state_after_next.position(),
                "state position after next mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );
            assert_eq!(
                restored_state_after_next.offset_of_leaf(),
                baseline_state_after_next.offset_of_leaf(),
                "state leaf offset after next mismatch for {metric_name} at {pos}",
                metric_name = metric_name,
                pos = pos
            );

            if pos > 0 {
                let mut baseline_prev = Cursor::new(rope, pos);
                let expected_prev = baseline_prev.prev::<M>();
                let expected_prev_pos = baseline_prev.pos();
                let expected_prev_leaf_offset = baseline_prev.get_leaf().map(|(_, offset)| offset);

                let mut restored_prev = state.restore(rope).expect("state restore should succeed");
                let actual_prev = restored_prev.prev::<M>();
                assert_eq!(
                    actual_prev,
                    expected_prev,
                    "prev mismatch for {metric}: {info}",
                    metric = metric_name,
                    info = pos
                );
                assert_eq!(
                    restored_prev.pos(),
                    expected_prev_pos,
                    "prev pos mismatch for {metric_name} at {pos}",
                    metric_name = metric_name,
                    pos = pos
                );
                assert_eq!(
                    restored_prev.get_leaf().map(|(_, offset)| offset),
                    expected_prev_leaf_offset,
                    "prev leaf offset mismatch for {metric_name} at {pos}",
                    metric_name = metric_name,
                    pos = pos
                );

                let restored_state_after_prev = restored_prev.state();
                let baseline_state_after_prev = baseline_prev.state();
                assert_eq!(
                    restored_state_after_prev.position(),
                    baseline_state_after_prev.position(),
                    "state position after prev mismatch for {metric_name} at {pos}",
                    metric_name = metric_name,
                    pos = pos
                );
                assert_eq!(
                    restored_state_after_prev.offset_of_leaf(),
                    baseline_state_after_prev.offset_of_leaf(),
                    "state leaf offset after prev mismatch for {metric_name} at {pos}",
                    metric_name = metric_name,
                    pos = pos
                );
            }
        }
    }

    #[test]
    fn cursor_state_round_trip_basic() {
        let text = Rope::from("one line\ntwo line\nthree\n");
        let positions = [0, 4, 9, text.len()];

        for &pos in &positions {
            let cursor = Cursor::new(&text, pos);
            let state = cursor.state();
            assert_eq!(state.is_valid(), cursor.get_leaf().is_some());

            if state.is_valid() {
                let restored = state.restore(&text).expect("state restore should succeed");
                assert_eq!(restored.pos(), cursor.pos());
                assert_eq!(restored.get_leaf().unwrap().1, cursor.get_leaf().unwrap().1);

                let restored_state = restored.state();
                assert_eq!(restored_state.position(), state.position());
                assert_eq!(restored_state.offset_of_leaf(), state.offset_of_leaf());
            } else {
                assert!(state.restore(&text).is_none());
            }

            let descriptor_from_state = state.to_descriptor();
            let state_from_descriptor = CursorState::from_descriptor(&descriptor_from_state);
            assert_eq!(state_from_descriptor.position(), state.position());
            assert_eq!(state_from_descriptor.offset_of_leaf(), state.offset_of_leaf());
            assert_eq!(state_from_descriptor.is_valid(), state.is_valid());

            let mut fresh = Cursor::new(&text, 0);
            let applied = fresh.apply_descriptor(&descriptor_from_state);
            if state.is_valid() {
                assert!(applied, "descriptor generated from state should apply");
                assert_eq!(fresh.pos(), cursor.pos());
                assert_eq!(fresh.get_leaf().unwrap().1, cursor.get_leaf().unwrap().1);
            } else {
                assert!(!applied, "invalid state descriptor should not apply");
            }

            let via_from_cursor = CursorState::from_cursor(&cursor);
            assert_eq!(via_from_cursor.position(), state.position());
            assert_eq!(via_from_cursor.offset_of_leaf(), state.offset_of_leaf());
            assert_eq!(via_from_cursor.is_valid(), state.is_valid());
        }

        let mut invalid_cursor = Cursor::new(&text, text.len());
        assert!(invalid_cursor.next::<LinesMetric>().is_none());
        let invalid_state = invalid_cursor.state();
        assert!(!invalid_state.is_valid());
        assert!(invalid_state.restore(&text).is_none());
    }

    #[test]
    fn cursor_state_handles_deep_paths() {
        let rope = build_deep_rope();
        let mut cursor = Cursor::new(&rope, rope.len() / 2);

        for _ in 0..8 {
            let state = cursor.state();
            assert!(state.is_valid());
            assert!(state.frames().len() > 4, "expected full path depth cached");
            let round_trip = state.restore(&rope).expect("restore should succeed");
            assert_eq!(round_trip.pos(), cursor.pos());
            assert_eq!(round_trip.get_leaf().unwrap().1, cursor.get_leaf().unwrap().1);

            if cursor.next_leaf().is_none() {
                break;
            }
        }
    }

    #[test]
    fn cursor_state_invalidates_after_edit() {
        let text = Rope::from("abcdefghij");
        let cursor = Cursor::new(&text, 3);
        let state = cursor.state();
        assert!(state.is_valid());

        let rebuilt = Rope::from("abcdefghijk");
        assert!(state.restore(&rebuilt).is_none(), "restore should fail on rebuilt tree");

        let descriptor = state.to_descriptor();
        let mut fallback = Cursor::new(&rebuilt, 0);
        let before = fallback.get_leaf().unwrap();
        assert!(!fallback.apply_descriptor(&descriptor));
        let after = fallback.get_leaf().unwrap();
        assert_eq!(after.0 as *const String, before.0 as *const String);
        assert_eq!(after.1, before.1);
        assert_eq!(fallback.pos(), 0);

        let state_from_descriptor = CursorState::from_descriptor(&descriptor);
        assert_eq!(state_from_descriptor.is_valid(), state.is_valid());
    }

    #[test]
    fn cursor_state_preserves_navigation_base_metric() {
        let rope = sample_rope();
        assert_state_navigation_parity::<BaseMetric>(&rope, type_name::<BaseMetric>());
    }

    #[test]
    fn cursor_state_preserves_navigation_lines_metric() {
        let rope = sample_rope();
        assert_state_navigation_parity::<LinesMetric>(&rope, type_name::<LinesMetric>());
    }

    #[test]
    fn cursor_state_preserves_navigation_utf16_metric() {
        let rope = sample_rope();
        assert_state_navigation_parity::<Utf16CodeUnitsMetric>(
            &rope,
            type_name::<Utf16CodeUnitsMetric>(),
        );
    }
}
