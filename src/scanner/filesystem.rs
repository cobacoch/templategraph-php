use std::fs;

use crate::error::{Error, Result};
use crate::path::AbsolutePath;
use crate::scanner::FileReader;

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemFileReader;

impl FilesystemFileReader {
    pub fn new() -> Self {
        Self
    }
}

impl FileReader for FilesystemFileReader {
    fn read_to_string(&self, path: &AbsolutePath) -> Result<String> {
        fs::read_to_string(path.as_path()).map_err(Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("hello.php");
        std::fs::write(&file_path, "<?php echo 'hello';").unwrap();

        let abs = AbsolutePath::new(file_path).unwrap();
        let content = FilesystemFileReader::new().read_to_string(&abs).unwrap();
        assert_eq!(content, "<?php echo 'hello';");
    }

    #[test]
    fn missing_file_returns_not_found_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.php");

        let abs = AbsolutePath::new(missing).unwrap();
        let err = FilesystemFileReader::new().read_to_string(&abs).unwrap_err();
        match err {
            Error::Io(io_err) => assert_eq!(io_err.kind(), io::ErrorKind::NotFound),
            other => panic!("expected Io error, got {:?}", other),
        }
    }
}
