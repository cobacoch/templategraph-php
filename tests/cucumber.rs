use cucumber::{given, then, World};

#[derive(Debug, Default, World)]
struct ExampleWorld;

#[given("a placeholder cucumber harness")]
fn placeholder(_world: &mut ExampleWorld) {}

#[then("the harness loads")]
fn loads(_world: &mut ExampleWorld) {}

fn main() {
    futures_executor::block_on(ExampleWorld::run("tests/features"));
}
