// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A meaning's stable form, with the `serde` feature. That a meaning
//! reads back equal, and that a form's faults are refused when its set is
//! read, is tested through its set, in `instruction_set/stable_form.rs`.

use crate::{Guard, Meaning, MeaningFault, Pauli};

/// A meaning written with a slip could not be in a set, so it has no
/// stable form: writing it is refused, naming the slip, rather than
/// writing a form that would read back as another meaning.
#[test]
fn a_meaning_written_with_a_slip_has_no_stable_form() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    for (row, meaning, slip) in [
        (
            "a guard before any step",
            Meaning::declared().only_if([Guard::Parameter(0)]),
            MeaningFault::GuardWithoutStep,
        ),
        (
            "a step guarded twice",
            x().only_if([Guard::Parameter(0)])
                .unless([Guard::Parameter(1)]),
            MeaningFault::GuardTwice { step: 0 },
        ),
        (
            "a generator mapped twice",
            Meaning::declared().clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::x(0), Pauli::y(0))]),
            MeaningFault::GeneratorTwice {
                step: 0,
                generator: Pauli::x(0),
            },
        ),
    ] {
        let refusal = serde_json::to_string(&meaning)
            .expect_err("a meaning with a slip is not written")
            .to_string();
        let slip = slip.to_string();
        assert!(refusal.contains(&slip), "{row}: {refusal:?} names {slip:?}");
    }
}
