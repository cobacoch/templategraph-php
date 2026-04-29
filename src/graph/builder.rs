use std::collections::{HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::graph::model::{Edge, EdgeKind, Graph, Node, NodeId, NodeKind};
use crate::parser::php::{self, RawIncludeDirective};
use crate::parser::resolver::{self, Resolved};
use crate::path::{AbsolutePath, RootRelativePath};
use crate::scanner::FileReader;

pub fn build_graph(
    entrypoints: &[AbsolutePath],
    project_root: &AbsolutePath,
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
                    graph.nodes.push(missing_file_node(&id, &file));
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
            handle_directive(directive, &file, &id, &mut graph, &mut queue);
        }
    }

    Ok(graph)
}

fn is_not_found(error: &Error) -> bool {
    matches!(error, Error::Io(io_err) if io_err.kind() == io::ErrorKind::NotFound)
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
    from_id: &NodeId,
    graph: &mut Graph,
    queue: &mut VecDeque<(AbsolutePath, bool)>,
) {
    match resolver::resolve(directive, current_file) {
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
    let normalized = normalize_path(&joined);
    AbsolutePath::new(normalized).expect("absolute by construction")
}

// Collapse `.` / `..` segments without touching the filesystem so that the
// same physical file reached via different syntactic paths shares a single
// node id.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
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
        reader.add(
            "/project/a.php",
            r#"<?php include __DIR__ . '/b.php';"#,
        );
        reader.add(
            "/project/b.php",
            r#"<?php require __DIR__ . '/c.php';"#,
        );
        reader.add("/project/c.php", "<?php echo 'c';");

        let graph = build_graph(&[entry("/project/a.php")], &root(), &reader).unwrap();

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::Include));
        assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::Require));
    }

    #[test]
    fn cycle_does_not_loop() {
        let mut reader = InMemoryFileReader::new();
        reader.add(
            "/project/a.php",
            r#"<?php include __DIR__ . '/b.php';"#,
        );
        reader.add(
            "/project/b.php",
            r#"<?php include __DIR__ . '/a.php';"#,
        );

        let graph = build_graph(&[entry("/project/a.php")], &root(), &reader).unwrap();

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

        let graph = build_graph(&[entry("/project/index.php")], &root(), &reader).unwrap();

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

        let graph = build_graph(&[entry("/project/a.php")], &root(), &reader).unwrap();

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

        let graph = build_graph(&[entry("/project/a/x.php")], &root(), &reader).unwrap();

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

        let graph = build_graph(&[entry("/project/a/x.php")], &root(), &reader).unwrap();

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

        let graph = build_graph(&[entry("/project/index.php")], &root(), &reader).unwrap();

        assert_eq!(graph.nodes.len(), 2);
        let unresolved = graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Unresolved)
            .unwrap();
        assert!(unresolved.absolute_path.is_none());
        assert!(unresolved.display_name.contains("file not found"));
        assert!(unresolved.display_name.contains("/project/missing.php"));
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::Include);
    }

    #[test]
    fn missing_entrypoint_propagates_error() {
        let reader = InMemoryFileReader::new();
        let result = build_graph(&[entry("/project/missing-entry.php")], &root(), &reader);
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

        let graph = build_graph(&[entry("/project/index.php")], &root(), &reader).unwrap();

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
}
