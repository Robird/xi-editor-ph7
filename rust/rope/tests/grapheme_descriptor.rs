#[cfg(feature = "serde")]
mod serde_grapheme_export {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;
    use xi_rope::serde_fixtures::grapheme_descriptors::{
        GraphemeDescriptorFile, GRAPHEME_DESCRIPTOR_FILENAME,
    };

    #[test]
    fn exporter_creates_grapheme_descriptor_file() {
        let temp_dir = tempdir().expect("create tempdir");
        let output_dir = temp_dir.path().join("grapheme_fixtures");
        let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
            .args(["--grapheme-descriptors", output_dir.to_str().expect("path to str")])
            .status()
            .expect("run exporter");
        assert!(status.success(), "exporter exited with {:?}", status);

        let export_path = output_dir.join(GRAPHEME_DESCRIPTOR_FILENAME);
        assert!(export_path.exists(), "missing grapheme descriptor export");
        let data = fs::read_to_string(&export_path).expect("read grapheme descriptor export");
        let fixtures: GraphemeDescriptorFile =
            serde_json::from_str(&data).expect("parse grapheme descriptor fixtures");

        assert!(
            fixtures.grapheme_descriptors.len() >= 10,
            "expected >=10 grapheme descriptors, got {}",
            fixtures.grapheme_descriptors.len()
        );
        assert_eq!(fixtures.metadata.schema_version, "1.0.0");
        assert!(fixtures.metadata.descriptor_count >= fixtures.grapheme_descriptors.len());
        assert!(fixtures.grapheme_descriptors.iter().any(|d| d.contains_zwj));
        assert!(fixtures.grapheme_descriptors.iter().any(|d| d.crosses_leaf));
        assert!(fixtures.grapheme_descriptors.iter().any(|d| d.requires_fallback));
    }
}
