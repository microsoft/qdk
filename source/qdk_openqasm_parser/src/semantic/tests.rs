// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;

pub mod assignment;
pub mod decls;

pub mod expression;
mod lowerer_errors;
pub mod statements;

use super::parse_source;
use crate::io::InMemorySourceResolver;
use crate::io::SourceResolver;
use expect_test::Expect;
use expect_test::expect;
use miette::Report;
use std::sync::Arc;

pub(super) fn check<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |p, _| p.to_string());
}

pub(super) fn check_classical_decl<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |program, symbol_table| {
        let kind = program
            .statements
            .first()
            .expect("reading first statement")
            .kind
            .clone();
        let super::ast::StmtKind::ClassicalDecl(decl) = kind.as_ref() else {
            panic!("expected classical declaration statement");
        };
        let mut value = decl.to_string();
        value.push('\n');
        let symbol = &symbol_table[decl.symbol_id];
        write!(value, "[{}] {symbol}", decl.symbol_id).expect("writing symbol id");
        value
    });
}

pub(super) fn check_classical_decls<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |program, symbol_table| {
        let kinds = program
            .statements
            .iter()
            .map(|stmt| stmt.kind.as_ref().clone())
            .collect::<Vec<_>>();
        let mut value = String::new();
        for kind in &kinds {
            let (symbol_id, str) = match kind {
                super::ast::StmtKind::ClassicalDecl(decl) => (decl.symbol_id, decl.to_string()),
                super::ast::StmtKind::OutputDeclaration(decl) => (decl.symbol_id, decl.to_string()),
                _ => panic!("unsupported stmt type {kind}"),
            };

            value.push_str(&str);
            value.push('\n');
            let symbol = &symbol_table[symbol_id];
            write!(value, "[{symbol_id}] {symbol}").expect("writing symbol id");
            value.push('\n');
        }

        value
    });
}

pub(super) fn check_stmt_kind<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |p, _| {
        p.statements
            .first()
            .expect("reading first statement")
            .kind
            .to_string()
    });
}

pub(super) fn check_last_stmt<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |p, _| {
        p.statements
            .last()
            .expect("reading last statement")
            .kind
            .to_string()
    });
}

pub(super) fn check_stmt_kinds<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    check_map(input, expect, |p, _| {
        p.statements
            .iter()
            .fold(String::new(), |acc, x| format!("{acc}{}\n", x.kind))
    });
}

fn check_map<S: Into<Arc<str>>>(
    input: S,
    expect: &Expect,
    selector: impl FnOnce(&super::ast::Program, &super::symbols::SymbolTable) -> String,
) {
    let input = input.into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), input.clone())]);
    let res = parse_source(input, "test", &mut resolver);

    let errors = res.all_errors();

    assert!(
        !res.has_syntax_errors(),
        "syntax errors: {:?}",
        res.sytax_errors()
    );

    if errors.is_empty() {
        expect.assert_eq(&selector(&res.program, &res.symbols));
    } else {
        expect.assert_eq(&format!(
            "{}\n\n{:?}",
            res.program,
            errors
                .iter()
                .map(|e| Report::new(e.clone()))
                .collect::<Vec<_>>()
        ));
    }
}

pub(super) fn check_err<S: Into<Arc<str>>>(input: S, expect: &Expect) {
    let input = input.into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), input.clone())]);
    let res = parse_source(input, "test", &mut resolver);

    let errors = res.all_errors();

    assert!(
        !res.has_syntax_errors(),
        "syntax errors: {:?}",
        res.sytax_errors()
    );

    assert!(res.has_errors(), "no errors");

    expect.assert_eq(&format!(
        "{:?}",
        errors
            .iter()
            .map(|e| Report::new(e.clone()))
            .collect::<Vec<_>>()
    ));
}

pub(super) fn check_all<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
    expect: &Expect,
) {
    check_map_all(path, sources, expect, |p, _| p.to_string());
}

fn check_map_all<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
    expect: &Expect,
    selector: impl FnOnce(&super::ast::Program, &super::symbols::SymbolTable) -> String,
) {
    let path = path.into();
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let source = resolver
        .resolve(&path, &path)
        .map_err(|e| vec![e])
        .expect("could not load source")
        .1;
    let res = parse_source(source, path, &mut resolver);

    let errors = res.all_errors();

    assert!(
        !res.has_syntax_errors(),
        "syntax errors: {:?}",
        res.sytax_errors()
    );

    if errors.is_empty() {
        expect.assert_eq(&selector(&res.program, &res.symbols));
    } else {
        expect.assert_eq(&format!(
            "{}\n\n{:?}",
            res.program,
            errors
                .iter()
                .map(|e| Report::new(e.clone()))
                .collect::<Vec<_>>()
        ));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn semantic_errors_map_to_their_corresponding_file_specific_spans() {
    let source0 = r#"OPENQASM 3.0;
    include "stdgates.inc";
    include "source1.qasm";
    bit c = r; // undefined symbol r
    "#;
    let source1 = r#"include "source2.qasm";
    angle j = 7.0;
    float k = j + false; // invalid cast"#;
    let source2 = "bit l = 1;
    bool l = v && l; // undefined y, redefine l";
    let all_sources = [
        ("source0.qasm".into(), source0.into()),
        ("source1.qasm".into(), source1.into()),
        ("source2.qasm".into(), source2.into()),
    ];

    check_all(
        "source0.qasm",
        all_sources,
        &expect![[r#"
            Program:
                version: 3.0
                pragmas: <empty>
                statements:
                    Stmt [196-206]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [196-206]:
                            symbol_id: 40
                            ty_span: [196-199]
                            ty_exprs: <empty>
                            init_expr: Expr [204-205]:
                                ty: const bit
                                kind: Lit: Bit(1)
                    Stmt [211-227]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [211-227]:
                            symbol_id: 40
                            ty_span: [211-215]
                            ty_exprs: <empty>
                            init_expr: Expr [220-226]:
                                ty: bool
                                kind: BinaryOpExpr:
                                    op: AndL
                                    lhs: Expr [220-221]:
                                        ty: unknown
                                        kind: SymbolId(41)
                                    rhs: Expr [225-226]:
                                        ty: bool
                                        kind: Cast [225-226]:
                                            ty: bool
                                            ty_exprs: <empty>
                                            expr: Expr [225-226]:
                                                ty: bit
                                                kind: SymbolId(40)
                                            kind: Implicit
                    Stmt [140-154]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [140-154]:
                            symbol_id: 42
                            ty_span: [140-145]
                            ty_exprs: <empty>
                            init_expr: Expr [150-153]:
                                ty: const angle
                                kind: Lit: Angle(0.7168146928204138)
                    Stmt [159-179]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [159-179]:
                            symbol_id: 43
                            ty_span: [159-164]
                            ty_exprs: <empty>
                            init_expr: Expr [169-178]:
                                ty: float
                                kind: BinaryOpExpr:
                                    op: Add
                                    lhs: Expr [169-170]:
                                        ty: angle
                                        kind: SymbolId(42)
                                    rhs: Expr [173-178]:
                                        ty: float
                                        kind: Cast [173-178]:
                                            ty: float
                                            ty_exprs: <empty>
                                            expr: Expr [173-178]:
                                                ty: const bool
                                                kind: Lit: Bool(false)
                                            kind: Implicit
                    Stmt [74-84]:
                        annotations: <empty>
                        kind: ClassicalDeclarationStmt [74-84]:
                            symbol_id: 45
                            ty_span: [74-77]
                            ty_exprs: <empty>
                            init_expr: Expr [82-83]:
                                ty: unknown
                                kind: SymbolId(44)

            [Qasm.Lowerer.UndefinedSymbol

              x undefined symbol: v
               ,-[source2.qasm:2:14]
             1 | bit l = 1;
             2 |     bool l = v && l; // undefined y, redefine l
               :              ^
               `----
            , Qasm.Lowerer.RedefinedSymbol

              x redefined symbol: l
               ,-[source2.qasm:2:10]
             1 | bit l = 1;
             2 |     bool l = v && l; // undefined y, redefine l
               :          ^
               `----
            , Qasm.Lowerer.CannotCast

              x cannot cast expression of type angle to type float
               ,-[source1.qasm:3:15]
             2 |     angle j = 7.0;
             3 |     float k = j + false; // invalid cast
               :               ^
               `----
            , Qasm.Lowerer.UndefinedSymbol

              x undefined symbol: r
               ,-[source0.qasm:4:13]
             3 |     include "source1.qasm";
             4 |     bit c = r; // undefined symbol r
               :             ^
             5 |     
               `----
            ]"#]],
    );
}
