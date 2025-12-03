// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::TracerConfig;

use super::FakeCompilation;
use super::Profiler;
use expect_test::{Expect, expect};
use qsc_eval::{GigaStack, backend::Tracer, debug::Frame};

fn check_groups(c: &FakeCompilation, instructions: &[(Vec<Frame>, &str)], expect: &Expect) {
    let mut tracer = Profiler::new(
        TracerConfig {
            max_operations: usize::MAX,
            source_locations: false,
            group_scopes: super::GroupScopesOptions::GroupScopes,
        },
        &FakeCompilation::user_package_ids(),
    );

    let qubit_id = 0;

    // Allocate qubit 0
    tracer.qubit_allocate(&GigaStack::default(), qubit_id);

    // Trace each instruction, applying it to qubit 0
    for i in instructions {
        let stack = GigaStack::from(i.0.clone());
        let name = i.1;

        tracer.gate(&stack, name, false, &[qubit_id], &[], None);
    }

    let circuit = tracer.finish(c);
    expect.assert_eq(&circuit.to_string());
}

#[test]
fn empty() {
    check_groups(
        &FakeCompilation::default(),
        &[],
        &expect![[r#"
            root (0)
        "#]],
    );
}

#[test]
fn single_op() {
    let mut c = FakeCompilation::default();
    let program = vec![(vec![c.user_code_frame("Main", 1)], "H")];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            Main (1)
        "#]],
    );
}

#[test]
fn two_ops_in_same_scope() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (vec![c.user_code_frame("Main", 1)], "H"),
        (vec![c.user_code_frame("Main", 2)], "X"),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            Main (2)
        "#]],
    );
}

#[test]
fn two_ops_in_separate_scopes() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (vec![c.user_code_frame("Foo", 1)], "H"),
        (vec![c.user_code_frame("Bar", 2)], "X"),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            root (2)
              Foo (1)
              Bar (1)
        "#]],
    );
}

#[test]
fn two_ops_same_grandparent() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![c.user_code_frame("Main", 1), c.user_code_frame("Foo", 2)],
            "H",
        ),
        (
            vec![c.user_code_frame("Main", 1), c.user_code_frame("Bar", 3)],
            "X",
        ),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            Main (2)
              Foo (1)
              Bar (1)
        "#]],
    );
}

#[test]
fn two_ops_same_parent_scope() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![c.user_code_frame("Main", 1), c.user_code_frame("Foo", 2)],
            "H",
        ),
        (
            vec![c.user_code_frame("Main", 1), c.user_code_frame("Foo", 3)],
            "X",
        ),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            Main (2)
              Foo (2)
        "#]],
    );
}

#[test]
fn two_ops_separate_grandparents() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![
                c.user_code_frame("A", 1),
                c.user_code_frame("B", 3),
                c.user_code_frame("C", 4),
            ],
            "X",
        ),
        (
            vec![
                c.user_code_frame("A", 2),
                c.user_code_frame("B", 3),
                c.user_code_frame("C", 4),
            ],
            "X",
        ),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            A (2)
              B (1)
                C (1)
              B (1)
                C (1)
        "#]],
    );
}

#[test]
fn same_grandparent_separate_parents() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![
                c.user_code_frame("A", 2),
                c.user_code_frame("B", 5),
                c.user_code_frame("F", 11),
            ],
            "Z",
        ),
        (
            vec![
                c.user_code_frame("A", 2),
                c.user_code_frame("B", 6),
                c.user_code_frame("F", 10),
            ],
            "Y",
        ),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            A (2)
              B (2)
                F (1)
                F (1)
        "#]],
    );
}

#[test]
fn back_up_to_grandparent() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![
                c.user_code_frame("A", 2),
                c.user_code_frame("B", 6),
                c.user_code_frame("C", 11),
            ],
            "X",
        ),
        (vec![c.user_code_frame("A", 1)], "Y"),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            A (2)
              B (1)
                C (1)
        "#]],
    );
}

#[test]
fn library_frames_excluded() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![
                c.library_frame(1),
                c.user_code_frame("A", 2),
                c.library_frame(2),
                c.user_code_frame("B", 6),
                c.library_frame(3),
            ],
            "X",
        ),
        (vec![c.library_frame(4)], "Y"),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            root (2)
              A (1)
                B (1)
        "#]],
    );
}

#[test]
fn adjoint_call_frame() {
    let mut c = FakeCompilation::default();
    let program = vec![
        (
            vec![
                c.user_code_frame("Main", 1),
                c.library_frame(5),
                c.user_code_frame("Foo", 2),
            ],
            "U",
        ),
        (
            vec![
                c.user_code_frame("Main", 1),
                c.library_frame(5),
                c.user_code_adjoint_frame("Foo", 3),
            ],
            "U",
        ),
    ];
    check_groups(
        &c,
        &program,
        &expect![[r#"
            Main (2)
              Foo (1)
              Foo† (1)
        "#]],
    );
}
