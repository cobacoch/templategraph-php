//! Graphviz DOT renderer for [`Graph`].
//!
//! Output is deterministic — node and edge order follow the `Graph`'s
//! insertion order, which the builder maintains as a function of entrypoint
//! order and traversal. This keeps snapshot tests stable.

use crate::graph::{Edge, EdgeKind, Graph, Node, NodeKind};

pub fn render(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("digraph templategraph {\n");
    out.push_str("    rankdir=LR;\n");
    out.push_str("    node [shape=box];\n");

    if !graph.nodes.is_empty() {
        out.push('\n');
    }
    for node in &graph.nodes {
        push_node(&mut out, node);
    }

    if !graph.edges.is_empty() {
        out.push('\n');
    }
    for edge in &graph.edges {
        push_edge(&mut out, edge);
    }

    out.push_str("}\n");
    out
}

fn push_node(out: &mut String, node: &Node) {
    out.push_str("    ");
    push_quoted(out, &node.id);
    out.push_str(" [label=");
    push_quoted(out, &node.display_name);
    if let Some(attrs) = node_attrs(node.kind) {
        out.push_str(", ");
        out.push_str(attrs);
    }
    out.push_str("];\n");
}

fn push_edge(out: &mut String, edge: &Edge) {
    out.push_str("    ");
    push_quoted(out, &edge.from);
    out.push_str(" -> ");
    push_quoted(out, &edge.to);
    out.push_str(" [label=\"");
    out.push_str(edge_label(edge.kind));
    out.push('"');
    if matches!(edge.kind, EdgeKind::Unresolved) {
        out.push_str(", style=dashed");
    }
    out.push_str("];\n");
}

fn node_attrs(kind: NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::Entry => Some("shape=doubleoctagon"),
        NodeKind::PhpTemplate => None,
        NodeKind::Unresolved => Some("style=dashed"),
    }
}

fn edge_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Include => "include",
        EdgeKind::Require => "require",
        EdgeKind::IncludeOnce => "include_once",
        EdgeKind::RequireOnce => "require_once",
        EdgeKind::Unresolved => "unresolved",
    }
}

// DOT double-quoted strings only need `"` and `\` escaped; newlines are
// represented by `\n` inside the quoted string but are unlikely in our labels
// (display names are single-line paths). We escape them defensively anyway.
fn push_quoted(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Graph, Node, NodeKind};

    fn node(id: &str, display: &str, kind: NodeKind, is_entry: bool) -> Node {
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

    #[test]
    fn empty_graph_renders_minimal_digraph() {
        let out = render(&Graph::new());
        assert_eq!(
            out,
            "digraph templategraph {\n    rankdir=LR;\n    node [shape=box];\n}\n"
        );
    }

    #[test]
    fn entry_node_uses_doubleoctagon_shape() {
        let mut g = Graph::new();
        g.nodes.push(node("a", "index.php", NodeKind::Entry, true));
        let out = render(&g);
        assert!(out.contains(r#""a" [label="index.php", shape=doubleoctagon];"#));
    }

    #[test]
    fn php_template_node_uses_default_attrs() {
        let mut g = Graph::new();
        g.nodes
            .push(node("a", "header.php", NodeKind::PhpTemplate, false));
        let out = render(&g);
        assert!(out.contains(r#""a" [label="header.php"];"#));
    }

    #[test]
    fn unresolved_node_uses_dashed_style() {
        let mut g = Graph::new();
        g.nodes.push(node(
            "u",
            "unresolved: $dynamic",
            NodeKind::Unresolved,
            false,
        ));
        let out = render(&g);
        assert!(out.contains(r#""u" [label="unresolved: $dynamic", style=dashed];"#));
    }

    #[test]
    fn edge_kinds_are_labeled() {
        let mut g = Graph::new();
        g.edges.push(Edge {
            from: "a".into(),
            to: "b".into(),
            kind: EdgeKind::Include,
        });
        g.edges.push(Edge {
            from: "a".into(),
            to: "c".into(),
            kind: EdgeKind::Require,
        });
        g.edges.push(Edge {
            from: "a".into(),
            to: "d".into(),
            kind: EdgeKind::IncludeOnce,
        });
        g.edges.push(Edge {
            from: "a".into(),
            to: "e".into(),
            kind: EdgeKind::RequireOnce,
        });
        let out = render(&g);
        assert!(out.contains(r#""a" -> "b" [label="include"];"#));
        assert!(out.contains(r#""a" -> "c" [label="require"];"#));
        assert!(out.contains(r#""a" -> "d" [label="include_once"];"#));
        assert!(out.contains(r#""a" -> "e" [label="require_once"];"#));
    }

    #[test]
    fn unresolved_edge_is_dashed() {
        let mut g = Graph::new();
        g.edges.push(Edge {
            from: "a".into(),
            to: "u".into(),
            kind: EdgeKind::Unresolved,
        });
        let out = render(&g);
        assert!(out.contains(r#""a" -> "u" [label="unresolved", style=dashed];"#));
    }

    #[test]
    fn quotes_and_backslashes_in_labels_are_escaped() {
        let mut g = Graph::new();
        g.nodes.push(node(
            r#"id-with-"quote""#,
            r#"name with " and \ chars"#,
            NodeKind::PhpTemplate,
            false,
        ));
        let out = render(&g);
        assert!(out.contains(r#""id-with-\"quote\"" [label="name with \" and \\ chars"];"#));
    }
}

#[cfg(test)]
mod snapshots {
    //! End-to-end snapshots driven through `build_graph` so that the captured
    //! DOT reflects realistic node ids / display names produced by the
    //! builder, not hand-crafted approximations.

    use std::path::PathBuf;

    use crate::graph::builder::build_graph;
    use crate::output::dot::render;
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
    fn snapshot_cycle_does_not_loop() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/a.php", r#"<?php include __DIR__ . '/b.php';"#);
        reader.add("/project/b.php", r#"<?php include __DIR__ . '/a.php';"#);

        let graph = build_graph(&[entry("/project/a.php")], &root(), None, &reader).unwrap();
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
}
