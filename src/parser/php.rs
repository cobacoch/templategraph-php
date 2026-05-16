#![allow(dead_code)]

use thiserror::Error;
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeKind {
    Include,
    Require,
    IncludeOnce,
    RequireOnce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawIncludeDirective {
    pub kind: IncludeKind,
    pub argument_source: String,
    /// True when the enclosing PHP source's tree contains any `ERROR` or
    /// `MISSING` node — set per-file rather than per-directive because
    /// tree-sitter's recovery often closes an `include_expression`'s subtree
    /// before the broken construct (e.g. a dangling `.` operator), so a
    /// directive whose own subtree looks clean can still sit in a file whose
    /// global parse failed. Tree-sitter is error-tolerant and still returns
    /// a tree in these cases, so callers cannot detect the failure via
    /// `extract_include_directives` returning `Err`; the resolver must
    /// branch on this flag to surface the directive as unresolved instead
    /// of silently evaluating an argument from a syntactically broken file.
    pub file_has_parse_error: bool,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to set tree-sitter PHP language: {0}")]
    Language(#[from] tree_sitter::LanguageError),

    #[error("failed to parse PHP source")]
    Parse,
}

pub fn extract_include_directives(source: &str) -> Result<Vec<RawIncludeDirective>, ParseError> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into())?;
    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let root = tree.root_node();
    let file_has_parse_error = root.has_error();
    let mut directives = Vec::new();
    walk(
        root,
        source.as_bytes(),
        file_has_parse_error,
        &mut directives,
    );
    Ok(directives)
}

fn walk(
    node: Node<'_>,
    source: &[u8],
    file_has_parse_error: bool,
    out: &mut Vec<RawIncludeDirective>,
) {
    if let Some(kind) = include_kind(node.kind()) {
        if let Some(argument_source) = include_argument(node, source) {
            out.push(RawIncludeDirective {
                kind,
                argument_source: argument_source.to_string(),
                file_has_parse_error,
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, file_has_parse_error, out);
    }
}

fn include_kind(node_kind: &str) -> Option<IncludeKind> {
    match node_kind {
        "include_expression" => Some(IncludeKind::Include),
        "require_expression" => Some(IncludeKind::Require),
        "include_once_expression" => Some(IncludeKind::IncludeOnce),
        "require_once_expression" => Some(IncludeKind::RequireOnce),
        _ => None,
    }
}

fn include_argument<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let argument_node = node.named_children(&mut cursor).next()?;
    argument_node.utf8_text(source).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_all_four_include_kinds() {
        let source = r#"<?php
include 'header.php';
require 'footer.php';
include_once 'config.php';
require_once 'init.php';
"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 4);
        assert_eq!(directives[0].kind, IncludeKind::Include);
        assert_eq!(directives[1].kind, IncludeKind::Require);
        assert_eq!(directives[2].kind, IncludeKind::IncludeOnce);
        assert_eq!(directives[3].kind, IncludeKind::RequireOnce);
    }

    #[test]
    fn captures_string_literal_argument_verbatim() {
        let source = r#"<?php include 'header.php';"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].argument_source, "'header.php'");
    }

    #[test]
    fn no_directives_yields_empty_vec() {
        let source = r#"<?php $x = 1;"#;
        let directives = extract_include_directives(source).unwrap();
        assert!(directives.is_empty());
    }

    #[test]
    fn handles_parenthesized_include() {
        let source = r#"<?php include('header.php');"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, IncludeKind::Include);
        assert_eq!(directives[0].argument_source, "('header.php')");
    }

    #[test]
    fn captures_concatenation_argument_as_raw_text() {
        let source = r#"<?php include __DIR__ . '/header.php';"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, IncludeKind::Include);
        assert!(directives[0].argument_source.contains("__DIR__"));
        assert!(directives[0].argument_source.contains("'/header.php'"));
    }

    #[test]
    fn flags_directive_when_file_has_dangling_concat() {
        // `__DIR__ . ;` leaves a dangling `.` operator whose right-hand side
        // is missing. Tree-sitter's recovery closes the `include_expression`
        // around `__DIR__` and emits the ERROR marker as a sibling, so the
        // include's own subtree looks clean — only the file-level
        // `has_error` reflects the failure. This test pins down that the
        // file-level flag (not the include-local one) is what gets propagated
        // to each directive.
        let source = "<?php include __DIR__ . ;";
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert!(
            directives[0].file_has_parse_error,
            "expected file_has_parse_error to be true, got: {:?}",
            directives[0]
        );
    }

    #[test]
    fn flags_unaffected_directive_when_file_has_unrelated_parse_error() {
        // The `include` is well-formed, but the trailing `$x =` is not. We
        // accept that a file-level parse failure marks every directive in
        // the file as suspect — this is the conservative trade-off for
        // surfacing broken-PHP cases as unresolved instead of silently
        // evaluating partial trees.
        let source = "<?php include 'header.php'; $x =";
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert!(directives[0].file_has_parse_error);
    }

    #[test]
    fn well_formed_file_does_not_flag_parse_error() {
        let source = r#"<?php include 'header.php';"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].file_has_parse_error);
    }

    #[test]
    fn captures_dirname_file_argument_as_raw_text() {
        let source = r#"<?php require dirname(__FILE__) . '/lib.php';"#;
        let directives = extract_include_directives(source).unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].kind, IncludeKind::Require);
        assert!(directives[0].argument_source.contains("dirname(__FILE__)"));
    }
}
