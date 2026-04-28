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

pub fn resolve(directive: &RawIncludeDirective, current_file: &AbsolutePath) -> Resolved {
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

    match evaluate(expr, wrapped.as_bytes(), current_file) {
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

fn evaluate(node: Node<'_>, source: &[u8], current_file: &AbsolutePath) -> Result<String, String> {
    match node.kind() {
        "string" => evaluate_string(node, source),
        "encapsed_string" => evaluate_encapsed_string(node, source),
        "name" => evaluate_name(node, source, current_file),
        "binary_expression" => evaluate_binary(node, source, current_file),
        "function_call_expression" => evaluate_function_call(node, source, current_file),
        "parenthesized_expression" => {
            let inner = node
                .named_child(0)
                .ok_or_else(|| "empty parenthesized expression".to_string())?;
            evaluate(inner, source, current_file)
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
            .ok_or_else(|| format!("__DIR__ has no parent: {}", current_file.as_path().display()))
            .map(|p| p.to_string_lossy().into_owned()),
        other => Err(format!("unsupported constant: {}", other)),
    }
}

fn evaluate_binary(
    node: Node<'_>,
    source: &[u8],
    current_file: &AbsolutePath,
) -> Result<String, String> {
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
    let left_value = evaluate(left, source, current_file)?;
    let right_value = evaluate(right, source, current_file)?;
    Ok(format!("{}{}", left_value, right_value))
}

fn evaluate_function_call(
    node: Node<'_>,
    source: &[u8],
    current_file: &AbsolutePath,
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
    let arg_value = evaluate(arg_inner, source, current_file)?;
    let path = std::path::Path::new(&arg_value);
    let parent = path
        .parent()
        .ok_or_else(|| format!("dirname argument has no parent: {}", arg_value))?;
    Ok(parent.to_string_lossy().into_owned())
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

    #[test]
    fn resolves_single_quoted_string_literal() {
        let r = resolve(&directive("'header.php'"), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("header.php")));
    }

    #[test]
    fn resolves_double_quoted_string_literal() {
        let r = resolve(&directive(r#""header.php""#), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("header.php")));
    }

    #[test]
    fn resolves_dir_constant() {
        let r = resolve(&directive("__DIR__"), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("/project/public")));
    }

    #[test]
    fn resolves_file_constant() {
        let r = resolve(&directive("__FILE__"), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("/project/public/index.php")));
    }

    #[test]
    fn resolves_dirname_file_call() {
        let r = resolve(&directive("dirname(__FILE__)"), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("/project/public")));
    }

    #[test]
    fn resolves_dir_with_concat() {
        let r = resolve(&directive("__DIR__ . '/header.php'"), &current_file());
        assert_eq!(
            r,
            Resolved::Path(PathBuf::from("/project/public/header.php"))
        );
    }

    #[test]
    fn resolves_dirname_with_concat() {
        let r = resolve(&directive("dirname(__FILE__) . '/lib.php'"), &current_file());
        assert_eq!(r, Resolved::Path(PathBuf::from("/project/public/lib.php")));
    }

    #[test]
    fn resolves_parenthesized_concat() {
        let r = resolve(&directive("(__DIR__ . '/header.php')"), &current_file());
        assert_eq!(
            r,
            Resolved::Path(PathBuf::from("/project/public/header.php"))
        );
    }

    #[test]
    fn unresolved_variable() {
        let r = resolve(&directive("$path"), &current_file());
        match r {
            Resolved::Unresolved {
                argument_source,
                reason: _,
            } => assert_eq!(argument_source, "$path"),
            _ => panic!("expected Unresolved"),
        }
    }

    #[test]
    fn unresolved_unknown_function() {
        let r = resolve(&directive("realpath('foo')"), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_unsupported_operator() {
        let r = resolve(&directive("'a' + 'b'"), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_dirname_with_zero_args() {
        let r = resolve(&directive("dirname()"), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_double_quoted_with_interpolation() {
        let r = resolve(&directive(r#""dir/$file.php""#), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_single_quoted_with_escape() {
        let r = resolve(&directive(r"'it\'s.php'"), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn unresolved_double_quoted_with_escape() {
        let r = resolve(&directive(r#""a\\b.php""#), &current_file());
        assert!(matches!(r, Resolved::Unresolved { .. }));
    }

    #[test]
    fn resolves_dir_constant_at_filesystem_root() {
        let root_file = AbsolutePath::new(PathBuf::from("/index.php")).unwrap();
        let r = resolve(&directive("__DIR__"), &root_file);
        assert_eq!(r, Resolved::Path(PathBuf::from("/")));
    }
}
