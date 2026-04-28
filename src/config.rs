use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::output::Format;

// Unknown fields are rejected to surface user typos. The `[php]` and `[blade]`
// sections documented in docs/08-config.md will be opted in when those
// features land (MVP+1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub root: Option<PathBuf>,

    #[serde(default)]
    pub entrypoints: Vec<PathBuf>,

    #[serde(default)]
    pub exclude: Vec<String>,

    #[serde(default)]
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    #[serde(default)]
    pub default_format: Option<Format>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config file {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub fn parse(content: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(content)
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&content).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_example() {
        let toml = r#"
root = "."
entrypoints = ["public/index.php", "public/about/index.php"]
exclude = ["vendor", "node_modules"]

[output]
default_format = "dot"
"#;
        let config = parse(toml).unwrap();
        assert_eq!(config.root, Some(PathBuf::from(".")));
        assert_eq!(
            config.entrypoints,
            vec![
                PathBuf::from("public/index.php"),
                PathBuf::from("public/about/index.php"),
            ]
        );
        assert_eq!(config.exclude, vec!["vendor", "node_modules"]);
        assert_eq!(config.output.default_format, Some(Format::Dot));
    }

    #[test]
    fn parse_empty_yields_defaults() {
        let config = parse("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn parse_partial_fills_defaults() {
        let config = parse(r#"entrypoints = ["index.php"]"#).unwrap();
        assert_eq!(config.entrypoints, vec![PathBuf::from("index.php")]);
        assert!(config.exclude.is_empty());
        assert!(config.root.is_none());
        assert_eq!(config.output, OutputConfig::default());
    }

    #[test]
    fn parse_accepts_json_format() {
        let config = parse(r#"output = { default_format = "json" }"#).unwrap();
        assert_eq!(config.output.default_format, Some(Format::Json));
    }

    #[test]
    fn parse_rejects_invalid_toml() {
        let result = parse("not = = valid");
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_unknown_root_field() {
        let result = parse(r#"unknown_key = "x""#);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_unknown_output_field() {
        let result = parse(r#"
[output]
default_format = "dot"
unexpected = true
"#);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_invalid_format() {
        let result = parse(r#"output = { default_format = "yaml" }"#);
        assert!(result.is_err());
    }
}
