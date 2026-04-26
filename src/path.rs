#![allow(dead_code)]

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,
    #[error("path is not absolute: {0:?}")]
    NotAbsolute(PathBuf),
    #[error("path is not relative: {0:?}")]
    NotRelative(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AbsolutePath(PathBuf);

impl AbsolutePath {
    pub fn new(path: PathBuf) -> Result<Self, PathError> {
        if path.as_os_str().is_empty() {
            return Err(PathError::Empty);
        }
        if !path.is_absolute() {
            return Err(PathError::NotAbsolute(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_inner(self) -> PathBuf {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RootRelativePath(PathBuf);

impl RootRelativePath {
    pub fn new(path: PathBuf) -> Result<Self, PathError> {
        if path.as_os_str().is_empty() {
            return Err(PathError::Empty);
        }
        if !path.is_relative() {
            return Err(PathError::NotRelative(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_inner(self) -> PathBuf {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_path_accepts_absolute() {
        let p = AbsolutePath::new(PathBuf::from("/foo/bar")).unwrap();
        assert_eq!(p.as_path(), Path::new("/foo/bar"));
    }

    #[test]
    fn absolute_path_rejects_relative() {
        assert!(matches!(
            AbsolutePath::new(PathBuf::from("foo/bar")),
            Err(PathError::NotAbsolute(_))
        ));
    }

    #[test]
    fn absolute_path_rejects_empty() {
        assert!(matches!(
            AbsolutePath::new(PathBuf::new()),
            Err(PathError::Empty)
        ));
    }

    #[test]
    fn root_relative_path_accepts_relative() {
        let p = RootRelativePath::new(PathBuf::from("foo/bar")).unwrap();
        assert_eq!(p.as_path(), Path::new("foo/bar"));
    }

    #[test]
    fn root_relative_path_rejects_absolute() {
        assert!(matches!(
            RootRelativePath::new(PathBuf::from("/foo/bar")),
            Err(PathError::NotRelative(_))
        ));
    }

    #[test]
    fn root_relative_path_rejects_empty() {
        assert!(matches!(
            RootRelativePath::new(PathBuf::new()),
            Err(PathError::Empty)
        ));
    }
}
