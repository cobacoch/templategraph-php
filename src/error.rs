#![allow(dead_code)]

use std::io;

use thiserror::Error;

use crate::config::ConfigError;
use crate::parser::blade::ViewNameError;
use crate::path::PathError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("dependency resolution error: {0}")]
    Resolve(String),

    #[error("path error: {0}")]
    Path(#[from] PathError),

    #[error("view name error: {0}")]
    ViewName(#[from] ViewNameError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_converts_via_from() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
