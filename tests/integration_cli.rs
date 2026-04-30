use std::fs;

use assert_cmd::Command;

fn templategraph() -> Command {
    Command::cargo_bin("templategraph-php").unwrap()
}

#[test]
fn help_succeeds() {
    templategraph().arg("--help").assert().success();
}

#[test]
fn version_succeeds() {
    templategraph().arg("--version").assert().success();
}

#[test]
fn scan_help_succeeds() {
    templategraph().args(["scan", "--help"]).assert().success();
}

#[test]
fn scan_without_entrypoints_fails() {
    templategraph().arg("scan").assert().failure();
}

#[test]
fn scan_with_unreadable_config_fails() {
    templategraph()
        .args([
            "scan",
            "--config",
            "/nonexistent/templategraph.toml",
            "public/index.php",
        ])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn scan_emits_dot_graph_to_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.php");
    let header = dir.path().join("header.php");
    fs::write(&index, b"<?php include __DIR__ . '/header.php';").unwrap();
    fs::write(&header, b"<?php echo 'header';").unwrap();

    let output = templategraph()
        .args(["scan", "--root"])
        .arg(dir.path())
        .arg(&index)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("digraph templategraph {"));
    assert!(stdout.contains("[label=\"index.php\", shape=doubleoctagon]"));
    assert!(stdout.contains("[label=\"header.php\"]"));
    assert!(stdout.contains("-> "));
    assert!(stdout.contains("[label=\"include\"]"));
}

#[test]
fn scan_with_output_flag_writes_to_file() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.php");
    fs::write(&index, b"<?php echo 'hi';").unwrap();
    let out_path = dir.path().join("graph.dot");

    templategraph()
        .args(["scan", "--root"])
        .arg(dir.path())
        .args(["--output"])
        .arg(&out_path)
        .arg(&index)
        .assert()
        .success();

    let written = fs::read_to_string(&out_path).unwrap();
    assert!(written.starts_with("digraph templategraph {"));
}

#[test]
fn scan_with_format_json_reports_not_implemented() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.php");
    fs::write(&index, b"<?php echo 'hi';").unwrap();

    let output = templategraph()
        .args(["scan", "--format", "json"])
        .arg(&index)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--format json is not yet implemented"));
}

#[test]
fn scan_with_valid_config_but_missing_entrypoint_fails_at_io_layer() {
    // The config loads cleanly; the failure must come from the entrypoint
    // I/O layer, not from a config-error short-circuit.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("templategraph.toml");
    fs::write(&config_path, b"").unwrap();
    let missing = dir.path().join("nonexistent.php");

    let output = templategraph()
        .args(["scan", "--config"])
        .arg(&config_path)
        .arg(&missing)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("config error"),
        "expected an I/O-layer error, got: {}",
        stderr
    );
}

#[test]
fn scan_uses_entrypoints_from_config_when_cli_omits_them() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("index.php"),
        b"<?php include __DIR__ . '/header.php';",
    )
    .unwrap();
    fs::write(dir.path().join("header.php"), b"<?php echo 'header';").unwrap();
    let config_path = dir.path().join("templategraph.toml");
    fs::write(&config_path, b"entrypoints = [\"index.php\"]\n").unwrap();

    let output = templategraph()
        .args(["scan", "--root"])
        .arg(dir.path())
        .args(["--config"])
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("[label=\"index.php\", shape=doubleoctagon]"));
    assert!(stdout.contains("[label=\"header.php\"]"));
}

#[test]
fn scan_uses_default_format_from_config() {
    // When the config requests json (currently unimplemented), the CLI should
    // still honor that selection rather than silently falling back to dot.
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.php");
    fs::write(&index, b"<?php echo 'hi';").unwrap();
    let config_path = dir.path().join("templategraph.toml");
    fs::write(
        &config_path,
        b"[output]\ndefault_format = \"json\"\n",
    )
    .unwrap();

    let output = templategraph()
        .args(["scan", "--config"])
        .arg(&config_path)
        .arg(&index)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--format json is not yet implemented"));
}

#[test]
fn scan_with_no_entrypoints_anywhere_reports_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("templategraph.toml");
    fs::write(&config_path, b"").unwrap();

    let output = templategraph()
        .args(["scan", "--config"])
        .arg(&config_path)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no entrypoints"));
}

// Helper for the directory-walking tests below.
fn write_at(dir: &std::path::Path, rel: &str, content: &[u8]) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn scan_walks_directory_and_demotes_included_files() {
    let dir = tempfile::tempdir().unwrap();
    write_at(
        dir.path(),
        "index.php",
        b"<?php include __DIR__ . '/inc/header.php';",
    );
    write_at(
        dir.path(),
        "about.php",
        b"<?php include __DIR__ . '/inc/header.php';",
    );
    write_at(dir.path(), "inc/header.php", b"<?php echo 'header';");

    let output = templategraph()
        .arg("scan")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // `index.php` and `about.php` are page-level → entrypoints.
    assert!(stdout.contains(r#"[label="about.php", shape=doubleoctagon]"#));
    assert!(stdout.contains(r#"[label="index.php", shape=doubleoctagon]"#));
    // `inc/header.php` is included by both → demoted to a non-entry node.
    assert!(stdout.contains(r#"[label="inc/header.php"];"#));
    assert!(!stdout.contains(r#"[label="inc/header.php", shape=doubleoctagon]"#));
}

#[test]
fn scan_resolves_server_document_root_against_inferred_root() {
    // No `--root`: the directory entrypoint is taken as the document root,
    // and `$_SERVER['DOCUMENT_ROOT']` resolves against it. The included
    // `inc/header.php` should therefore be demoted from the entrypoint set.
    let dir = tempfile::tempdir().unwrap();
    write_at(
        dir.path(),
        "page.php",
        b"<?php include $_SERVER['DOCUMENT_ROOT'] . \"/inc/header.php\";",
    );
    write_at(dir.path(), "inc/header.php", b"<?php echo 'header';");

    let output = templategraph()
        .arg("scan")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#"[label="page.php", shape=doubleoctagon]"#));
    assert!(stdout.contains(r#"[label="inc/header.php"];"#));
    assert!(!stdout.contains(r#"[label="inc/header.php", shape=doubleoctagon]"#));
    // No unresolved nodes — the include resolved cleanly.
    assert!(!stdout.contains("unresolved"));
}

#[test]
fn scan_directory_skips_excluded_dirs_via_config() {
    let dir = tempfile::tempdir().unwrap();
    write_at(dir.path(), "index.php", b"<?php echo 'top';");
    write_at(dir.path(), "vendor/lib.php", b"<?php echo 'should be excluded';");
    let config_path = dir.path().join("templategraph.toml");
    fs::write(&config_path, b"exclude = [\"vendor\"]\n").unwrap();

    let output = templategraph()
        .args(["scan", "--config"])
        .arg(&config_path)
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#"[label="index.php", shape=doubleoctagon]"#));
    assert!(!stdout.contains("vendor/lib.php"));
}

#[test]
fn scan_directory_with_no_php_files_reports_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    write_at(dir.path(), "readme.md", b"hello");

    let output = templategraph()
        .arg("scan")
        .arg(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no PHP entrypoints"));
}
