// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::{self, Write},
    rc::Rc,
};

use crate::{
    builder::{
        LexicalScope, OperationOrGroup, QubitWire, ScopeId, SourceLocationMetadata,
        add_op_with_grouping,
    },
    circuit::PackageOffset,
    rir_to_circuit::ScopeLookup,
};
use expect_test::{Expect, expect};
use indenter::indented;
use qsc_fir::fir::StoreItemId;
use rustc_hash::FxHashMap;

// TODO: add tests to this crate that validate source locations

#[allow(clippy::needless_pass_by_value)]
fn check(instructions: Vec<Instruction>, expect: Expect) {
    let (ops, scopes) = program(instructions);

    let mut grouped = vec![];
    for (op, metadata) in ops {
        let op_call_stack = metadata
            .and_then(|md| md.dbg_location)
            .map(|l| l.call_stack)
            .unwrap_or_default();

        add_op_with_grouping(false, true, &[], &mut grouped, op, op_call_stack);
    }

    let fmt_ops = |grouped: &[OperationOrGroup]| -> String {
        let mut s = String::new();
        fmt_ops(&mut s, 0, grouped, &scopes).expect("formatting failed");
        s
    };

    expect.assert_eq(&fmt_ops(&grouped));
}

struct Location {
    scope: Rc<str>,
    offset: u32,
}

struct Instruction {
    name: String,
    qubits: Vec<QubitWire>,
    stack: Option<Vec<Location>>,
}

#[derive(Clone, Debug)]
pub struct InstructionMetadata {
    dbg_location: Option<DbgLocation>,
}

#[derive(Default)]
struct Scopes {
    id_to_name: FxHashMap<ScopeId, Rc<str>>,
    name_to_id: FxHashMap<Rc<str>, ScopeId>,
}

impl Scopes {
    fn get_or_create_scope(&mut self, name: &Rc<str>) -> ScopeId {
        if let Some(scope_id) = self.name_to_id.get(name) {
            *scope_id
        } else {
            let scope_id = ScopeId(StoreItemId {
                package: 0.into(),
                item: self.id_to_name.len().into(),
            });
            self.id_to_name.insert(scope_id, name.clone());
            self.name_to_id.insert(name.clone(), scope_id);
            scope_id
        }
    }
}

impl ScopeLookup for Scopes {
    fn resolve_scope(&self, scope: ScopeId) -> crate::builder::LexicalScope {
        let name = self
            .id_to_name
            .get(&scope)
            .expect("unknown scope id")
            .clone();
        LexicalScope::Named {
            name,
            location: PackageOffset {
                package_id: scope.0.package,
                offset: 0,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DbgLocation {
    call_stack: Vec<SourceLocationMetadata>,
}

fn program(
    instructions: Vec<Instruction>,
) -> (Vec<(OperationOrGroup, Option<InstructionMetadata>)>, Scopes) {
    let mut ops = vec![];
    let mut scopes = Scopes::default();

    for i in instructions {
        ops.push((
            OperationOrGroup::new_unitary(&i.name, false, &i.qubits, &[], vec![]),
            Some(InstructionMetadata {
                dbg_location: i.stack.map(|stack| DbgLocation {
                    call_stack: stack
                        .iter()
                        .map(|loc| {
                            let scope_id = scopes.get_or_create_scope(&loc.scope);
                            SourceLocationMetadata::new(
                                PackageOffset {
                                    package_id: 0.into(),
                                    offset: loc.offset,
                                },
                                scope_id,
                            )
                        })
                        .collect(),
                }),
            }),
        ));
    }
    (ops, scopes)
}

#[test]
fn empty() {
    check(vec![], expect![" <empty>"]);
}

#[test]
fn single_op_no_metadata() {
    check(
        vec![Instruction {
            name: "H".into(),
            qubits: vec![],
            stack: None,
        }],
        expect!["H qubits="],
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
            [Main] qubits= stack= Main
                H qubits="#]],
    );
}

#[test]
fn two_ops_in_same_scope() {
    check(
        vec![
            instruction(&[("Main", 1)], "H"),
            instruction(&[("Main", 2)], "X"),
        ],
        expect![[r#"
            [Main] qubits=0 stack= Main
                H qubits=0
                X qubits=0"#]],
    );
}

#[test]
fn two_ops_in_separate_scopes() {
    check(
        vec![
            instruction(&[("Foo", 1)], "H"),
            instruction(&[("Bar", 2)], "X"),
        ],
        expect![[r#"
            [Foo] qubits=0 stack= Foo
                H qubits=0
            [Bar] qubits=0 stack= Bar
                X qubits=0"#]],
    );
}

#[test]
fn two_ops_same_grandparent() {
    check(
        vec![
            instruction(&[("Main", 1), ("Foo", 2)], "H"),
            instruction(&[("Main", 1), ("Bar", 3)], "X"),
        ],
        expect![[r#"
            [Main] qubits=0 stack= Main
                [Foo] qubits=0 stack= Main@1->Foo
                    H qubits=0
                [Bar] qubits=0 stack= Main@1->Bar
                    X qubits=0"#]],
    );
}

#[test]
fn two_ops_same_parent_scope() {
    check(
        vec![
            instruction(&[("Main", 1), ("Foo", 2)], "H"),
            instruction(&[("Main", 1), ("Foo", 3)], "X"),
        ],
        expect![[r#"
            [Main] qubits=0 stack= Main
                [Foo] qubits=0 stack= Main@1->Foo
                    H qubits=0
                    X qubits=0"#]],
    );
}

#[test]
fn two_ops_separate_grandparents() {
    check(
        vec![
            instruction(&[("A", 1), ("B", 3), ("C", 4)], "X"),
            instruction(&[("A", 2), ("B", 3), ("C", 4)], "X"),
        ],
        expect![[r#"
            [A] qubits=0 stack= A
                [B] qubits=0 stack= A@1->B
                    [C] qubits=0 stack= A@1->B@3->C
                        X qubits=0
                [B] qubits=0 stack= A@2->B
                    [C] qubits=0 stack= A@2->B@3->C
                        X qubits=0"#]],
    );
}

#[test]
fn ad_hoc() {
    check(
        vec![
            instruction(&[("A", 1), ("B", 5), ("F", 9)], "X"),
            instruction(&[("A", 1), ("B", 5), ("F", 10)], "Y"),
            instruction(&[("A", 1), ("B", 5), ("F", 11)], "Z"),
            instruction(&[("A", 2), ("B", 5), ("F", 9)], "X"),
            instruction(&[("A", 2), ("B", 5), ("F", 10)], "Y"),
            instruction(&[("A", 2), ("B", 5), ("F", 11)], "Z"),
            instruction(&[("A", 2), ("B", 6), ("F", 10)], "Y"),
            instruction(&[("A", 2), ("B", 6), ("F", 11)], "Z"),
            instruction(&[("A", 1), ("B", 5), ("F", 9)], "X"),
            instruction(&[("A", 1)], "Y"),
            instruction(&[("A", 1)], "Z"),
            instruction(&[("A", 2), ("B", 5), ("F", 10)], "Y"),
            instruction(&[("A", 2), ("B", 5), ("F", 11)], "Z"),
            instruction(&[("A", 3)], "C"),
            instruction(&[("A", 4), ("D", 7)], "H"),
            instruction(&[("A", 4), ("D", 8)], "I"),
            instruction(&[("A", 5)], "E"),
            instruction(&[("A", 5)], "G"),
        ],
        expect![[r#"
            [A] qubits=0 stack= A
                [B] qubits=0 stack= A@1->B
                    [F] qubits=0 stack= A@1->B@5->F
                        X qubits=0
                        Y qubits=0
                        Z qubits=0
                [B] qubits=0 stack= A@2->B
                    [F] qubits=0 stack= A@2->B@5->F
                        X qubits=0
                        Y qubits=0
                        Z qubits=0
                    [F] qubits=0 stack= A@2->B@6->F
                        Y qubits=0
                        Z qubits=0
                [B] qubits=0 stack= A@1->B
                    [F] qubits=0 stack= A@1->B@5->F
                        X qubits=0
                Y qubits=0
                Z qubits=0
                [B] qubits=0 stack= A@2->B
                    [F] qubits=0 stack= A@2->B@5->F
                        Y qubits=0
                        Z qubits=0
                C qubits=0
                [D] qubits=0 stack= A@4->D
                    H qubits=0
                    I qubits=0
                E qubits=0
                G qubits=0"#]],
    );
}

fn instruction(stack: &[(&str, u32)], name: &str) -> Instruction {
    Instruction {
        name: name.into(),
        qubits: vec![QubitWire(0)],
        stack: Some(stack.iter().map(|(s, o)| location(s, *o)).collect()),
    }
}

fn location(scope: &str, offset: u32) -> Location {
    Location {
        scope: scope.into(),
        offset,
    }
}

#[allow(dead_code)]
fn fmt_ops(
    f: &mut impl Write,
    indent_level: usize,
    ops: &[OperationOrGroup],
    scope_resolver: &impl ScopeLookup,
) -> fmt::Result {
    let mut iter = ops.iter().peekable();
    if iter.peek().is_none() {
        write!(f, " <empty>")
    } else {
        while let Some(elt) = iter.next() {
            fmt_op(f, indent_level, elt, scope_resolver)?;
            if iter.peek().is_some() {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

fn fmt_op(
    f: &mut impl Write,
    indent_level: usize,
    op: &OperationOrGroup,
    scope_resolver: &impl ScopeLookup,
) -> fmt::Result {
    if let Some(scope_stack) = op.scope_stack_if_group() {
        write!(
            &mut set_indentation(indented(f), indent_level),
            "[{}]",
            scope_resolver
                .resolve_scope(scope_stack.current_lexical_scope())
                .name()
        )?;
    } else {
        write!(
            &mut set_indentation(indented(f), indent_level),
            "{}",
            op.name(scope_resolver)
        )?;
    }

    write!(f, " qubits=")?;

    let qubits = op.all_qubits();
    let mut qubits = qubits.iter().peekable();
    while let Some(q) = qubits.next() {
        write!(f, "{}", q.0)?;
        if qubits.peek().is_some() {
            write!(f, ", ")?;
        }
    }

    if let Some(scope_stack) = op.scope_stack_if_group() {
        write!(f, " stack= {}", scope_stack.fmt(scope_resolver))?;
    }

    if let Some(children) = op.children() {
        writeln!(f)?;
        fmt_ops(f, indent_level + 1, children, scope_resolver)?;
    }

    Ok(())
}

/// Takes an `indenter::Indented` and changes its indentation level.
fn set_indentation<T>(indent: indenter::Indented<'_, T>, level: usize) -> indenter::Indented<'_, T>
where
    T: fmt::Write,
{
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        3 => indent.with_str("            "),
        4 => indent.with_str("                "),
        5 => indent.with_str("                    "),
        6 => indent.with_str("                        "),
        7 => indent.with_str("                            "),
        8 => indent.with_str("                                "),
        _ => indent.with_str("                                ..."),
    }
}
