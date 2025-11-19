// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{CircuitTracer, TracerConfig, builder::tests::FakeCompilation};
use expect_test::{Expect, expect};
use qsc_eval::backend::Tracer;

fn check(instructions: &'static [(&'static [(&'static str, u32)], &'static str)], expect: &Expect) {
    let qubit_id = 0;
    let mut tracer = CircuitTracer::new(
        TracerConfig {
            max_operations: usize::MAX,
            source_locations: false,
            group_scopes: true,
        },
        &FakeCompilation::user_package_ids(),
    );
    tracer.qubit_allocate(&[], qubit_id);
    let mut c = FakeCompilation::default();

    for i in instructions {
        let stack =
            i.0.iter()
                .map(|(scope, offset)| c.user_code_frame(scope, *offset))
                .collect::<Vec<_>>();
        let name = i.1;

        tracer.gate(&stack, name, false, &[qubit_id], &[], None);
    }

    let circuit = tracer.finish(&c);
    expect.assert_eq(&circuit.to_string());
}

#[test]
fn empty() {
    // TODO: we disabled source labels in these tests, these shouldn't show up
    check(
        &[],
        &expect![[r#"
            q_0
        "#]],
    );
}

#[test]
fn single_op_no_metadata() {
    check(
        &[(&[], "H")],
        &expect![[r#"
        q_0    ── H ──
    "#]],
    );
}

#[test]
fn single_op() {
    check(
        &[(&[("Main", 1)], "H")],
        &expect![[r#"
            q_0    ─ [[ ─── [Main] ──── H ─── ]] ──
        "#]],
    );
}

#[test]
fn two_ops_in_same_scope() {
    check(
        &[(&[("Main", 1)], "H"), (&[("Main", 2)], "X")],
        &expect![[r#"
            q_0    ─ [[ ─── [Main] ──── H ──── X ─── ]] ──
        "#]],
    );
}

#[test]
fn two_ops_in_separate_scopes() {
    check(
        &[(&[("Foo", 1)], "H"), (&[("Bar", 2)], "X")],
        &expect![[r#"
            q_0    ─ [[ ─── [Foo] ─── H ─── ]] ─── [[ ─── [Bar] ─── X ─── ]] ──
        "#]],
    );
}

#[test]
fn two_ops_same_grandparent() {
    check(
        &[
            (&[("Main", 1), ("Foo", 2)], "H"),
            (&[("Main", 1), ("Bar", 3)], "X"),
        ],
        &expect![[r#"
            q_0    ─ [[ ─── [Main] ─── [[ ─── [Foo] ─── H ─── ]] ─── [[ ─── [Bar] ─── X ─── ]] ─── ]] ──
        "#]],
    );
}

#[test]
fn two_ops_same_parent_scope() {
    check(
        &[
            (&[("Main", 1), ("Foo", 2)], "H"),
            (&[("Main", 1), ("Foo", 3)], "X"),
        ],
        &expect![[r#"
            q_0    ─ [[ ─── [Main] ─── [[ ─── [Foo] ─── H ──── X ─── ]] ─── ]] ──
        "#]],
    );
}

#[test]
fn two_ops_separate_grandparents() {
    check(
        &[
            (&[("A", 1), ("B", 3), ("C", 4)], "X"),
            (&[("A", 2), ("B", 3), ("C", 4)], "X"),
        ],
        &expect![[r#"
            q_0    ─ [[ ─── [A] ── [[ ─── [B] ── [[ ─── [C] ─── X ─── ]] ─── ]] ─── [[ ─── [B] ── [[ ─── [C] ─── X ─── ]] ─── ]] ─── ]] ──
        "#]],
    );
}

#[test]
fn ad_hoc() {
    check(
        &[
            (&[("A", 1), ("B", 5), ("F", 9)], "X"),
            (&[("A", 1), ("B", 5), ("F", 10)], "Y"),
            (&[("A", 1), ("B", 5), ("F", 11)], "Z"),
            (&[("A", 2), ("B", 5), ("F", 9)], "X"),
            (&[("A", 2), ("B", 5), ("F", 10)], "Y"),
            (&[("A", 2), ("B", 5), ("F", 11)], "Z"),
            (&[("A", 2), ("B", 6), ("F", 10)], "Y"),
            (&[("A", 2), ("B", 6), ("F", 11)], "Z"),
            (&[("A", 1), ("B", 5), ("F", 9)], "X"),
            (&[("A", 1)], "Y"),
            (&[("A", 1)], "Z"),
            (&[("A", 2), ("B", 5), ("F", 10)], "Y"),
            (&[("A", 2), ("B", 5), ("F", 11)], "Z"),
            (&[("A", 3)], "C"),
            (&[("A", 4), ("D", 7)], "H"),
            (&[("A", 4), ("D", 8)], "I"),
            (&[("A", 5)], "E"),
            (&[("A", 5)], "G"),
        ],
        &expect![[r#"
            q_0    ─ [[ ─── [A] ── [[ ─── [B] ── [[ ─── [F] ─── X ──── Y ──── Z ─── ]] ─── ]] ─── [[ ─── [B] ── [[ ─── [F] ─── X ──── Y ──── Z ─── ]] ─── [[ ─── [F] ─── Y ──── Z ─── ]] ─── ]] ─── [[ ─── [B] ── [[ ─── [F] ─── X ─── ]] ─── ]] ──── Y ──── Z ─── [[ ─── [B] ── [[ ─── [F] ─── Y ──── Z ─── ]] ─── ]] ──── C ─── [[ ─── [D] ─── H ──── I ─── ]] ──── E ──── G ─── ]] ──
        "#]],
    );
}
