#![allow(dead_code)]

use crate::error::Result;
use crate::path::AbsolutePath;

pub trait FileReader {
    fn read_to_string(&self, path: &AbsolutePath) -> Result<String>;
}

pub trait DirWalker {
    fn walk(&self, root: &AbsolutePath) -> Result<Vec<AbsolutePath>>;
}
