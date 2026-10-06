use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use local_english_dict::{Dictionary, run, verify_pack};
use std::path::{Path, PathBuf};

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
    /// Verify all data checksums, indexes, and database integrity, then exit.
    #[arg(long, conflicts_with = "query")]
    verify_data: bool,
    /// Disable colored output.
    #[arg(long)]
    no_color: bool,
    /// Disable mouse capture, keeping native terminal text selection.
    #[arg(long)]
    no_mouse: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let executable = std::env::current_exe().context("Cannot locate executable")?;
    let user_data = ProjectDirs::from("org", "local-english-dict", "dict")
        .map(|d| d.data_dir().join("english"));
    let env_data = std::env::var_os("WORDGLIDE_DATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let path = resolve_data(args.data, env_data, &executable, user_data)?;
    if args.verify_data {
        verify_pack(&path)?;
        println!("Data pack verified");
        return Ok(());
    }
    let dict = Dictionary::open(&path)?;
    let color = color_enabled(args.no_color);
    run(
        dict,
        args.query.as_deref().unwrap_or(""),
        color,
        !args.no_mouse,
    )
}

fn color_enabled(no_color_flag: bool) -> bool {
    color_enabled_from(no_color_flag, std::env::var_os("NO_COLOR").as_deref())
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
        "Cannot determine data directory; download the with-data bundle, or use --data DIRECTORY",
    )
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
}
