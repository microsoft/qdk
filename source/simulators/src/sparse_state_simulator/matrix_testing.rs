// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::f64::consts::FRAC_1_SQRT_2;
use ndarray::{Array2, array};
use num_complex::Complex64;
use num_traits::One;
use num_traits::Zero;

use super::{SparseStateSim, controlled, nearly_zero::NearlyZero};
use core::f64::consts::PI;

/// Returns a unitary matrix representing the `X` operation.
#[must_use]
pub fn x() -> Array2<Complex64> {
    array![
        [Complex64::zero(), Complex64::one()],
        [Complex64::one(), Complex64::zero()]
    ]
}

/// Returns a unitary matrix representing the `Y` operation.
#[must_use]
pub fn y() -> Array2<Complex64> {
    array![
        [Complex64::zero(), -Complex64::i()],
        [Complex64::i(), Complex64::zero()]
    ]
}

/// Returns a unitary matrix representing the `Z` operation.
#[must_use]
pub fn z() -> Array2<Complex64> {
    array![
        [Complex64::one(), Complex64::zero()],
        [Complex64::zero(), -Complex64::one()]
    ]
}

/// Returns a unitary matrix representing the single-qubit Hadamard transformation.
#[must_use]
pub fn h() -> Array2<Complex64> {
    array![
        [Complex64::one(), Complex64::one()],
        [Complex64::one(), -Complex64::one()]
    ] * FRAC_1_SQRT_2
}

/// Returns a unitary matrix representing the `T` operation.
#[must_use]
pub fn t() -> Array2<Complex64> {
    array![
        [Complex64::one(), Complex64::zero()],
        [
            Complex64::zero(),
            Complex64::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2)
        ]
    ]
}

/// Returns a unitary matrix representing the `S` operation.
#[must_use]
pub fn s() -> Array2<Complex64> {
    array![
        [Complex64::one(), Complex64::zero()],
        [Complex64::zero(), Complex64::i()]
    ]
}

/// Returns a unitary matrix representing the `Rx` operation with the given angle.
#[must_use]
pub fn rx(theta: f64) -> Array2<Complex64> {
    let cos_theta = f64::cos(theta / 2.0);
    let sin_theta = f64::sin(theta / 2.0);
    array![
        [
            Complex64::new(cos_theta, 0.0),
            Complex64::new(0.0, -sin_theta)
        ],
        [
            Complex64::new(0.0, -sin_theta),
            Complex64::new(cos_theta, 0.0)
        ]
    ]
}

/// Returns a unitary matrix representing the `Ry` operation with the given angle.
#[must_use]
pub fn ry(theta: f64) -> Array2<Complex64> {
    let cos_theta = f64::cos(theta / 2.0);
    let sin_theta = f64::sin(theta / 2.0);
    array![
        [
            Complex64::new(cos_theta, 0.0),
            Complex64::new(-sin_theta, 0.0)
        ],
        [
            Complex64::new(sin_theta, 0.0),
            Complex64::new(cos_theta, 0.0)
        ]
    ]
}

/// Returns a unitary matrix representing the `Rz` operation with the given angle.
#[must_use]
pub fn rz(theta: f64) -> Array2<Complex64> {
    let exp_theta = Complex64::exp(Complex64::new(0.0, theta / 2.0));
    let neg_exp_theta = Complex64::exp(Complex64::new(0.0, -theta / 2.0));
    array![
        [neg_exp_theta, Complex64::zero()],
        [Complex64::zero(), exp_theta]
    ]
}

/// Returns a unitary matrix representing the `G` or `GlobalPhase` operation with the given angle.
#[must_use]
pub fn g(theta: f64) -> Array2<Complex64> {
    let neg_exp_theta = Complex64::exp(Complex64::new(0.0, -theta / 2.0));
    array![
        [Complex64::one(), Complex64::zero()],
        [Complex64::zero(), neg_exp_theta]
    ]
}

/// Returns a unitary matrix representing the `SWAP` operation.
#[must_use]
pub fn swap() -> Array2<Complex64> {
    array![
        [
            Complex64::one(),
            Complex64::zero(),
            Complex64::zero(),
            Complex64::zero()
        ],
        [
            Complex64::zero(),
            Complex64::zero(),
            Complex64::one(),
            Complex64::zero()
        ],
        [
            Complex64::zero(),
            Complex64::one(),
            Complex64::zero(),
            Complex64::zero()
        ],
        [
            Complex64::zero(),
            Complex64::zero(),
            Complex64::zero(),
            Complex64::one()
        ]
    ]
}

/// Transforms the given matrix into it's adjoint using the transpose of the complex conjugate.
#[must_use]
pub fn adjoint(u: &Array2<Complex64>) -> Array2<Complex64> {
    u.t().map(Complex64::conj)
}

fn is_self_adjoint(arr: &Array2<Complex64>) -> bool {
    arr == adjoint(arr)
}

fn are_equal_to_precision(actual: Array2<Complex64>, expected: Array2<Complex64>) -> bool {
    // If we use assert_eq here, we'll get bitten by finite precision.
    // We also can't use LAPACK, since that greatly complicates bindings,
    // so we do an ad hoc implementation here.
    (actual - expected).map(|x| x.norm()).sum() <= 1e-10
}

#[test]
fn h_is_self_adjoint() {
    assert!(is_self_adjoint(&h()));
}

#[test]
fn x_is_self_adjoint() {
    assert!(is_self_adjoint(&x()));
}

#[test]
fn y_is_self_adjoint() {
    assert!(is_self_adjoint(&y()));
}

#[test]
fn z_is_self_adjoint() {
    assert!(is_self_adjoint(&z()));
}

#[test]
fn swap_is_self_adjoint() {
    assert!(is_self_adjoint(&swap()));
}

#[test]
fn s_squares_to_z() {
    assert_eq!(s().dot(&s()), z());
}

#[test]
fn t_squares_to_s() {
    assert!(are_equal_to_precision(t().dot(&t()), s()));
}

#[test]
fn rx_pi_is_x() {
    assert!(are_equal_to_precision(Complex64::i() * rx(PI), x()));
}

#[test]
fn ry_pi_is_y() {
    assert!(are_equal_to_precision(Complex64::i() * ry(PI), y()));
}

#[test]
fn rz_pi_is_z() {
    assert!(are_equal_to_precision(Complex64::i() * rz(PI), z()));
}

#[test]
fn gate_multiplication() {
    assert!(are_equal_to_precision(x().dot(&y()), Complex64::i() * z()));
}

#[test]
fn controlled_extension() {
    fn cnot() -> Array2<Complex64> {
        array![
            [
                Complex64::one(),
                Complex64::zero(),
                Complex64::zero(),
                Complex64::zero()
            ],
            [
                Complex64::zero(),
                Complex64::one(),
                Complex64::zero(),
                Complex64::zero()
            ],
            [
                Complex64::zero(),
                Complex64::zero(),
                Complex64::zero(),
                Complex64::one()
            ],
            [
                Complex64::zero(),
                Complex64::zero(),
                Complex64::one(),
                Complex64::zero()
            ]
        ]
    }
    assert!(are_equal_to_precision(controlled(&x(), 1), cnot()));
    assert!(are_equal_to_precision(
        controlled(&x(), 2),
        controlled(&cnot(), 1)
    ));
    assert_eq!(controlled(&x(), 3).nrows(), 2_usize.pow(4));
}

/// Utility for testing operation equivalence.
fn assert_operation_equal_referenced<F1, F2>(mut op: F1, mut reference: F2, count: usize)
where
    F1: FnMut(&mut SparseStateSim, &[usize]),
    F2: FnMut(&mut SparseStateSim, &[usize]),
{
    let mut sim = SparseStateSim::default();

    // Allocate the controls we use to verify behavior.
    // Allocate the requested number of targets, entangling the control with them.
    let mut ctls = vec![];
    let mut qs = vec![];
    for _ in 0..count {
        let ctl = sim.allocate();
        let q = sim.allocate();
        sim.h(ctl);
        sim.mcx(&[ctl], q);
        qs.push(q);
        ctls.push(ctl);
    }

    op(&mut sim, &qs);
    reference(&mut sim, &qs);

    // Undo the entanglement.
    for (q, ctl) in qs.iter().zip(&ctls) {
        sim.mcx(&[*ctl], *q);
        sim.h(*ctl);
    }

    println!("{}", sim.dump());

    // We know the operations are equal if the qubits are left in the zero state.
    for (q, ctl) in qs.iter().zip(&ctls) {
        assert!(sim.joint_probability(&[*q]).is_nearly_zero());
        assert!(sim.joint_probability(&[*ctl]).is_nearly_zero());
    }

    // Sparse state vector should have one entry for |0⟩.
    assert_eq!(sim.state.len(), 1);
    // If the operations are equal including the phase, the entry should be 1.
    assert!(
        (sim.state
            .first()
            .expect("state should have at least one entry")
            .1
            - Complex64::one())
        .is_nearly_zero()
    );
}

#[test]
fn test_h() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.h(qs[0]);
        },
        |sim, qs| {
            sim.apply(&h(), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_x() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.x(qs[0]);
        },
        |sim, qs| {
            sim.apply(&x(), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_y() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.y(qs[0]);
        },
        |sim, qs| {
            sim.apply(&y(), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_z() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.z(qs[0]);
        },
        |sim, qs| {
            sim.apply(&z(), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_s() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.s(qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&s()), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_sadj() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.sadj(qs[0]);
        },
        |sim, qs| {
            sim.apply(&s(), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_cx() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcx(&[qs[0]], qs[1]);
        },
        |sim, qs| {
            sim.apply(&x(), &[qs[1]], Some(&[qs[0]]));
        },
        2,
    );
}

#[test]
fn test_cz() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcz(&[qs[0]], qs[1]);
        },
        |sim, qs| {
            sim.apply(&z(), &[qs[1]], Some(&[qs[0]]));
        },
        2,
    );
}

#[test]
fn test_swap() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.swap_qubit_ids(qs[0], qs[1]);
        },
        |sim, qs| {
            sim.apply(&swap(), &[qs[0], qs[1]], None);
        },
        2,
    );
}

#[test]
fn test_rz() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rz(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rz(PI / 7.0)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rz_pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rz(PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rz(PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(PI / 7.0)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx_pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx_2pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(2.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(2.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx_zero() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(0.0, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(0.0)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx_3pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(3.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(3.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_rx_4pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.rx(4.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&rx(4.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(PI / 7.0, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(PI / 7.0)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry_pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry_2pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(2.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(2.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry_zero() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(0.0, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(0.0)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry_3pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(3.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(3.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_ry_4pi() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.ry(4.0 * PI, qs[0]);
        },
        |sim, qs| {
            sim.apply(&adjoint(&ry(4.0 * PI)), &[qs[0]], None);
        },
        1,
    );
}

#[test]
fn test_mcri() {
    assert_operation_equal_referenced(
        |sim, qs| {
            sim.mcphase(
                &qs[2..3],
                Complex64::exp(Complex64::new(0.0, -(PI / 7.0) / 2.0)),
                qs[1],
            );
        },
        |sim, qs| {
            sim.apply(&adjoint(&g(PI / 7.0)), &[qs[1]], Some(&qs[2..3]));
        },
        3,
    );
}

#[test]
fn test_apply_four_qubit_unitary() {
    let mut sim = SparseStateSim::default();
    let qs: Vec<usize> = (0..4).map(|_| sim.allocate()).collect();

    let mut unitary = Array2::eye(16);
    unitary.swap((0, 0), (0, 1));
    unitary.swap((1, 0), (1, 1));

    sim.apply(&unitary, &qs, None);

    assert!(sim.qubit_is_zero(qs[0]));
    assert!(sim.qubit_is_zero(qs[1]));
    assert!(sim.qubit_is_zero(qs[2]));
    assert!((sim.joint_probability(&[qs[3]]) - 1.0).is_nearly_zero());
}
