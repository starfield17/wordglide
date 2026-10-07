use std::process::Command;

#[test]
fn download_flag_conflicts_with_lookup_and_explicit_data_operations() {
    for conflicting in [vec!["word"], vec!["--data", "pack"], vec!["--info"], vec!["--verify-data"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_wordglide"))
            .arg("--download-data").args(conflicting).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    }
}

#[test]
fn missing_data_explains_the_explicit_download_command() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--data", root.path().join("absent").to_str().unwrap()])
        .env_remove("WORDGLIDE_DATA").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wordglide --download-data"));
}
