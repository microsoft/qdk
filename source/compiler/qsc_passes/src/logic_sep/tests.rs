// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::{Expect, expect};
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, span::Span,
    target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{
    hir::{ExprKind, NodeId, Stmt},
    visit::{Visitor, walk_stmt},
};
use rustc_hash::FxHashMap;

use crate::logic_sep::find_quantum_stmts;

struct StmtSpans {
    spans: FxHashMap<NodeId, Span>,
}

impl<'a> Visitor<'a> for StmtSpans {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        self.spans.insert(stmt.id, stmt.span);
        walk_stmt(self, stmt);
    }
}

fn check(block_str: &str, expect: &Expect) {
    let mut store = PackageStore::new(compile::core());
    let std = store.insert(compile::std(&store, TargetCapabilityFlags::all()));
    let unit = compile(
        &store,
        &[(std, None)],
        SourceMap::new([], Some(block_str.into())),
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let entry = unit.package.entry.expect("entry should exist");
    let ExprKind::Block(block) = &entry.kind else {
        panic!("test should be given block expression, given {entry}");
    };
    let mut stmt_map = StmtSpans {
        spans: FxHashMap::default(),
    };
    stmt_map.visit_block(block);

    match find_quantum_stmts(block) {
        Ok(mut quantum_stmts) => {
            let mut stmts = quantum_stmts.drain().collect::<Vec<_>>();
            stmts.sort_unstable();
            let mut actual = Vec::new();
            for id in stmts {
                actual.push(
                    &block_str[stmt_map
                        .spans
                        .get(&id)
                        .expect("nodes should be present in tree")],
                );
            }
            expect.assert_eq(&actual.join("\n"));
        }
        Err(e) => expect.assert_debug_eq(&e),
    }
}

#[test]
fn empty_block_produces_empty_quantum_stmts() {
    check("{}", &expect![""]);
}

#[test]
fn pure_classical_block_produces_empty_quantum_stmts() {
    check("{let x = 4; let y = x * 3;}", &expect![""]);
}

#[test]
fn op_calls_are_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = 4; Z(q);}",
        &expect![[r#"
            X(q);
            Z(q);"#]],
    );
}

#[test]
fn if_with_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val {Z(q);}}",
        &expect![[r#"
            X(q);
            if val {Z(q);}
            Z(q);"#]],
    );
}

#[test]
fn if_else_with_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val {Z(q);} else {I(q);}}",
        &expect![[r#"
            X(q);
            if val {Z(q);} else {I(q);}
            Z(q);
            I(q);"#]],
    );
}

#[test]
fn if_without_op_call_not_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val {let a = 1;} else {let a = 2;}}",
        &expect!["X(q);"],
    );
}

#[test]
fn qubit_scope_expr_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); use scope_q = Qubit() {let val = 4; CNOT(q, scope_q); let val2 = val + 1;} Z(q);}",
        &expect![[r#"
        X(q);
        use scope_q = Qubit() {let val = 4; CNOT(q, scope_q); let val2 = val + 1;}
        CNOT(q, scope_q);
        Z(q);"#]],
    );
}

#[test]
fn if_with_nested_if_with_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val { if val { Z(q);}}}",
        &expect![[r#"
            X(q);
            if val { if val { Z(q);}}
            if val { Z(q);}
            Z(q);"#]],
    );
}

#[test]
fn if_with_nested_if_with_mix_of_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val { if val { Z(q);} if val {let x = 1;}}}",
        &expect![[r#"
            X(q);
            if val { if val { Z(q);} if val {let x = 1;}}
            if val { Z(q);}
            Z(q);"#]],
    );
}

#[test]
fn if_with_nested_conjugate_with_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); X(q); let val = true; if val { within { Z(q);} apply {}}}",
        &expect![[r#"
            X(q);
            if val { within { Z(q);} apply {}}
            within { Z(q);} apply {}
            Z(q);"#]],
    );
}

#[test]
fn conjugate_with_op_call_is_quantum_stmts() {
    check(
        "{use q = Qubit(); within {X(q); let val = 0;} apply {Y(q); let val2 = 1;} let val = 2; Z(q);}",
        &expect![[r#"
            within {X(q); let val = 0;} apply {Y(q); let val2 = 1;}
            X(q);
            Y(q);
            Z(q);"#]],
    );
}

#[test]
fn conjugate_without_op_call_not_quantum_stmts() {
    check(
        "{use q = Qubit(); within {let val = 0;} apply {let val2 = 1;} let val = 2; Z(q);}",
        &expect![[r#"Z(q);"#]],
    );
}

#[test]
fn for_loop_with_op_call_is_quantum_stmts() {
    check(
        "{use qs = Qubit[2]; for q in qs { X(q); let val = 4; }}",
        &expect![[r#"
            for q in qs { X(q); let val = 4; }
            X(q);"#]],
    );
}

#[test]
fn for_loop_with_func_call_and_op_call_is_quantum_stmts() {
    check(
        r#"{use qs = Qubit[2]; for q in qs { Message(""); X(q); }}"#,
        &expect![[r#"
            for q in qs { Message(""); X(q); }
            X(q);"#]],
    );
}

#[test]
fn for_loop_with_op_call_and_func_call_is_quantum_stmts() {
    check(
        r#"{use qs = Qubit[2]; for q in qs { X(q); Message(""); }}"#,
        &expect![[r#"
            for q in qs { X(q); Message(""); }
            X(q);"#]],
    );
}

#[test]
fn for_loop_body_no_op_call_not_quantum_stmts() {
    check(
        "{use qs = Qubit[2]; for q in qs { let val = 4; }}",
        &expect![""],
    );
}

#[test]
fn op_call_in_non_unit_block_forbidden() {
    check(
        "{use q = Qubit(); X(q); 4}",
        &expect![[r#"
            [
                NonUnitBlock(
                    "Int",
                    Span {
                        lo: 0,
                        hi: 26,
                    },
                ),
                OpCallForbidden(
                    Span {
                        lo: 18,
                        hi: 22,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn non_unit_block_allowed_in_classical_context() {
    check(
        "{use q = Qubit(); let x = {let y = 1; y + 1}; X(q);}",
        &expect!["X(q);"],
    );
}

#[test]
fn op_call_in_binding_forbidden() {
    check(
        "{let x = {use q = Qubit(); X(q); false};}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 27,
                    hi: 31,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_qubit_binding_forbidden() {
    check(
        "{use qs = Qubit[{use q = Qubit(); X(q); 3}];}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 34,
                    hi: 38,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_callee_forbidden() {
    check(
        "{use q = Qubit(); (if true {use q = Qubit(); X(q); Z} else {I})(q);}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 45,
                    hi: 49,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_args_forbidden() {
    check(
        "{use q = Qubit(); let _ = Length([{use q = Qubit(); X(q);}]);}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 52,
                    hi: 56,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_iter_expr_forbidden() {
    check(
        "{use q = Qubit(); for _ in [X(q)] {}}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 28,
                    hi: 32,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_if_cond_forbidden() {
    check(
        "{use q = Qubit(); if X(q) == () {}}",
        &expect![[r#"
        [
            OpCallForbidden(
                Span {
                    lo: 21,
                    hi: 25,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn op_call_in_interpolated_string_forbidden() {
    check(
        r#"{ use q = Qubit(); let x = $"foo {X(q)}"; }"#,
        &expect![[r#"
            [
                OpCallForbidden(
                    Span {
                        lo: 34,
                        hi: 38,
                    },
                ),
            ]
        "#]],
    );
}

#[test]
fn assign_forbidden() {
    check(
        "{mutable val = 0; set val = 1;}",
        &expect![[r#"
        [
            ExprForbidden(
                Span {
                    lo: 18,
                    hi: 29,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn assignop_forbidden() {
    check(
        "{mutable val = 0; set val += 1;}",
        &expect![[r#"
        [
            ExprForbidden(
                Span {
                    lo: 18,
                    hi: 30,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn assignupate_forbidden() {
    check(
        "{mutable val = [0]; set val w/= 0 <- 1;}",
        &expect![[r#"
        [
            ExprForbidden(
                Span {
                    lo: 20,
                    hi: 38,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn repeat_loop_forbidden() {
    check(
        "{repeat{}until true;}",
        &expect![[r#"
        [
            ExprForbidden(
                Span {
                    lo: 1,
                    hi: 19,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn while_loop_forbidden() {
    check(
        "{while true {}}",
        &expect![[r#"
        [
            ExprForbidden(
                Span {
                    lo: 1,
                    hi: 14,
                },
            ),
        ]
    "#]],
    );
}

#[test]
fn return_forbidden() {
    check(
        "{return 4;}",
        &expect![[r#"
            [
                ExprForbidden(
                    Span {
                        lo: 1,
                        hi: 9,
                    },
                ),
            ]
        "#]],
    );
}
