use std::process::Command;

#[test]
fn download_flag_conflicts_with_lookup_and_explicit_data_operations() {
    for conflicting in [
        vec!["word"],
        vec!["--data", "pack"],
        vec!["--info"],
        vec!["--verify-data"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wordglide"))
            .arg("--download-data")
            .args(conflicting)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    }
}

#[test]
fn missing_data_metadata_and_verification_remain_strict_and_explain_download() {
    let root = tempfile::tempdir().unwrap();
    for operation in ["--info", "--verify-data"] {
        let output = Command::new(env!("CARGO_BIN_EXE_wordglide"))
            .args([
                operation,
                "--data",
                root.path().join("absent").to_str().unwrap(),
            ])
            .env_remove("WORDGLIDE_DATA")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("wordglide --download-data"));
    }
}

#[test]
fn public_download_can_be_cancelled_before_network_or_file_operations() {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let error = wordglide::download_data_with_cancel(cancel).unwrap_err();
    assert!(error.to_string().contains("Download cancelled"));
}

#[test]
fn print_and_json_cli_modes_and_prune_data() {
    let dir = tempfile::tempdir().unwrap();
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample");
    let pack = dir.path().join("pack");
    wordglide::build_pack(
        &sample.join("entries.jsonl"),
        &sample.join("source.json"),
        &pack,
    )
    .unwrap();

    // --print --no-color writes clean plain text without ANSI escapes
    let plain = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--print", "--no-color", "--data"])
        .arg(&pack)
        .arg("take")
        .output()
        .unwrap();
    assert!(plain.status.success(), "{:?}", plain);
    let stdout = String::from_utf8(plain.stdout).unwrap();
    assert!(stdout.contains("take"), "{stdout}");
    assert!(!stdout.contains("\x1b["), "unexpected ANSI in plain print");

    // --json writes valid JSON matching Preview schema
    let json = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--json", "--data"])
        .arg(&pack)
        .arg("take")
        .output()
        .unwrap();
    assert!(json.status.success(), "{:?}", json);
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(parsed["entry"]["headword"], "take");
    assert!(parsed["related"].is_array());

    // --print and --json conflict with each other and missing word exits non-zero
    let conflict = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--print", "--json", "--data"])
        .arg(&pack)
        .arg("take")
        .output()
        .unwrap();
    assert_eq!(conflict.status.code(), Some(2));

    let missing = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--print", "--data"])
        .arg(&pack)
        .arg("nonexistent_word_xyz_check")
        .output()
        .unwrap();
    assert!(!missing.status.success());

    // --print and --json exit cleanly on BrokenPipe when piped consumer closes early
    for flag in ["--print", "--json"] {
        let (rx, tx) = std::io::pipe().unwrap();
        drop(rx);
        let broken = Command::new(env!("CARGO_BIN_EXE_wordglide"))
            .args([flag, "--data"])
            .arg(&pack)
            .arg("take")
            .stdout(std::process::Stdio::from(tx))
            .output()
            .unwrap();
        assert!(
            broken.status.success(),
            "{flag}: {:?}",
            String::from_utf8_lossy(&broken.stderr)
        );
    }

    // --prune-data exits cleanly even with empty user data dir, and preserves active+rollback packs
    let xdg = tempfile::tempdir().unwrap();
    let prune = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .arg("--prune-data")
        .env("XDG_DATA_HOME", xdg.path())
        .env("HOME", xdg.path())
        .output()
        .unwrap();
    assert!(
        prune.status.success(),
        "{:?}",
        String::from_utf8_lossy(&prune.stderr)
    );

    let dl_root = if cfg!(target_os = "macos") {
        xdg.path()
            .join("Library/Application Support/org.wordglide.dict/downloads")
    } else {
        xdg.path().join("dict/downloads")
    };
    let packs_dir = dl_root.join("packs");
    let active_dir = packs_dir.join("pack-active1");
    let rollback_dir = packs_dir.join("pack-rollback1");
    let stale_dir = packs_dir.join("pack-stale1");
    for dir in [&active_dir, &rollback_dir, &stale_dir] {
        std::fs::create_dir_all(dir.join("english-pack")).unwrap();
        std::fs::copy(
            pack.join("manifest.json"),
            dir.join("english-pack/manifest.json"),
        )
        .unwrap();
    }
    std::fs::write(
        dl_root.join("current.json"),
        serde_json::json!({
            "directory": "packs/pack-active1",
            "previous_directory": "packs/pack-rollback1",
            "archive_sha256": "0".repeat(64),
            "release": "v1",
            "snapshot": "2026-11-13"
        })
        .to_string(),
    )
    .unwrap();
    let prune_populated = Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .arg("--prune-data")
        .env("XDG_DATA_HOME", xdg.path())
        .env("HOME", xdg.path())
        .output()
        .unwrap();
    assert!(
        prune_populated.status.success(),
        "{:?}",
        String::from_utf8_lossy(&prune_populated.stderr)
    );
    assert!(active_dir.exists(), "active pack must remain");
    assert!(rollback_dir.exists(), "rollback pack must remain");
    assert!(!stale_dir.exists(), "stale pack must be pruned");
}
