mod cli;
mod config;
mod error;
mod graph;
mod output;
mod parser;
mod path;
mod scanner;

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::ExitCode;
use crate::config::Config;
use crate::graph::builder::build_graph_with_discovery;
use crate::graph::{Graph, NodeKind};
use crate::output::{Format, dot, json};
use crate::path::AbsolutePath;
use crate::scanner::DirWalker;
use crate::scanner::filesystem::{FilesystemDirWalker, FilesystemFileReader};

fn main() -> std::process::ExitCode {
    let args = cli::Cli::parse();
    let code = match args.command {
        cli::Command::Scan(scan_args) => run_scan(scan_args),
    };
    code.into()
}

fn run_scan(args: cli::ScanArgs) -> ExitCode {
    match try_run_scan(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {}", err);
            ExitCode::Fatal
        }
    }
}

fn try_run_scan(args: cli::ScanArgs) -> Result<ExitCode, String> {
    let config = load_config(args.config.as_deref(), args.verbose)?;

    let format = args
        .format
        .or(config.output.default_format)
        .unwrap_or(Format::Dot);

    // CLI inputs are absolutized first so they can both inform `project_root`
    // (when no `--root` was given) and feed the entrypoint pipeline.
    let cli_inputs = absolutize_cli_inputs(&args.entrypoints)?;

    let project_root =
        resolve_project_root(args.root.as_deref(), config.root.as_deref(), &cli_inputs)?;

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
    let (explicit, discovered) = classify_entrypoints(&raw_inputs, &config.exclude, &reader)?;

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
    write_output(args.output.as_deref(), &rendered).map_err(|e| e.to_string())?;

    let had_unresolved = report_unresolved(&graph, &mut io::stderr());
    Ok(if had_unresolved {
        ExitCode::WarningSuccess
    } else {
        ExitCode::Success
    })
}

// Emits a one-line summary plus per-edge details for every edge whose target
// is an `Unresolved` node, and returns whether any were found. The check is
// on target node kind (not edge kind) because edges to missing-file
// unresolved targets keep their original PHP include kind (`include` /
// `require` / etc.) on the wire. Always on regardless of `--verbose`: an
// unresolved include is a finding the user should see even on a non-verbose
// run. The boolean return is what `try_run_scan` uses to map to
// `ExitCode::WarningSuccess` (2); a zero-warning scan returns
// `ExitCode::Success` (0).
//
// Counting policy: the header reports the total number of unresolved edges
// (so PHP-level repeats stay visible at a glance), while body lines collapse
// duplicate `(from, to)` pairs into a single entry suffixed with `(xN)`.
// Body lines are stably sorted by `from` for cross-run readability; pairs
// sharing the same `from` keep their first-appearance order.
fn report_unresolved<W: Write>(graph: &Graph, sink: &mut W) -> bool {
    let edges: Vec<(String, String)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let to = graph.find_node(&edge.to)?;
            if to.kind != NodeKind::Unresolved {
                return None;
            }
            let from = graph.find_node(&edge.from)?;
            Some((from.display_name.clone(), to.display_name.clone()))
        })
        .collect();

    if edges.is_empty() {
        return false;
    }

    let mut order: Vec<(String, String)> = Vec::new();
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for pair in &edges {
        counts
            .entry(pair.clone())
            .and_modify(|c| *c += 1)
            .or_insert_with(|| {
                order.push(pair.clone());
                1
            });
    }
    order.sort_by(|a, b| a.0.cmp(&b.0));

    let suffix = if edges.len() == 1 { "" } else { "s" };
    let _ = writeln!(
        sink,
        "warning: {} unresolved include{}:",
        edges.len(),
        suffix
    );
    for pair in &order {
        let count = counts[pair];
        if count == 1 {
            let _ = writeln!(sink, "  - {} -> {}", pair.0, pair.1);
        } else {
            let _ = writeln!(sink, "  - {} -> {} (x{})", pair.0, pair.1, count);
        }
    }
    true
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
        let absolute =
            absolutize(p).map_err(|e| format!("failed to resolve root {}: {}", p.display(), e))?;
        return AbsolutePath::new(absolute).map_err(|e| format!("invalid root path: {}", e));
    }

    if let Some(dir) = cli_inputs.iter().find(|p| is_directory(p.as_path())) {
        return Ok(dir.clone());
    }

    let cwd =
        std::env::current_dir().map_err(|e| format!("failed to read current directory: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::builder::build_graph;
    use crate::scanner::in_memory::InMemoryFileReader;

    fn root() -> AbsolutePath {
        AbsolutePath::new(PathBuf::from("/project")).unwrap()
    }

    fn entry(path: &str) -> AbsolutePath {
        AbsolutePath::new(PathBuf::from(path)).unwrap()
    }

    fn capture(graph: &Graph) -> (String, bool) {
        let mut buf: Vec<u8> = Vec::new();
        let had = report_unresolved(graph, &mut buf);
        (String::from_utf8(buf).unwrap(), had)
    }

    #[test]
    fn report_is_silent_when_graph_has_no_unresolved_nodes() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add("/project/header.php", "<?php");

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, had) = capture(&graph);
        assert_eq!(out, "");
        assert!(!had, "no unresolved targets must report had=false");
    }

    #[test]
    fn report_lists_dynamic_argument_unresolved_edges() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/index.php", r#"<?php include $dynamic;"#);

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, had) = capture(&graph);
        assert!(had);
        assert!(out.starts_with("warning: 1 unresolved include:\n"));
        assert!(out.contains("  - index.php -> unresolved: $dynamic\n"));
    }

    #[test]
    fn report_lists_missing_file_targets_even_though_edge_kind_is_include() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include __DIR__ . '/missing.php';"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, had) = capture(&graph);
        assert!(had);
        assert!(out.starts_with("warning: 1 unresolved include:\n"));
        assert!(out.contains("  - index.php -> unresolved: file not found /project/missing.php\n"));
    }

    #[test]
    fn report_pluralizes_count_when_multiple_unresolved_edges_exist() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php
include $a;
include __DIR__ . '/missing.php';
"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, _had) = capture(&graph);
        assert!(out.starts_with("warning: 2 unresolved includes:\n"));
        assert_eq!(out.matches("  - ").count(), 2);
    }

    #[test]
    fn report_collapses_duplicate_from_to_pairs_with_x_count_suffix() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include $dynamic; include $dynamic;"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, _had) = capture(&graph);
        // Header reports total edges (PHP-level occurrences), not unique pairs.
        assert!(
            out.starts_with("warning: 2 unresolved includes:\n"),
            "header should count raw edges, got: {}",
            out
        );
        // Body collapses the duplicate pair into a single line with `(x2)`.
        assert!(
            out.contains("  - index.php -> unresolved: $dynamic (x2)\n"),
            "expected collapsed (x2) line, got: {}",
            out
        );
        assert_eq!(out.matches("  - ").count(), 1);
    }

    #[test]
    fn report_sorts_body_lines_by_from_path_alphabetically() {
        // Entrypoint order is b first, a second; BFS preserves that order, so
        // without sorting the b-line would precede the a-line. The report
        // must reorder them alphabetically by `from`.
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/b.php", r#"<?php include $b;"#);
        reader.add("/project/a.php", r#"<?php include $a;"#);

        let graph = build_graph(
            &[entry("/project/b.php"), entry("/project/a.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();
        let (out, _had) = capture(&graph);
        let a_pos = out.find("- a.php ->").expect("a.php line present");
        let b_pos = out.find("- b.php ->").expect("b.php line present");
        assert!(
            a_pos < b_pos,
            "a.php should be listed before b.php, got: {}",
            out
        );
    }

    #[test]
    fn report_preserves_first_appearance_order_within_same_from() {
        // Two different unresolved targets from the same file: the first
        // emitted edge in the source must appear first, since stable sort by
        // `from` keeps equal-key entries in original order.
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php
include $z;
include $a;
"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        let (out, _had) = capture(&graph);
        let z_pos = out.find("unresolved: $z").expect("$z line present");
        let a_pos = out.find("unresolved: $a").expect("$a line present");
        assert!(z_pos < a_pos, "first-appearance order must hold: {}", out);
    }
}
