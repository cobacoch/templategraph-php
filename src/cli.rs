mod exit;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::output::Format;
pub use exit::ExitCode;

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
    /// One or more entrypoint PHP files to start the scan from. Falls back to
    /// `entrypoints` in `templategraph.toml` if none are given.
    pub entrypoints: Vec<PathBuf>,

    /// Output format. Falls back to `[output] default_format` in
    /// `templategraph.toml`, then to `dot`.
    #[arg(long, value_enum)]
    pub format: Option<Format>,

    /// Write output to the given file instead of stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Path to a templategraph.toml configuration file.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Project root used to display paths relative to a stable base.
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Web document root used to resolve `$_SERVER['DOCUMENT_ROOT']` in
    /// include directives. Falls back to `document_root` in
    /// `templategraph.toml`, then (if a single directory entrypoint is given)
    /// to that directory. When unset, occurrences of
    /// `$_SERVER['DOCUMENT_ROOT']` are reported as unresolved.
    #[arg(long)]
    pub document_root: Option<PathBuf>,

    /// Emit additional progress information to stderr.
    #[arg(short, long)]
    pub verbose: bool,
}
