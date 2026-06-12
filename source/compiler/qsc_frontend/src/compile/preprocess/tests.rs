// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compile::preprocess::{Conditional, RemoveCircuitSpans};
use crate::compile::{SourceMap, parse_all};
use qsc_ast::ast::{
    Attr, CallableBody, CallableDecl, Expr, ExprKind, Ident, NodeId, Path, PathKind,
};
use qsc_ast::ast::{ItemKind, Package, TopLevelNode};
use qsc_ast::mut_visit::MutVisitor;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_data_structures::span::Span;
use std::sync::Arc;

use crate::compile::{TargetCapabilityFlags, preprocess::matches_config};

fn named_attr(name: &str) -> Attr {
    Attr {
        name: Box::new(Ident {
            name: name.into(),
            span: Span::default(),
            id: NodeId::default(),
        }),
        arg: Box::new(Expr {
            id: NodeId::default(),
            span: Span::default(),
            kind: Box::new(ExprKind::Tuple(Box::new([]))),
        }),
        span: Span::default(),
        id: NodeId::default(),
    }
}

fn name_value_attr(name: &str, value: &str) -> Attr {
    Attr {
        name: Box::new(Ident {
            name: name.into(),
            span: Span::default(),
            id: NodeId::default(),
        }),
        arg: Box::new(Expr {
            id: NodeId::default(),
            span: Span::default(),
            kind: Box::new(ExprKind::Paren(Box::new(Expr {
                id: NodeId::default(),
                span: Span::default(),
                kind: Box::new(ExprKind::Path(PathKind::Ok(Box::new(Path {
                    id: NodeId::default(),
                    span: Span::default(),
                    segments: None,
                    name: Box::new(Ident {
                        name: value.into(),
                        span: Span::default(),
                        id: NodeId::default(),
                    }),
                })))),
            }))),
        }),
        span: Span::default(),
        id: NodeId::default(),
    }
}

fn prepare_ast_for_circuit_tests() -> Package {
    let circuit_source = (
        Arc::from("circuit.qsc"),
        Arc::from("namespace CircuitTest { operation Circuit(qs : Qubit[]) : Unit { X(qs[0]); } }"),
    );
    let qsharp_source = (
        Arc::from("test.qs"),
        Arc::from("namespace Test { operation Main(qs : Qubit[]) : Unit { X(qs[0]); } }"),
    );
    let sources = SourceMap::new([circuit_source, qsharp_source], None);
    let (mut package, errs) = parse_all(&sources, LanguageFeatures::default());
    assert!(errs.is_empty(), "{errs:?}");
    let mut visitor = RemoveCircuitSpans::new(&sources);
    visitor.visit_package(&mut package);
    package
}

/// Helper to find a callable declaration by name in a package AST.
fn find_callable<'a>(package: &'a Package, name: &str) -> &'a CallableDecl {
    package
        .nodes
        .iter()
        .find_map(|node| {
            if let TopLevelNode::Namespace(ns) = node {
                ns.items.iter().find_map(|item| {
                    if let ItemKind::Callable(decl) = item.kind.as_ref() {
                        if decl.name.name.as_ref() == name {
                            Some(decl.as_ref())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("{name} callable not found"))
}

#[test]
fn dropped_item_spans_cover_the_excluded_item() {
    let source = "namespace Test { @Config(Adaptive) operation Excluded() : Unit {} operation Included() : Unit {} }";
    let sources = SourceMap::new([(Arc::from("test.qs"), Arc::from(source))], None);
    let (mut package, errs) = parse_all(&sources, LanguageFeatures::default());
    assert!(errs.is_empty(), "{errs:?}");

    // Base profile (empty capabilities) drops the `@Config(Adaptive)` item.
    let mut cond = Conditional::new(TargetCapabilityFlags::empty());
    cond.visit_package(&mut package);
    let dropped_spans = cond.take_dropped_spans();

    assert_eq!(dropped_spans.len(), 1, "exactly one item should be dropped");
    let span = dropped_spans[0];
    // The span should start at the `@Config` attribute and cover the operation.
    let excluded = &source[span.lo as usize..span.hi as usize];
    assert_eq!(excluded, "@Config(Adaptive) operation Excluded() : Unit {}");
}

#[test]
fn no_attrs_matches() {
    assert!(matches_config(&[], TargetCapabilityFlags::empty()));
}

#[test]
fn unknown_attrs_matches() {
    assert!(matches_config(
        &[Box::new(named_attr("unknown"))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn none_attrs_matches_empty() {
    assert!(matches_config(
        &[Box::new(name_value_attr("Config", "Base"))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn none_attrs_does_not_match_all() {
    assert!(!matches_config(
        &[Box::new(name_value_attr("Config", "Base"))],
        TargetCapabilityFlags::all()
    ));
}

#[test]
fn none_attrs_does_not_match_adaptive() {
    assert!(!matches_config(
        &[Box::new(name_value_attr("Config", "Base"))],
        TargetCapabilityFlags::Adaptive
    ));
}

#[test]
fn adaptive_attrs_does_not_match_empty() {
    assert!(!matches_config(
        &[Box::new(name_value_attr("Config", "Adaptive"))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn integercomputations_attrs_does_not_match_empty() {
    assert!(!matches_config(
        &[Box::new(name_value_attr("Config", "IntegerComputations"))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn floatingpointcomputations_attrs_does_not_match_empty() {
    assert!(!matches_config(
        &[Box::new(name_value_attr(
            "Config",
            "FloatingPointComputations"
        ))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn unrestricted_attrs_does_not_match_empty() {
    assert!(!matches_config(
        &[Box::new(name_value_attr("Config", "Unrestricted"))],
        TargetCapabilityFlags::empty()
    ));
}

#[test]
fn unrestricted_attrs_matches_all() {
    assert!(matches_config(
        &[Box::new(name_value_attr("Config", "Unrestricted"))],
        TargetCapabilityFlags::all()
    ));
}

#[test]
fn remove_circuit_spans_clears_stmt_spans_in_qsc_files() {
    let ast = prepare_ast_for_circuit_tests();
    let circuit_callable = find_callable(&ast, "Circuit");

    // Assert the callable declaration has a non-default span
    assert_ne!(
        circuit_callable.span,
        Span::default(),
        "Callable span should not be default"
    );

    // Assert that the body is a block and all statement spans inside the callable are cleared
    match circuit_callable.body.as_ref() {
        CallableBody::Block(block) => {
            for stmt in &block.stmts {
                assert_eq!(
                    stmt.span,
                    Span::default(),
                    "Statement span inside Circuit should be cleared"
                );
            }
        }
        CallableBody::Specs(_) => panic!("Expected Circuit body to be a block"),
    }
}

#[test]
fn remove_circuit_spans_does_not_clear_spans_outside_qsc_files() {
    let ast = prepare_ast_for_circuit_tests();
    let main_callable = find_callable(&ast, "Main");

    // Assert the callable declaration has a non-default span
    assert_ne!(
        main_callable.span,
        Span::default(),
        "Callable span should not be default"
    );

    // Assert that the body is a block and all statement spans inside the callable are NOT cleared
    match main_callable.body.as_ref() {
        CallableBody::Block(block) => {
            for stmt in &block.stmts {
                assert_ne!(
                    stmt.span,
                    Span::default(),
                    "Statement span inside Main should NOT be cleared"
                );
            }
        }
        CallableBody::Specs(_) => panic!("Expected Main body to be a block"),
    }
}
