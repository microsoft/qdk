// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;

use crate::rir_to_circuit::{Op, OperationKind, fmt_ops, group_operations, tracer::QubitRegister};
use expect_test::{Expect, expect};
use qsc_data_structures::span::Span;
use qsc_partial_eval::rir::{
    DbgLocation, DbgMetadataScope, InstructionMetadata, MetadataPackageSpan,
};

#[allow(clippy::needless_pass_by_value)]
fn check(instructions: Vec<Instruction>, expect: Expect) {
    let (locations, scopes, ops) = program(instructions);
    let grouped = group_operations(&locations, &scopes, ops.clone());

    expect.assert_eq(&fmt_ops(&locations, &scopes, &grouped));
}
struct Location {
    scope: String,
    offset: u32,
}

struct Instruction {
    name: String,
    qubits: Vec<QubitRegister>,
    stack: Option<Vec<Location>>,
}

fn program(instructions: Vec<Instruction>) -> (Vec<DbgLocation>, Vec<DbgMetadataScope>, Vec<Op>) {
    let mut locations = vec![];
    let mut scopes = vec![];
    let mut ops = vec![];

    for i in instructions {
        if let Some(stack) = i.stack {
            let mut last_location = None;
            for loc in stack {
                // use existing scope if it exsists
                let scope_index = scopes.iter().position(|s| match s {
                    DbgMetadataScope::SubProgram { name, .. } => name.as_ref() == loc.scope,
                });
                if scope_index.is_none() {
                    scopes.push(DbgMetadataScope::SubProgram {
                        name: Rc::from(loc.scope.as_str()),
                        location: MetadataPackageSpan {
                            package: 2,
                            span: Span {
                                lo: loc.offset,
                                hi: loc.offset + 1,
                            },
                        },
                    });
                }
                let scope_index = scope_index.unwrap_or(scopes.len() - 1);

                // use existing location if it exists
                // (we could do this more efficiently with a map)
                let location_index = locations.iter().position(|l: &DbgLocation| {
                    l.location.package == 2
                        && l.location.span.lo == loc.offset
                        && l.location.span.hi == loc.offset + 1
                        && l.scope == scope_index
                });
                if location_index.is_some() {
                    last_location = location_index;
                    continue;
                }

                locations.push(DbgLocation {
                    location: MetadataPackageSpan {
                        package: 2,
                        span: Span {
                            lo: loc.offset,
                            hi: loc.offset + 1,
                        },
                    },
                    scope: scope_index,
                    inlined_at: last_location,
                });
                last_location = Some(locations.len() - 1);
            }
            ops.push(unitary(
                i.name,
                i.qubits,
                Some(InstructionMetadata {
                    dbg_location: Some(locations.len() - 1),
                }),
            ));
        } else {
            ops.push(unitary(i.name, i.qubits, None));
        }
    }

    (locations, scopes, ops)
}

fn unitary(label: String, qubits: Vec<QubitRegister>, metadata: Option<InstructionMetadata>) -> Op {
    Op {
        kind: OperationKind::Unitary { metadata },
        label,
        target_qubits: qubits,
        control_qubits: vec![],
        target_results: vec![],
        control_results: vec![],
        is_adjoint: false,
        args: vec![],
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
                qubits: vec![QubitRegister(0)],
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
                qubits: vec![QubitRegister(0)],
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
