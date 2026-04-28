use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::path::AbsolutePath;
use crate::scanner::FileReader;

#[derive(Debug, Default, Clone)]
pub struct InMemoryFileReader {
    files: HashMap<PathBuf, String>,
}

impl InMemoryFileReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }
}

impl FileReader for InMemoryFileReader {
    fn read_to_string(&self, path: &AbsolutePath) -> Result<String> {
        self.files.get(path.as_path()).cloned().ok_or_else(|| {
            Error::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "file not found in InMemoryFileReader: {}",
                    path.as_path().display()
                ),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_added_content() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/a/b.php", "<?php echo 1;");
        let path = AbsolutePath::new(PathBuf::from("/a/b.php")).unwrap();
        assert_eq!(reader.read_to_string(&path).unwrap(), "<?php echo 1;");
    }

    #[test]
    fn missing_file_yields_io_error() {
        let reader = InMemoryFileReader::new();
        let path = AbsolutePath::new(PathBuf::from("/missing.php")).unwrap();
        let err = reader.read_to_string(&path).unwrap_err();
        assert!(matches!(err, Error::Io(_)));
    }
}
