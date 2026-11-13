use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use wordglide::{
    AppearanceOverrides, Dictionary, PackInfo, RunOptions, ThemePreset, download_data_with_cancel,
    downloaded_data_path, pack_info, prune_downloaded_packs, run_with_options,
    run_without_dictionary, verify_pack,
};

#[derive(Parser)]
#[command(
    version,
    about = "Offline English dictionary: type to look up, no Enter needed"
)]
struct Args {
    /// Initial word or phrase (quote phrases in the shell).
    query: Option<String>,
    /// Directory containing manifest.json and the prepared data files.
    #[arg(long)]
    data: Option<PathBuf>,
    /// Download and verify the latest prepared dictionary, then exit.
    #[arg(long, conflicts_with_all = ["query", "data", "info", "verify_data", "prune_data", "print", "json"])]
    download_data: bool,
    /// Remove stale unreferenced downloaded dictionary packs while retaining the active and rollback packs, then exit.
    #[arg(long, conflicts_with_all = ["query", "data", "download_data", "info", "verify_data", "print", "json"])]
    prune_data: bool,
    /// Verify all data checksums, indexes, and database integrity, then exit.
    #[arg(long, conflicts_with_all = ["query", "print", "json"])]
    verify_data: bool,
    /// Print pack metadata and exit.
    #[arg(long, conflicts_with_all = ["query", "verify_data", "print", "json"])]
    info: bool,
    /// Print the matched dictionary entry cleanly to stdout and exit without opening the TUI.
    #[arg(long, conflicts_with_all = ["download_data", "prune_data", "info", "verify_data", "json"])]
    print: bool,
    /// Print the matched dictionary entry and candidate list as JSON to stdout and exit.
    #[arg(long, conflicts_with_all = ["download_data", "prune_data", "info", "verify_data", "print"])]
    json: bool,
    /// Disable colored output.
    #[arg(long)]
    no_color: bool,
    /// Disable mouse capture, keeping native terminal text selection.
    #[arg(long)]
    no_mouse: bool,
    /// Reading palette: default, orange, gruvbox_light, gruvbox_dark_v2, whiteout, nord, tokyo_night, dracula, sepia.
    #[arg(long)]
    theme: Option<ThemePreset>,
    /// Use the theme's background instead of the terminal background.
    #[arg(long, action = clap::ArgAction::Set)]
    theme_background: Option<bool>,
    /// Use RGB colors; false converts them to xterm 256 colors.
    #[arg(long, action = clap::ArgAction::Set)]
    truecolor: Option<bool>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.download_data {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // CLI owns the process-lifetime handler, including when output is redirected.
        signal_hook::flag::register(signal_hook::consts::SIGINT, std::sync::Arc::clone(&cancel))?;
        download_data_with_cancel(cancel)?;
        return Ok(());
    }
    if args.prune_data {
        let removed = prune_downloaded_packs()?;
        if removed == 0 {
            println!("No inactive dictionary packs to prune.");
        } else {
            println!("Pruned {removed} inactive dictionary pack(s).");
        }
        return Ok(());
    }
    let executable = std::env::current_exe().context("Cannot locate executable")?;
    let project_dirs = ProjectDirs::from("org", "wordglide", "dict");
    let user_data = project_dirs.as_ref().map(|d| d.data_dir().join("english"));
    let env_data = std::env::var_os("WORDGLIDE_DATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let path = resolve_with_download(
        args.data,
        env_data,
        &executable,
        user_data,
        downloaded_data_path,
    );
    if args.info {
        let path = path?;
        print_pack_info(&path, &pack_info(&path)?);
        return Ok(());
    }
    if args.verify_data {
        let path = path?;
        let started = std::time::Instant::now();
        let entries = verify_pack(&path).with_context(|| format!(
            "Cannot verify dictionary at {}. Run wordglide --download-data to install the latest compatible pack, or check --data / WORDGLIDE_DATA.", path.display()))?;
        println!(
            "Data pack verified: {entries} entries checked in {:.1}s",
            started.elapsed().as_secs_f64()
        );
        return Ok(());
    }
    let dictionary = path.and_then(|path| Dictionary::open(&path).with_context(|| format!(
        "Cannot open dictionary at {}. Check --data / WORDGLIDE_DATA overrides if an older pack is selected.", path.display())));
    if args.print || args.json {
        let query = args
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .with_context(|| {
                format!(
                    "Provide a word or phrase, e.g. wordglide --{} word",
                    if args.json { "json" } else { "print" }
                )
            })?;
        let mut dictionary = dictionary?;
        let candidates = dictionary.search(query)?;
        let first = candidates
            .first()
            .with_context(|| format!("No matches for {query:?}"))?;
        let preview = dictionary.preview(first)?;
        let result: std::io::Result<()> = (|| {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            if args.json {
                let payload = serde_json::json!({
                    "query": query,
                    "match": first,
                    "entry": &*preview.entry,
                    "related": preview.related.iter().map(|e| &**e).collect::<Vec<_>>(),
                    "candidates": &candidates,
                });
                serde_json::to_writer_pretty(&mut out, &payload).map_err(|e| {
                    std::io::Error::new(e.io_error_kind().unwrap_or(std::io::ErrorKind::Other), e)
                })?;
                writeln!(out)
            } else {
                use std::io::IsTerminal;
                let color = color_enabled(args.no_color) && std::io::stdout().is_terminal();
                let rendered = wordglide::format_preview(&preview, color);
                out.write_all(rendered.as_bytes())
            }
        })();
        if let Err(err) = result
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(err.into());
        }
        return Ok(());
    }
    let options = RunOptions {
        color: color_enabled(args.no_color),
        mouse: !args.no_mouse,
        appearance: AppearanceOverrides {
            color_theme: args.theme,
            theme_background: args.theme_background,
            truecolor: args.truecolor,
        },
        config_path: project_dirs.map(|d| d.config_dir().join("config.json")),
    };
    let query = args.query.as_deref().unwrap_or("");
    match dictionary {
        Ok(dictionary) => run_with_options(dictionary, query, options),
        Err(error) => run_without_dictionary(query, format!("{error:#}"), options),
    }
}

fn color_enabled(no_color_flag: bool) -> bool {
    color_enabled_from(no_color_flag, std::env::var_os("NO_COLOR").as_deref())
}

fn print_pack_info(path: &Path, info: &PackInfo) {
    println!("Pack:         {}", path.display());
    println!("Schema:       {}", info.schema_version);
    println!("Entries:      {}", info.candidate_count);
    println!("Snapshot:     {}", info.snapshot);
    println!("Source:       {} <{}>", info.source, info.source_url);
    println!("Input sha256: {}", info.input_sha256);
    for license in &info.licenses {
        println!("Data license: {license}");
    }
    println!("Code license: {}", env!("CARGO_PKG_LICENSE"));
}

fn color_enabled_from(no_color_flag: bool, no_color_env: Option<&std::ffi::OsStr>) -> bool {
    if no_color_flag {
        return false;
    }
    !matches!(no_color_env, Some(val) if !val.is_empty())
}

fn resolve_data(
    explicit: Option<PathBuf>,
    env_data: Option<PathBuf>,
    executable: &Path,
    user_data: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = env_data {
        return Ok(path);
    }
    let executable = executable
        .canonicalize()
        .context("Cannot resolve executable location")?;
    if let Some(parent) = executable.parent() {
        let bundled = parent.join("english-pack");
        if bundled.try_exists()? || bundled.is_symlink() {
            return Ok(bundled);
        }
    }
    user_data.context(
        "Cannot determine data directory; run `wordglide --download-data`, set WORDGLIDE_DATA, or pass --data DIRECTORY",
    )
}

fn resolve_with_download(
    explicit: Option<PathBuf>,
    env_data: Option<PathBuf>,
    executable: &Path,
    user_data: Option<PathBuf>,
    managed: impl FnOnce() -> Result<Option<PathBuf>>,
) -> Result<PathBuf> {
    if explicit.is_some() || env_data.is_some() {
        return resolve_data(explicit, env_data, executable, user_data);
    }
    if let Some(path) = managed()? {
        return Ok(path);
    }
    resolve_data(None, None, executable, user_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn explicit_then_env_then_adjacent_then_user_data_without_silent_corrupt_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("wordglide");
        fs::write(&executable, "fixture").unwrap();
        let user_data = dir.path().join("user-data");
        assert_eq!(
            resolve_data(None, None, &executable, Some(user_data.clone())).unwrap(),
            user_data
        );
        let from_env = dir.path().join("env-data");
        assert_eq!(
            resolve_data(
                None,
                Some(from_env.clone()),
                &executable,
                Some(user_data.clone())
            )
            .unwrap(),
            from_env
        );
        let adjacent = dir.path().canonicalize().unwrap().join("english-pack");
        fs::write(&adjacent, "invalid pack is still selected for validation").unwrap();
        assert_eq!(
            resolve_data(None, None, &executable, Some(user_data.clone())).unwrap(),
            adjacent
        );
        let explicit = dir.path().join("explicit");
        assert_eq!(
            resolve_data(
                Some(explicit.clone()),
                Some(from_env.clone()),
                &executable,
                Some(user_data.clone())
            )
            .unwrap(),
            explicit
        );
        #[cfg(unix)]
        {
            let links = dir.path().join("links");
            fs::create_dir(&links).unwrap();
            let link = links.join("wordglide");
            std::os::unix::fs::symlink(&executable, &link).unwrap();
            assert_eq!(
                resolve_data(None, None, &link, Some(user_data)).unwrap(),
                adjacent
            );
        }
    }

    #[test]
    fn color_enabled_precedence_and_env() {
        use std::ffi::OsStr;
        // When neither flag nor env is set: color enabled.
        assert!(color_enabled_from(false, None));
        // Empty NO_COLOR env: color enabled.
        assert!(color_enabled_from(false, Some(OsStr::new(""))));
        // Non-empty NO_COLOR env: color disabled.
        assert!(!color_enabled_from(false, Some(OsStr::new("1"))));
        assert!(!color_enabled_from(false, Some(OsStr::new("0"))));
        assert!(!color_enabled_from(false, Some(OsStr::new("true"))));
        // --no-color flag set: color disabled regardless of env.
        assert!(!color_enabled_from(true, None));
        assert!(!color_enabled_from(true, Some(OsStr::new(""))));
        assert!(!color_enabled_from(true, Some(OsStr::new("1"))));
    }

    #[test]
    fn managed_download_precedes_bundled_data_but_not_explicit_overrides() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("wordglide");
        fs::write(&executable, "fixture").unwrap();
        fs::create_dir(root.path().join("english-pack")).unwrap();
        let managed = root.path().join("managed");
        assert_eq!(
            resolve_with_download(None, None, &executable, None, || Ok(Some(managed.clone())))
                .unwrap(),
            managed
        );
        for (explicit, environment) in
            [(Some(managed.clone()), None), (None, Some(managed.clone()))]
        {
            assert_eq!(
                resolve_with_download(explicit, environment, &executable, None, || anyhow::bail!(
                    "invalid receipt"
                ))
                .unwrap(),
                managed
            );
        }
        assert!(
            resolve_with_download(None, None, &executable, None, || anyhow::bail!(
                "invalid receipt"
            ))
            .is_err()
        );
    }
}
