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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Entry,
    PhpTemplate,
    Unresolved,
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
