use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
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
pub struct OutputConfig {
    #[serde(default)]
    pub default_format: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

pub fn parse(content: &str) -> Result<Config, ConfigError> {
    Ok(toml::from_str(content)?)
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&content)
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
        assert_eq!(config.output.default_format.as_deref(), Some("dot"));
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
    fn parse_rejects_invalid_toml() {
        let result = parse("not = = valid");
        assert!(matches!(result, Err(ConfigError::Parse(_))));
    }
}
