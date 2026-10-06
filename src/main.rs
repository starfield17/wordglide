use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use local_english_dict::{Dictionary, run};
use std::path::PathBuf;

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
}

fn main() -> Result<()> {
    let args = Args::parse();
    let path = args
        .data
        .or_else(|| {
            ProjectDirs::from("org", "local-english-dict", "dict")
                .map(|d| d.data_dir().join("english"))
        })
        .context("Cannot determine data directory; use --data DIRECTORY")?;
    let dict = Dictionary::open(&path)?;
    run(dict, args.query.as_deref().unwrap_or(""))
}
