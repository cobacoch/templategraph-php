pub mod filesystem;
// `in_memory` is a test-only fixture: every consumer of `InMemoryFileReader`
// lives under `#[cfg(test)]`. Gating the whole module the same way keeps
// dead-code lints honest in non-test builds.
#[cfg(test)]
pub mod in_memory;

use crate::error::Result;
use crate::path::AbsolutePath;

pub trait FileReader {
    fn read_to_string(&self, path: &AbsolutePath) -> Result<String>;
}

pub trait DirWalker {
    fn walk(&self, root: &AbsolutePath) -> Result<Vec<AbsolutePath>>;
}
