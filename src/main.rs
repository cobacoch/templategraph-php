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
use crate::graph::builder::build_graph_with_discovery;
use crate::output::{Format, dot, json};
use crate::path::AbsolutePath;
use crate::scanner::DirWalker;
use crate::scanner::filesystem::{FilesystemDirWalker, FilesystemFileReader};

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

    // CLI inputs are absolutized first so they can both inform `project_root`
    // (when no `--root` was given) and feed the entrypoint pipeline.
    let cli_inputs = absolutize_cli_inputs(&args.entrypoints)?;

    let project_root = resolve_project_root(
        args.root.as_deref(),
        config.root.as_deref(),
        &cli_inputs,
    )?;

    let document_root = resolve_document_root(
        args.document_root.as_deref(),
        config.document_root.as_deref(),
        &project_root,
        &cli_inputs,
    )?;

    let raw_inputs = if cli_inputs.is_empty() {
        absolutize_config_inputs(&config.entrypoints, &project_root)?
    } else {
        cli_inputs
    };

    let reader = FilesystemFileReader::new();
    let (explicit, discovered) =
        classify_entrypoints(&raw_inputs, &config.exclude, &reader)?;

    let graph = build_graph_with_discovery(
        &explicit,
        &discovered,
        &project_root,
        document_root.as_ref(),
        &reader,
    )
    .map_err(|e| e.to_string())?;

    let any_entry = graph.nodes.iter().any(|n| n.is_entrypoint);
    if !any_entry {
        return Err(
            "no PHP entrypoints found (every walked file is included by another walked file)"
                .into(),
        );
    }

    let rendered = match format {
        Format::Dot => dot::render(&graph),
        Format::Json => json::render(&graph),
    };
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

// Resolution order: `--root` wins over `config.root`. If neither is given,
// fall back to the first directory among the CLI inputs (so labels in the
// output are relative to that directory); failing that, use the current
// directory. `--root` only controls how paths are displayed; it does NOT
// control include resolution — see `resolve_document_root` for that.
//
// `canonicalize` is intentionally avoided so symlinked project layouts are
// preserved as-is.
fn resolve_project_root(
    cli_root: Option<&Path>,
    config_root: Option<&Path>,
    cli_inputs: &[AbsolutePath],
) -> Result<AbsolutePath, String> {
    if let Some(p) = cli_root.or(config_root) {
        let absolute = absolutize(p)
            .map_err(|e| format!("failed to resolve root {}: {}", p.display(), e))?;
        return AbsolutePath::new(absolute).map_err(|e| format!("invalid root path: {}", e));
    }

    if let Some(dir) = cli_inputs.iter().find(|p| is_directory(p.as_path())) {
        return Ok(dir.clone());
    }

    let cwd = std::env::current_dir()
        .map_err(|e| format!("failed to read current directory: {}", e))?;
    AbsolutePath::new(path::normalize(&cwd)).map_err(|e| format!("invalid CWD: {}", e))
}

// Resolution order: `--document-root` wins over `config.document_root`. If
// neither is given, auto-infer ONLY when there is exactly one directory
// input on the CLI — that single directory is the user's clear intent for
// "the webroot." Multiple directory inputs are ambiguous and the CLI
// declines to guess; the user must pass `--document-root` explicitly. With
// no document root configured, occurrences of `$_SERVER['DOCUMENT_ROOT']`
// in include directives are reported as unresolved (a safe failure mode
// that is visible in the output) rather than silently mis-resolved.
fn resolve_document_root(
    cli_doc_root: Option<&Path>,
    config_doc_root: Option<&Path>,
    project_root: &AbsolutePath,
    cli_inputs: &[AbsolutePath],
) -> Result<Option<AbsolutePath>, String> {
    // Explicit CLI value (resolved against CWD).
    if let Some(p) = cli_doc_root {
        let absolute = absolutize(p)
            .map_err(|e| format!("failed to resolve document root {}: {}", p.display(), e))?;
        return AbsolutePath::new(absolute)
            .map(Some)
            .map_err(|e| format!("invalid document root: {}", e));
    }
    // Config value (resolved against project_root, since the config file
    // conceptually anchors at the project).
    if let Some(p) = config_doc_root {
        let joined = if p.is_absolute() {
            p.to_path_buf()
        } else {
            project_root.as_path().join(p)
        };
        return AbsolutePath::new(path::normalize(&joined))
            .map(Some)
            .map_err(|e| format!("invalid document root: {}", e));
    }
    // Auto-infer only when unambiguous.
    let dir_inputs: Vec<&AbsolutePath> = cli_inputs
        .iter()
        .filter(|p| is_directory(p.as_path()))
        .collect();
    Ok(if dir_inputs.len() == 1 {
        Some(dir_inputs[0].clone())
    } else {
        None
    })
}

fn is_directory(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

// Splits raw inputs into explicit files (always entrypoints) and walked
// files (entrypoint candidates that the builder may demote based on
// include relationships).
fn classify_entrypoints(
    raw_inputs: &[AbsolutePath],
    excludes: &[String],
    file_reader: &FilesystemFileReader,
) -> Result<(Vec<AbsolutePath>, Vec<AbsolutePath>), String> {
    if raw_inputs.is_empty() {
        return Err(
            "no entrypoints provided (pass on the command line or list them in templategraph.toml)"
                .into(),
        );
    }

    let walker = FilesystemDirWalker::new().with_excludes(excludes.iter().cloned());
    let mut explicit_files: Vec<AbsolutePath> = Vec::new();
    let mut discovered_files: Vec<AbsolutePath> = Vec::new();

    for input in raw_inputs {
        let meta = fs::metadata(input.as_path()).map_err(|e| {
            format!(
                "failed to stat entrypoint {}: {}",
                input.as_path().display(),
                e
            )
        })?;
        if meta.is_dir() {
            let walked = walker.walk(input).map_err(|e| {
                format!(
                    "failed to walk directory {}: {}",
                    input.as_path().display(),
                    e
                )
            })?;
            discovered_files.extend(walked);
        } else if meta.is_file() {
            explicit_files.push(input.clone());
        } else {
            return Err(format!(
                "entrypoint {} is neither a regular file nor a directory",
                input.as_path().display()
            ));
        }
    }

    if explicit_files.is_empty() && discovered_files.is_empty() {
        return Err(
            "no PHP entrypoints found (the given directories contain no top-level PHP files)"
                .into(),
        );
    }

    let _ = file_reader; // reserved for future use (e.g., early validation)
    Ok((explicit_files, discovered_files))
}

// CLI paths are interpreted relative to the current directory (matching
// shell expectations).
fn absolutize_cli_inputs(cli_entrypoints: &[PathBuf]) -> Result<Vec<AbsolutePath>, String> {
    cli_entrypoints
        .iter()
        .map(|p| {
            let abs = absolutize(p)
                .map_err(|e| format!("failed to resolve entrypoint {}: {}", p.display(), e))?;
            AbsolutePath::new(abs).map_err(|e| format!("invalid entrypoint {}: {}", p.display(), e))
        })
        .collect()
}

// Config paths are interpreted relative to `project_root` (matching the way
// the example in `templategraph.toml` is written —
// `entrypoints = ["public/..."]` alongside `root = "."`).
fn absolutize_config_inputs(
    config_entrypoints: &[PathBuf],
    project_root: &AbsolutePath,
) -> Result<Vec<AbsolutePath>, String> {
    config_entrypoints
        .iter()
        .map(|p| {
            let joined = if p.is_absolute() {
                p.clone()
            } else {
                project_root.as_path().join(p)
            };
            AbsolutePath::new(path::normalize(&joined))
                .map_err(|e| format!("invalid entrypoint {}: {}", p.display(), e))
        })
        .collect()
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

