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
