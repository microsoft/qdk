// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::{self, Write};

use crate::{
    builder::{LexicalScope, OperationOrGroupExt, QubitWire, add_op_with_grouping},
    circuit::PackageOffset,
    rir_to_circuit::{DbgStuffExt, ScopeResolver, ScopeStack},
};
use expect_test::{Expect, expect};
use indenter::indented;
use qsc_fir::fir::PackageId;
use rustc_hash::FxHashSet;

#[allow(clippy::needless_pass_by_value)]
fn check(instructions: Vec<Instruction>, expect: Expect) {
    let ops = program(instructions);

    let mut grouped = vec![];
    for op in ops {
        add_op_with_grouping(&[], &(), &mut grouped, op);
    }

    let fmt_ops = |grouped: &[Op]| -> String {
        let mut s = String::new();
        fmt_ops(&mut s, 0, grouped).expect("formatting failed");
        s
    };

    expect.assert_eq(&fmt_ops(&grouped));
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

#[derive(Clone, Debug)]
pub struct InstructionMetadata {
    dbg_location: Option<DbgLocation>,
}

impl DbgStuffExt for () {
    type SourceLocation = (String, u32);
    type Scope = String;

    fn package_id(&self, _location: &Self::SourceLocation) -> qsc_fir::fir::PackageId {
        PackageId::CORE
    }

    fn lexical_scope(&self, location: &Self::SourceLocation) -> Self::Scope {
        location.0.clone()
    }

    fn source_location(&self, location: &Self::SourceLocation) -> PackageOffset {
        PackageOffset {
            package_id: PackageId::CORE,
            offset: location.1,
        }
    }
}

impl ScopeResolver for () {
    type ScopeId = String;

    fn resolve_scope(&self, scope: &Self::ScopeId) -> crate::builder::LexicalScope {
        LexicalScope::Named {
            name: scope.clone().into(),
            location: PackageOffset {
                package_id: 0.into(),
                offset: 0,
            },
        }
    }
}

enum Op {
    Single {
        name: String,
        call_stack: Vec<(String, u32)>,
        qubits: Vec<QubitWire>,
    },
    Group {
        scope_stack: ScopeStack<(String, u32), String>,
        children: Vec<Op>,
        qubits: Vec<QubitWire>,
    },
}

impl OperationOrGroupExt for Op {
    type Scope = String;
    type SourceLocation = (String, u32);
    type DbgStuff<'a> = ();

    fn group(
        scope_stack: ScopeStack<Self::SourceLocation, Self::Scope>,
        children: Vec<Self>,
    ) -> Self
    where
        Self: std::marker::Sized,
    {
        let all_qubits = children
            .iter()
            .flat_map(OperationOrGroupExt::all_qubits)
            .collect::<FxHashSet<QubitWire>>()
            .into_iter()
            .collect::<Vec<QubitWire>>();
        Op::Group {
            scope_stack,
            children,
            qubits: all_qubits,
        }
    }

    fn name(
        &self,
        _dbg_stuff: &impl DbgStuffExt<SourceLocation = Self::SourceLocation, Scope = Self::Scope>,
    ) -> String {
        match self {
            Op::Single { name, .. } => name.clone(),
            Op::Group { scope_stack, .. } => scope_stack.current_lexical_scope().to_string(),
        }
    }

    fn instruction_stack(&self, _dbg_stuff: &Self::DbgStuff<'_>) -> Vec<Self::SourceLocation> {
        match self {
            Op::Single { call_stack, .. } => call_stack.clone(),
            Op::Group { .. } => {
                panic!("didn't expect instruction_stack to be called for a group")
            }
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Self>>
    where
        Self: std::marker::Sized,
    {
        match self {
            Op::Group { children, .. } => Some(children),
            Op::Single { .. } => None,
        }
    }

    fn scope_stack_if_group(
        &self,
    ) -> Option<&crate::rir_to_circuit::ScopeStack<Self::SourceLocation, Self::Scope>> {
        match self {
            Op::Group { scope_stack, .. } => Some(scope_stack),
            Op::Single { .. } => None,
        }
    }

    fn all_qubits(&self) -> Vec<QubitWire> {
        match self {
            Op::Group { qubits, .. } | Op::Single { qubits, .. } => qubits.clone(),
        }
    }

    fn all_results(&self) -> Vec<crate::builder::ResultWire> {
        vec![]
    }

    fn extend_target_qubits(&mut self, target_qubits: &[QubitWire]) {
        match self {
            Op::Group { qubits, .. } | Op::Single { qubits, .. } => {
                for q in target_qubits {
                    if !qubits.contains(q) {
                        qubits.push(*q);
                    }
                }
            }
        }
    }

    fn extend_target_results(&mut self, _target_results: &[crate::builder::ResultWire]) {}
}

#[derive(Clone, Debug, PartialEq)]
struct DbgLocation {
    call_stack: Vec<(String, u32)>,
}

fn program(instructions: Vec<Instruction>) -> Vec<Op> {
    let mut ops = vec![];

    for i in instructions {
        ops.push(unitary(
            i.name,
            i.qubits,
            Some(InstructionMetadata {
                dbg_location: i.stack.map(|stack| DbgLocation {
                    call_stack: stack
                        .iter()
                        .map(|loc| (loc.scope.clone(), loc.offset))
                        .collect(),
                }),
            }),
        ));
    }
    ops
}

fn unitary(label: String, qubits: Vec<QubitWire>, metadata: Option<InstructionMetadata>) -> Op {
    Op::Single {
        name: label,
        qubits,
        call_stack: metadata
            .and_then(|m| m.dbg_location)
            .map(|d| d.call_stack)
            .unwrap_or_default()
            .iter()
            .map(|(s, o)| (s.clone(), *o))
            .collect(),
    }
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
        expect!["[H] qubits="],
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
                [H] qubits="#]],
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
                [H] qubits=0
                [X] qubits=0"#]],
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
                [H] qubits=0
            [Bar] qubits=0 stack= Bar
                [X] qubits=0"#]],
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
                    [H] qubits=0
                [Bar] qubits=0 stack= Main@1->Bar
                    [X] qubits=0"#]],
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
                    [H] qubits=0
                    [X] qubits=0"#]],
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
            instruction(&[("A", 1)], "Y"),
            instruction(&[("A", 1)], "Z"),
            instruction(&[("A", 2), ("B", 5), ("F", 10)], "Y"),
            instruction(&[("A", 2), ("B", 5), ("F", 11)], "Z"),
            instruction(&[("A", 3)], "C"),
            instruction(&[("A", 4), ("D", 7)], "H"),
            instruction(&[("A", 4), ("D", 8)], "I"),
            instruction(&[("A", 5)], "E"),
        ],
        expect![[r#"
            [A] qubits=0 stack= A
                [B] qubits=0 stack= A@1->B
                    [F] qubits=0 stack= A@1->B@5->F
                        [X] qubits=0
                        [Y] qubits=0
                        [Z] qubits=0
                [B] qubits=0 stack= A@2->B
                    [F] qubits=0 stack= A@2->B@5->F
                        [X] qubits=0
                        [Y] qubits=0
                        [Z] qubits=0
                    [F] qubits=0 stack= A@2->B@6->F
                        [Y] qubits=0
                        [Z] qubits=0
                [Y] qubits=0
                [Z] qubits=0
                [B] qubits=0 stack= A@2->B
                    [F] qubits=0 stack= A@2->B@5->F
                        [Y] qubits=0
                        [Z] qubits=0
                [C] qubits=0
                [D] qubits=0 stack= A@4->D
                    [H] qubits=0
                    [I] qubits=0
                [E] qubits=0"#]],
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
fn fmt_ops(f: &mut impl Write, indent_level: usize, ops: &[Op]) -> fmt::Result {
    let mut iter = ops.iter().peekable();
    if iter.peek().is_none() {
        write!(f, " <empty>")
    } else {
        while let Some(elt) = iter.next() {
            fmt_op(f, indent_level, elt)?;
            if iter.peek().is_some() {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

fn fmt_op(f: &mut impl Write, indent_level: usize, op: &Op) -> fmt::Result {
    let name = op.name(&());

    write!(
        &mut set_indentation(indented(f), indent_level),
        "[{name}] qubits="
    )?;

    let qubits = op.all_qubits();
    let mut qubits = qubits.iter().peekable();
    while let Some(q) = qubits.next() {
        write!(f, "{}", q.0)?;
        if qubits.peek().is_some() {
            write!(f, ", ")?;
        }
    }

    if let Op::Group { scope_stack, .. } = &op {
        write!(f, " stack= {}", scope_stack.fmt(&()))?;
    }

    if let Op::Group { children, .. } = &op {
        writeln!(f)?;
        fmt_ops(f, indent_level + 1, children)?;
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
        _ => unimplemented!("indentation level not supported: {}", level),
    }
}
