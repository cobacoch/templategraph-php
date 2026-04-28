use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scan PHP entrypoints and produce an include dependency graph.
    Scan(ScanArgs),
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    /// One or more entrypoint PHP files to start the scan from.
    #[arg(required = true)]
    pub entrypoints: Vec<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Dot)]
    pub format: Format,

    /// Write output to the given file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Path to a templategraph.toml configuration file.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Project root used to resolve relative paths.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Emit additional progress information to stderr.
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Dot,
    Json,
}
