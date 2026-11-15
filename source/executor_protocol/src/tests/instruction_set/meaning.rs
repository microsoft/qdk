// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The canonical form of what an instruction does: two meanings written
//! otherwise compare equal. What a meaning refers to is checked when its
//! set is built, so its faults are tested with the set's declaration, in
//! `tests/instruction_set.rs`.

use crate::{Angle, Guard, Meaning, Pauli};

/// Two meanings are equal exactly when they do the same: the order of
/// Clifford images and of guard terms, unchanged generators and terms that
/// cancel are ignored; `only_if` and `unless`, declared and undeclared,
/// and angles bit for bit are told apart.
#[test]
fn meanings_are_equal_exactly_when_they_do_the_same() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    let cx_x = || (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let cx_z = || (Pauli::z(1), Pauli::z(0) * Pauli::z(1));
    let x = || Meaning::declared().pauli(Pauli::x(0));
    let rotate = |angle: f64| Meaning::declared().rotate(Pauli::z(0), Angle::Literal(angle));
    for (row, a, b, equal) in [
        // A `cx` listing that it keeps `Z_0` equals the one that leaves it
        // out.
        (
            "a Clifford listing a generator it keeps",
            cx(vec![cx_x(), (Pauli::z(0), Pauli::z(0))]),
            cx(vec![cx_x()]),
            true,
        ),
        (
            "Clifford images in another order",
            cx(vec![cx_x(), cx_z()]),
            cx(vec![cx_z(), cx_x()]),
            true,
        ),
        (
            "guard terms in another order",
            x().only_if([Guard::Parameter(0), Guard::Outcome(0)]),
            x().only_if([Guard::Outcome(0), Guard::Parameter(0)]),
            true,
        ),
        // A term twice cancels in a parity.
        (
            "a guard term given twice",
            x().only_if([
                Guard::Parameter(0),
                Guard::Parameter(1),
                Guard::Parameter(0),
            ]),
            x().only_if([Guard::Parameter(1)]),
            true,
        ),
        // `unless` an empty parity always runs, so the step is as
        // unguarded.
        ("`unless` no term", x().unless([]), x(), true),
        // A guard before any step is a slip (`GuardWithoutStep`) that the
        // meaning keeps, so it differs from one with the same steps, here
        // none, and no slip.
        (
            "a slip",
            Meaning::declared().only_if([Guard::Parameter(0)]),
            Meaning::declared(),
            false,
        ),
        (
            "`only_if` and `unless` one parity",
            x().only_if([Guard::Parameter(0)]),
            x().unless([Guard::Parameter(0)]),
            false,
        ),
        // Declared with no step, an instruction does nothing; undeclared,
        // it makes no claim on what it does.
        (
            "declared and undeclared",
            Meaning::declared(),
            Meaning::undeclared(),
            false,
        ),
        // Angles compare bit for bit, so a meaning is `Eq`.
        ("one angle", rotate(0.5), rotate(0.5), true),
        ("zero and minus zero", rotate(0.0), rotate(-0.0), false),
        ("NaN and itself", rotate(f64::NAN), rotate(f64::NAN), true),
    ] {
        assert_eq!(a == b, equal, "{row}");
    }
}

/// A loader may start every meaning undeclared and add the steps it reads:
/// a step declares it.
#[test]
fn step_added_to_an_undeclared_meaning_declares_it() {
    assert_eq!(
        Meaning::undeclared().pauli(Pauli::x(0)),
        Meaning::declared().pauli(Pauli::x(0))
    );
}
