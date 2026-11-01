// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Action, Angle, Pauli};

/// A Pauli's factors are kept by qubit, so the same operator written in
/// another order is equal: Qodec's `X_1 X_0` is its `X_0 X_1`.
#[test]
fn a_paulis_factors_compare_whatever_their_order() {
    assert_eq!(Pauli::x(1) * Pauli::x(0), Pauli::x(0) * Pauli::x(1));
}

/// Factors on the same qubit multiply as Paulis do, the phase included, so
/// that `X_0 Y_0` is `i Z_0`, its reverse `-i Z_0`, and `X_0 X_0` the
/// identity.
#[test]
fn factors_on_one_qubit_multiply_into_one() {
    let (x, y, z) = (Pauli::x(0), Pauli::y(0), Pauli::z(0));
    let i = Pauli::i;
    assert_eq!(x.clone() * y.clone(), i() * z.clone());
    assert_eq!(y.clone() * z.clone(), i() * x.clone());
    assert_eq!(z.clone() * x.clone(), i() * y.clone());
    assert_eq!(y.clone() * x.clone(), -(i() * z));
    assert_eq!(x.clone() * x, Pauli::identity());
}

/// Paulis on many shared qubits multiply as on one: `Y_q X_q = -i Z_q` on
/// each of 100 qubits gives `(-i)^100 = 1` times the Zs.
#[test]
fn paulis_sharing_many_qubits_multiply_into_one() {
    let on_all =
        |axis: fn(u32) -> Pauli| (0..100).fold(Pauli::identity(), |product, q| product * axis(q));
    let product = on_all(Pauli::y) * on_all(Pauli::x);
    assert_eq!(product, on_all(Pauli::z));
}

/// A sign is part of a Pauli: `-Z_0` is not `Z_0`, and negating twice
/// gives the operator back.
#[test]
fn a_negated_pauli_differs_until_negated_again() {
    assert_ne!(-Pauli::z(0), Pauli::z(0));
    assert_eq!(-(-Pauli::z(0)), Pauli::z(0));
}

/// A Pauli is written in Qodec's syntax, so that a failed match reads as
/// the file it came from.
#[test]
fn a_pauli_is_written_in_qodecs_syntax() {
    let written = [
        Pauli::x(1) * Pauli::z(0),
        -Pauli::z(0),
        Pauli::i() * Pauli::y(1),
        -(Pauli::i() * Pauli::x(0)),
        Pauli::identity(),
        -Pauli::identity(),
        Pauli::i(),
    ]
    .map(|pauli| pauli.to_string());
    assert_eq!(
        written,
        ["Z_0 X_1", "-Z_0", "i Y_1", "-i X_0", "I", "-I", "i"]
    );
}

/// A Clifford lists the generators it changes: one mapped to itself is
/// fixed, as one not listed is, so the two actions are the same.
#[test]
fn a_clifford_drops_the_generators_it_fixes() {
    let cx = |images: Vec<(Pauli, Pauli)>| Action::new([1, 1], [1, 1]).clifford(images);
    let changed = (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let fixed = (Pauli::z(0), Pauli::z(0));
    assert_eq!(cx(vec![changed.clone(), fixed]), cx(vec![changed]));
}

/// A Clifford's images compare whatever the order they are listed in.
#[test]
fn a_cliffords_images_compare_whatever_their_order() {
    let cx = |images: Vec<(Pauli, Pauli)>| Action::new([1, 1], [1, 1]).clifford(images);
    let x = (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let z = (Pauli::z(1), Pauli::z(0) * Pauli::z(1));
    assert_eq!(cx(vec![x.clone(), z.clone()]), cx(vec![z, x]));
}

/// The blocks are part of an action: a preparation, which takes no block,
/// differs from a reset, which takes one, though both prepare `Z_0`'s
/// state.
#[test]
fn actions_on_other_blocks_differ() {
    let prepare = Action::new([], [1]).stabilize([Pauli::z(0)]);
    let reset = Action::new([1], [1]).stabilize([Pauli::z(0)]);
    assert_ne!(prepare, reset);
}

/// Rotations by the same literal angle, or by the same parameter, are the
/// same; by another angle, or a parameter instead of a literal, they differ.
#[test]
fn rotations_are_the_same_by_the_same_angle() {
    let rotate = |angle| Action::new([1], [1]).rotate(Pauli::z(0), angle);
    assert_eq!(rotate(Angle::Literal(0.5)), rotate(Angle::Literal(0.5)));
    assert_eq!(rotate(Angle::Parameter(0)), rotate(Angle::Parameter(0)));
    assert_ne!(rotate(Angle::Literal(0.5)), rotate(Angle::Literal(0.25)));
    assert_ne!(rotate(Angle::Parameter(0)), rotate(Angle::Parameter(1)));
    assert_ne!(rotate(Angle::Literal(0.0)), rotate(Angle::Parameter(0)));
}

/// Literal angles compare bit for bit, so that a set's instructions stay
/// `Eq`: a rotation by NaN is the same as itself, and a rotation by `-0.0`
/// differs from one by `0.0`.
#[test]
fn literal_angles_compare_bit_for_bit() {
    let rotate = |angle| Action::new([1], [1]).rotate(Pauli::z(0), Angle::Literal(angle));
    assert_eq!(rotate(f64::NAN), rotate(f64::NAN));
    assert_ne!(rotate(0.0), rotate(-0.0));
}

/// An action's steps run in order: observing then preparing is not
/// preparing then observing.
#[test]
fn actions_with_their_steps_in_another_order_differ() {
    let blocks = || Action::new([1], [1]);
    let measure_reset = blocks().observe([Pauli::z(0)]).stabilize([Pauli::z(0)]);
    let reset_measure = blocks().stabilize([Pauli::z(0)]).observe([Pauli::z(0)]);
    assert_ne!(measure_reset, reset_measure);
}
