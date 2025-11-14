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
