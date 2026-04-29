mod cli;
mod config;
mod error;
mod graph;
mod output;
mod parser;
mod path;
mod scanner;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;

use crate::config::Config;
use crate::graph::builder::build_graph;
use crate::output::{Format, dot};
use crate::path::AbsolutePath;
use crate::scanner::filesystem::FilesystemFileReader;

fn main() {
    let args = cli::Cli::parse();
    let exit_code = match args.command {
        cli::Command::Scan(scan_args) => run_scan(scan_args),
    };
    process::exit(exit_code);
}

fn run_scan(args: cli::ScanArgs) -> i32 {
    match try_run_scan(args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("error: {}", err);
            1
        }
    }
}

fn try_run_scan(args: cli::ScanArgs) -> Result<(), String> {
    let config = load_config(args.config.as_deref(), args.verbose)?;

    let format = args
        .format
        .or(config.output.default_format)
        .unwrap_or(Format::Dot);
    if matches!(format, Format::Json) {
        return Err("--format json is not yet implemented".into());
    }

    let project_root = resolve_project_root(args.root.as_deref(), config.root.as_deref())?;
    let entrypoints = resolve_entrypoints(&args.entrypoints, &config.entrypoints, &project_root)?;

    let reader = FilesystemFileReader::new();
    let graph = build_graph(&entrypoints, &project_root, &reader).map_err(|e| e.to_string())?;

    let rendered = dot::render(&graph);
    write_output(args.output.as_deref(), &rendered).map_err(|e| e.to_string())
}

fn load_config(config_path: Option<&Path>, verbose: bool) -> Result<Config, String> {
    let Some(path) = config_path else {
        return Ok(Config::default());
    };
    let cfg = config::load(path).map_err(|e| e.to_string())?;
    if verbose {
        eprintln!("loaded config from {}", path.display());
    }
    Ok(cfg)
}

// `--root` wins over the config value, and both win over CWD. The chosen
// path is absolutized against CWD if necessary; we deliberately do not call
// `canonicalize` so that symlinked project layouts are preserved as-is.
fn resolve_project_root(
    cli_root: Option<&Path>,
    config_root: Option<&Path>,
) -> Result<AbsolutePath, String> {
    let candidate = match cli_root.or(config_root) {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|e| format!("failed to read current directory: {}", e))?,
    };
    let absolute = absolutize(&candidate)
        .map_err(|e| format!("failed to resolve root {}: {}", candidate.display(), e))?;
    AbsolutePath::new(absolute).map_err(|e| format!("invalid root path: {}", e))
}

// CLI entrypoints win over config entrypoints. CLI paths are interpreted
// relative to the current directory (matching shell expectations); config
// paths are interpreted relative to `project_root` (matching the way the
// example in `templategraph.toml` is written — `entrypoints = ["public/..."]`
// alongside `root = "."`).
fn resolve_entrypoints(
    cli_entrypoints: &[PathBuf],
    config_entrypoints: &[PathBuf],
    project_root: &AbsolutePath,
) -> Result<Vec<AbsolutePath>, String> {
    let resolved: Vec<AbsolutePath> = if !cli_entrypoints.is_empty() {
        cli_entrypoints
            .iter()
            .map(|p| absolutize_entrypoint(p))
            .collect::<Result<_, _>>()?
    } else {
        config_entrypoints
            .iter()
            .map(|p| {
                let joined = if p.is_absolute() {
                    p.clone()
                } else {
                    project_root.as_path().join(p)
                };
                let normalized = path::normalize(&joined);
                AbsolutePath::new(normalized)
                    .map_err(|e| format!("invalid entrypoint {}: {}", p.display(), e))
            })
            .collect::<Result<_, _>>()?
    };

    if resolved.is_empty() {
        return Err(
            "no entrypoints provided (pass on the command line or list them in templategraph.toml)"
                .into(),
        );
    }
    Ok(resolved)
}

fn absolutize_entrypoint(p: &Path) -> Result<AbsolutePath, String> {
    let absolute =
        absolutize(p).map_err(|e| format!("failed to resolve entrypoint {}: {}", p.display(), e))?;
    AbsolutePath::new(absolute).map_err(|e| format!("invalid entrypoint {}: {}", p.display(), e))
}

// Apply `path::normalize` after the join so that user-typed `./public/x.php`
// produces the same node id as an `__DIR__ . '/x.php'` include hitting the
// same file from a sibling — the graph builder normalizes include targets
// the same way.
fn absolutize(p: &Path) -> io::Result<PathBuf> {
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()?.join(p)
    };
    Ok(path::normalize(&joined))
}

fn write_output(target: Option<&Path>, content: &str) -> io::Result<()> {
    match target {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content.as_bytes())
        }
    }
}
