use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::path::{self, AbsolutePath};
use crate::scanner::{DirWalker, FileReader};

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

// Recursive directory walker that lists files matching the configured
// extensions. Symlinks (both files and directories) are skipped; we never
// follow them so that pathological project layouts (e.g. a symlink loop in
// `vendor/`) cannot cause infinite traversal. Directory and file names that
// match `excludes` exactly are skipped — this is intentionally simple
// (no glob support) and mirrors how `templategraph.toml` documents
// `exclude = ["vendor", "node_modules", ".git"]`.
#[derive(Debug, Clone)]
pub struct FilesystemDirWalker {
    excludes: Vec<String>,
    extensions: Vec<String>,
}

impl Default for FilesystemDirWalker {
    fn default() -> Self {
        Self {
            excludes: Vec::new(),
            extensions: vec!["php".to_string()],
        }
    }
}

impl FilesystemDirWalker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_excludes<I, S>(mut self, excludes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.excludes = excludes.into_iter().map(Into::into).collect();
        self
    }

    fn walk_into(&self, dir: &Path, out: &mut Vec<AbsolutePath>) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(Error::Io)? {
            let entry = entry.map_err(Error::Io)?;
            let file_type = entry.file_type().map_err(Error::Io)?;

            // A non-UTF-8 file name cannot be matched against the configured
            // exclude / extension lists in a meaningful way; skip it rather
            // than silently miscategorizing.
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if self.excludes.iter().any(|e| e == &name) {
                continue;
            }

            let entry_path = entry.path();
            if file_type.is_dir() {
                self.walk_into(&entry_path, out)?;
            } else if file_type.is_file() {
                let matches_ext = entry_path
                    .extension()
                    .and_then(|os| os.to_str())
                    .is_some_and(|ext| self.extensions.iter().any(|e| e == ext));
                if matches_ext {
                    if let Ok(absolute) = AbsolutePath::new(path::normalize(&entry_path)) {
                        out.push(absolute);
                    }
                }
            }
        }
        Ok(())
    }
}

impl DirWalker for FilesystemDirWalker {
    fn walk(&self, root: &AbsolutePath) -> Result<Vec<AbsolutePath>> {
        let mut out = Vec::new();
        self.walk_into(root.as_path(), &mut out)?;
        out.sort_by(|a, b| a.as_path().cmp(b.as_path()));
        Ok(out)
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
        let err = FilesystemFileReader::new()
            .read_to_string(&abs)
            .unwrap_err();
        match err {
            Error::Io(io_err) => assert_eq!(io_err.kind(), io::ErrorKind::NotFound),
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn dir_walker_collects_php_files_recursively() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("index.php"), b"");
        write(&dir.path().join("sub/page.php"), b"");
        write(&dir.path().join("sub/deeper/inc.php"), b"");
        write(&dir.path().join("readme.md"), b"");

        let root = AbsolutePath::new(dir.path().to_path_buf()).unwrap();
        let walker = FilesystemDirWalker::new();
        let files = walker.walk(&root).unwrap();

        let names: Vec<String> = files
            .iter()
            .map(|p| {
                p.as_path()
                    .strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            vec!["index.php", "sub/deeper/inc.php", "sub/page.php"]
        );
    }

    #[test]
    fn dir_walker_skips_excluded_directories_and_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("index.php"), b"");
        write(&dir.path().join("vendor/lib.php"), b"");
        write(&dir.path().join("node_modules/x.php"), b"");
        write(&dir.path().join(".git/hook.php"), b"");

        let root = AbsolutePath::new(dir.path().to_path_buf()).unwrap();
        let walker = FilesystemDirWalker::new().with_excludes(["vendor", "node_modules", ".git"]);
        let files = walker.walk(&root).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].as_path().ends_with("index.php"));
    }

    #[test]
    fn dir_walker_only_returns_configured_extensions() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.php"), b"");
        write(&dir.path().join("b.phtml"), b"");
        write(&dir.path().join("c.txt"), b"");

        let root = AbsolutePath::new(dir.path().to_path_buf()).unwrap();
        let files = FilesystemDirWalker::new().walk(&root).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].as_path().ends_with("a.php"));
    }

    #[test]
    fn dir_walker_returns_sorted_paths() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("c.php"), b"");
        write(&dir.path().join("a.php"), b"");
        write(&dir.path().join("b.php"), b"");

        let root = AbsolutePath::new(dir.path().to_path_buf()).unwrap();
        let files = FilesystemDirWalker::new().walk(&root).unwrap();
        let names: Vec<&str> = files
            .iter()
            .map(|p| p.as_path().file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["a.php", "b.php", "c.php"]);
    }

    #[cfg(unix)]
    #[test]
    fn dir_walker_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("real/index.php"), b"");
        symlink(dir.path().join("real"), dir.path().join("link")).unwrap();

        let root = AbsolutePath::new(dir.path().to_path_buf()).unwrap();
        let files = FilesystemDirWalker::new().walk(&root).unwrap();
        // Only the "real" path should appear; the symlinked dir is skipped.
        assert_eq!(files.len(), 1);
        assert!(files[0].as_path().ends_with("real/index.php"));
    }
}
