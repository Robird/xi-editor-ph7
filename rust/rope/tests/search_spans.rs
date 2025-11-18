#[cfg(feature = "serde")]
mod serde_search_export {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;
    use xi_rope::serde_fixtures::search_spans::{SearchSpansFile, SEARCH_SPANS_FILENAME};

    #[test]
    fn exporter_creates_search_spans_file() {
        let temp_dir = tempdir().expect("create tempdir");
        let output_dir = temp_dir.path().join("search_spans");
        let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
            .args(["--search-spans", output_dir.to_str().expect("path to str")])
            .status()
            .expect("run exporter");
        assert!(status.success(), "exporter exited with {:?}", status);

        let export_path = output_dir.join(SEARCH_SPANS_FILENAME);
        assert!(export_path.exists(), "missing search spans export");
        let data = fs::read_to_string(&export_path).expect("read search spans export");
        let payload: SearchSpansFile = serde_json::from_str(&data).expect("parse search spans");

        assert_eq!(payload.metadata.schema_version, "1.0.0");
        assert_eq!(payload.metadata.case_count, payload.search_cases.len());
        assert!(!payload.search_cases.is_empty(), "expected search cases");
        for case in &payload.search_cases {
            assert!(!case.hits.is_empty(), "case must contain hits");
            for hit in &case.hits {
                assert!(hit.range.end > hit.range.start, "range must cover data");
                assert!(hit.context_before.len() <= 80);
                assert!(hit.context_after.len() <= 80);
            }
            assert!(case.span_windows.len() >= case.hits.len());
        }
    }
}
