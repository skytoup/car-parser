mod extract;
mod format;
mod info;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "car-cli",
    about = "Inspect and extract Apple .car asset catalog files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print .car file info as JSON (similar to assetutil -I)
    Info {
        /// Path to the .car file
        file: PathBuf,
    },
    /// Extract all assets from a .car file to disk
    Extract {
        /// Path to the .car file
        file: PathBuf,
        /// Output directory (created if it doesn't exist)
        #[arg(short, long)]
        output: PathBuf,
        /// Overwrite existing files
        #[arg(long)]
        overwrite: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Info { file } => {
            let car = car::Car::new(&file)?;
            info::run(&car)?;
        }
        Command::Extract {
            file,
            output,
            overwrite,
        } => {
            let car = car::Car::new(&file)?;
            extract::run(&car, &output, overwrite)?;
        }
    }
    Ok(())
}
