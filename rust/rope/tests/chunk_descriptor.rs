#[cfg(feature = "serde")]
mod serde_chunk_export {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;
    use xi_rope::serde_fixtures::chunk_descriptors::{
        ChunkDescriptorFile, LineEndingKind, CHUNK_DESCRIPTOR_FILENAME,
    };

    #[test]
    fn exporter_creates_chunk_and_line_descriptor_files() {
        let temp_dir = tempdir().expect("create tempdir");
        let output_dir = temp_dir.path().join("chunk_fixtures");
        let status = Command::new(env!("CARGO_BIN_EXE_export-serde-fixtures"))
            .args(["--chunk-descriptors", output_dir.to_str().expect("path to str")])
            .status()
            .expect("run exporter");
        assert!(status.success(), "exporter exited with {:?}", status);

        let export_path = output_dir.join(CHUNK_DESCRIPTOR_FILENAME);
        assert!(export_path.exists(), "chunk descriptor export missing");
        let data = fs::read_to_string(&export_path).expect("read chunk descriptor export");
        let fixtures: ChunkDescriptorFile =
            serde_json::from_str(&data).expect("parse chunk descriptor file");

        assert!(
            fixtures.chunk_descriptors.len() >= 8,
            "expected >=8 chunk descriptors, got {}",
            fixtures.chunk_descriptors.len()
        );
        assert!(
            fixtures.line_descriptors.len() >= 4,
            "expected >=4 line descriptors, got {}",
            fixtures.line_descriptors.len()
        );
        assert_eq!(fixtures.metadata.schema_version, "1.0.0");
        assert!(fixtures.metadata.chunk_descriptor_count >= fixtures.chunk_descriptors.len());
        assert!(fixtures.metadata.line_descriptor_count >= fixtures.line_descriptors.len());
        assert!(fixtures.chunk_descriptors.iter().any(|d| d.contains_crlf));
        assert!(fixtures.chunk_descriptors.iter().any(|d| d.tags.iter().any(|tag| tag == "emoji")));
        assert!(fixtures
            .line_descriptors
            .iter()
            .any(|d| matches!(d.newline_kind, LineEndingKind::CrLf)));
    }
}
