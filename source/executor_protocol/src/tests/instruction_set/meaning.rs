// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The canonical form of what an instruction does: two meanings written
//! otherwise compare equal. What a meaning refers to is checked when its
//! set is built, so its faults are tested with the set's declaration, in
//! `tests/instruction_set.rs`.

use crate::{Angle, Guard, Meaning, Pauli};

/// A `cx` listing that it keeps `Z_0` equals the one that leaves it out.
#[test]
fn clifford_lists_only_the_generators_it_changes() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    assert_eq!(
        cx(vec![
            (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
            (Pauli::z(0), Pauli::z(0)),
        ]),
        cx(vec![(Pauli::x(0), Pauli::x(0) * Pauli::x(1))]),
    );
}

#[test]
fn clifford_images_compare_in_any_order() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    let x = (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let z = (Pauli::z(1), Pauli::z(0) * Pauli::z(1));
    assert_eq!(cx(vec![x.clone(), z.clone()]), cx(vec![z, x]));
}

#[test]
fn guard_terms_compare_in_any_order() {
    let guarded = |guards: [Guard; 2]| Meaning::declared().pauli(Pauli::x(0)).only_if(guards);
    assert_eq!(
        guarded([Guard::Parameter(0), Guard::Outcome(0)]),
        guarded([Guard::Outcome(0), Guard::Parameter(0)]),
    );
}

/// A term twice cancels in a parity.
#[test]
fn guard_term_given_twice_cancels() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_eq!(
        x().only_if([
            Guard::Parameter(0),
            Guard::Parameter(1),
            Guard::Parameter(0)
        ]),
        x().only_if([Guard::Parameter(1)]),
    );
}

/// `unless` an empty parity always runs, so the step is as unguarded.
#[test]
fn unless_no_term_is_no_guard() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_eq!(x().unless([]), x());
}

/// A guard before any step is a slip (`GuardWithoutStep`) that the meaning
/// keeps, so it differs from one with the same steps, here none, and no
/// slip.
#[test]
fn meaning_with_a_slip_differs_from_one_without() {
    assert_ne!(
        Meaning::declared().only_if([Guard::Parameter(0)]),
        Meaning::declared()
    );
}

#[test]
fn if_and_unless_differ() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_ne!(
        x().only_if([Guard::Parameter(0)]),
        x().unless([Guard::Parameter(0)])
    );
}

/// Declared with no step, an instruction does nothing; undeclared, it
/// makes no claim on what it does.
#[test]
fn declared_and_undeclared_meanings_differ() {
    assert_ne!(Meaning::declared(), Meaning::undeclared());
}

/// Angles compare bit for bit, so a meaning is `Eq`.
#[test]
fn angles_compare_bit_for_bit() {
    let rotate = |angle: f64| Meaning::declared().rotate(Pauli::z(0), Angle::Literal(angle));
    assert_eq!(rotate(0.5), rotate(0.5));
    assert_ne!(rotate(0.0), rotate(-0.0));
    assert_eq!(rotate(f64::NAN), rotate(f64::NAN));
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
