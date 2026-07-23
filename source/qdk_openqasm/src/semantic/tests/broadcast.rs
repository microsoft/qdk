// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::io::InMemorySourceResolver;
use crate::semantic::ast::TimeUnit;
use crate::semantic::ast::{
    ExprKind, GateCall, GateCallBroadcast, GateModifierKind, GateOperandKind, Stmt, StmtKind,
};
use crate::semantic::broadcast::{BroadcastExpansionError, expand_gate_call};
use crate::semantic::parse_source;
use crate::semantic::types::Type;
use crate::semantic::visit::{Visitor, walk_gate_call_stmt};
use crate::stdlib::duration::Duration;

fn analyze(source: &str) -> crate::semantic::AnalysisResult {
    let result = parse_source(source, "test", &mut InMemorySourceResolver::from_iter([]));
    assert!(
        !result.has_errors(),
        "unexpected errors: {:?}",
        result.all_errors()
    );
    result
}

fn gate_calls_in_stmt(stmt: &Stmt) -> Vec<&GateCall> {
    match stmt.kind.as_ref() {
        StmtKind::GateCall(call) => vec![call],
        StmtKind::Block(block) => block.stmts.iter().flat_map(gate_calls_in_stmt).collect(),
        StmtKind::For(for_stmt) => gate_calls_in_stmt(&for_stmt.body),
        StmtKind::If(if_stmt) => gate_calls_in_stmt(&if_stmt.if_body)
            .into_iter()
            .chain(
                if_stmt
                    .else_body
                    .iter()
                    .flat_map(|stmt| gate_calls_in_stmt(stmt)),
            )
            .collect(),
        StmtKind::WhileLoop(while_stmt) => gate_calls_in_stmt(&while_stmt.body),
        _ => Vec::new(),
    }
}

#[test]
fn broadcast_gate_call_retains_one_statement_and_original_operands() {
    let source = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[3] controls;
qubit[3] targets;
cx controls, targets;
"#;
    let result = analyze(source);
    let gate_calls = result
        .program
        .statements
        .iter()
        .filter_map(|stmt| match stmt.kind.as_ref() {
            StmtKind::GateCall(call) => Some(call),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(gate_calls.len(), 1);
    let gate_call = gate_calls[0];
    assert_eq!(
        gate_call.broadcast,
        GateCallBroadcast::Broadcast { width: 3 }
    );
    assert_eq!(gate_call.qubits.len(), 2);
    assert!(gate_call.qubits.iter().all(|operand| {
        matches!(
            &operand.kind,
            GateOperandKind::Expr(expr) if expr.ty == Type::QubitArray(3)
        )
    }));
}

#[test]
fn broadcast_expansion_zips_registers_and_repeats_hardware_scalar() {
    let source = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[3] targets;
cx $0, targets;
"#;
    let result = analyze(source);
    let gate_call = result
        .program
        .statements
        .iter()
        .find_map(|stmt| match stmt.kind.as_ref() {
            StmtKind::GateCall(call) => Some(call),
            _ => None,
        })
        .expect("gate call");

    let expansion = expand_gate_call(gate_call).expect("valid broadcast");
    assert_eq!(expansion.len(), 3);
    for (expected_index, scalar) in expansion.enumerate() {
        assert!(std::ptr::eq(scalar.source(), gate_call));
        assert!(matches!(
            scalar.qubits()[0].kind,
            GateOperandKind::HardwareQubit(_)
        ));
        let GateOperandKind::Expr(expr) = &scalar.qubits()[1].kind else {
            panic!("expected indexed register operand");
        };
        let crate::semantic::ast::ExprKind::IndexedExpr(indexed) = expr.kind.as_ref() else {
            panic!("expected indexed expression");
        };
        let crate::semantic::ast::Index::Expr(index) = indexed.index.as_ref() else {
            panic!("expected expression index");
        };
        let crate::semantic::ast::ExprKind::Lit(crate::semantic::ast::LiteralKind::Int(index)) =
            index.kind.as_ref()
        else {
            panic!("expected integer index literal");
        };
        assert_eq!(
            *index,
            i64::try_from(expected_index).expect("broadcast index should fit in i64")
        );
    }
}

#[test]
fn broadcast_expansion_zips_multiple_registers_and_repeats_virtual_scalar() {
    let source = r#"OPENQASM 3.0;
gate tri a, b, c {}
qubit scalar;
qubit[2] left;
qubit[2] right;
pow(2) @ tri scalar, left, right;
"#;
    let result = analyze(source);
    let gate_call = result
        .program
        .statements
        .iter()
        .flat_map(gate_calls_in_stmt)
        .next()
        .expect("gate call");

    let expansion = expand_gate_call(gate_call).expect("valid broadcast");
    assert_eq!(expansion.len(), 2);
    for (expected_index, scalar) in expansion.enumerate() {
        assert!(std::ptr::eq(scalar.source(), gate_call));
        assert_eq!(scalar.source().span, gate_call.span);
        assert_eq!(scalar.source().modifiers.len(), 1);
        assert!(matches!(
            scalar.qubits()[0].kind,
            GateOperandKind::Expr(ref expr) if expr.ty == Type::Qubit
        ));
        for operand in &scalar.qubits()[1..] {
            let GateOperandKind::Expr(expr) = &operand.kind else {
                panic!("expected indexed register operand");
            };
            let ExprKind::IndexedExpr(indexed) = expr.kind.as_ref() else {
                panic!("expected indexed expression");
            };
            let crate::semantic::ast::Index::Expr(index) = indexed.index.as_ref() else {
                panic!("expected expression index");
            };
            let ExprKind::Lit(crate::semantic::ast::LiteralKind::Int(index)) = index.kind.as_ref()
            else {
                panic!("expected integer index literal");
            };
            assert_eq!(
                *index,
                i64::try_from(expected_index).expect("broadcast index should fit in i64")
            );
            assert_eq!(operand.span, indexed.collection.span);
        }
    }
}

#[test]
fn invalid_recovered_broadcast_operand_returns_error() {
    let source = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
x q;
"#;
    let result = analyze(source);
    let mut gate_call = result
        .program
        .statements
        .iter()
        .find_map(|stmt| match stmt.kind.as_ref() {
            StmtKind::GateCall(call) => Some(call.clone()),
            _ => None,
        })
        .expect("gate call");
    gate_call.broadcast = GateCallBroadcast::Broadcast { width: 3 };

    assert!(matches!(
        expand_gate_call(&gate_call),
        Err(BroadcastExpansionError::WidthMismatch {
            expected: 3,
            actual: 2,
            ..
        })
    ));
}

#[test]
fn scalar_gate_call_has_explicit_scalar_metadata() {
    let result = analyze("OPENQASM 3.0; include \"stdgates.inc\"; qubit q; x q;");
    let gate_call = result
        .program
        .statements
        .iter()
        .flat_map(gate_calls_in_stmt)
        .next()
        .expect("gate call");

    assert_eq!(gate_call.broadcast, GateCallBroadcast::Scalar);
    assert_eq!(expand_gate_call(gate_call).expect("scalar call").len(), 1);
}

#[test]
fn alias_and_slice_operands_retain_validated_broadcast_width() {
    let result = analyze(
        r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[4] source;
let selected = source[1:2];
h selected;
"#,
    );
    let gate_call = result
        .program
        .statements
        .iter()
        .flat_map(gate_calls_in_stmt)
        .next()
        .expect("gate call");

    assert_eq!(
        gate_call.broadcast,
        GateCallBroadcast::Broadcast { width: 2 }
    );
    assert!(matches!(
        &gate_call.qubits[0].kind,
        GateOperandKind::Expr(expr) if expr.ty == Type::QubitArray(2)
    ));
}

#[test]
fn gphase_modifiers_and_annotations_remain_on_one_compact_statement() {
    let result = analyze(
        r#"OPENQASM 3.0;
qubit[2] q;
@vendor.note payload
ctrl @ inv @ pow(2) @ gphase(pi / 2) q;
"#,
    );
    let statement = result.program.statements.last().expect("gphase statement");
    let gate_call = gate_calls_in_stmt(statement)
        .into_iter()
        .next()
        .expect("gate call");

    assert_eq!(statement.annotations.len(), 1);
    assert_eq!(gate_call.modifiers.len(), 3);
    assert!(
        gate_call
            .modifiers
            .iter()
            .any(|modifier| matches!(modifier.kind, GateModifierKind::Inv))
    );
    assert!(
        gate_call
            .modifiers
            .iter()
            .any(|modifier| matches!(modifier.kind, GateModifierKind::Pow(_)))
    );
    assert!(
        gate_call
            .modifiers
            .iter()
            .any(|modifier| matches!(modifier.kind, GateModifierKind::Ctrl(_)))
    );
    assert_eq!(
        gate_call.broadcast,
        GateCallBroadcast::Broadcast { width: 2 }
    );
}

#[test]
fn compact_broadcast_preserves_single_statement_control_flow_blocks() {
    let result = analyze(
        r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
if (true) x q;
while (false) h q;
for int i in [0:0] z q;
"#,
    );
    let control_flow = &result.program.statements[1..];

    assert_eq!(control_flow.len(), 3);
    for statement in control_flow {
        let calls = gate_calls_in_stmt(statement);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].broadcast,
            GateCallBroadcast::Broadcast { width: 2 }
        );
        match statement.kind.as_ref() {
            StmtKind::If(if_stmt) => {
                assert!(matches!(if_stmt.if_body.kind.as_ref(), StmtKind::Block(_)));
            }
            StmtKind::WhileLoop(while_stmt) => {
                assert!(matches!(while_stmt.body.kind.as_ref(), StmtKind::Block(_)));
            }
            StmtKind::For(for_stmt) => {
                assert!(matches!(for_stmt.body.kind.as_ref(), StmtKind::Block(_)));
            }
            kind => panic!("expected control-flow statement, found {kind}"),
        }
    }
}

#[test]
fn semantic_visitor_observes_broadcast_gate_once() {
    struct Counter(usize);

    impl Visitor for Counter {
        fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
            self.0 += 1;
            walk_gate_call_stmt(self, stmt);
        }
    }

    let result = analyze("OPENQASM 3.0; include \"stdgates.inc\"; qubit[8] q; x q;");
    let mut counter = Counter(0);
    counter.visit_program(&result.program);

    assert_eq!(counter.0, 1);
}

#[test]
fn durationof_multiplies_gate_duration_by_broadcast_width() {
    let result = analyze(
        "OPENQASM 3.0;
qubit[3] q;
const duration total = durationof({ U(0, 0, 0) [5ns] q; });
",
    );
    let StmtKind::ClassicalDecl(declaration) = result
        .program
        .statements
        .last()
        .expect("duration declaration")
        .kind
        .as_ref()
    else {
        panic!("expected duration declaration");
    };
    let ExprKind::EvaluatedDurationof(durationof) = declaration.init_expr.kind.as_ref() else {
        panic!("expected evaluated durationof expression");
    };

    assert_eq!(durationof.duration, Duration::new(15.0, TimeUnit::Ns));
}
