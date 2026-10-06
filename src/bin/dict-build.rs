use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Assemble a local data pack from canonical, source-grounded entries")]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    source: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    local_english_dict::build_pack(&args.input, &args.source, &args.output)?;
    println!("Pack ready: {}", args.output.display());
    Ok(())
}
