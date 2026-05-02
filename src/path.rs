#![allow(dead_code)]

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

// Collapses `.` / `..` segments without touching the filesystem so that the
// same physical file reached via different syntactic paths shares a single
// node id. Symlinks are intentionally left alone — `canonicalize` is avoided
// because it would resolve symlinks, which we want to surface in the graph
// as the user wrote them.
pub fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

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

    #[test]
    fn normalize_collapses_parent_dir() {
        assert_eq!(normalize(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn normalize_drops_current_dir() {
        assert_eq!(normalize(Path::new("/a/./b/./c")), PathBuf::from("/a/b/c"));
    }

    #[test]
    fn normalize_handles_mixed() {
        assert_eq!(
            normalize(Path::new("/cwd/./public/../public/index.php")),
            PathBuf::from("/cwd/public/index.php")
        );
    }

    #[test]
    fn normalize_is_idempotent() {
        let p = Path::new("/already/clean/path");
        assert_eq!(normalize(p), normalize(&normalize(p)));
    }
}
