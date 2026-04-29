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
    let config = match args.config.as_deref() {
        Some(config_path) => match config::load(config_path) {
            Ok(cfg) => {
                if args.verbose {
                    eprintln!("loaded config from {}", config_path.display());
                }
                cfg
            }
            Err(err) => {
                eprintln!("error: {}", err);
                return 1;
            }
        },
        None => Config::default(),
    };

    if matches!(args.format, Format::Json) {
        eprintln!("error: --format json is not yet implemented");
        return 1;
    }

    let project_root = match resolve_project_root(args.root.as_deref(), config.root.as_deref()) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("error: {}", err);
            return 1;
        }
    };

    let entrypoints = match resolve_entrypoints(&args.entrypoints) {
        Ok(eps) => eps,
        Err(err) => {
            eprintln!("error: {}", err);
            return 1;
        }
    };

    let reader = FilesystemFileReader::new();
    let graph = match build_graph(&entrypoints, &project_root, &reader) {
        Ok(g) => g,
        Err(err) => {
            eprintln!("error: {}", err);
            return 1;
        }
    };

    let rendered = dot::render(&graph);
    if let Err(err) = write_output(args.output.as_deref(), &rendered) {
        eprintln!("error: {}", err);
        return 1;
    }
    0
}

// `--root` wins over the config value, and both win over CWD. The chosen
// path is absolutized against CWD if necessary; we deliberately do not call
// `canonicalize` so that symlinked project layouts are preserved as-is.
fn resolve_project_root(
    cli_root: Option<&Path>,
    config_root: Option<&Path>,
) -> Result<AbsolutePath, String> {
    let candidate = cli_root
        .or(config_root)
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(|| {
            std::env::current_dir().map_err(|e| format!("failed to read current directory: {}", e))
        })?;
    let absolute = absolutize(&candidate)
        .map_err(|e| format!("failed to resolve root {}: {}", candidate.display(), e))?;
    AbsolutePath::new(absolute).map_err(|e| format!("invalid root path: {}", e))
}

fn resolve_entrypoints(entrypoints: &[PathBuf]) -> Result<Vec<AbsolutePath>, String> {
    entrypoints
        .iter()
        .map(|p| {
            let absolute = absolutize(p)
                .map_err(|e| format!("failed to resolve entrypoint {}: {}", p.display(), e))?;
            AbsolutePath::new(absolute).map_err(|e| format!("invalid entrypoint path: {}", e))
        })
        .collect()
}

fn absolutize(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path))
    }
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
