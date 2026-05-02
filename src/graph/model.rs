use std::path::PathBuf;

use crate::parser::php::IncludeKind;
use crate::path::{AbsolutePath, RootRelativePath};

pub type NodeId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub absolute_path: Option<AbsolutePath>,
    pub root_relative_path: Option<RootRelativePath>,
    pub kind: NodeKind,
    pub display_name: String,
    pub is_entrypoint: bool,
    /// Populated iff `kind == NodeKind::Unresolved`. The wire formats
    /// (DOT, JSON v1) keep using `display_name`; this field exists so
    /// internal callers and any future schema version can branch on the
    /// reason without re-parsing the human-readable label.
    pub unresolved_reason: Option<UnresolvedReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Entry,
    PhpTemplate,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The include argument couldn't be resolved at scan time (a
    /// variable, an unsupported expression, etc.). The string is the
    /// original PHP source of the `include` argument as it appeared in
    /// the file.
    DynamicArgument(String),
    /// The argument resolved statically to a path, but no file was
    /// present there. The path is the absolute, normalized location
    /// that was looked up.
    FileNotFound(PathBuf),
}

impl UnresolvedReason {
    /// Human-readable label that has shipped as `display_name` for
    /// unresolved nodes since v1 of the JSON schema. Computing it here
    /// keeps the wire format and the structured field from drifting
    /// apart over time.
    pub fn display_name(&self) -> String {
        match self {
            Self::DynamicArgument(arg) => format!("unresolved: {}", arg),
            Self::FileNotFound(path) => {
                format!("unresolved: file not found {}", path.display())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Include,
    Require,
    IncludeOnce,
    RequireOnce,
    Unresolved,
}

impl From<IncludeKind> for EdgeKind {
    fn from(kind: IncludeKind) -> Self {
        match kind {
            IncludeKind::Include => EdgeKind::Include,
            IncludeKind::Require => EdgeKind::Require,
            IncludeKind::IncludeOnce => EdgeKind::IncludeOnce,
            IncludeKind::RequireOnce => EdgeKind::RequireOnce,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_for_dynamic_argument_matches_v1_wire_format() {
        let r = UnresolvedReason::DynamicArgument("$dynamic".into());
        assert_eq!(r.display_name(), "unresolved: $dynamic");
    }

    #[test]
    fn display_name_for_file_not_found_matches_v1_wire_format() {
        let r = UnresolvedReason::FileNotFound(PathBuf::from("/project/missing.php"));
        assert_eq!(
            r.display_name(),
            "unresolved: file not found /project/missing.php"
        );
    }
}
