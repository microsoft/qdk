// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;
use std::rc::Rc;

use crate::{
    builder::{QubitWire, add_op_with_grouping},
    rir_to_circuit::{DbgStuff, Op, OperationKind},
};
use expect_test::{Expect, expect};
use qsc_data_structures::{
    debug::{
        DbgInfo, DbgLocation, DbgLocationId, DbgMetadataScope, DbgScopeId, InstructionMetadata,
    },
    span::Span,
};
use qsc_eval::PackageSpan;

#[allow(clippy::needless_pass_by_value)]
fn check(instructions: Vec<Instruction>, expect: Expect) {
    let (dbg_info, ops) = program(instructions);
    let dbg_stuff = DbgStuff {
        dbg_info: &dbg_info,
    };

    let mut grouped = vec![];
    for op in ops {
        add_op_with_grouping(&[], &dbg_stuff, &mut grouped, op);
    }

    expect.assert_eq(&fmt_ops(&dbg_info, &grouped));
}
struct Location {
    scope: String,
    offset: u32,
}

struct Instruction {
    name: String,
    qubits: Vec<QubitWire>,
    stack: Option<Vec<Location>>,
}

fn program(instructions: Vec<Instruction>) -> (DbgInfo, Vec<Op>) {
    let mut locations = vec![];
    let mut scopes = vec![];
    let mut ops = vec![];

    for i in instructions {
        if let Some(stack) = i.stack {
            let mut last_location = None;
            for loc in stack {
                // use existing scope if it exsists
                let scope_index: Option<DbgScopeId> = scopes
                    .iter()
                    .position(|s| match s {
                        DbgMetadataScope::SubProgram { name, .. } => name.as_ref() == loc.scope,
                    })
                    .map(std::convert::Into::into);
                if scope_index.is_none() {
                    scopes.push(DbgMetadataScope::SubProgram {
                        name: Rc::from(loc.scope.as_str()),
                        location: PackageSpan {
                            package: 2.into(), // TODO: uh oh
                            span: Span {
                                lo: loc.offset,
                                hi: loc.offset + 1,
                            },
                        },
                    });
                }
                let scope_index = scope_index.unwrap_or((scopes.len() - 1).into());

                // use existing location if it exists
                // (we could do this more efficiently with a map)
                let location_index: Option<DbgLocationId> = locations
                    .iter()
                    .position(|l: &DbgLocation| {
                        l.location.package == 2.into() // TODO: uh oh
                        && l.location.span.lo == loc.offset
                        && l.location.span.hi == loc.offset + 1
                        && l.scope == scope_index
                    })
                    .map(std::convert::Into::into);
                if location_index.is_some() {
                    last_location = location_index;
                    continue;
                }

                locations.push(DbgLocation {
                    location: PackageSpan {
                        package: 2.into(),
                        span: Span {
                            lo: loc.offset,
                            hi: loc.offset + 1,
                        },
                    },
                    scope: scope_index,
                    inlined_at: last_location,
                });
                last_location = Some((locations.len() - 1).into());
            }
            ops.push(unitary(
                i.name,
                i.qubits,
                Some(InstructionMetadata {
                    dbg_location: Some((locations.len() - 1).into()),
                }),
            ));
        } else {
            ops.push(unitary(i.name, i.qubits, None));
        }
    }

    (
        DbgInfo {
            dbg_locations: locations,
            dbg_metadata_scopes: scopes,
        },
        ops,
    )
}

fn unitary(label: String, qubits: Vec<QubitWire>, metadata: Option<InstructionMetadata>) -> Op {
    Op {
        kind: OperationKind::Unitary { label },
        target_qubits: qubits,
        control_qubits: vec![],
        target_results: vec![],
        control_results: vec![],
        is_adjoint: false,
        args: vec![],
        location: metadata.and_then(|md| md.dbg_location),
    }
}

#[test]
fn empty() {
    check(
        vec![],
        expect![[r#"
        [
        ]
    "#]],
    );
}

#[test]
fn single_op_no_metadata() {
    check(
        vec![Instruction {
            name: "H".into(),
            qubits: vec![],
            stack: None,
        }],
        expect![[r#"
            [
              (H, q=[])
            ]
        "#]],
    );
}

#[test]
fn single_op() {
    check(
        vec![Instruction {
            name: "H".into(),
            qubits: vec![],
            stack: Some(vec![Location {
                scope: "Main".into(),
                offset: 1,
            }]),
        }],
        expect![[r#"
            [
              (Main, q=[], stack=Main, children=[(H, q=[]), ])
            ]
        "#]],
    );
}

#[test]
fn two_ops_in_separate_scopes() {
    check(
        vec![
            Instruction {
                name: "H".into(),
                qubits: vec![],
                stack: Some(vec![Location {
                    scope: "Main".into(),
                    offset: 1,
                }]),
            },
            Instruction {
                name: "X".into(),
                qubits: vec![],
                stack: Some(vec![Location {
                    scope: "Main".into(),
                    offset: 2,
                }]),
            },
        ],
        expect![[r#"
            [
              (Main, q=[], stack=Main, children=[(H, q=[]), (X, q=[]), ])
            ]
        "#]],
    );
}

#[test]
fn two_ops_same_grandparent() {
    check(
        vec![
            Instruction {
                name: "H".into(),
                qubits: vec![],
                stack: Some(vec![
                    Location {
                        scope: "Main".into(),
                        offset: 1,
                    },
                    Location {
                        scope: "Foo".into(),
                        offset: 2,
                    },
                ]),
            },
            Instruction {
                name: "X".into(),
                qubits: vec![],
                stack: Some(vec![
                    Location {
                        scope: "Main".into(),
                        offset: 1,
                    },
                    Location {
                        scope: "Bar".into(),
                        offset: 3,
                    },
                ]),
            },
        ],
        expect![[r#"
            [
              (Main, q=[], stack=Main, children=[(Foo, q=[]), stack=Main@1->Foo, children=[(H, q=[]), ], (Bar, q=[]), stack=Main@1->Bar, children=[(X, q=[]), ], ])
            ]
        "#]],
    );
}

#[test]
fn two_ops_same_parent_scope() {
    check(
        vec![
            Instruction {
                name: "H".into(),
                qubits: vec![QubitWire(0)],
                stack: Some(vec![
                    Location {
                        scope: "Main".into(),
                        offset: 1,
                    },
                    Location {
                        scope: "Foo".into(),
                        offset: 2,
                    },
                ]),
            },
            Instruction {
                name: "X".into(),
                qubits: vec![QubitWire(0)],
                stack: Some(vec![
                    Location {
                        scope: "Main".into(),
                        offset: 1,
                    },
                    Location {
                        scope: "Foo".into(),
                        offset: 3,
                    },
                ]),
            },
        ],
        expect![[r#"
            [
              (Main, q=[0], stack=Main, children=[(Foo, q=[0]), stack=Main@1->Foo, children=[(H, q=[0]), (X, q=[0]), ], ])
            ]
        "#]],
    );
}

#[allow(dead_code)]
fn fmt_ops_with_trailing_comma(dbg_info: &DbgInfo, ops: &[Op]) -> String {
    let items: Vec<String> = ops
        .iter()
        .map(|op| {
            let name = match &op.kind {
                OperationKind::Unitary { label }
                | OperationKind::Measurement { label }
                | OperationKind::Ket { label }
                | OperationKind::ConditionalGroup { label, .. } => label.clone(),
                OperationKind::Group { scope_stack, .. } => {
                    scope_stack.resolve_scope(dbg_info).name()
                }
            };
            let stack_and_children = match &op.kind {
                OperationKind::Group {
                    children,
                    scope_stack: stack,
                    scope_span: _,
                } => {
                    format!(
                        "stack={}, children={}",
                        stack.fmt_with_resolved_scopes(&DbgStuff { dbg_info }, dbg_info),
                        fmt_ops_with_trailing_comma(dbg_info, children)
                    )
                }
                OperationKind::ConditionalGroup { children, .. } => {
                    format!(
                        "children={}",
                        fmt_ops_with_trailing_comma(dbg_info, children)
                    )
                }
                _ => String::new(),
            };
            if stack_and_children.is_empty() {
                format!(
                    "({name}, q={:?})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>()
                )
            } else {
                format!(
                    "({name}, q={:?}), {}",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>(),
                    stack_and_children
                )
            }
        })
        .collect();
    format!(
        "[{}]",
        if items.is_empty() {
            String::new()
        } else {
            format!("{}, ", items.join(", "))
        }
    )
}

#[allow(dead_code)]
fn fmt_ops(dbg_info: &DbgInfo, ops: &[Op]) -> String {
    let items: Vec<String> = ops
        .iter()
        .map(|op| {
            let name = match &op.kind {
                OperationKind::Unitary { label }
                | OperationKind::Measurement { label }
                | OperationKind::Ket { label }
                | OperationKind::ConditionalGroup { label, .. } => label.clone(),
                OperationKind::Group { scope_stack, .. } => {
                    scope_stack.resolve_scope(dbg_info).name()
                }
            };
            let stack_and_children = match &op.kind {
                OperationKind::Group {
                    children,
                    scope_stack: stack,
                    scope_span: _,
                } => {
                    format!(
                        "stack={}, children={}",
                        stack.fmt_with_resolved_scopes(&DbgStuff { dbg_info }, dbg_info),
                        fmt_ops_with_trailing_comma(dbg_info, children)
                    )
                }
                OperationKind::ConditionalGroup { children, .. } => {
                    format!(
                        "children={}",
                        fmt_ops_with_trailing_comma(dbg_info, children)
                    )
                }
                _ => String::new(),
            };
            if stack_and_children.is_empty() {
                format!(
                    "({name}, q={:?})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>()
                )
            } else {
                format!(
                    "({name}, q={:?}, {})",
                    op.target_qubits.iter().map(|q| q.0).collect::<Vec<_>>(),
                    stack_and_children
                )
            }
        })
        .collect();
    let mut s = String::new();
    let _ = writeln!(s, "[");
    for item in items {
        let _ = writeln!(s, "  {item}");
    }
    let _ = writeln!(s, "]");

    s
}
