#[test]
fn inline_snapshot_sanity() {
    insta::assert_snapshot!("Hello, world!", @"Hello, world!");
}
