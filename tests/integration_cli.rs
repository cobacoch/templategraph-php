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
fn scan_with_entrypoint_returns_not_implemented() {
    templategraph()
        .args(["scan", "public/index.php"])
        .assert()
        .failure()
        .code(1);
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
