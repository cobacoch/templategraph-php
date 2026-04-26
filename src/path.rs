use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AbsolutePath(PathBuf);

impl AbsolutePath {
    pub fn new(path: PathBuf) -> Option<Self> {
        if path.is_absolute() {
            Some(Self(path))
        } else {
            None
        }
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
    pub fn new(path: PathBuf) -> Option<Self> {
        if path.is_relative() {
            Some(Self(path))
        } else {
            None
        }
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
        assert!(AbsolutePath::new(PathBuf::from("foo/bar")).is_none());
    }

    #[test]
    fn root_relative_path_accepts_relative() {
        let p = RootRelativePath::new(PathBuf::from("foo/bar")).unwrap();
        assert_eq!(p.as_path(), Path::new("foo/bar"));
    }

    #[test]
    fn root_relative_path_rejects_absolute() {
        assert!(RootRelativePath::new(PathBuf::from("/foo/bar")).is_none());
    }
}
