#[cfg(feature = "serde")]
mod serde_diff_export {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use tempfile::{tempdir_in, TempDir};
    use xi_rope::serde_fixtures::diff_regions::{
        DiffOpKind, DiffRegionsFile, DIFF_REGIONS_FILENAME,
    };

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("workspace root")
    }

    fn temp_output_dir() -> (TempDir, PathBuf) {
        let root = workspace_root();
        let target_dir = root.join("target");
        std::fs::create_dir_all(&target_dir).expect("create target dir");
        let temp_dir = tempdir_in(target_dir).expect("create scoped tempdir");
        let output_dir = temp_dir.path().join("diff_regions_export");
        (temp_dir, output_dir)
    }

    #[test]
    fn exporter_creates_diff_regions_file() {
        let (_temp_guard, output_dir) = temp_output_dir();
        let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
            .args(["--diff-regions", output_dir.to_str().expect("path to str")])
            .status()
            .expect("run exporter");
        assert!(status.success(), "exporter exited with {:?}", status);

        let export_path = output_dir.join(DIFF_REGIONS_FILENAME);
        assert!(export_path.exists(), "missing diff regions export");
        let data = fs::read_to_string(&export_path).expect("read diff regions export");
        let payload: DiffRegionsFile = serde_json::from_str(&data).expect("parse diff regions");

        assert_eq!(payload.metadata.schema_version, "1.0.0");
        assert_eq!(payload.metadata.case_count, payload.diff_cases.len());
        assert!(!payload.diff_cases.is_empty(), "expected diff cases");

        let mut saw_insert = false;
        let mut saw_delete = false;
        for case in &payload.diff_cases {
            assert!(
                !std::path::Path::new(&case.base_path).is_absolute(),
                "base path should be relative"
            );
            assert!(
                !std::path::Path::new(&case.target_path).is_absolute(),
                "target path should be relative"
            );
            assert!(!case.ops.is_empty(), "diff case must contain ops");
            for op in &case.ops {
                match op.kind {
                    DiffOpKind::Insert => saw_insert = true,
                    DiffOpKind::Delete => saw_delete = true,
                    DiffOpKind::Copy => {}
                }
                assert!(op.byte_len > 0, "byte_len must be positive");
            }
        }
        assert!(saw_insert, "should include at least one insert op");
        assert!(saw_delete, "should include at least one delete op");
    }
}
