#[cfg(feature = "serde")]
mod serde_breaks_export {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;
    use xi_rope::serde_fixtures::breaks_descriptors::{
        BreakMetricKind, BreaksDescriptorFile, BREAKS_DESCRIPTOR_FILENAME,
    };

    #[test]
    fn exporter_creates_breaks_descriptor_file() {
        let temp_dir = tempdir().expect("create tempdir");
        let output_dir = temp_dir.path().join("breaks_fixtures");
        let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
            .args(["--breaks-descriptors", output_dir.to_str().expect("path to str")])
            .status()
            .expect("run exporter");
        assert!(status.success(), "exporter exited with {:?}", status);

        let export_path = output_dir.join(BREAKS_DESCRIPTOR_FILENAME);
        assert!(export_path.exists(), "missing breaks descriptor export");
        let data = fs::read_to_string(&export_path).expect("read breaks descriptor export");
        let payload: BreaksDescriptorFile =
            serde_json::from_str(&data).expect("parse breaks descriptor fixtures");

        assert_eq!(payload.metadata.schema_version, "1.0.0");
        assert_eq!(payload.metadata.descriptor_count, payload.break_sets.len());
        assert!(!payload.break_sets.is_empty(), "expected at least one break set");
        assert!(payload
            .break_sets
            .iter()
            .all(|set| set.metric == BreakMetricKind::BreaksMetric));
        assert!(payload.break_sets.iter().any(|set| !set.leaf_runs.is_empty()));
        assert!(payload
            .break_sets
            .iter()
            .any(|set| set.tags.iter().any(|tag| tag == "emoji")));
        assert!(payload
            .break_sets
            .iter()
            .any(|set| set.break_count == set.break_offsets.len()));
    }
}
