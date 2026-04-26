use assert_cmd::Command;

#[test]
fn binary_runs_successfully() {
    Command::cargo_bin("templategraph-php")
        .unwrap()
        .assert()
        .success();
}
