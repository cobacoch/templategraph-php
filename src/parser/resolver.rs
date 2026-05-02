//! Resolves the static argument of an `include` / `require` directive into a
//! filesystem path.
//!
//! MVP scope notes:
//!
//! - String literals are stripped of their surrounding quotes but not
//!   otherwise interpreted. Any backslash inside the literal (`\'`, `\\`,
//!   `\n`, …) is reported as unresolved rather than expanded — real-world
//!   template include paths almost never need escape sequences, so the
//!   resolver avoids the cost and bug surface of a hand-rolled string parser
//!   and instead surfaces such cases for the unresolved-dependencies report.
//! - Path concatenation joins evaluated parts with simple string append and
//!   assumes `/` as the separator (Unix-first MVP). The graph builder is
//!   responsible for normalizing the resulting paths against the project
//!   root.

#![allow(dead_code)]

use std::path::PathBuf;

use tree_sitter::{Node, Parser};

use crate::parser::php::RawIncludeDirective;
use crate::path::AbsolutePath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Path(PathBuf),
    Unresolved {
        argument_source: String,
        reason: String,
    },
}

// Context bundles the per-call inputs that propagate through the recursive
// `evaluate` walk: the file owning the include directive (used to resolve
// `__FILE__` / `__DIR__`) and an optional document root (used to resolve
// `$_SERVER['DOCUMENT_ROOT']`, the conventional pattern in PHP static sites
// for anchoring includes at the public webroot).
//
// `document_root` is intentionally optional and distinct from any "project
// root" concept: in many layouts (`root = "."`, `entrypoints =
// ["public/..."]`) the document root is a subdirectory of the project, not
// the project itself. When `document_root` is `None`, occurrences of
// `$_SERVER['DOCUMENT_ROOT']` are reported as unresolved rather than
// silently substituted with a misleading path.
pub struct Context<'a> {
    pub current_file: &'a AbsolutePath,
    pub document_root: Option<&'a AbsolutePath>,
}

pub fn resolve(directive: &RawIncludeDirective, ctx: &Context<'_>) -> Resolved {
    let wrapped = format!("<?php {};", directive.argument_source);

    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .is_err()
    {
        return unresolved(directive, "failed to set tree-sitter PHP language");
    }

    let Some(tree) = parser.parse(&wrapped, None) else {
        return unresolved(directive, "tree-sitter failed to parse argument");
    };

    let Some(expr) = find_argument_expression(tree.root_node()) else {
        return unresolved(directive, "argument expression not found");
    };

    match evaluate(expr, wrapped.as_bytes(), ctx) {
        Ok(value) => Resolved::Path(PathBuf::from(value)),
        Err(reason) => unresolved(directive, &reason),
    }
}

fn unresolved(directive: &RawIncludeDirective, reason: &str) -> Resolved {
    Resolved::Unresolved {
        argument_source: directive.argument_source.clone(),
        reason: reason.to_string(),
    }
}

fn find_argument_expression(root: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "expression_statement" {
            return child.named_child(0);
        }
    }
    None
}

fn evaluate(node: Node<'_>, source: &[u8], ctx: &Context<'_>) -> Result<String, String> {
    match node.kind() {
        "string" => evaluate_string(node, source),
        "encapsed_string" => evaluate_encapsed_string(node, source),
        "name" => evaluate_name(node, source, ctx.current_file),
        "binary_expression" => evaluate_binary(node, source, ctx),
        "function_call_expression" => evaluate_function_call(node, source, ctx),
        "subscript_expression" => evaluate_subscript(node, source, ctx.document_root),
        "parenthesized_expression" => {
            let inner = node
                .named_child(0)
                .ok_or_else(|| "empty parenthesized expression".to_string())?;
            evaluate(inner, source, ctx)
        }
        kind => Err(format!("unsupported expression kind: {}", kind)),
    }
}

fn evaluate_string(node: Node<'_>, source: &[u8]) -> Result<String, String> {
    let text = node.utf8_text(source).map_err(|e| e.to_string())?;
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return Err(format!("string literal too short: {}", text));
    }
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if !((first == b'\'' && last == b'\'') || (first == b'"' && last == b'"')) {
        return Err(format!("not a quoted string literal: {}", text));
    }
    let inner = &text[1..text.len() - 1];
    if inner.contains('\\') {
        return Err("string literal with escape sequences is not supported".to_string());
    }
    Ok(inner.to_string())
}

fn evaluate_encapsed_string(node: Node<'_>, source: &[u8]) -> Result<String, String> {
    // The MVP only handles literal-only double-quoted strings. Any `$`
    // inside the literal indicates variable interpolation and any `\`
    // indicates an escape sequence; both are reported as unresolved so the
    // resolver does not silently produce a wrong path.
    let text = node.utf8_text(source).map_err(|e| e.to_string())?;
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return Err(format!("not a double-quoted string literal: {}", text));
    }
    let inner = &text[1..text.len() - 1];
    if inner.contains('$') {
        return Err("double-quoted string with interpolation is not supported".to_string());
    }
    if inner.contains('\\') {
        return Err("double-quoted string with escape sequences is not supported".to_string());
    }
    Ok(inner.to_string())
}

fn evaluate_name(
    node: Node<'_>,
    source: &[u8],
    current_file: &AbsolutePath,
) -> Result<String, String> {
    let text = node.utf8_text(source).map_err(|e| e.to_string())?;
    match text {
        "__FILE__" => Ok(current_file.as_path().to_string_lossy().into_owned()),
        "__DIR__" => current_file
            .as_path()
            .parent()
            .ok_or_else(|| {
                format!(
                    "__DIR__ has no parent: {}",
                    current_file.as_path().display()
                )
            })
            .map(|p| p.to_string_lossy().into_owned()),
        other => Err(format!("unsupported constant: {}", other)),
    }
}

fn evaluate_binary(node: Node<'_>, source: &[u8], ctx: &Context<'_>) -> Result<String, String> {
    let operator = node
        .child_by_field_name("operator")
        .ok_or_else(|| "binary expression has no operator".to_string())?
        .utf8_text(source)
        .map_err(|e| e.to_string())?;
    if operator != "." {
        return Err(format!("unsupported binary operator: {}", operator));
    }
    let left = node
        .child_by_field_name("left")
        .ok_or_else(|| "binary expression has no left operand".to_string())?;
    let right = node
        .child_by_field_name("right")
        .ok_or_else(|| "binary expression has no right operand".to_string())?;
    let left_value = evaluate(left, source, ctx)?;
    let right_value = evaluate(right, source, ctx)?;
    Ok(format!("{}{}", left_value, right_value))
}

fn evaluate_function_call(
    node: Node<'_>,
    source: &[u8],
    ctx: &Context<'_>,
) -> Result<String, String> {
    let function_name = node
        .child_by_field_name("function")
        .ok_or_else(|| "function call has no function name".to_string())?
        .utf8_text(source)
        .map_err(|e| e.to_string())?;
    if function_name != "dirname" {
        return Err(format!("unsupported function: {}", function_name));
    }
    let arguments = node
        .child_by_field_name("arguments")
        .ok_or_else(|| "function call has no arguments".to_string())?;
    let mut cursor = arguments.walk();
    let arg_list: Vec<Node<'_>> = arguments.named_children(&mut cursor).collect();
    if arg_list.len() != 1 {
        return Err(format!(
            "dirname expects 1 argument, got {}",
            arg_list.len()
        ));
    }
    let arg = arg_list[0];
    // tree-sitter-php wraps each call argument in an `argument` node; unwrap if so.
    let arg_inner = if arg.kind() == "argument" {
        arg.named_child(0)
            .ok_or_else(|| "empty function argument".to_string())?
    } else {
        arg
    };
    let arg_value = evaluate(arg_inner, source, ctx)?;
    let path = std::path::Path::new(&arg_value);
    let parent = path
        .parent()
        .ok_or_else(|| format!("dirname argument has no parent: {}", arg_value))?;
    Ok(parent.to_string_lossy().into_owned())
}

// Handles the conventional `$_SERVER['DOCUMENT_ROOT']` (or its double-quoted
// variant) by substituting the caller-provided document root. Other
// `$_SERVER` keys (`HTTP_HOST`, etc.) and other superglobals are reported as
// unresolved — they have no static value at scan time. If `document_root`
// is `None`, even `DOCUMENT_ROOT` is unresolved (so a user with a non-trivial
// project layout doesn't silently get the wrong path).
fn evaluate_subscript(
    node: Node<'_>,
    source: &[u8],
    document_root: Option<&AbsolutePath>,
) -> Result<String, String> {
    let array_node = node
        .named_child(0)
        .ok_or_else(|| "subscript expression has no array operand".to_string())?;
    let index_node = node
        .named_child(1)
        .ok_or_else(|| "subscript expression has no index".to_string())?;

    // The MVP only resolves bare-variable subscripts (e.g. `$_SERVER[...]`).
    // Anything more elaborate — `getenv()['x']`, `$arr['a']['b']`,
    // `$obj->prop['x']` — has a non-`variable_name` array operand and is
    // reported as unresolved. Checking AST kind before text keeps this
    // function in line with how the other `evaluate_*` helpers dispatch.
    let array_kind = array_node.kind();
    if array_kind != "variable_name" {
        return Err(format!("unsupported subscript array kind: {}", array_kind));
    }
    let array_text = array_node.utf8_text(source).map_err(|e| e.to_string())?;
    if array_text != "$_SERVER" {
        return Err(format!("unsupported subscript array: {}", array_text));
    }

    let key = match index_node.kind() {
        "string" => evaluate_string(index_node, source)?,
        "encapsed_string" => evaluate_encapsed_string(index_node, source)?,
        kind => return Err(format!("unsupported subscript index kind: {}", kind)),
    };

    if key != "DOCUMENT_ROOT" {
        return Err(format!("unsupported $_SERVER key: {}", key));
    }
    match document_root {
        Some(root) => Ok(root.as_path().to_string_lossy().into_owned()),
        None => Err("$_SERVER['DOCUMENT_ROOT'] is not configured \
             (pass --document-root or set document_root in templategraph.toml)"
            .to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::php::IncludeKind;

    fn directive(arg: &str) -> RawIncludeDirective {
        RawIncludeDirective {
            kind: IncludeKind::Include,
            argument_source: arg.to_string(),
        }
    }

    fn current_file() -> AbsolutePath {
        AbsolutePath::new(PathBuf::from("/project/public/index.php")).unwrap()
    }

    fn document_root() -> AbsolutePath {
        AbsolutePath::new(PathBuf::from("/project/public")).unwrap()
    }

    fn ctx_for<'a>(file: &'a AbsolutePath, root: &'a AbsolutePath) -> Context<'a> {
        Context {
            current_file: file,
            document_root: Some(root),
        }
    }

    // Test helpers: build a fresh `Context` and run the resolver on
    // `arg` in one call. The previous `ctx()` / `ctx_without_document_root()`
    // helpers used `Box::leak` to hand back a `Context<'static>`, which would
    // accumulate leaks on each invocation once these tests get plugged into
    // proptest or similar high-iteration drivers.
    fn run(arg: &str) -> Resolved {
        let file = current_file();
        let root = document_root();
        resolve(&directive(arg), &ctx_for(&file, &root))
    }

    fn run_no_doc_root(arg: &str) -> Resolved {
        let file = current_file();
        let ctx = Context {
            current_file: &file,
            document_root: None,
        };
        resolve(&directive(arg), &ctx)
    }

    #[test]
    fn resolves_single_quoted_string_literal() {
        assert_eq!(
            run("'header.php'"),
            Resolved::Path(PathBuf::from("header.php"))
        );
    }

    #[test]
    fn resolves_double_quoted_string_literal() {
        assert_eq!(
            run(r#""header.php""#),
            Resolved::Path(PathBuf::from("header.php"))
        );
    }

    #[test]
    fn resolves_dir_constant() {
        assert_eq!(
            run("__DIR__"),
            Resolved::Path(PathBuf::from("/project/public"))
        );
    }

    #[test]
    fn resolves_file_constant() {
        assert_eq!(
            run("__FILE__"),
            Resolved::Path(PathBuf::from("/project/public/index.php"))
        );
    }

    #[test]
    fn resolves_dirname_file_call() {
        assert_eq!(
            run("dirname(__FILE__)"),
            Resolved::Path(PathBuf::from("/project/public"))
        );
    }

    #[test]
    fn resolves_dir_with_concat() {
        assert_eq!(
            run("__DIR__ . '/header.php'"),
            Resolved::Path(PathBuf::from("/project/public/header.php"))
        );
    }

    #[test]
    fn resolves_dirname_with_concat() {
        assert_eq!(
            run("dirname(__FILE__) . '/lib.php'"),
            Resolved::Path(PathBuf::from("/project/public/lib.php"))
        );
    }

    #[test]
    fn resolves_parenthesized_concat() {
        assert_eq!(
            run("(__DIR__ . '/header.php')"),
            Resolved::Path(PathBuf::from("/project/public/header.php"))
        );
    }

    #[test]
    fn unresolved_variable() {
        match run("$path") {
            Resolved::Unresolved {
                argument_source,
                reason: _,
            } => assert_eq!(argument_source, "$path"),
            _ => panic!("expected Unresolved"),
        }
    }

    #[test]
    fn unresolved_unknown_function() {
        assert!(matches!(
            run("realpath('foo')"),
            Resolved::Unresolved { .. }
        ));
    }

    #[test]
    fn unresolved_unsupported_operator() {
        assert!(matches!(run("'a' + 'b'"), Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_dirname_with_zero_args() {
        assert!(matches!(run("dirname()"), Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_double_quoted_with_interpolation() {
        assert!(matches!(
            run(r#""dir/$file.php""#),
            Resolved::Unresolved { .. }
        ));
    }

    #[test]
    fn unresolved_single_quoted_with_escape() {
        assert!(matches!(run(r"'it\'s.php'"), Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_double_quoted_with_escape() {
        assert!(matches!(run(r#""a\\b.php""#), Resolved::Unresolved { .. }));
    }

    #[test]
    fn resolves_dir_constant_at_filesystem_root() {
        let root_file = AbsolutePath::new(PathBuf::from("/index.php")).unwrap();
        let root = document_root();
        let r = resolve(&directive("__DIR__"), &ctx_for(&root_file, &root));
        assert_eq!(r, Resolved::Path(PathBuf::from("/")));
    }

    #[test]
    fn resolves_server_document_root_single_quoted() {
        assert_eq!(
            run("$_SERVER['DOCUMENT_ROOT']"),
            Resolved::Path(PathBuf::from("/project/public"))
        );
    }

    #[test]
    fn resolves_server_document_root_double_quoted() {
        assert_eq!(
            run(r#"$_SERVER["DOCUMENT_ROOT"]"#),
            Resolved::Path(PathBuf::from("/project/public"))
        );
    }

    #[test]
    fn resolves_server_document_root_with_concat() {
        assert_eq!(
            run(r#"$_SERVER['DOCUMENT_ROOT'] . "/inc/header.php""#),
            Resolved::Path(PathBuf::from("/project/public/inc/header.php"))
        );
    }

    #[test]
    fn resolves_server_document_root_inside_parens() {
        assert_eq!(
            run(r#"($_SERVER['DOCUMENT_ROOT'] . "/inc/header.php")"#),
            Resolved::Path(PathBuf::from("/project/public/inc/header.php"))
        );
    }

    #[test]
    fn unresolved_other_server_keys() {
        assert!(matches!(
            run("$_SERVER['HTTP_HOST']"),
            Resolved::Unresolved { .. }
        ));
    }

    #[test]
    fn unresolved_other_superglobals() {
        assert!(matches!(
            run("$_GET['DOCUMENT_ROOT']"),
            Resolved::Unresolved { .. }
        ));
    }

    #[test]
    fn unresolved_subscript_with_non_variable_array_operand() {
        // `getenv()['DOCUMENT_ROOT']` parses as a subscript whose array
        // operand is a `function_call_expression`, not a `variable_name`.
        // The AST-kind check rejects it before any text comparison runs.
        match run("getenv()['DOCUMENT_ROOT']") {
            Resolved::Unresolved { reason, .. } => {
                assert!(
                    reason.contains("subscript array kind"),
                    "expected kind-level rejection, got: {}",
                    reason
                );
            }
            _ => panic!("expected Unresolved for non-variable subscript operand"),
        }
    }

    #[test]
    fn document_root_unset_leaves_subscript_unresolved() {
        match run_no_doc_root(r#"$_SERVER['DOCUMENT_ROOT'] . "/header.php""#) {
            Resolved::Unresolved { reason, .. } => {
                assert!(reason.contains("DOCUMENT_ROOT"));
                assert!(reason.contains("--document-root"));
            }
            _ => panic!("expected Unresolved when document_root is None"),
        }
    }
}
