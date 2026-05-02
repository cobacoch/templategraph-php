//! JSON renderer for [`Graph`].
//!
//! Output is deterministic — node and edge order follow the `Graph`'s
//! insertion order (a function of entrypoint order and BFS traversal in the
//! builder). Pretty-printed with two-space indentation for diffability.
//!
//! Domain types in `graph::model` intentionally do not derive `Serialize`;
//! the renderer constructs serializable DTOs here so the JSON wire shape
//! can evolve independently of the in-memory graph representation.
//!
//! The output schema is documented in `schemas/output-graph-v1.schema.json`
//! and identified at runtime by the `schema_version` field. Any breaking
//! change to the wire shape MUST bump `SCHEMA_VERSION` and ship a new
//! `output-graph-vN.schema.json` alongside the existing one.

use serde::Serialize;

use crate::graph::{Edge, EdgeKind, Graph, Node, NodeKind};

pub const SCHEMA_VERSION: u32 = 1;

pub fn render(graph: &Graph) -> String {
    let dto = JsonGraph {
        schema_version: SCHEMA_VERSION,
        nodes: graph.nodes.iter().map(JsonNode::from).collect(),
        edges: graph.edges.iter().map(JsonEdge::from).collect(),
    };
    serde_json::to_string_pretty(&dto).expect("DTO contains only infallibly-serializable types")
}

#[derive(Serialize)]
struct JsonGraph {
    schema_version: u32,
    nodes: Vec<JsonNode>,
    edges: Vec<JsonEdge>,
}

#[derive(Serialize)]
struct JsonNode {
    id: String,
    kind: &'static str,
    display_name: String,
    is_entrypoint: bool,
    // Optional path fields are emitted as strings via `to_string_lossy`,
    // which substitutes U+FFFD for any non-UTF-8 byte sequences. Real-world
    // PHP filenames are virtually always UTF-8 so this is a non-issue in
    // practice; consumers needing byte-exact paths should use a different
    // output mechanism.
    #[serde(skip_serializing_if = "Option::is_none")]
    absolute_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    root_relative_path: Option<String>,
}

#[derive(Serialize)]
struct JsonEdge {
    from: String,
    to: String,
    kind: &'static str,
}

impl From<&Node> for JsonNode {
    fn from(node: &Node) -> Self {
        Self {
            id: node.id.clone(),
            kind: node_kind_str(node.kind),
            display_name: node.display_name.clone(),
            is_entrypoint: node.is_entrypoint,
            absolute_path: node
                .absolute_path
                .as_ref()
                .map(|p| p.as_path().to_string_lossy().into_owned()),
            root_relative_path: node
                .root_relative_path
                .as_ref()
                .map(|p| p.as_path().to_string_lossy().into_owned()),
        }
    }
}

impl From<&Edge> for JsonEdge {
    fn from(edge: &Edge) -> Self {
        Self {
            from: edge.from.clone(),
            to: edge.to.clone(),
            kind: edge_kind_str(edge.kind),
        }
    }
}

// Keep these strings in sync with `output::dot::edge_label` /
// `node_attrs`. Both renderers describe the same kinds; divergence would
// confuse consumers comparing DOT and JSON output.
fn node_kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Entry => "entry",
        NodeKind::PhpTemplate => "php_template",
        NodeKind::Unresolved => "unresolved",
    }
}

fn edge_kind_str(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Include => "include",
        EdgeKind::Require => "require",
        EdgeKind::IncludeOnce => "include_once",
        EdgeKind::RequireOnce => "require_once",
        EdgeKind::Unresolved => "unresolved",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Graph, Node, NodeKind, UnresolvedReason};
    use crate::path::{AbsolutePath, RootRelativePath};
    use std::path::PathBuf;

    // Helper for resolved nodes (Entry / PhpTemplate). Constructing an
    // Unresolved node through this path would skip `unresolved_reason`,
    // so route those tests through `Node::unresolved` instead.
    fn node(id: &str, display: &str, kind: NodeKind, is_entry: bool) -> Node {
        debug_assert!(
            !matches!(kind, NodeKind::Unresolved),
            "use Node::unresolved for unresolved test fixtures"
        );
        Node {
            id: id.to_string(),
            absolute_path: None,
            root_relative_path: None,
            kind,
            display_name: display.to_string(),
            is_entrypoint: is_entry,
            unresolved_reason: None,
        }
    }

    fn parse(json: &str) -> serde_json::Value {
        serde_json::from_str(json).expect("renderer produced valid JSON")
    }

    #[test]
    fn empty_graph_renders_empty_arrays_with_schema_version() {
        let v = parse(&render(&Graph::new()));
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(v["edges"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn schema_version_constant_matches_output() {
        let v = parse(&render(&Graph::new()));
        assert_eq!(v["schema_version"], SCHEMA_VERSION);
    }

    #[test]
    fn node_kinds_serialize_to_snake_case() {
        let mut g = Graph::new();
        g.nodes.push(node("e", "index.php", NodeKind::Entry, true));
        g.nodes
            .push(node("t", "header.php", NodeKind::PhpTemplate, false));
        g.nodes.push(Node::unresolved(
            "u".into(),
            UnresolvedReason::DynamicArgument("$x".into()),
        ));
        let v = parse(&render(&g));
        let nodes = v["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["kind"], "entry");
        assert_eq!(nodes[1]["kind"], "php_template");
        assert_eq!(nodes[2]["kind"], "unresolved");
    }

    #[test]
    fn edge_kinds_serialize_to_snake_case() {
        let mut g = Graph::new();
        for (kind, _) in [
            (EdgeKind::Include, "include"),
            (EdgeKind::Require, "require"),
            (EdgeKind::IncludeOnce, "include_once"),
            (EdgeKind::RequireOnce, "require_once"),
            (EdgeKind::Unresolved, "unresolved"),
        ] {
            g.edges.push(Edge {
                from: "a".into(),
                to: "b".into(),
                kind,
            });
        }
        let v = parse(&render(&g));
        let edges = v["edges"].as_array().unwrap();
        assert_eq!(edges[0]["kind"], "include");
        assert_eq!(edges[1]["kind"], "require");
        assert_eq!(edges[2]["kind"], "include_once");
        assert_eq!(edges[3]["kind"], "require_once");
        assert_eq!(edges[4]["kind"], "unresolved");
    }

    #[test]
    fn node_emits_optional_paths_when_present() {
        let mut g = Graph::new();
        g.nodes.push(Node {
            id: "id".into(),
            absolute_path: Some(AbsolutePath::new(PathBuf::from("/project/index.php")).unwrap()),
            root_relative_path: Some(RootRelativePath::new(PathBuf::from("index.php")).unwrap()),
            kind: NodeKind::Entry,
            display_name: "index.php".into(),
            is_entrypoint: true,
            unresolved_reason: None,
        });
        let v = parse(&render(&g));
        let n = &v["nodes"][0];
        assert_eq!(n["absolute_path"], "/project/index.php");
        assert_eq!(n["root_relative_path"], "index.php");
        assert_eq!(n["display_name"], "index.php");
        assert_eq!(n["is_entrypoint"], true);
        assert_eq!(n["id"], "id");
    }

    #[test]
    fn node_omits_optional_paths_when_absent() {
        let mut g = Graph::new();
        g.nodes.push(Node::unresolved(
            "u".into(),
            UnresolvedReason::DynamicArgument("$x".into()),
        ));
        let v = parse(&render(&g));
        let n = &v["nodes"][0];
        // Absent fields are omitted (not present in the JSON object).
        assert!(n.get("absolute_path").is_none());
        assert!(n.get("root_relative_path").is_none());
    }

    #[test]
    fn edge_emits_from_to_and_kind() {
        let mut g = Graph::new();
        g.edges.push(Edge {
            from: "a".into(),
            to: "b".into(),
            kind: EdgeKind::Include,
        });
        let v = parse(&render(&g));
        let e = &v["edges"][0];
        assert_eq!(e["from"], "a");
        assert_eq!(e["to"], "b");
        assert_eq!(e["kind"], "include");
    }

    #[test]
    fn output_is_pretty_printed_with_two_space_indent() {
        let mut g = Graph::new();
        g.nodes.push(node("e", "i.php", NodeKind::Entry, true));
        let out = render(&g);
        // Pretty-printed JSON should have newlines between top-level keys.
        assert!(out.contains("\n"));
        assert!(out.contains("  \"nodes\""));
    }

    #[test]
    fn special_characters_in_display_name_round_trip() {
        // serde_json handles JSON-string escaping. We assert that quotes,
        // backslashes, newlines, and tabs survive a round-trip — both that
        // the renderer produces valid JSON and that consumers reading it
        // recover the exact source string.
        let mut g = Graph::new();
        let tricky = "name with \" and \\ and \n newline and \t tab";
        g.nodes
            .push(node("id", tricky, NodeKind::PhpTemplate, false));
        let out = render(&g);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["nodes"][0]["display_name"], tricky);
    }
}

#[cfg(test)]
mod snapshots {
    //! End-to-end snapshots driven through `build_graph` so that the captured
    //! JSON reflects realistic node ids / display names produced by the
    //! builder, not hand-crafted approximations.

    use std::path::PathBuf;

    use crate::graph::builder::build_graph;
    use crate::output::json::render;
    use crate::path::AbsolutePath;
    use crate::scanner::in_memory::InMemoryFileReader;

    fn root() -> AbsolutePath {
        AbsolutePath::new(PathBuf::from("/project")).unwrap()
    }

    fn entry(path: &str) -> AbsolutePath {
        AbsolutePath::new(PathBuf::from(path)).unwrap()
    }

    #[test]
    fn snapshot_single_include() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/public/index.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add("/project/public/header.php", "<?php echo 'header';");

        let graph = build_graph(
            &[entry("/project/public/index.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();
        insta::assert_snapshot!(render(&graph));
    }

    #[test]
    fn snapshot_all_include_kinds() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php
include __DIR__ . '/a.php';
require __DIR__ . '/b.php';
include_once __DIR__ . '/c.php';
require_once __DIR__ . '/d.php';
"#,
        );
        reader.add("/project/a.php", "<?php");
        reader.add("/project/b.php", "<?php");
        reader.add("/project/c.php", "<?php");
        reader.add("/project/d.php", "<?php");

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        insta::assert_snapshot!(render(&graph));
    }

    #[test]
    fn snapshot_unresolved_dynamic_argument() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/index.php", r#"<?php include $dynamic;"#);

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        insta::assert_snapshot!(render(&graph));
    }

    #[test]
    fn snapshot_missing_include_target() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include __DIR__ . '/missing.php';"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();
        insta::assert_snapshot!(render(&graph));
    }

    #[test]
    fn snapshot_multiple_entrypoints_share_node() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/public/index.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add(
            "/project/public/about.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add("/project/public/header.php", "<?php echo 'header';");

        let graph = build_graph(
            &[
                entry("/project/public/index.php"),
                entry("/project/public/about.php"),
            ],
            &root(),
            None,
            &reader,
        )
        .unwrap();
        insta::assert_snapshot!(render(&graph));
    }

    #[test]
    fn snapshot_cycle_does_not_loop() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/a.php", r#"<?php include __DIR__ . '/b.php';"#);
        reader.add("/project/b.php", r#"<?php include __DIR__ . '/a.php';"#);

        let graph = build_graph(&[entry("/project/a.php")], &root(), None, &reader).unwrap();
        insta::assert_snapshot!(render(&graph));
    }
}
