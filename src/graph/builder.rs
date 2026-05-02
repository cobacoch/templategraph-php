use std::collections::{HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::graph::model::{Edge, EdgeKind, Graph, Node, NodeId, NodeKind};
use crate::parser::php::{self, RawIncludeDirective};
use crate::parser::resolver::{self, Resolved};
use crate::path::{self, AbsolutePath, RootRelativePath};
use crate::scanner::FileReader;

pub fn build_graph(
    entrypoints: &[AbsolutePath],
    project_root: &AbsolutePath,
    document_root: Option<&AbsolutePath>,
    file_reader: &dyn FileReader,
) -> Result<Graph> {
    let mut graph = Graph::new();
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<(AbsolutePath, bool)> = VecDeque::new();

    for entry in entrypoints {
        queue.push_back((entry.clone(), true));
    }

    while let Some((file, is_entrypoint)) = queue.pop_front() {
        let id = node_id_for(&file);
        if !visited.insert(id.clone()) {
            continue;
        }

        let source = match file_reader.read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                // A missing entrypoint is fatal — the user explicitly named it.
                // A missing include target is recorded as Unresolved so the
                // rest of the graph can still be built.
                if !is_entrypoint && is_not_found(&e) {
                    // Rewrite the parent's edge target to the namespaced
                    // missing-file id so all unresolved nodes share a single
                    // `unresolved::` id scheme. The edge `kind` is left as
                    // the original include kind to preserve the PHP-level
                    // information; output layers can re-classify if desired.
                    let missing_id = missing_file_id(&file);
                    for edge in graph.edges.iter_mut() {
                        if edge.to == id {
                            edge.to = missing_id.clone();
                        }
                    }
                    graph.nodes.push(missing_file_node(&missing_id, &file));
                    continue;
                }
                return Err(e);
            }
        };

        let kind = if is_entrypoint {
            NodeKind::Entry
        } else {
            NodeKind::PhpTemplate
        };
        graph.nodes.push(Node {
            id: id.clone(),
            absolute_path: Some(file.clone()),
            root_relative_path: relative_to(&file, project_root),
            kind,
            display_name: display_name(&file, project_root),
            is_entrypoint,
        });

        let directives = match php::extract_include_directives(&source) {
            Ok(ds) => ds,
            // Tree-sitter is error-tolerant, so this branch is effectively
            // unreachable today. Skipping the file leaves the rest of the
            // graph intact if it ever does fail.
            Err(_) => continue,
        };

        for directive in &directives {
            handle_directive(directive, &file, document_root, &id, &mut graph, &mut queue);
        }
    }

    Ok(graph)
}

// Builds the graph from a union of explicit and auto-discovered candidates,
// then post-hoc demotes auto-discovered nodes that have an incoming edge
// from another candidate (i.e., are included by some other walked file).
//
// The single BFS replaces the older two-pass design that read and parsed
// each candidate twice — once to compute include targets, then again inside
// `build_graph`. Demotion is restricted to edges between candidates so a
// "page" that is also reachable via an external (non-candidate) include
// chain still stays an entrypoint.
//
// Files in `explicit_entrypoints` are never demoted regardless of incoming
// edges — the user named them.
pub fn build_graph_with_discovery(
    explicit_entrypoints: &[AbsolutePath],
    discovered_candidates: &[AbsolutePath],
    project_root: &AbsolutePath,
    document_root: Option<&AbsolutePath>,
    file_reader: &dyn FileReader,
) -> Result<Graph> {
    let mut all_seeds: Vec<AbsolutePath> = explicit_entrypoints
        .iter()
        .chain(discovered_candidates.iter())
        .cloned()
        .collect();
    all_seeds.sort_by(|a, b| a.as_path().cmp(b.as_path()));
    all_seeds.dedup();

    let mut graph = build_graph(&all_seeds, project_root, document_root, file_reader)?;

    let candidate_ids: HashSet<NodeId> = all_seeds.iter().map(node_id_for).collect();
    let explicit_ids: HashSet<NodeId> = explicit_entrypoints.iter().map(node_id_for).collect();
    let demotable: HashSet<NodeId> = graph
        .edges
        .iter()
        .filter(|e| candidate_ids.contains(&e.from))
        .map(|e| e.to.clone())
        .collect();

    for node in &mut graph.nodes {
        if explicit_ids.contains(&node.id) {
            continue;
        }
        if demotable.contains(&node.id) && node.is_entrypoint {
            node.is_entrypoint = false;
            node.kind = NodeKind::PhpTemplate;
        }
    }
    Ok(graph)
}

// Only `NotFound` is treated as recoverable for include targets in the MVP.
// Other I/O errors (`PermissionDenied`, `IsADirectory`, `InvalidData` for
// non-UTF-8 content, etc.) are propagated as fatal so misconfigurations are
// visible rather than silently dropped. Future work may broaden this set.
fn is_not_found(error: &Error) -> bool {
    matches!(error, Error::Io(io_err) if io_err.kind() == io::ErrorKind::NotFound)
}

fn missing_file_id(file: &AbsolutePath) -> NodeId {
    format!("unresolved::missing::{}", file.as_path().display())
}

fn missing_file_node(id: &NodeId, file: &AbsolutePath) -> Node {
    Node {
        id: id.clone(),
        absolute_path: None,
        root_relative_path: None,
        kind: NodeKind::Unresolved,
        display_name: format!("unresolved: file not found {}", file.as_path().display()),
        is_entrypoint: false,
    }
}

fn handle_directive(
    directive: &RawIncludeDirective,
    current_file: &AbsolutePath,
    document_root: Option<&AbsolutePath>,
    from_id: &NodeId,
    graph: &mut Graph,
    queue: &mut VecDeque<(AbsolutePath, bool)>,
) {
    let ctx = resolver::Context {
        current_file,
        document_root,
    };
    match resolver::resolve(directive, &ctx) {
        Resolved::Path(path) => {
            let target = absolutize(&path, current_file);
            let target_id = node_id_for(&target);
            graph.edges.push(Edge {
                from: from_id.clone(),
                to: target_id,
                kind: directive.kind.into(),
            });
            queue.push_back((target, false));
        }
        Resolved::Unresolved {
            argument_source, ..
        } => {
            let target_id = unresolved_id(&argument_source);
            if graph.find_node(&target_id).is_none() {
                graph.nodes.push(Node {
                    id: target_id.clone(),
                    absolute_path: None,
                    root_relative_path: None,
                    kind: NodeKind::Unresolved,
                    display_name: format!("unresolved: {}", argument_source),
                    is_entrypoint: false,
                });
            }
            graph.edges.push(Edge {
                from: from_id.clone(),
                to: target_id,
                kind: EdgeKind::Unresolved,
            });
        }
    }
}

fn node_id_for(path: &AbsolutePath) -> NodeId {
    path.as_path().display().to_string()
}

fn unresolved_id(argument_source: &str) -> NodeId {
    format!("unresolved::{}", argument_source)
}

fn relative_to(file: &AbsolutePath, root: &AbsolutePath) -> Option<RootRelativePath> {
    file.as_path()
        .strip_prefix(root.as_path())
        .ok()
        .and_then(|p| RootRelativePath::new(p.to_path_buf()).ok())
}

fn display_name(file: &AbsolutePath, root: &AbsolutePath) -> String {
    file.as_path()
        .strip_prefix(root.as_path())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| file.as_path().display().to_string())
}

fn absolutize(path: &Path, current_file: &AbsolutePath) -> AbsolutePath {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let parent = current_file
            .as_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"));
        parent.join(path)
    };
    AbsolutePath::new(path::normalize(&joined)).expect("absolute by construction")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::in_memory::InMemoryFileReader;

    fn root() -> AbsolutePath {
        AbsolutePath::new(PathBuf::from("/project")).unwrap()
    }

    fn entry(path: &str) -> AbsolutePath {
        AbsolutePath::new(PathBuf::from(path)).unwrap()
    }

    #[test]
    fn single_entrypoint_with_one_include() {
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

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::Include);

        let entry_node = graph.nodes.iter().find(|n| n.is_entrypoint).unwrap();
        assert_eq!(entry_node.kind, NodeKind::Entry);
        assert_eq!(
            entry_node.root_relative_path.as_ref().unwrap().as_path(),
            Path::new("public/index.php")
        );
        assert_eq!(entry_node.display_name, "public/index.php");

        let header_node = graph.nodes.iter().find(|n| !n.is_entrypoint).unwrap();
        assert_eq!(header_node.kind, NodeKind::PhpTemplate);
    }

    #[test]
    fn chain_of_includes() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/a.php", r#"<?php include __DIR__ . '/b.php';"#);
        reader.add("/project/b.php", r#"<?php require __DIR__ . '/c.php';"#);
        reader.add("/project/c.php", "<?php echo 'c';");

        let graph = build_graph(&[entry("/project/a.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::Include));
        assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::Require));
    }

    #[test]
    fn cycle_does_not_loop() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/a.php", r#"<?php include __DIR__ . '/b.php';"#);
        reader.add("/project/b.php", r#"<?php include __DIR__ . '/a.php';"#);

        let graph = build_graph(&[entry("/project/a.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn multiple_entrypoints() {
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

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.nodes.iter().filter(|n| n.is_entrypoint).count(), 2);
    }

    #[test]
    fn unresolved_dependency_recorded() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/index.php", r#"<?php include $dynamic;"#);

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        let unresolved = graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Unresolved)
            .unwrap();
        assert!(unresolved.absolute_path.is_none());
        assert!(unresolved.root_relative_path.is_none());

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::Unresolved);
    }

    #[test]
    fn duplicate_unresolved_argument_dedupes_node_but_keeps_edges() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/a.php",
            r#"<?php include $dynamic; include $dynamic;"#,
        );

        let graph = build_graph(&[entry("/project/a.php")], &root(), None, &reader).unwrap();

        let unresolved_count = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Unresolved)
            .count();
        assert_eq!(unresolved_count, 1);
        assert_eq!(
            graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Unresolved)
                .count(),
            2
        );
    }

    #[test]
    fn dotdot_in_path_is_normalized() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/a/x.php",
            r#"<?php include __DIR__ . '/../b/c.php';"#,
        );
        reader.add("/project/b/c.php", "<?php echo 'c';");

        let graph = build_graph(&[entry("/project/a/x.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        let target = graph.nodes.iter().find(|n| !n.is_entrypoint).unwrap();
        assert_eq!(
            target.absolute_path.as_ref().unwrap().as_path(),
            Path::new("/project/b/c.php")
        );
    }

    #[test]
    fn same_file_via_different_paths_dedupes_to_one_node() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/a/x.php",
            r#"<?php
include __DIR__ . '/../b/c.php';
include __DIR__ . '/../../project/b/c.php';
"#,
        );
        reader.add("/project/b/c.php", "<?php echo 'c';");

        let graph = build_graph(&[entry("/project/a/x.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 2);
        let target_id = graph
            .nodes
            .iter()
            .find(|n| !n.is_entrypoint)
            .unwrap()
            .id
            .clone();
        assert!(graph.edges.iter().all(|e| e.to == target_id));
    }

    #[test]
    fn missing_include_target_becomes_unresolved_node() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include __DIR__ . '/missing.php';"#,
        );

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        let unresolved = graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Unresolved)
            .unwrap();
        assert!(unresolved.absolute_path.is_none());
        assert!(unresolved.display_name.contains("file not found"));
        assert!(unresolved.display_name.contains("/project/missing.php"));
        assert_eq!(unresolved.id, "unresolved::missing::/project/missing.php");

        assert_eq!(graph.edges.len(), 1);
        // Edge kind preserves the original PHP include kind even though the
        // target is unresolved.
        assert_eq!(graph.edges[0].kind, EdgeKind::Include);
        // Edge target uses the namespaced unresolved id, matching the node.
        assert_eq!(graph.edges[0].to, unresolved.id);
    }

    #[test]
    fn missing_entrypoint_propagates_error() {
        let reader = InMemoryFileReader::new();
        let result = build_graph(
            &[entry("/project/missing-entry.php")],
            &root(),
            None,
            &reader,
        );
        assert!(matches!(result, Err(Error::Io(_))));
    }

    #[test]
    fn includes_outside_project_root_use_absolute_display_name() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include '/external/lib.php';"#,
        );
        reader.add("/external/lib.php", "<?php echo 'lib';");

        let graph = build_graph(&[entry("/project/index.php")], &root(), None, &reader).unwrap();

        let external = graph
            .nodes
            .iter()
            .find(|n| {
                n.absolute_path
                    .as_ref()
                    .map(|p| p.as_path() == Path::new("/external/lib.php"))
                    .unwrap_or(false)
            })
            .unwrap();
        assert!(external.root_relative_path.is_none());
        assert_eq!(external.display_name, "/external/lib.php");
    }

    #[test]
    fn discovery_demotes_candidate_with_incoming_edge_from_another_candidate() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/index.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add("/project/header.php", "<?php");

        let graph = build_graph_with_discovery(
            &[],
            &[entry("/project/index.php"), entry("/project/header.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();

        let header = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "header.php")
            .unwrap();
        assert!(!header.is_entrypoint, "header.php should be demoted");
        assert_eq!(header.kind, NodeKind::PhpTemplate);

        let index = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "index.php")
            .unwrap();
        assert!(index.is_entrypoint);
        assert_eq!(index.kind, NodeKind::Entry);
    }

    #[test]
    fn discovery_keeps_orphan_candidate_as_entrypoint() {
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/orphan.php", "<?php");

        let graph = build_graph_with_discovery(
            &[],
            &[entry("/project/orphan.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();

        let orphan = &graph.nodes[0];
        assert!(orphan.is_entrypoint);
        assert_eq!(orphan.kind, NodeKind::Entry);
    }

    #[test]
    fn discovery_keeps_candidate_reached_only_via_non_candidate() {
        // discovered = [a, c]; a includes b (not a candidate), b includes c.
        // c gets an incoming edge from b, but b is not a candidate, so c
        // must remain an entrypoint.
        let mut reader = InMemoryFileReader::new();
        reader.add("/project/a.php", r#"<?php include __DIR__ . '/b.php';"#);
        reader.add("/project/b.php", r#"<?php include __DIR__ . '/c.php';"#);
        reader.add("/project/c.php", "<?php");

        let graph = build_graph_with_discovery(
            &[],
            &[entry("/project/a.php"), entry("/project/c.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();

        let c = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "c.php")
            .unwrap();
        assert!(
            c.is_entrypoint,
            "c reached only via non-candidate b stays Entry"
        );
        let b = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "b.php")
            .unwrap();
        assert!(!b.is_entrypoint);
        assert_eq!(b.kind, NodeKind::PhpTemplate);
    }

    #[test]
    fn discovery_never_demotes_explicit_entrypoint() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/page.php",
            r#"<?php include __DIR__ . '/header.php';"#,
        );
        reader.add("/project/header.php", "<?php");

        let graph = build_graph_with_discovery(
            &[entry("/project/header.php")],
            &[entry("/project/page.php"), entry("/project/header.php")],
            &root(),
            None,
            &reader,
        )
        .unwrap();

        let header = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "header.php")
            .unwrap();
        assert!(
            header.is_entrypoint,
            "explicit entrypoint must not be demoted"
        );
    }

    #[test]
    fn discovery_resolves_server_document_root_when_provided() {
        let doc_root = AbsolutePath::new(PathBuf::from("/project")).unwrap();
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/page.php",
            r#"<?php include $_SERVER['DOCUMENT_ROOT'] . "/inc/header.php";"#,
        );
        reader.add("/project/inc/header.php", "<?php");

        let graph = build_graph_with_discovery(
            &[],
            &[entry("/project/page.php"), entry("/project/inc/header.php")],
            &root(),
            Some(&doc_root),
            &reader,
        )
        .unwrap();

        let header = graph
            .nodes
            .iter()
            .find(|n| n.display_name == "inc/header.php")
            .unwrap();
        assert!(
            !header.is_entrypoint,
            "header included via DOCUMENT_ROOT is demoted"
        );
    }

    #[test]
    fn build_graph_without_document_root_leaves_server_subscript_unresolved() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/page.php",
            r#"<?php include $_SERVER['DOCUMENT_ROOT'] . "/inc/header.php";"#,
        );

        let graph = build_graph(&[entry("/project/page.php")], &root(), None, &reader).unwrap();
        let unresolved = graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Unresolved)
            .unwrap();
        assert!(unresolved.display_name.contains("DOCUMENT_ROOT"));
    }
}
